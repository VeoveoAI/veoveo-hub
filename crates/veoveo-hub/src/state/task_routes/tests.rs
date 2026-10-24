use super::*;
use crate::test_store as fixture;

fn draft() -> HubTaskRouteDraft {
    let subject = crate::mcp::task_ownership_tests::subject();
    HubTaskRouteDraft {
        tenant: subject.authority.tenant.clone(),
        work_context: subject.authority.work_context.clone(),
        profile: HubProfileId::parse("admin").unwrap(),
        server: ServerSlug::parse("upstream-a").unwrap(),
        deployment: deployment(1),
        execution: capture().context(),
        source_task_id: "opaque-provider-task/one".into(),
        authority_digest: Sha256Digest::from_bytes([0xa; 32]),
        ownership: HubTaskOwnership::from_invocation(&subject.actor, &subject.authority),
        ttl_ms: Some(60_000),
        delegated_caller: None,
    }
}

fn deployment(index: u8) -> DeploymentId {
    DeploymentId::parse(format!("019a5b30-0000-7000-8000-{index:012}")).unwrap()
}

fn capture() -> veoveo_hub_sdk::ResolvedComposition {
    veoveo_hub_sdk::ResolvedComposition::new(
        [
            ("upstream-a", deployment(1)),
            ("another-server", deployment(2)),
        ]
        .into_iter()
        .map(|(slug, deployment)| {
            (
                ServerSlug::parse(slug).unwrap(),
                veoveo_hub_sdk::ResolvedComponent::Deployment { deployment },
            )
        })
        .collect(),
    )
}

/// The tenant of [`draft`]'s route.
fn input_tenant() -> veoveo_types::TenantId {
    draft().tenant
}

/// Task routes reference the Work Context the active configuration defines.
async fn database() -> fixture::TestDb {
    let db = fixture::TestDb::new().await;
    let input = draft();
    crate::test_catalog::publish_work_context(
        &crate::store::HubStore::new(db.a.clone()),
        &input.tenant,
        &input.work_context,
    )
    .await;
    let store = crate::store::HubStore::new(db.a.clone());
    store.retain_composition(&capture()).await.unwrap();
    for (slug, deployment) in [
        ("upstream-a", deployment(1)),
        ("another-server", deployment(2)),
        ("upstream-a", deployment(4)),
    ] {
        use veoveo_hub_sdk::registration::{
            AuthorizationDeclaration, ClientMetadata, RegistrationDeclaration, ServerEndpoint,
            ServerVersion, StorageBinding,
        };
        let key = veoveo_hub_sdk::registration::statement::ServerKeyPair::generate().unwrap();
        let metadata = ClientMetadata::new(
            *key.public_key(),
            RegistrationDeclaration {
                deployment,
                slug: ServerSlug::parse(slug).unwrap(),
                uri_scheme: veoveo_types::ResourceScheme::parse(slug).unwrap(),
                endpoint: ServerEndpoint::new(
                    url::Url::parse(&format!("https://{slug}.example/mcp")).unwrap(),
                )
                .unwrap(),
                version: ServerVersion::new("0.1.0").unwrap(),
            },
            AuthorizationDeclaration {
                permissions: Vec::new(),
                requirements: Default::default(),
                uses: Vec::new(),
                calls_hub: false,
            },
        )
        .unwrap();
        let storage = StorageBinding::SharedLogicalSchema {
            server: ServerSlug::parse(slug).unwrap(),
        };
        let thumbprint = metadata.key().thumbprint();
        sqlx::query_file!(
            "src/state/task_routes/tests/deployment.sql",
            deployment.as_uuid(),
            slug,
            thumbprint.as_str(),
            serde_json::to_value(&metadata).unwrap(),
            serde_json::to_value(storage).unwrap()
        )
        .execute(db.a.pool())
        .await
        .unwrap();
    }
    db
}

#[tokio::test]
async fn concurrent_projection_and_retry_reuse_one_route_without_extending_retention() {
    let db = database().await;
    let left = HubState::new(db.a.clone());
    let right = HubState::new(db.b.clone());
    let input = draft();
    let replies = futures::future::join_all((0..12).map(|i| {
        let state = if i % 2 == 0 { &left } else { &right };
        state.create_task_route(input.clone())
    }))
    .await
    .into_iter()
    .map(Result::unwrap)
    .collect::<Vec<_>>();
    let (canonical, route) = &replies[0];
    for (id, row) in &replies {
        assert_eq!(id, canonical);
        assert_eq!(row.expires_at, route.expires_at);
    }
    let mut retry = input.clone();
    retry.ttl_ms = Some(120_000);
    let (same, retained) = right.create_task_route(retry).await.unwrap();
    assert_eq!(&same, canonical);
    assert_eq!(retained.expires_at, route.expires_at);
    assert!(
        left.task_route(canonical, &input_tenant())
            .await
            .unwrap()
            .is_some()
    );
    // A lookup reads only the caller's tenant.
    let other = veoveo_types::TenantId::parse("other-tenant").unwrap();
    assert!(left.task_route(canonical, &other).await.unwrap().is_none());
    let routes = sqlx::query_file_scalar!("src/state/task_routes/tests/count.sql")
        .fetch_one(db.a.pool())
        .await
        .unwrap();
    assert_eq!(routes, 1);
}

