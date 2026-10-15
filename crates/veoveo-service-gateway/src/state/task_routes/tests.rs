use super::*;
use crate::test_store as fixture;

fn draft() -> GatewayTaskRouteDraft {
    let subject = crate::mcp::task_ownership_tests::subject();
    GatewayTaskRouteDraft {
        tenant: subject.authority.tenant.clone(),
        work_context: subject.authority.work_context.clone(),
        profile: GatewayProfileId::parse("admin").unwrap(),
        server: ServerSlug::parse("upstream-a").unwrap(),
        source_task_id: "opaque-provider-task/one".into(),
        authority_digest: Sha256Digest::from_bytes([0xa; 32]),
        ownership: GatewayTaskOwnership::from_invocation(&subject.actor, &subject.authority),
        ttl_ms: Some(60_000),
        delegated_caller: None,
    }
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
        &crate::store::GatewayStore::new(db.a.clone()),
        &input.tenant,
        &input.work_context,
    )
    .await;
    db
}

#[tokio::test]
async fn concurrent_projection_and_retry_reuse_one_route_without_extending_retention() {
    let db = database().await;
    let left = GatewayState::new(db.a.clone());
    let right = GatewayState::new(db.b.clone());
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
    let state = GatewayState::new(db.a.clone());
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
    let state = GatewayState::new(db.a.clone());
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
    let state = GatewayState::new(db.a.clone());
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
