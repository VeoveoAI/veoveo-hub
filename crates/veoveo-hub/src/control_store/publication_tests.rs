//! Native transaction evidence; no production failure or synchronization hook.
use super::*;
use crate::audit::AuditAppend;
use crate::control_plane::{HubControlPlaneRevisionSource, PolicySet, TenantDefinition};
use crate::store::TransactionAppend;
use chrono::{SubsecRound, Utc};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::{Notify, oneshot};
use veoveo_audit_contract::{AuditActor, AuditContext, AuditPrincipalKind, AuditRequest};
use veoveo_types::{PolicyVersion, PrincipalId, TenantId};

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
        let (objects, audit) = publication(&store, &candidate);
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
        let (objects, audit) = publication(&left, &winner);
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
        // Publication records every tenant a revision defines and removes none: the
        // base's omitted tenant keeps its row, and the stale loser's never appears.
        let tenants: Vec<_> = after["tenants"].as_array().unwrap().iter()
            .map(|row| {
                let slug = row["slug"].as_str().unwrap();
                assert_eq!(row["id"], veoveo_store::deterministic_tenant_id(slug).unwrap().to_string());
                assert_eq!(row["enterprise"], veoveo_store::deterministic_enterprise_id().to_string());
                slug
            })
            .collect::<BTreeSet<_>>().into_iter().collect();
        assert_eq!(tenants, ["removed", "retained", "winner"]);
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
        // Independent semantic expectation: the winner's publication is its whole batch.
        let expected_batch = vec![
            serde_json::json!({"class": "hub.control_plane_publication", "target": "veoveo://hub",
                "outcome": "success", "actor": "publication-fixture",
                "detail": {"revision": "gcp-overlap-winner", "digest": format!("sha256:{}", expected_winner.sha256)}}),
        ];
        successful_batch.sort_by_key(Value::to_string);
        assert_eq!(successful_batch, expected_batch, "winner commits the exact successful semantic batch");
        assert!(after["objects"].as_array().unwrap().iter()
            .all(|row| row["revision"] != "gcp-overlap-loser"));
        assert!(after["audit"].as_array().unwrap().iter()
            .all(|row| !row["draft"].to_string().contains("loser")),
            "stale publisher must not append its publication batch");
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
    revision: &HubControlPlaneRevision,
) -> (Vec<ControlObject>, AuditAppend) {
    let sections = store.admission.validate(&revision.control_plane).unwrap();
    let objects = control_plane_objects(&revision.control_plane, &sections).unwrap();
    let audit = AuditAppend::new(
        super::audit::changes(&crate::audit_registry(), &context(), revision).unwrap(),
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
        },
        via: None,
        request: AuditRequest::background(),
    }
}

/// A revision that defines one tenant for each of `tenants`.
fn revision(id: &str, digest: char, tenants: &[&str]) -> HubControlPlaneRevision {
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
        policies: vec![],
        data_labels: vec![],
        oauth_clients: vec![],
        oidc_clients: vec![],
        secrets: vec![],
        metadata: serde_json::json!({}),
    };
    for key in tenants {
        plane.tenants.push(TenantDefinition {
            id: TenantId::parse(*key).unwrap(),
            title: Some(format!("{id}/{key}")),
            description: None,
            metadata: serde_json::json!({}),
        });
    }
    plane.policies.push(PolicySet {
        version: PolicyVersion::parse("publication-r1").unwrap(),
        rules: vec![],
        metadata: serde_json::Value::Null,
    });
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
