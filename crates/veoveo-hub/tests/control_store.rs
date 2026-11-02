#[path = "support/catalog_admission.rs"]
mod catalog_admission;
#[path = "support/database.rs"]
mod database;
use chrono::{SubsecRound, Utc};
use database as fixture;

use veoveo_hub::control_plane::{
    HubControlPlane, HubControlPlaneRevision, HubControlPlaneRevisionId,
    HubControlPlaneRevisionSource, PolicySet, TenantDefinition,
};
use veoveo_hub::{HubControlStore, is_revision_conflict};
use veoveo_store::{PlatformStore, deterministic_tenant_id};
use veoveo_types::{PolicyVersion, PrincipalId, TenantId};

#[tokio::test]
async fn publishes_immutable_revisions_and_moves_active_pointer_atomically() {
    tokio::time::timeout(std::time::Duration::from_secs(90), publication())
        .await
        .expect("control publication exceeded 90 seconds");
}

async fn publication() {
    let db = fixture::TestDb::new().await;
    let store = HubControlStore::from_platform_store(db.a.clone(), catalog_admission::binding());

    assert!(store.load_active_revision().await.unwrap().is_none());
    assert!(store.load_active_revision_head().await.unwrap().is_none());

    let audit_context = audit_context();
    let first = revision("gcp-first", "a".repeat(64), empty_control_plane());
    store.record_revision(&first, &audit_context).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(first.clone())
    );
    let first_head = store.load_active_revision_head().await.unwrap().unwrap();
    assert_eq!(first_head.revision_id, first.revision_id);
    assert_eq!(first_head.sha256, first.sha256);
    assert_eq!(revision_count(&db.a).await, 1);
    assert_eq!(active_object_count(&db.a).await, 0);

    let mut second_plane = empty_control_plane();
    second_plane.tenants.push(TenantDefinition {
        id: TenantId::parse("tenant-integration").unwrap(),
        title: Some("Integration tenant".to_owned()),
        description: None,
        metadata: serde_json::json!({}),
    });
    second_plane.policies.push(PolicySet {
        version: PolicyVersion::parse("r1").unwrap(),
        rules: Vec::new(),
        metadata: serde_json::Value::Null,
    });
    let second = revision("gcp-second", "b".repeat(64), second_plane);
    store
        .record_revision(&second, &audit_context)
        .await
        .unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(second.clone())
    );
    let second_head = store.load_active_revision_head().await.unwrap().unwrap();
    assert_eq!(second_head.revision_id, second.revision_id);
    assert_eq!(second_head.sha256, second.sha256);
    assert_eq!(revision_count(&db.a).await, 2);
    assert_eq!(active_object_count(&db.a).await, 2);
    let tenant_id = deterministic_tenant_id("tenant-integration").unwrap();
    assert_eq!(
        tenant_slug(&db.b, tenant_id).await.as_deref(),
        Some("tenant-integration")
    );

    // A tenant a later revision omits keeps its identity row, as the principals and
    // Task routes that name it do.
    let mut third_plane = second.control_plane.clone();
    third_plane.tenants.clear();
    let third = revision("gcp-third", "c".repeat(64), third_plane);
    store.record_revision(&third, &audit_context).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(third.clone())
    );
    assert_eq!(revision_count(&db.a).await, 3);
    assert_eq!(active_object_count(&db.a).await, 1);
    assert_eq!(
        tenant_slug(&db.a, tenant_id).await.as_deref(),
        Some("tenant-integration")
    );

    let duplicate = store
        .record_revision(&second, &audit_context)
        .await
        .expect_err("duplicate immutable revision unexpectedly succeeded");
    assert!(is_revision_conflict(&duplicate));
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(third.clone()),
        "failed publication moved the active pointer"
    );
    assert_eq!(revision_count(&db.a).await, 3);
    assert_eq!(
        publications(&db.a).await,
        ["gcp-first", "gcp-second", "gcp-third"],
        "even an empty or owner-only revision has an atomic publication record"
    );
}

#[tokio::test]
async fn concurrent_publications_activate_one_revision_and_refuse_the_other() {
    let db = fixture::TestDb::new().await;
    let left = HubControlStore::from_platform_store(db.a.clone(), catalog_admission::binding());
    let right = HubControlStore::from_platform_store(db.b.clone(), catalog_admission::binding());
    let context = audit_context();
    let base = revision("gcp-base", "d".repeat(64), empty_control_plane());
    left.record_revision(&base, &context).await.unwrap();
    let one = revision("gcp-one", "e".repeat(64), empty_control_plane());
    let two = revision("gcp-two", "f".repeat(64), empty_control_plane());
    // Both publishers base their revision on the same active one, whichever commits first.
    let (first, second) = tokio::join!(
        left.record_revision_on(Some(&base), &one, &context),
        right.record_revision_on(Some(&base), &two, &context),
    );
    let outcomes = [first, second];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    let refused = outcomes
        .iter()
        .find_map(|outcome| outcome.as_ref().err())
        .unwrap();
    assert!(is_revision_conflict(refused), "{refused:#}");
    let active = left.load_active_revision().await.unwrap().unwrap();
    assert!(active == one || active == two);
    assert_eq!(revision_count(&db.a).await, 2);
}

async fn revision_count(store: &PlatformStore) -> i64 {
    sqlx::query_file_scalar!("tests/control_store/revision_count.sql")
        .fetch_one(store.pool())
        .await
        .unwrap()
}

async fn active_object_count(store: &PlatformStore) -> i64 {
    sqlx::query_file_scalar!("tests/control_store/active_object_count.sql")
        .fetch_one(store.pool())
        .await
        .unwrap()
}

async fn publications(store: &PlatformStore) -> Vec<String> {
    sqlx::query_file_scalar!("tests/control_store/publications.sql")
        .fetch_all(store.pool())
        .await
        .unwrap()
}

async fn tenant_slug(store: &PlatformStore, tenant: veoveo_store::TenantId) -> Option<String> {
    sqlx::query_file_scalar!(
        "tests/control_store/tenant_slug.sql",
        tenant as veoveo_store::TenantId
    )
    .fetch_optional(store.pool())
    .await
    .unwrap()
}

fn audit_context() -> veoveo_audit_contract::AuditContext {
    veoveo_audit_contract::AuditContext {
        actor: veoveo_audit_contract::AuditActor {
            principal: PrincipalId::parse("integration-admin").unwrap(),
            kind: veoveo_audit_contract::AuditPrincipalKind::Service,
            tenant: None,
        },
        via: None,
        request: veoveo_audit_contract::AuditRequest::background(),
    }
}

fn revision(id: &str, sha256: String, control_plane: HubControlPlane) -> HubControlPlaneRevision {
    HubControlPlaneRevision {
        revision_id: HubControlPlaneRevisionId::parse(id).unwrap(),
        sha256,
        source: HubControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now().trunc_subsecs(6),
        applied_by: PrincipalId::parse("integration-admin").unwrap(),
        tenant: None,
        control_plane,
    }
}

fn empty_control_plane() -> HubControlPlane {
    HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: Vec::new(),
        authorization_servers: Vec::new(),
        profiles: Vec::new(),
        authorization: Default::default(),
        registration: None,
        extensions: Default::default(),
        tenants: Vec::new(),
        policies: Vec::new(),
        data_labels: Vec::new(),
        oauth_clients: Vec::new(),
        oidc_clients: Vec::new(),
        secrets: Vec::new(),
        metadata: serde_json::json!({}),
    }
}
