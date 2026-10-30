//! Native transaction evidence; no production failure or synchronization hook.
use super::*;
use crate::audit::AuditAppend;
use crate::control_plane::{
    HubControlPlaneRevisionSource, PolicySet, TenantDefinition, WorkContextDefinition,
    WorkContextMembershipRule,
};
use crate::store::TransactionAppend;
use chrono::{SubsecRound, Utc};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::{Notify, oneshot};
use veoveo_audit_contract::{AuditActor, AuditContext, AuditPrincipalKind, AuditRequest};
use veoveo_types::{
    AccessLevel, AccessSubject, GroupId, PolicyVersion, PrincipalId, TenantId, WorkContextGrant,
    WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

struct FailAfterAppend {
    audit: AuditAppend,
}

impl TransactionAppend for FailAfterAppend {
    async fn append(&self, connection: &mut sqlx::PgConnection) -> Result<(), HubStoreError> {
        let observation = sqlx::query_file!("src/control_store/tests/connection.sql")
            .fetch_one(&mut *connection)
            .await?;
        assert_eq!(observation.isolation, "read committed");
        self.audit.append(connection).await?;
        Err(HubStoreError::Append(Box::new(std::io::Error::other(
            "publication fixture fails after the actual successful batch was appended",
        ))))
    }
}

struct HoldAfterAppend {
    audit: AuditAppend,
    entered: std::sync::Mutex<Option<oneshot::Sender<i32>>>,
    release: Arc<Notify>,
}

impl TransactionAppend for HoldAfterAppend {
    async fn append(&self, connection: &mut sqlx::PgConnection) -> Result<(), HubStoreError> {
        let observation = sqlx::query_file!("src/control_store/tests/connection.sql")
            .fetch_one(&mut *connection)
            .await?;
        assert_eq!(observation.isolation, "read committed");
        self.audit.append(connection).await?;
        self.entered
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(observation.pid)
            .unwrap();
        self.release.notified().await;
        Ok(())
    }
}

#[tokio::test]
async fn publication_late_append_failure_rolls_back_all_staged_rows() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let store = control_store(db.a.clone());
        let base = revision("gcp-publication-base", 'a', &["removed", "retained"]);
        store
            .record_revision_on(None, &base, &context())
            .await
            .unwrap();
        let before = snapshot(&db.b).await;
        let candidate = revision("gcp-publication-failed", 'b', &["retained", "new"]);
        let (objects, audit) = publication(&store, &base, &candidate);
        let result = store
            .store
            .publish_control_revision(
                ControlPublication {
                    revision: &candidate,
                    replaces: Some(&base.revision_id),
                    objects: &objects,
                },
                &FailAfterAppend { audit },
            )
            .await;
        assert!(
            matches!(result, Err(HubStoreError::Append(_))),
            "{result:?}"
        );
        // Waiting for a new locker observes SQLx's queued rollback completion;
        // dropping the Rust transaction alone is not an acknowledged rollback.
        sqlx::query_file!("src/control_store/tests/lock.sql")
            .fetch_one(db.b.pool())
            .await
            .unwrap();
        assert_eq!(
            snapshot(&db.b).await,
            before,
            "late failure must leave no revision/object/identity/success-audit contribution"
        );
        assert_eq!(store.load_active_revision().await.unwrap(), Some(base));
    })
    .await
    .expect("late publication rollback exceeded 90 seconds");
}

