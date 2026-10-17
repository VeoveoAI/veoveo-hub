use super::*;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{
        Implementation, ReadResourceResponse, ResourceContents, ServerCapabilities, ServerConfig,
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use veoveo_audit_contract::{AuditPartition, AuditQuery, AuditReadScope};
use veoveo_types::{ResourceUri, ServerSlug};

fn projection() -> HubResourceProjection {
    HubResourceProjection {
        server: ServerSlug::parse("upstream-a").unwrap(),
        hub_uri: ResourceUri::new("fixture://docs/design").unwrap(),
        upstream_uri: ResourceUri::new("fixture://docs/design").unwrap(),
    }
}

fn result() -> ReadResourceResult {
    ReadResourceResult::new(vec![ResourceContents::text(
        "source content",
        projection().upstream_uri.to_string(),
    )])
}

#[test]
fn delivery_preserves_content_and_metadata_and_disables_caching() {
    let mut full = result();
    full.meta = Some(rmcp::model::MetaObject::default());
    full.meta
        .as_mut()
        .unwrap()
        .insert("fixture/metadata".into(), serde_json::json!({"id": 7}));
    let mut delivered = full.clone();
    prepare_delivery(&mut delivered, &projection()).unwrap();
    assert_eq!(delivered.contents, full.contents);
    assert_eq!(delivered.ttl_ms, Some(0));
    assert_eq!(delivered.cache_scope, Some(CacheScope::Private));
    assert_eq!(
        delivered.meta.as_ref().unwrap().get("fixture/metadata"),
        Some(&serde_json::json!({"id": 7}))
    );
}

#[derive(Clone)]
struct Source {
    calls: Arc<AtomicUsize>,
    denied: Arc<AtomicBool>,
}
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_resources().build();
        ServerConfig::new(capabilities).with_server_info(Implementation::new("source", "1"))
    }
    async fn read_resource(
        &self,
        _: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.denied.load(Ordering::SeqCst) {
            return Err(McpError::invalid_request("fixture authority revoked", None));
        }
        Ok(result().into())
    }
}

#[tokio::test]
async fn every_read_is_audited_before_delivery_and_fails_when_the_writer_stops() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("read audit qualification deadline");
}
async fn qualify() {
    let db = crate::test_store::TestDb::new().await;
    // A second pool reads what the Hub's writer committed.
    let reader = veoveo_audit::AuditStore::new(db.b.clone(), crate::audit_registry());
    let state = crate::HubState::new(db.a.clone())
        .bind_token_extensions(Default::default())
        .unwrap()
        .bind_oauth_client_resolver(std::sync::Arc::new(
            crate::oauth_clients::CatalogOAuthClientResolver,
        ))
        .unwrap();
    let hub = super::super::task_ownership_tests::hub(
        state.clone(),
        serde_json::from_str(include_str!("../../../testdata/hub.json")).unwrap(),
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
    let full = read_upstream(client.peer().clone(), params())
        .await
        .unwrap();
    hub.finish_resource_read(&subject, &projection(), Ok(full))
        .await
        .unwrap();
    let scope = AuditReadScope::new(subject.actor.tenant.clone(), false);
    let query = AuditQuery::new(AuditPartition::Tenant(
        subject.actor.tenant.clone().unwrap(),
    ));
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].draft.outcome(), AuditOutcome::Success);
    let draft = &page.records[0].draft;
    assert_eq!(
        reader
            .registry()
            .read::<Read>(draft.class(), draft.detail())
            .unwrap(),
        Read {
            method: McpReadMethod::ResourceRead,
            invocation: Some(subject.audit_invocation()),
        }
    );
    denied.store(true, Ordering::SeqCst);
    let denial = read_upstream(client.peer().clone(), params())
        .await
        .map_err(crate::mcp_support::upstream_error)
        .unwrap_err();
    assert_eq!(denial.code, rmcp::model::ErrorCode::INVALID_REQUEST);
    let delivered = hub
        .finish_resource_read(&subject, &projection(), Err(denial.clone().into()))
        .await
        .unwrap_err();
    assert_eq!(delivered.into_protocol(), denial);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 2);
    assert_eq!(page.records[1].draft.outcome(), AuditOutcome::Denied);
    hub.finish_resource_read(
        &subject,
        &projection(),
        Err(McpError::resource_not_found("fixture member missing", None).into()),
    )
    .await
    .unwrap_err();
    let page = reader.page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 3);
    assert_eq!(
        page.records[2].draft.reason().map(|reason| reason.as_str()),
        Some("not_found")
    );
    denied.store(false, Ordering::SeqCst);
    state
        .audit_writer()
        .await
        .shutdown(Duration::from_secs(5))
        .await
        .unwrap();
    let full = read_upstream(client.peer().clone(), params())
        .await
        .unwrap();
    let error = hub
        .finish_resource_read(&subject, &projection(), Ok(full))
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("source content"));
    assert_eq!(reader.page(&scope, &query).await.unwrap().records.len(), 3);
    client.cancel().await.unwrap();
    source.cancel().await.unwrap();
}
