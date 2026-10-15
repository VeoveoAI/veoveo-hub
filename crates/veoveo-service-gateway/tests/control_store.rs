#[path = "support/catalog_admission.rs"]
mod catalog_admission;
#[path = "support/database.rs"]
mod database;
use database as fixture;
use std::collections::BTreeSet;

use chrono::{SubsecRound, Utc};

use veoveo_sdk_rust::OAuthClientId;
use veoveo_service_gateway::control_plane::{
    GatewayControlPlane, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, PolicySet, TenantDefinition, WorkContextDefinition,
    WorkContextMembershipRule,
};
use veoveo_service_gateway::{GatewayControlStore, is_revision_conflict};
use veoveo_store::{PlatformStore, deterministic_tenant_id};
use veoveo_types::{AccessSubject, GroupId, PolicyVersion, PrincipalId, TenantId, WorkContextId};
use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

#[tokio::test]
async fn publishes_immutable_revisions_and_moves_active_pointer_atomically() {
    tokio::time::timeout(std::time::Duration::from_secs(90), publication())
        .await
        .expect("control publication exceeded 90 seconds");
}

async fn publication() {
    let db = fixture::TestDb::new().await;
    let store =
        GatewayControlStore::from_platform_store(db.a.clone(), catalog_admission::binding());

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
    second_plane.work_contexts.push(WorkContextDefinition {
        id: WorkContextId::parse("operations").unwrap(),
        tenant: TenantId::parse("tenant-integration").unwrap(),
        title: "Operations".to_owned(),
        policy_revision: PolicyVersion::parse("r1").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Group(GroupId::parse("operations").unwrap()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        memberships: vec![WorkContextMembershipRule {
            level: WorkContextMembershipLevel::Contributor,
            principals: BTreeSet::new(),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            oauth_clients: BTreeSet::from([OAuthClientId::parse("automation").unwrap()]),
        }],
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
    assert_eq!(active_object_count(&db.a).await, 3);
    let tenant_id = deterministic_tenant_id("tenant-integration").unwrap();
    let context =
        db.b.work_context_by_key(tenant_id, "operations")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(context.title, "Operations");
    assert_eq!(context.output_policy.owner_key, "operations");
    assert_eq!(
        context.membership_for_oauth_client("automation"),
        Some(veoveo_store::WorkContextMembershipLevel::Contributor)
    );

    let mut third_plane = second.control_plane.clone();
    third_plane.work_contexts.clear();
    let third = revision("gcp-third", "c".repeat(64), third_plane);
    store.record_revision(&third, &audit_context).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(third.clone())
    );
    assert_eq!(revision_count(&db.a).await, 3);
    assert_eq!(active_object_count(&db.a).await, 2);
    assert!(
        db.a.work_context_by_key(tenant_id, "operations")
            .await
            .unwrap()
            .is_none()
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
    assert!(
        db.a.work_context_by_key(tenant_id, "operations")
            .await
            .unwrap()
            .is_none(),
        "failed re-publication must not recreate a deleted Work Context"
    );
    assert_eq!(
        account_activities(&db.a).await,
        ["create", "delete"],
        "failed re-publication must not re-audit a deleted Work Context"
    );
}

#[tokio::test]
async fn concurrent_publications_activate_one_revision_and_refuse_the_other() {
    let db = fixture::TestDb::new().await;
    let left = GatewayControlStore::from_platform_store(db.a.clone(), catalog_admission::binding());
    let right =
        GatewayControlStore::from_platform_store(db.b.clone(), catalog_admission::binding());
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

async fn account_activities(store: &PlatformStore) -> Vec<String> {
    sqlx::query_file_scalar!("tests/control_store/account_details.sql")
        .fetch_all(store.pool())
        .await
        .unwrap()
}

fn audit_context() -> veoveo_audit_contract::AuditContext {
    veoveo_audit_contract::AuditContext {
        actor: veoveo_audit_contract::AuditActor {
            principal: PrincipalId::parse("integration-admin").unwrap(),
            kind: veoveo_audit_contract::AuditPrincipalKind::Service,
            tenant: None,
            work_context: None,
        },
        via: None,
        request: veoveo_audit_contract::AuditRequest::background(),
    }
}

fn revision(
    id: &str,
    sha256: String,
    control_plane: GatewayControlPlane,
) -> GatewayControlPlaneRevision {
    GatewayControlPlaneRevision {
        revision_id: GatewayControlPlaneRevisionId::parse(id).unwrap(),
        sha256,
        source: GatewayControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now().trunc_subsecs(6),
        applied_by: PrincipalId::parse("integration-admin").unwrap(),
        tenant: None,
        control_plane,
    }
}

fn empty_control_plane() -> GatewayControlPlane {
    GatewayControlPlane {
        branding: None,
        identity_providers: Vec::new(),
        authorization_servers: Vec::new(),
        profiles: Vec::new(),
        authorization: Default::default(),
        registration: None,
        extensions: Default::default(),
        tenants: Vec::new(),
        work_contexts: Vec::new(),
        policies: Vec::new(),
        data_labels: Vec::new(),
        oauth_clients: Vec::new(),
        oidc_clients: Vec::new(),
        secrets: Vec::new(),
        lore: None,
        metadata: serde_json::json!({}),
    }
}