#[tokio::test]
async fn changed_authority_or_expired_routes_cannot_be_rebound_by_an_upstream_retry() {
    let db = database().await;
    let state = HubState::new(db.a.clone());
    let input = draft();
    let (canonical, original) = state.create_task_route(input.clone()).await.unwrap();
    let mut work_context = input.clone();
    work_context.work_context = veoveo_types::WorkContextId::parse("other").unwrap();
    let mut authority = input.clone();
    authority.authority_digest = Sha256Digest::from_bytes([0xb; 32]);
    let mut labels = input.clone();
    labels.ownership.data_labels.clear();
    for altered in [work_context, authority, labels] {
        assert!(state.create_task_route(altered).await.is_err());
        let current = state
            .task_route(&canonical, &input_tenant())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.authority_digest, original.authority_digest);
        assert_eq!(current.expires_at, original.expires_at);
    }
    sqlx::query_file!(
        "src/state/task_routes/tests/expire.sql",
        canonical.as_str(),
        Utc::now() - TimeDelta::seconds(1),
    )
    .execute(db.a.pool())
    .await
    .unwrap();
    assert!(state.create_task_route(input.clone()).await.is_err());
    assert!(
        state
            .task_route(&canonical, &input_tenant())
            .await
            .unwrap()
            .is_none()
    );
    let mut another = input.clone();
    another.server = ServerSlug::parse("another-server").unwrap();
    another.deployment = deployment(2);
    let (independent, _) = state.create_task_route(another).await.unwrap();
    assert_ne!(canonical, independent);
    let mut overflow = input;
    overflow.source_task_id = "another-task".into();
    overflow.ttl_ms = Some(i64::MAX as u64);
    assert!(state.create_task_route(overflow).await.is_err());
}

#[tokio::test]
async fn retention_deletes_expired_routes_in_batches_and_keeps_live_ones() {
    let db = database().await;
    let state = HubState::new(db.a.clone());
    let mut expired = Vec::new();
    for index in 0..3 {
        let mut input = draft();
        input.source_task_id = format!("expired-task/{index}");
        expired.push(state.create_task_route(input).await.unwrap().0);
    }
    let mut live = draft();
    live.source_task_id = "live-task".into();
    let (live, _) = state.create_task_route(live).await.unwrap();
    for canonical in &expired {
        sqlx::query_file!(
            "src/state/task_routes/tests/expire.sql",
            canonical.as_str(),
            Utc::now() - TimeDelta::seconds(1),
        )
        .execute(db.a.pool())
        .await
        .unwrap();
    }
    let batch = veoveo_store::MaintenanceBatch::new(2).unwrap();
    let pruned = state
        .prune_expired_task_routes(Utc::now(), batch)
        .await
        .unwrap();
    assert_eq!((pruned.rows, pruned.batches), (3, 2));
    let routes = sqlx::query_file_scalar!("src/state/task_routes/tests/count.sql")
        .fetch_one(db.a.pool())
        .await
        .unwrap();
    assert_eq!(routes, 1);
    assert!(
        state
            .task_route(&live, &input_tenant())
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn stored_routes_keep_their_owner_provenance_and_labels() {
    let db = database().await;
    let state = HubState::new(db.a.clone());
    let subject = crate::mcp::task_ownership_tests::subject();
    let (canonical, created) = state.create_task_route(draft()).await.unwrap();
    let read = state
        .task_route(&canonical, &input_tenant())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.ownership, created.ownership);
    assert!(read.ownership.allows(&subject.actor, &subject.authority));
    let mut unlabeled = subject.actor.clone();
    unlabeled.data_labels.clear();
    assert!(!read.ownership.allows(&unlabeled, &subject.authority));
    assert_eq!(
        read.owner,
        deterministic_principal_id(subject.authority.tenant.as_str(), subject.actor.id.as_str())
            .unwrap()
    );
}

#[tokio::test]
async fn same_deployment_retry_preserves_first_execution_capture() {
    let db = database().await;
    let state = HubState::new(db.a.clone());
    let input = draft();
    let (canonical, first) = state.create_task_route(input.clone()).await.unwrap();
    let second = veoveo_hub_sdk::ResolvedComposition::new(capture().components().clone());
    state.hub_store().retain_composition(&second).await.unwrap();
    let mut retry = input;
    retry.execution = second.context();
    let (same, retained) = state.create_task_route(retry).await.unwrap();
    assert_eq!(same, canonical);
    assert_eq!(retained.execution, first.execution);
    assert_eq!(retained.deployment, first.deployment);
}

#[tokio::test]
async fn identical_source_tasks_from_two_deployments_never_share_a_route() {
    let db = database().await;
    let state = HubState::new(db.a.clone());
    let input = draft();
    let (first, _) = state.create_task_route(input.clone()).await.unwrap();
    let mut components = capture().components().clone();
    components.insert(
        input.server.clone(),
        veoveo_hub_sdk::ResolvedComponent::Deployment {
            deployment: deployment(4),
        },
    );
    let composition = veoveo_hub_sdk::ResolvedComposition::new(components);
    state
        .hub_store()
        .retain_composition(&composition)
        .await
        .unwrap();
    let mut replacement = input.clone();
    replacement.deployment = deployment(4);
    replacement.execution = composition.context();
    let (second, _) = state.create_task_route(replacement).await.unwrap();
    assert_ne!(first, second);
    assert_eq!(state.create_task_route(input).await.unwrap().0, first);
    assert_eq!(
        state
            .task_route(&first, &input_tenant())
            .await
            .unwrap()
            .unwrap()
            .deployment,
        deployment(1)
    );
    assert_eq!(
        state
            .task_route(&second, &input_tenant())
            .await
            .unwrap()
            .unwrap()
            .deployment,
        deployment(4)
    );
}
