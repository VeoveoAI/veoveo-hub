//! Actual ordinary-store overlap; the only injected behavior is an INSERT gate in
//! this disposable database. No service preflight, audit or reload is simulated.
use super::*;
use chrono::{SubsecRound, TimeDelta};
use serde_json::{Value, json};
use std::time::Duration;
use veoveo_hub_sdk::registration::{
    AuthorizationDeclaration, RegistrationDeclaration, Requirements, ServerEndpoint, ServerVersion,
    statement::ServerKeyPair,
};
use veoveo_store::ServerSchema;
use veoveo_types::ResourceScheme;

struct Candidate {
    client: OAuthClientId,
    metadata: ClientMetadata,
    statement: SoftwareStatement,
    digest: Sha256Digest,
    login: LoginName,
    password: SecretString,
    now: DateTime<Utc>,
}

impl Candidate {
    fn new(
        key: &ServerKeyPair,
        deployment: DeploymentId,
        suffix: &str,
        revision: u8,
        now: DateTime<Utc>,
    ) -> Self {
        let slug = ServerSlug::parse("registration-probe").unwrap();
        let metadata = ClientMetadata::new(
            *key.public_key(),
            RegistrationDeclaration {
                deployment,
                slug: slug.clone(),
                uri_scheme: ResourceScheme::parse("registration-probe").unwrap(),
                version: ServerVersion::new(format!("0.{revision}.0")).unwrap(),
                endpoint: ServerEndpoint::new(
                    format!("http://127.0.0.1:{}/registration-probe/mcp", 9 + revision)
                        .parse()
                        .unwrap(),
                )
                .unwrap(),
                surfaces: None,
            },
            AuthorizationDeclaration {
                permissions: vec![],
                requirements: Requirements::default(),
                uses: vec![],
                calls_hub: false,
            },
        )
        .unwrap();
        let statement = key
            .sign_statement(
                &metadata,
                &"https://registration-probe.example/register"
                    .parse()
                    .unwrap(),
                now,
            )
            .unwrap();
        Self {
            client: OAuthClientId::parse(format!("registration-probe-{suffix}")).unwrap(),
            metadata,
            statement,
            digest: crate::registration::digest(&format!("fixture-access-{suffix}")),
            login: ServerSchema::new(&slug).unwrap().login(suffix).unwrap(),
            password: SecretString::from(format!("fixture-password-{suffix}")),
            now,
        }
    }

    async fn register(&self, store: &RegistrationStore) -> Result<(Pin, Vec<EndedRegistration>)> {
        store
            .register(NewRegistration {
                client_id: &self.client,
                metadata: &self.metadata,
                software_statement: &self.statement,
                token_digest: &self.digest,
                login: &self.login,
                password: &self.password,
                now: self.now,
            })
            .await
    }
}

/// Hash signed fixture statements before any snapshot can enter panic/log output.
async fn snapshot(store: &PlatformStore, deployment: DeploymentId, logins: &[String]) -> Value {
    let mut value = sqlx::query_file!(
        "src/registration/store/tests/snapshot.sql",
        deployment.as_uuid(),
        "registration-probe",
        logins,
    )
    .fetch_one(store.pool())
    .await
    .unwrap()
    .snapshot;
    for row in value["registrations"].as_array_mut().unwrap() {
        let statement = row
            .as_object_mut()
            .unwrap()
            .remove("software_statement")
            .unwrap();
        row["software_statement_digest"] =
            json!(crate::registration::digest(statement.as_str().unwrap()).as_str());
    }
    value
}

fn timestamp(value: &Value) -> DateTime<Utc> {
    value.as_str().unwrap().parse().unwrap()
}

fn assert_candidate(row: &Value, candidate: &Candidate, deployment: DeploymentId) {
    assert_eq!(row["client_id"], json!(candidate.client.as_str()));
    assert_eq!(row["deployment"], json!(deployment));
    assert_eq!(row["server"], "registration-probe");
    assert_eq!(row["metadata"], json!(candidate.metadata));
    assert_eq!(row["access_token_digest"], json!(candidate.digest.as_str()));
    assert_eq!(row["login"], json!(candidate.login.as_str()));
    assert_eq!(
        row["software_statement_digest"],
        json!(crate::registration::digest(candidate.statement.as_str()).as_str())
    );
    assert_eq!(timestamp(&row["issued_at"]), candidate.now);
    assert_eq!(timestamp(&row["updated_at"]), candidate.now);
    assert!(row["orphaned_at"].is_null());
}