#[tokio::test]
async fn publication_blocked_stale_writer_leaves_no_owned_rows_or_success_batch() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let left = control_store(db.a.clone());
        let right = control_store(db.b.clone());
        let base = revision("gcp-overlap-base", 'c', &["removed", "retained"]);
        left.record_revision_on(None, &base, &context()).await.unwrap();
        let before = snapshot(&db.b).await;
        let winner = revision("gcp-overlap-winner", 'd', &["retained", "winner"]);
        let expected_winner = winner.clone();
        let loser = revision("gcp-overlap-loser", 'e', &["loser"]);
        let (objects, audit) = publication(&left, &base, &winner);
        let expected_objects = objects.clone();
        let (entered, arrived) = oneshot::channel();
        let release = Arc::new(Notify::new());
        let appender = HoldAfterAppend {
            audit, entered: std::sync::Mutex::new(Some(entered)), release: release.clone(),
        };
        let expected = base.revision_id.clone();
        let winner_task = tokio::spawn(async move {
            left.store.publish_control_revision(
                ControlPublication { revision: &winner, replaces: Some(&expected), objects: &objects },
                &appender,
            ).await
        });
        let owner = arrived.await.expect("first publisher reached real append");
        let previous = base.clone();
        let loser_task = tokio::spawn(async move {
            right.record_revision_on(Some(&previous), &loser, &context()).await
        });
        // This observes a real blocked lock query; scheduling a second future is
        // insufficient evidence that it competed before the winner committed.
        loop {
            let blocked = sqlx::query_file!("src/control_store/tests/blocked.sql", owner)
                .fetch_one(db.b.pool()).await.unwrap().blocked;
            if blocked { break; }
            tokio::task::yield_now().await;
        }
        assert_eq!(snapshot(&db.b).await, before,
            "one statement must observe the old committed projection while writes are staged");
        release.notify_one();
        winner_task.await.unwrap().unwrap();
        let error = loser_task.await.unwrap().expect_err("stale predecessor must refuse");
        assert!(is_revision_conflict(&error), "{error:#}");
        let after = snapshot(&db.b).await;
        let ids: Vec<_> = after["revisions"].as_array().unwrap().iter()
            .map(|row| row["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["gcp-overlap-base", "gcp-overlap-winner"]);
        assert_eq!(after["active"], "gcp-overlap-winner");
        let keys: Vec<_> = after["contexts"].as_array().unwrap().iter()
            .map(|row| row["context_key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["retained", "winner"]);
        assert_eq!(after["grants"].as_array().unwrap().len(), 4);
        assert_eq!(after["memberships"].as_array().unwrap().len(), 4);
        for expected in &expected_winner.control_plane.work_contexts {
            let id = veoveo_store::deterministic_work_context_id(
                expected.tenant.as_str(), expected.id.as_str()).unwrap().to_string();
            let tenant = veoveo_store::deterministic_tenant_id(expected.tenant.as_str())
                .unwrap().to_string();
            let actual = after["contexts"].as_array().unwrap().iter()
                .find(|row| row["id"] == id).expect("complete winner context row");
            assert_eq!(actual["tenant"], tenant);
            assert_eq!(actual["context_key"], expected.id.as_str());
            assert_eq!(actual["title"], expected.title);
            assert_eq!(actual["policy_revision"], "publication-r1");
            assert_eq!(actual["owner_kind"], "group");
            assert_eq!(actual["owner_key"], "gcp-overlap-winner-owners");
            assert_eq!(actual["classification"], Value::Null);
            assert_eq!(actual["data_labels"], serde_json::json!([]));
            let updated: chrono::DateTime<Utc> = actual["updated_at"].as_str().unwrap().parse().unwrap();
            assert_eq!(updated, expected_winner.applied_at);
            let created: chrono::DateTime<Utc> = actual["created_at"].as_str().unwrap().parse().unwrap();
            assert_eq!(created, if expected.id.as_str() == "retained" {
                base.applied_at
            } else { expected_winner.applied_at });
            let grants: Vec<_> = after["grants"].as_array().unwrap().iter()
                .filter(|row| row["work_context"] == id).cloned().collect();
            assert_eq!(grants, vec![
                serde_json::json!({"work_context": id, "ordinal": 0, "subject_kind": "group",
                    "subject_key": "gcp-overlap-winner-readers", "permission": "read"}),
                serde_json::json!({"work_context": id, "ordinal": 1, "subject_kind": "principal",
                    "subject_key": "gcp-overlap-winner-editor", "permission": "write"}),
            ], "winner replaces the retained context's exact grant ordinals and values");
            let memberships: Vec<_> = after["memberships"].as_array().unwrap().iter()
                .filter(|row| row["work_context"] == id).cloned().collect();
            assert_eq!(memberships, vec![
                serde_json::json!({"work_context": id, "ordinal": 0, "level": "viewer",
                    "principals": ["gcp-overlap-winner-viewer"], "groups": [], "roles": [], "oauth_clients": []}),
                serde_json::json!({"work_context": id, "ordinal": 1, "level": "contributor",
                    "principals": ["gcp-overlap-winner-contributor"], "groups": [], "roles": [], "oauth_clients": []}),
            ], "winner replaces exact membership ordinals and selectors");
        }
        let winner_objects: Vec<_> = after["objects"].as_array().unwrap().iter()
            .filter(|row| row["revision"] == "gcp-overlap-winner").cloned().collect();
        let mut complete_objects: Vec<_> = expected_objects.iter().map(|object|
            serde_json::json!({"revision": "gcp-overlap-winner", "kind": object.kind,
                "object_id": object.id, "tenant": object.tenant, "document": object.document,
                "profile_policy_version": object.profile_policy_version})).collect();
        complete_objects.sort_by(|left, right| (left["kind"].as_str(), left["object_id"].as_str())
            .cmp(&(right["kind"].as_str(), right["object_id"].as_str())));
        assert_eq!(winner_objects, complete_objects, "all supplied winner objects commit");
        let publications: Vec<_> = after["audit"].as_array().unwrap().iter()
            .filter(|row| row["class"] == "hub.control_plane_publication")
            .map(|row| row["draft"]["detail"]["revision"].as_str().unwrap()).collect();
        assert_eq!(publications, ["gcp-overlap-base", "gcp-overlap-winner"]);
        let before_audit_ids: BTreeSet<_> = before["audit"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        let mut successful_batch: Vec<_> = after["audit"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| !before_audit_ids.contains(row["id"].as_str().unwrap()))
            .map(|row| serde_json::json!({
                "class": row["class"], "target": row["target"], "outcome": row["outcome"],
                "actor": row["draft"]["actor"]["principal"], "detail": row["draft"]["detail"],
            }))
            .collect();
        // Independent semantic expectation: the changed retained context, new
        // context and removed context each contribute their owning account event.
        let mut expected_batch = vec![
            serde_json::json!({"class": "hub.control_plane_publication", "target": "veoveo://hub",
                "outcome": "success", "actor": "publication-fixture",
                "detail": {"revision": "gcp-overlap-winner", "digest": format!("sha256:{}", expected_winner.sha256)}}),
            serde_json::json!({"class": "hub.account_change",
                "target": "veoveo://tenants/publication-fixture/work-contexts/retained",
                "outcome": "success", "actor": "publication-fixture", "detail": {"activity": "update"}}),
            serde_json::json!({"class": "hub.account_change",
                "target": "veoveo://tenants/publication-fixture/work-contexts/winner",
                "outcome": "success", "actor": "publication-fixture", "detail": {"activity": "create"}}),
            serde_json::json!({"class": "hub.account_change",
                "target": "veoveo://tenants/publication-fixture/work-contexts/removed",
                "outcome": "success", "actor": "publication-fixture", "detail": {"activity": "delete"}}),
        ];
        successful_batch.sort_by_key(Value::to_string);
        expected_batch.sort_by_key(Value::to_string);
        assert_eq!(successful_batch, expected_batch, "winner commits the exact successful semantic batch");
        assert!(after["objects"].as_array().unwrap().iter()
            .all(|row| row["revision"] != "gcp-overlap-loser"));
        assert!(after["audit"].as_array().unwrap().iter()
            .all(|row| !row["draft"].to_string().contains("loser")),
            "stale publisher must not append its publication or Work Context batch");
        assert_eq!(control_store(db.b.clone()).load_active_revision().await.unwrap(),
            Some(expected_winner), "the complete winner document is loaded after commit");
    }).await.expect("blocked publication overlap exceeded 90 seconds");
}

fn control_store(platform: PlatformStore) -> HubControlStore {
    HubControlStore::from_platform_store(
        platform,
        HubCatalogAdmission::unbound()
            .bind(crate::kernel_catalog_registry().unwrap())
            .unwrap(),
    )
}

fn publication(
    store: &HubControlStore,
    previous: &HubControlPlaneRevision,
    revision: &HubControlPlaneRevision,
) -> (Vec<ControlObject>, AuditAppend) {
    let sections = store.admission.validate(&revision.control_plane).unwrap();
    let objects = control_plane_objects(&revision.control_plane, &sections).unwrap();
    let audit = AuditAppend::new(
        super::audit::changes(
            &crate::audit_registry(),
            &previous.control_plane.work_contexts,
            &revision.control_plane.work_contexts,
            &context(),
            revision,
        )
        .unwrap(),
    );
    (objects, audit)
}

async fn snapshot(store: &PlatformStore) -> Value {
    sqlx::query_file!("src/control_store/tests/snapshot.sql")
        .fetch_one(store.pool())
        .await
        .unwrap()
        .snapshot
}

fn context() -> AuditContext {
    AuditContext {
        actor: AuditActor {
            principal: PrincipalId::parse("publication-fixture").unwrap(),
            kind: AuditPrincipalKind::Service,
            tenant: None,
            work_context: None,
        },
        via: None,
        request: AuditRequest::background(),
    }
}

fn revision(id: &str, digest: char, contexts: &[&str]) -> HubControlPlaneRevision {
    let mut plane = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![],
        authorization_servers: vec![],
        profiles: vec![],
        authorization: Default::default(),
        registration: None,
        extensions: Default::default(),
        tenants: vec![],
        work_contexts: vec![],
        policies: vec![],
        data_labels: vec![],
        oauth_clients: vec![],
        oidc_clients: vec![],
        secrets: vec![],
        metadata: serde_json::json!({}),
    };
    let tenant = TenantId::parse("publication-fixture").unwrap();
    plane.tenants.push(TenantDefinition {
        id: tenant.clone(),
        title: Some("Publication fixture".to_owned()),
        description: None,
        metadata: serde_json::json!({}),
    });
    plane.policies.push(PolicySet {
        version: PolicyVersion::parse("publication-r1").unwrap(),
        rules: vec![],
        metadata: serde_json::Value::Null,
    });
    for key in contexts {
        plane.work_contexts.push(WorkContextDefinition {
            id: WorkContextId::parse(*key).unwrap(),
            tenant: tenant.clone(),
            title: format!("{id}/{key}"),
            policy_revision: PolicyVersion::parse("publication-r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Group(GroupId::parse(format!("{id}-owners")).unwrap()),
                initial_grants: vec![
                    WorkContextGrant {
                        subject: AccessSubject::Group(
                            GroupId::parse(format!("{id}-readers")).unwrap(),
                        ),
                        level: AccessLevel::Read,
                    },
                    WorkContextGrant {
                        subject: AccessSubject::Principal(
                            PrincipalId::parse(format!("{id}-editor")).unwrap(),
                        ),
                        level: AccessLevel::Write,
                    },
                ],
                classification: None,
                data_labels: BTreeSet::new(),
            },
            memberships: vec![
                membership(WorkContextMembershipLevel::Viewer, &format!("{id}-viewer")),
                membership(
                    WorkContextMembershipLevel::Contributor,
                    &format!("{id}-contributor"),
                ),
            ],
        });
    }
    HubControlPlaneRevision {
        revision_id: HubControlPlaneRevisionId::parse(id).unwrap(),
        sha256: digest.to_string().repeat(64),
        source: HubControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now().trunc_subsecs(6),
        applied_by: PrincipalId::parse("publication-fixture").unwrap(),
        tenant: None,
        control_plane: plane,
    }
}

fn membership(level: WorkContextMembershipLevel, principal: &str) -> WorkContextMembershipRule {
    WorkContextMembershipRule {
        level,
        principals: BTreeSet::from([PrincipalId::parse(principal).unwrap()]),
        groups: BTreeSet::new(),
        roles: BTreeSet::new(),
        oauth_clients: BTreeSet::new(),
    }
}
