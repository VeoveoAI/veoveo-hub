use super::*;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{Implementation, ReadResourceResponse, ServerCapabilities, ServerConfig},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use veoveo_audit_contract::{AuditPartition, AuditQuery, AuditReadScope};
use veoveo_types::{ResourceScheme, ResourceUri, ServerSlug};

fn projection() -> GatewayResourceProjection {
    GatewayResourceProjection {
        server: ServerSlug::parse("upstream-a").unwrap(),
        gateway_uri: ResourceUri::new("fixture://docs/design").unwrap(),
        upstream_uri: ResourceUri::new("fixture://docs/design").unwrap(),
    }
}
fn observation() -> Observation {
    let descriptor = knowledge::docs::collection(
        &projection().server,
        &ResourceScheme::parse("fixture").unwrap(),
    );
    Observation::builder(
        descriptor.collection().clone(),
        Revision::parse("source-revision").unwrap(),
        knowledge::content_digest("source content"),
        chrono::Utc::now(),
    )
    .build(&descriptor)
    .unwrap()
}
fn result(meta: &rmcp::model::RequestMetaObject) -> ReadResourceResult {
    knowledge::server::member_result(
        &projection().upstream_uri,
        "text/plain",
        "source content".into(),
        observation(),
        &knowledge::docs::collection(
            &projection().server,
            &ResourceScheme::parse("fixture").unwrap(),
        ),
        Some(meta),
    )
    .unwrap()
}

#[test]
fn delivery_preserves_content_and_unknown_metadata_but_observations_require_negotiation() {
    let mut meta = rmcp::model::RequestMetaObject::default();
    knowledge::client::declare_read(&mut meta, None);
    let full = result(&meta);
    for declared in [false, true] {
        let mut delivered = full.clone();
        delivered
            .meta
            .as_mut()
            .unwrap()
            .insert("fixture/metadata".into(), serde_json::json!({"id": 7}));
        prepare_delivery(&mut delivered, &projection(), declared).unwrap();
        assert_eq!(delivered.contents, full.contents);
        assert_eq!(delivered.ttl_ms, Some(0));
        assert_eq!(delivered.cache_scope, Some(CacheScope::Private));
        assert_eq!(
            delivered.meta.as_ref().unwrap().get("fixture/metadata"),
            Some(&serde_json::json!({"id": 7}))
        );
        assert_eq!(
            knowledge::client::observation(&delivered)
                .unwrap()
                .is_some(),
            declared
        );
    }
}

#[derive(Clone)]
struct Source {
    calls: Arc<AtomicUsize>,
    denied: Arc<AtomicBool>,
}
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder().enable_resources().build();
        knowledge::server::declare(&mut capabilities);
        ServerConfig::new(capabilities).with_server_info(Implementation::new("source", "1"))
    }
    async fn read_resource(
        &self,
        _: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(knowledge::server::requested(Some(&context.meta)).unwrap());
        if self.denied.load(Ordering::SeqCst) {
            return Err(McpError::invalid_request("fixture authority revoked", None));
        }
        Ok(result(&context.meta).into())
    }
}

#[tokio::test]
async fn negotiated_read_commits_observation_before_delivery_and_fails_when_writer_stops() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("read audit qualification deadline");
}
async fn qualify() {
    let db = crate::test_store::TestDb::new().await;
    // A second pool reads what the Gateway's writer committed.
    let reader = veoveo_audit::AuditStore::new(db.b.clone(), crate::kernel_audit_target_registry());
    let state = crate::GatewayState::new(db.a.clone())
        .bind_token_extensions(Default::default())
        .unwrap()
        .bind_oauth_client_resolver(std::sync::Arc::new(
            crate::oauth_clients::CatalogOAuthClientResolver,
        ))
        .unwrap();
    let gateway = super::super::task_ownership_tests::gateway(
        state.clone(),
        serde_json::from_str(include_str!("../../../testdata/gateway.json")).unwrap(),
    );
    let subject = super::super::task_ownership_tests::subject();
    let calls = Arc::new(AtomicUsize::new(0));
    let denied = Arc::new(AtomicBool::new(false));
    let (client_io, server_io) = tokio::io::duplex(32768);
    let (source, client) = tokio::join!(
        Source {
            calls: calls.clone(),
            denied: denied.clone()
        }
        .serve(server_io),
        ().serve(client_io)
    );
    let source = source.unwrap();
    let client = client.unwrap();
    let params = || ReadResourceRequestParams::new(projection().upstream_uri.to_string());
    let (full, observation) = read_upstream(client.peer().clone(), params(), &projection(), None)
        .await
        .unwrap();
    let observation = observation.unwrap();
    gateway
        .finish_resource_read(
            &subject,
            &projection(),
            Ok((full, Some(observation.clone()))),
        )
        .await
        .unwrap();
    let scope = AuditReadScope::new(subject.actor.tenant.clone(), false);
    let query = AuditQuery::new(AuditPartition::Tenant(
        subject.actor.tenant.clone().unwrap(),
    ));
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].draft.outcome(), AuditOutcome::Succeeded);
    assert!(
        matches!(page.records[0].draft.detail(), AuditDetail::KnowledgeRead { observation: Some(recorded), status: KnowledgeReadStatus::Read, .. } if **recorded == (&observation).into())
    );
    let (conditional, observed) = read_upstream(
        client.peer().clone(),
        params(),
        &projection(),
        Some(observation.revision()),
    )
    .await
    .unwrap();
    assert!(conditional.contents.is_empty());
    gateway
        .finish_resource_read(&subject, &projection(), Ok((conditional, observed)))
        .await
        .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "conditional read must reach source authority again"
    );
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 2);
    assert!(matches!(
        page.records[1].draft.detail(),
        AuditDetail::KnowledgeRead {
            status: KnowledgeReadStatus::NotModified,
            ..
        }
    ));
    denied.store(true, Ordering::SeqCst);
    let denial = read_upstream(
        client.peer().clone(),
        params(),
        &projection(),
        Some(observation.revision()),
    )
    .await
    .map_err(crate::mcp_support::upstream_error)
    .unwrap_err();
    assert_eq!(denial.code, rmcp::model::ErrorCode::INVALID_REQUEST);
    let delivered = gateway
        .finish_resource_read(&subject, &projection(), Err(denial.clone().into()))
        .await
        .unwrap_err();
    assert_eq!(delivered.into_protocol(), denial);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 3);
    assert_eq!(page.records[2].draft.outcome(), AuditOutcome::Denied);
    gateway
        .finish_resource_read(
            &subject,
            &projection(),
            Err(McpError::resource_not_found("fixture member missing", None).into()),
        )
        .await
        .unwrap_err();
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 4);
    assert_eq!(page.records[3].draft.reason(), AuditReason::NotFound);
    denied.store(false, Ordering::SeqCst);
    state
        .audit_writer()
        .await
        .shutdown(Duration::from_secs(5))
        .await
        .unwrap();
    let (full, observed) = read_upstream(client.peer().clone(), params(), &projection(), None)
        .await
        .unwrap();
    let error = gateway
        .finish_resource_read(&subject, &projection(), Ok((full, observed)))
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("source content"));
    assert_eq!(reader.page(&scope, &query).await.unwrap().records.len(), 4);
    client.cancel().await.unwrap();
    source.cancel().await.unwrap();
}