#[tokio::test]
async fn ordinary_replica_and_replacement_have_a_serial_registration_outcome() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let store = RegistrationStore::new(db.a.clone());
        let second_store = RegistrationStore::new(db.b.clone());
        let key = ServerKeyPair::generate().unwrap();
        let deployment = DeploymentId::new();
        let now = Utc::now().trunc_subsecs(6);
        let base = Candidate::new(&key, deployment, "00000000000001", 1, now);
        let replica = std::sync::Arc::new(Candidate::new(
            &key,
            deployment,
            "00000000000002",
            1,
            now + TimeDelta::seconds(1),
        ));
        let replacement = std::sync::Arc::new(Candidate::new(
            &key,
            deployment,
            "00000000000003",
            2,
            now + TimeDelta::seconds(2),
        ));
        assert_eq!(base.metadata, replica.metadata);
        assert_ne!(base.metadata, replacement.metadata);
        let (pin, ended) = base.register(&store).await.unwrap();
        assert_eq!(pin, Pin::Claimed);
        assert!(ended.is_empty());
        let logins =
            [&base, &replica, &replacement].map(|candidate| candidate.login.as_str().to_owned());
        let initial = snapshot(&db.a, deployment, &logins).await;
        assert_candidate(&initial["registrations"][0], &base, deployment);

        sqlx::query_file!("src/registration/store/tests/gate.sql")
            .execute(db.a.pool())
            .await
            .unwrap();
        let mut gate = db.a.pool().begin().await.unwrap();
        let connection = sqlx::query_file!("src/registration/store/tests/connection.sql")
            .fetch_one(&mut *gate)
            .await
            .unwrap();
        assert_eq!(connection.isolation, "read committed");
        sqlx::query_file!("src/registration/store/tests/hold_gate.sql")
            .execute(&mut *gate)
            .await
            .unwrap();
        let replica_task = {
            let candidate = replica.clone();
            tokio::spawn(async move { candidate.register(&store).await })
        };
        // Establish that the actual INSERT has reached the fixture barrier, not
        // just that a task was spawned. All earlier production locks remain held.
        let replica_pid = loop {
            let waiters =
                sqlx::query_file!("src/registration/store/tests/blocked.sql", connection.pid)
                    .fetch_all(db.a.pool())
                    .await
                    .unwrap();
            if let Some(waiter) = waiters
                .into_iter()
                .find(|w| w.query.contains("INSERT INTO hub.registration"))
            {
                eprintln!(
                    "phase=replica-at-insert pid={} event={} statement={}",
                    waiter.pid, waiter.event, waiter.query
                );
                break waiter.pid;
            }
            assert!(
                !replica_task.is_finished(),
                "replica ended before INSERT gate"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let replacement_task = {
            let candidate = replacement.clone();
            tokio::spawn(async move { candidate.register(&second_store).await })
        };
        loop {
            let waiters =
                sqlx::query_file!("src/registration/store/tests/blocked.sql", replica_pid)
                    .fetch_all(db.a.pool())
                    .await
                    .unwrap();
            if let Some(waiter) = waiters.into_iter().next() {
                eprintln!(
                    "phase=replacement-blocked pid={} owner={} event={} statement={}",
                    waiter.pid, replica_pid, waiter.event, waiter.query
                );
                break;
            }
            if replacement_task.is_finished() {
                eprintln!("phase=replacement-completed-before-release");
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let before_release = snapshot(&db.a, deployment, &logins).await;
        eprintln!("phase=before-release snapshot={before_release}");
        gate.commit().await.unwrap();
        let (replica_pin, replica_ended) = replica_task.await.unwrap().unwrap();
        let (replacement_pin, replacement_ended) = replacement_task.await.unwrap().unwrap();
        assert_eq!(replica_pin, Pin::Held);
        assert_eq!(replacement_pin, Pin::Held);
        let final_state = snapshot(&db.a, deployment, &logins).await;
        eprintln!("phase=finished snapshot={final_state}");
        let rows = final_state["registrations"].as_array().unwrap();
        // A then B leaves only B; B then A leaves only A (A now replaces B).
        // A mixed old/new result matches neither ordinary-register serial order.
        assert_eq!(
            rows.len(),
            1,
            "nonserial registration outcome: {final_state}"
        );
        let winner = if rows[0]["client_id"] == json!(replacement.client.as_str()) {
            assert!(replica_ended.is_empty());
            let ended: std::collections::BTreeSet<_> = replacement_ended
                .iter()
                .map(|r| (r.client_id.as_str(), r.server.as_str(), r.login.as_str()))
                .collect();
            assert_eq!(
                ended,
                std::collections::BTreeSet::from([
                    (
                        base.client.as_str(),
                        "registration-probe",
                        base.login.as_str()
                    ),
                    (
                        replica.client.as_str(),
                        "registration-probe",
                        replica.login.as_str()
                    ),
                ])
            );
            &*replacement
        } else {
            assert_eq!(rows[0]["client_id"], json!(replica.client.as_str()));
            assert_eq!(replacement_ended.len(), 1);
            assert_eq!(replacement_ended[0].client_id, base.client);
            assert_eq!(replica_ended.len(), 1);
            assert_eq!(replica_ended[0].client_id, replacement.client);
            &*replica
        };
        assert_candidate(&rows[0], winner, deployment);
        let mut expected_deployment = initial["deployment"].clone();
        expected_deployment["metadata"] = json!(winner.metadata);
        assert_eq!(final_state["deployment"], expected_deployment);
        assert_eq!(final_state["claim"], initial["claim"]);
        assert_eq!(
            final_state["logins"],
            json!([{
                "name": winner.login.as_str(), "login": true, "superuser": false,
                "create_db": false, "create_role": false, "replication": false,
                "bypass_rls": false, "connection_limit": -1, "valid_until": null,
                "memberships": ["server_registration_probe"]
            }])
        );
    })
    .await
    .expect("bounded ordinary registration overlap");
}

/// A deterministic store-level schedule corresponding to service.authenticate's
/// read/digest preflight preceding its later store.update call. This does not run
/// the HTTP endpoint, bearer parser, service audit or postcommit reload.
#[tokio::test]
async fn stale_update_cannot_overwrite_a_replaced_registration_deployment() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let store = RegistrationStore::new(db.a.clone());
        let replacer = RegistrationStore::new(db.b.clone());
        let key = ServerKeyPair::generate().unwrap();
        let deployment = DeploymentId::new();
        let now = Utc::now().trunc_subsecs(6);
        let base = Candidate::new(&key, deployment, "00000000000011", 1, now);
        let replacement = Candidate::new(
            &key,
            deployment,
            "00000000000013",
            2,
            now + TimeDelta::seconds(1),
        );
        let stale = Candidate::new(
            &key,
            deployment,
            "00000000000011",
            3,
            now + TimeDelta::seconds(2),
        );
        let (pin, ended) = base.register(&store).await.unwrap();
        assert_eq!(pin, Pin::Claimed);
        assert!(ended.is_empty());

        // Capture the actual row while it still exists. The digest comparison
        // maps exactly to authenticate's store read and token-digest condition;
        // it is not a claim that the service authentication function ran.
        let captured = store.registration(&base.client).await.unwrap().unwrap();
        assert_eq!(captured.client_id, base.client);
        assert_eq!(captured.token_digest, base.digest);
        assert_eq!(captured.metadata, base.metadata);
        assert_eq!(captured.issued_at, base.now);
        assert_ne!(stale.metadata, captured.metadata);
        assert_eq!(stale.client, captured.client_id);
        assert_eq!(
            stale.metadata.registration.deployment,
            captured.metadata.registration.deployment
        );
        assert_eq!(stale.metadata.slug(), captured.metadata.slug());
        assert_eq!(stale.metadata.key(), captured.metadata.key());
        stale.metadata.check().unwrap();
        let mut audience: url::Url = "https://registration-probe.example/oauth/register"
            .parse()
            .unwrap();
        audience
            .path_segments_mut()
            .unwrap()
            .push(captured.client_id.as_str());
        let statement = key
            .sign_statement(&stale.metadata, &audience, stale.now)
            .unwrap();
        veoveo_hub_sdk::registration::statement::verify_statement(
            &statement,
            &stale.metadata,
            Some(captured.metadata.key()),
            &audience,
            stale.now,
        )
        .unwrap();
        eprintln!(
            "phase=preflight-captured client={} version=0.1.0",
            captured.client_id
        );

        let (pin, ended) = replacement.register(&replacer).await.unwrap();
        assert_eq!(pin, Pin::Held);
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].client_id, base.client);
        assert_eq!(ended[0].server.as_str(), "registration-probe");
        assert_eq!(ended[0].login, base.login);
        assert!(
            store
                .registration(&captured.client_id)
                .await
                .unwrap()
                .is_none()
        );
        let logins = [&base, &replacement].map(|candidate| candidate.login.as_str().to_owned());
        let before = snapshot(&db.a, deployment, &logins).await;
        assert_eq!(before["registrations"].as_array().unwrap().len(), 1);
        assert_candidate(&before["registrations"][0], &replacement, deployment);
        assert_eq!(
            before["deployment"]["metadata"],
            json!(replacement.metadata)
        );
        assert_eq!(
            before["logins"],
            json!([{
                "name": replacement.login.as_str(), "login": true, "superuser": false,
                "create_db": false, "create_role": false, "replication": false,
                "bypass_rls": false, "connection_limit": -1, "valid_until": null,
                "memberships": ["server_registration_probe"]
            }])
        );
        eprintln!("phase=replacement-committed snapshot={before}");

        let update = store
            .update(&captured.client_id, &stale.metadata, &statement, stale.now)
            .await;
        // The invariant is independent of success/error policy: record the
        // result without requiring a new rejection contract for an in-flight
        // operation that authenticated before replacement.
        let outcome = if update.is_ok() { "ok" } else { "error" };
        let after = snapshot(&db.a, deployment, &logins).await;
        eprintln!("phase=stale-update result={outcome} snapshot={after}");
        assert_eq!(
            after, before,
            "a stale update must not overwrite the replacement's committed state"
        );
    })
    .await
    .expect("bounded stale registration update schedule");
}
