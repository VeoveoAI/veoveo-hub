//! Registration-store overlaps and service/audit probes in disposable databases.
//! Store probes use test-only gates; service probes exercise real authority,
//! signatures, typed responses, audit and explicit persisted-state catalog reloads.
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
    snapshot_for_server(store, deployment, "registration-probe", logins).await
}

async fn snapshot_for_server(
    store: &PlatformStore,
    deployment: DeploymentId,
    server: &str,
    logins: &[String],
) -> Value {
    let mut value = sqlx::query_file!(
        "src/registration/store/tests/snapshot.sql",
        deployment.as_uuid(),
        server,
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

/// A joined instance target changes while its process registration stays exactly the
/// same. Only the real kernel notification and watcher may publish the new route.
#[tokio::test]
async fn a_retargeted_instance_publishes_its_complete_live_registration() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let directory = tempfile::tempdir().unwrap();
        let installation = crate::local::generate(directory.path(), "https://hub.test").unwrap();
        let catalog = crate::HubCatalogHandle::new(std::sync::Arc::new(
            crate::HubCatalog::load_json(&installation.config, crate::catalog_fixture::binding())
                .unwrap(),
        ));
        let signing = ServerKeyPair::generate().unwrap();
        let service = crate::registration::RegistrationService::new(
            db.a.clone(),
            catalog.clone(),
            crate::registration::RegistrationIdentity {
                issuer: veoveo_hub_sdk::TokenIssuer::parse("https://hub.test").unwrap(),
                signing_key: veoveo_hub_sdk::HubInternalSigningKey::new(signing.pkcs8_der())
                    .unwrap(),
                verification_key: None,
            },
            crate::registration::ServerDatabase {
                url: veoveo_hub_sdk::registration::DatabaseUrl::new(
                    "postgres://127.0.0.1/unused".parse().unwrap(),
                )
                .unwrap(),
            },
            Duration::from_secs(60),
            None,
        )
        .unwrap();
        let original_link = crate::factory::Link {
            name: veoveo_hub_sdk::hub::LinkName::parse("team-a").unwrap(),
            repository: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            tracked: crate::factory::TrackedBranch {
                id: "11111111111111111111111111111111".into(),
                name: "first".into(),
            },
        };
        let instance = crate::registration::create_runner(
            &db.a,
            "0123456789abcdef0123456789abcdef",
            &DatabaseName::installation(&db.a).unwrap(),
            &BranchName::parse("retarget-publication").unwrap(),
            &original_link,
        )
        .await
        .unwrap();
        let store = RegistrationStore::new(db.a.clone());
        let candidate = Candidate::new(
            &signing,
            instance.deployment,
            "00000000000021",
            1,
            Utc::now().trunc_subsecs(6),
        );
        candidate.register(&store).await.unwrap();
        let stop = tokio_util::sync::CancellationToken::new();
        let _cancel_on_drop = stop.clone().drop_guard();
        let watcher = service.spawn_watcher(stop.clone());
        let mut changes = catalog.subscribe();
        let before = loop {
            let snapshot = catalog.snapshot();
            if let Some(server) = snapshot
                .catalog()
                .registrations()
                .deployment(&instance.deployment)
            {
                break (snapshot.generation(), server.clone());
            }
            changes.changed().await.unwrap();
        };
        assert_eq!(
            before.1.runner.as_ref().unwrap().repository.as_str(),
            original_link.repository
        );

        // Use the production target statement, with no register/read/refresh call.
        // A fresh process registration must not be necessary to observe the target.
        let target = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        sqlx::query_file!(
            "src/factory/record/linked.sql",
            instance.deployment.as_uuid(),
            target,
            "22222222222222222222222222222222",
            "release",
            "https://identity.example#new-pusher",
        )
        .execute(db.a.pool())
        .await
        .unwrap();
        let after = loop {
            let snapshot = catalog.snapshot();
            let server = snapshot
                .catalog()
                .registrations()
                .deployment(&instance.deployment)
                .cloned();
            if let Some(server) = server
                && server
                    .runner
                    .as_ref()
                    .is_some_and(|runner| runner.repository.as_str() == target)
            {
                break (snapshot.generation(), server);
            }
            changes.changed().await.unwrap();
        };
        let row = store
            .registration(&candidate.client)
            .await
            .unwrap()
            .unwrap();
        stop.cancel();
        watcher.await.unwrap();

        let mut expected = before.1;
        expected.runner.as_mut().unwrap().repository =
            veoveo_hub_sdk::registration::RepositoryId::parse(target).unwrap();
        assert!(after.0 > before.0, "the live generation must advance");
        assert_eq!(
            after.1, expected,
            "all other registered fields must be preserved"
        );
        assert_eq!(after.1.client_id, candidate.client);
        assert_eq!(after.1.deployment, instance.deployment);
        assert_eq!(row.metadata, candidate.metadata);
        assert_eq!(row.issued_at, candidate.now);
    })
    .await
    .expect("bounded joined-instance routing publication");
}

#[tokio::test]
async fn an_instance_server_coming_up_starts_a_new_generation() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let directory = tempfile::tempdir().unwrap();
        let installation = crate::local::generate(directory.path(), "https://hub.test").unwrap();
        let catalog = crate::HubCatalogHandle::new(std::sync::Arc::new(
            crate::HubCatalog::load_json(&installation.config, crate::catalog_fixture::binding())
                .unwrap(),
        ));
        let signing = ServerKeyPair::generate().unwrap();
        let service = crate::registration::RegistrationService::new(
            db.a.clone(),
            catalog.clone(),
            crate::registration::RegistrationIdentity {
                issuer: veoveo_hub_sdk::TokenIssuer::parse("https://hub.test").unwrap(),
                signing_key: veoveo_hub_sdk::HubInternalSigningKey::new(signing.pkcs8_der())
                    .unwrap(),
                verification_key: None,
            },
            crate::registration::ServerDatabase {
                url: veoveo_hub_sdk::registration::DatabaseUrl::new(
                    "postgres://127.0.0.1/unused".parse().unwrap(),
                )
                .unwrap(),
            },
            Duration::from_secs(60),
            None,
        )
        .unwrap();
        let link = crate::factory::Link {
            name: veoveo_hub_sdk::hub::LinkName::parse("team-a").unwrap(),
            repository: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            tracked: crate::factory::TrackedBranch {
                id: "11111111111111111111111111111111".into(),
                name: "first".into(),
            },
        };
        let instance = crate::registration::create_runner(
            &db.a,
            "0123456789abcdef0123456789abcdef",
            &DatabaseName::installation(&db.a).unwrap(),
            &BranchName::parse("served-publication").unwrap(),
            &link,
        )
        .await
        .unwrap();
        let store = RegistrationStore::new(db.a.clone());
        Candidate::new(
            &signing,
            instance.deployment,
            "00000000000022",
            1,
            Utc::now().trunc_subsecs(6),
        )
        .register(&store)
        .await
        .unwrap();
        let stop = tokio_util::sync::CancellationToken::new();
        let _cancel_on_drop = stop.clone().drop_guard();
        let watcher = service.spawn_watcher(stop.clone());
        let mut changes = catalog.subscribe();
        // The generation at which the instance is routed and `done` holds.
        macro_rules! until {
            ($done:expr) => {
                loop {
                    let snapshot = catalog.snapshot();
                    let server = snapshot
                        .catalog()
                        .registrations()
                        .deployment(&instance.deployment)
                        .cloned();
                    if let Some(server) = server
                        && $done(&server)
                    {
                        break (snapshot.generation(), server.served_at);
                    }
                    changes.changed().await.unwrap();
                }
            };
        }
        let before = until!(|_: &crate::registration::RegisteredServer| true);
        assert_eq!(
            before.1, None,
            "registering in the store alone serves nothing"
        );

        // A resume, with no registration changing: the declaration is the same.
        store.served(instance.deployment).await.unwrap();
        let first =
            until!(|server: &crate::registration::RegisteredServer| { server.served_at.is_some() });
        assert!(first.0 > before.0, "coming up must start a new generation");

        // A restart and another resume.
        sqlx::query_file!(
            "src/factory/record/unserved.sql",
            instance.deployment.as_uuid()
        )
        .execute(db.a.pool())
        .await
        .unwrap();
        store.served(instance.deployment).await.unwrap();
        let second = until!(|server: &crate::registration::RegisteredServer| {
            server.served_at.is_some() && server.served_at != first.1
        });
        assert!(second.0 > first.0, "coming up again must start another");
        stop.cancel();
        watcher.await.unwrap();
    })
    .await
    .expect("bounded served publication");
}

/// Only public identifiers/metadata and a statement digest enter diagnostics; a
/// successful service response also carries secrets which must never be printed.
fn registration_outcome(
    result: &std::result::Result<
        veoveo_hub_sdk::registration::ClientInformation,
        crate::registration::RegistrationError,
    >,
) -> Value {
    match result {
        Ok(information) => json!({
            "kind": "accepted",
            "client": information.client_id.as_str(),
            "deployment": information.metadata.registration.deployment,
            "metadata": information.metadata,
            "issued_at": information.client_id_issued_at,
            "login": information.database.as_ref().map(|grant| grant.username.as_str()),
            "statement_digest": crate::registration::digest(information.software_statement.as_str()).as_str(),
        }),
        Err(crate::registration::RegistrationError::Unauthorized { token_sent }) => {
            json!({"kind": "unauthorized", "token_sent": token_sent})
        }
        Err(crate::registration::RegistrationError::Rejected { code, .. }) => {
            json!({"kind": "rejected", "code": code})
        }
        Err(crate::registration::RegistrationError::Internal(_)) => {
            json!({"kind": "internal_error"})
        }
    }
}

async fn member_logins(store: &PlatformStore, group: &str) -> Vec<String> {
    sqlx::query_file_scalar!("src/registration/store/tests/member_logins.sql", group)
        .fetch_all(store.pool())
        .await
        .unwrap()
}

/// A first registration has already passed real service authority/signature
/// checks and reached INSERT when removal starts. This admission-first case must
/// accept the registration, then remove its row/login without leaving an orphan.
/// The final explicit refresh checks persisted catalog contents;
/// this test makes no notification-delivery or native sandbox-deletion claim.
#[tokio::test]
async fn an_authorized_first_registration_cannot_survive_instance_removal() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let store = RegistrationStore::new(db.a.clone());
        let directory = tempfile::tempdir().unwrap();
        let installation = crate::local::generate(directory.path(), "https://hub.test").unwrap();
        let catalog = crate::HubCatalogHandle::new(std::sync::Arc::new(
            crate::HubCatalog::load_json(&installation.config, crate::catalog_fixture::binding())
                .unwrap(),
        ));
        let signing = ServerKeyPair::generate().unwrap();
        let service = crate::registration::RegistrationService::new(
            db.a.clone(), catalog.clone(),
            crate::registration::RegistrationIdentity {
                issuer: veoveo_hub_sdk::TokenIssuer::parse("https://hub.test").unwrap(),
                signing_key: veoveo_hub_sdk::HubInternalSigningKey::new(signing.pkcs8_der()).unwrap(),
                verification_key: None,
            },
            crate::registration::ServerDatabase {
                url: veoveo_hub_sdk::registration::DatabaseUrl::new(
                    "postgres://127.0.0.1/unused".parse().unwrap(),
                ).unwrap(),
            }, Duration::from_secs(60), None,
        ).unwrap();
        let database = DatabaseName::installation(&db.a).unwrap();
        let instance = crate::registration::create_instance(
            &db.a, "0123456789abcdef0123456789abcdef", &database,
            &BranchName::parse("registration-removal").unwrap(),
        ).await.unwrap();
        let deployment = instance.deployment;
        let token_digest = crate::registration::digest(instance.registration_token.expose_secret());
        assert_eq!(store.instance(&token_digest).await.unwrap(), Some((deployment, database.clone())));
        assert!(store.registrations().await.unwrap().is_empty());

        // TestDb databases share a process-local cluster and therefore roles. A
        // deployment-derived fixture slug isolates this test's role enumeration;
        // its initially empty membership is verified rather than presumed.
        let slug = ServerSlug::parse(format!("removal-{}", deployment.as_uuid().simple())).unwrap();
        let group = ServerSchema::new(&slug).unwrap().group_role();
        let before_logins = member_logins(&db.a, &group).await;
        assert!(before_logins.is_empty(), "fixture server already has login roles");
        let candidate = Candidate::new(&signing, deployment, "00000000000031", 1, Utc::now());
        let mut metadata = candidate.metadata;
        metadata.registration.slug = slug.clone();
        metadata.registration.uri_scheme = ResourceScheme::parse(slug.as_str()).unwrap();
        metadata.check().unwrap();
        let request = veoveo_hub_sdk::registration::ClientRegistrationRequest {
            software_statement: signing.sign_statement(&metadata, service.registration_endpoint(), Utc::now()).unwrap(),
            metadata,
        };
        let body = serde_json::to_vec(&request).unwrap();
        sqlx::query_file!("src/registration/store/tests/instance_insert_gate.sql")
            .execute(db.a.pool()).await.unwrap();
        let mut gate = db.a.pool().begin().await.unwrap();
        let connection = sqlx::query_file!("src/registration/store/tests/connection.sql")
            .fetch_one(&mut *gate).await.unwrap();
        assert_eq!(connection.isolation, "read committed");
        sqlx::query_file!("src/registration/store/tests/hold_gate.sql")
            .execute(&mut *gate).await.unwrap();
        let registering = {
            let service = service.clone();
            tokio::spawn(async move {
                service.register(
                    super::super::service::Bearer::Presented(instance.registration_token.expose_secret()),
                    &body,
                ).await
            })
        };
        let registration_pid = loop {
            let waiters = sqlx::query_file!("src/registration/store/tests/blocked.sql", connection.pid)
                .fetch_all(db.a.pool()).await.unwrap();
            if let Some(waiter) = waiters.into_iter().find(|waiter| waiter.query.contains("INSERT INTO hub.registration")) {
                eprintln!("phase=authorized-first-registration-at-insert pid={} event={} statement={}", waiter.pid, waiter.event, waiter.query);
                break waiter.pid;
            }
            if registering.is_finished() {
                let result = registering.await.unwrap();
                panic!("registration ended before the actual INSERT gate: {}", registration_outcome(&result));
            }
            tokio::task::yield_now().await;
        };
        let mut removing = {
            let store = db.b.clone();
            tokio::spawn(async move { crate::registration::remove_instance(&store, deployment).await })
        };
        // A corrected shared lock may legitimately make removal wait. Observe
        // completion OR the real dependency on the registration backend; no time
        // delay stands in for either ordering fact.
        let completed_before_release = tokio::select! {
            result = &mut removing => {
                let removed = result.unwrap().unwrap();
                eprintln!("phase=removal-completed-before-release database={removed:?}");
                Some(removed)
            },
            waiter = async {
                loop {
                    let waiters = sqlx::query_file!("src/registration/store/tests/blocked.sql", registration_pid)
                        .fetch_all(db.a.pool()).await.unwrap();
                    if let Some(waiter) = waiters.into_iter().next() { break waiter; }
                    tokio::task::yield_now().await;
                }
            } => {
                eprintln!("phase=removal-blocked-before-release pid={} owner={} event={} statement={}", waiter.pid, registration_pid, waiter.event, waiter.query);
                None
            }
        };
        let before_release = (
            store.is_instance(deployment).await.unwrap(),
            store.instance(&token_digest).await.unwrap().is_some(),
            snapshot_for_server(&db.a, deployment, slug.as_str(), &before_logins).await,
        );
        eprintln!("phase=before-release removal_completed={} state={before_release:?}", completed_before_release.is_some());
        gate.commit().await.unwrap();
        let registration = registering.await.unwrap();
        let removed = match completed_before_release {
            Some(removed) => removed,
            None => removing.await.unwrap().unwrap(),
        };
        let outcome = registration_outcome(&registration);
        let accepted = registration.is_ok();
        let logins = member_logins(&db.a, &group).await;
        let final_snapshot = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        let present = store.is_instance(deployment).await.unwrap();
        let authority = store.instance(&token_digest).await.unwrap().is_some();
        // Explicit persisted-state observation, not a claim that the asynchronous
        // registration listener has drained or published in a particular order.
        service.refresh().await.unwrap();
        let routed = catalog.snapshot().catalog().registrations().deployment(&deployment)
            .map(|server| (server.client_id.as_str().to_owned(), server.deployment));
        eprintln!("phase=completed registration={outcome} removed={removed:?} instance={present} authority={authority} snapshot={final_snapshot} routed={routed:?}");
        assert_eq!(
            (removed, accepted, present, authority, final_snapshot["registrations"].clone(), logins, routed),
            (Some(database), true, false, false, json!([]), before_logins, None),
            "completed registration/removal left an orphan; service outcome={outcome}, snapshot={final_snapshot}"
        );
    }).await.expect("bounded first-instance-registration/removal overlap");
}

/// Removal owns the live instance row while preflight can still read its old
/// committed authority. The already-authorized service call must wait for that
/// removal, then return its existing invalid-token result with no admission.
#[tokio::test]
async fn an_instance_registration_waiting_for_removal_uses_the_invalid_token_outcome() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let store = RegistrationStore::new(db.a.clone());
        let directory = tempfile::tempdir().unwrap();
        let installation = crate::local::generate(directory.path(), "https://hub.test").unwrap();
        let catalog = crate::HubCatalogHandle::new(std::sync::Arc::new(
            crate::HubCatalog::load_json(&installation.config, crate::catalog_fixture::binding())
                .unwrap(),
        ));
        let signing = ServerKeyPair::generate().unwrap();
        let service = crate::registration::RegistrationService::new(
            db.a.clone(), catalog.clone(),
            crate::registration::RegistrationIdentity {
                issuer: veoveo_hub_sdk::TokenIssuer::parse("https://hub.test").unwrap(),
                signing_key: veoveo_hub_sdk::HubInternalSigningKey::new(signing.pkcs8_der()).unwrap(),
                verification_key: None,
            },
            crate::registration::ServerDatabase {
                url: veoveo_hub_sdk::registration::DatabaseUrl::new(
                    "postgres://127.0.0.1/unused".parse().unwrap(),
                ).unwrap(),
            }, Duration::from_secs(60), None,
        ).unwrap();
        let database = DatabaseName::installation(&db.a).unwrap();
        let instance = crate::registration::create_instance(
            &db.a, "0123456789abcdef0123456789abcdef", &database,
            &BranchName::parse("registration-removal-reverse").unwrap(),
        ).await.unwrap();
        let deployment = instance.deployment;
        let token_digest = crate::registration::digest(instance.registration_token.expose_secret());
        assert_eq!(store.instance(&token_digest).await.unwrap(), Some((deployment, database.clone())));
        assert!(store.registrations().await.unwrap().is_empty());

        // TestDb databases share a process-local cluster and therefore roles. A
        // deployment-derived fixture slug isolates this test's role enumeration;
        // its initially empty membership is verified rather than presumed.
        let slug = ServerSlug::parse(format!("removal-{}", deployment.as_uuid().simple())).unwrap();
        let group = ServerSchema::new(&slug).unwrap().group_role();
        let before_logins = member_logins(&db.a, &group).await;
        assert!(before_logins.is_empty(), "fixture server already has login roles");
        let candidate = Candidate::new(&signing, deployment, "00000000000032", 1, Utc::now());
        let mut metadata = candidate.metadata;
        metadata.registration.slug = slug.clone();
        metadata.registration.uri_scheme = ResourceScheme::parse(slug.as_str()).unwrap();
        metadata.check().unwrap();
        let request = veoveo_hub_sdk::registration::ClientRegistrationRequest {
            software_statement: signing.sign_statement(&metadata, service.registration_endpoint(), Utc::now()).unwrap(),
            metadata,
        };
        let body = serde_json::to_vec(&request).unwrap();
        sqlx::query_file!("src/registration/store/tests/instance_delete_gate.sql")
            .execute(db.a.pool()).await.unwrap();
        let mut gate = db.a.pool().begin().await.unwrap();
        let connection = sqlx::query_file!("src/registration/store/tests/connection.sql")
            .fetch_one(&mut *gate).await.unwrap();
        assert_eq!(connection.isolation, "read committed");
        sqlx::query_file!("src/registration/store/tests/hold_gate.sql")
            .execute(&mut *gate).await.unwrap();
        let removing = {
            let store = db.b.clone();
            tokio::spawn(async move { crate::registration::remove_instance(&store, deployment).await })
        };
        let removal_pid = loop {
            let waiters = sqlx::query_file!("src/registration/store/tests/blocked.sql", connection.pid)
                .fetch_all(db.a.pool()).await.unwrap();
            if let Some(waiter) = waiters.into_iter().find(|waiter| waiter.query.contains("DELETE FROM hub.instance")) {
                eprintln!("phase=removal-owns-instance-at-delete pid={} event={} statement={}", waiter.pid, waiter.event, waiter.query);
                break waiter.pid;
            }
            if removing.is_finished() {
                panic!("removal ended before the actual DELETE gate");
            }
            tokio::task::yield_now().await;
        };
        // A plain authority SELECT remains readable before the gated DELETE.
        assert_eq!(store.instance(&token_digest).await.unwrap(), Some((deployment, database.clone())));
        let registering = {
            let service = service.clone();
            tokio::spawn(async move {
                service.register(
                    super::super::service::Bearer::Presented(instance.registration_token.expose_secret()),
                    &body,
                ).await
            })
        };
        loop {
            let waiters = sqlx::query_file!("src/registration/store/tests/blocked.sql", removal_pid)
                .fetch_all(db.a.pool()).await.unwrap();
            if let Some(waiter) = waiters.into_iter().find(|waiter|
                waiter.query.contains("FROM hub.instance") && waiter.query.contains("FOR UPDATE")) {
                eprintln!("phase=authorized-registration-waits-for-removal pid={} owner={} event={} statement={}", waiter.pid, removal_pid, waiter.event, waiter.query);
                break;
            }
            if registering.is_finished() {
                let result = registering.await.unwrap();
                panic!("registration ended before waiting on removal's instance row: {}", registration_outcome(&result));
            }
            tokio::task::yield_now().await;
        }
        let before_release = (
            store.is_instance(deployment).await.unwrap(),
            store.instance(&token_digest).await.unwrap().is_some(),
            snapshot_for_server(&db.a, deployment, slug.as_str(), &before_logins).await,
        );
        eprintln!("phase=reverse-before-release state={before_release:?}");
        gate.commit().await.unwrap();
        let removed = removing.await.unwrap().unwrap();
        let registration = registering.await.unwrap();
        let outcome = registration_outcome(&registration);
        let refused = matches!(&registration,
            Err(crate::registration::RegistrationError::Unauthorized { token_sent: true }));
        let logins = member_logins(&db.a, &group).await;
        let final_snapshot = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        let present = store.is_instance(deployment).await.unwrap();
        let authority = store.instance(&token_digest).await.unwrap().is_some();
        service.refresh().await.unwrap();
        let routed = catalog.snapshot().catalog().registrations().deployment(&deployment)
            .map(|server| (server.client_id.as_str().to_owned(), server.deployment));
        eprintln!("phase=reverse-completed registration={outcome} removed={removed:?} instance={present} authority={authority} snapshot={final_snapshot} routed={routed:?}");
        assert_eq!(
            (removed, refused, present, authority, final_snapshot["registrations"].clone(), logins, routed),
            (Some(database), true, false, false, json!([]), before_logins, None),
            "removal-first registration did not refuse without admission; service outcome={outcome}, snapshot={final_snapshot}"
        );
    }).await.expect("bounded removal-first registration overlap");
}

/// Test-only signed-service setup shared by the audit probes. No watcher or
/// Factory runs: explicit refresh below observes persisted catalog contents only.
struct AuditRegistrationFixture {
    db: crate::test_store::TestDb,
    store: RegistrationStore,
    service: crate::registration::RegistrationService,
    catalog: crate::HubCatalogHandle,
    instance: crate::registration::NewInstance,
    database: DatabaseName,
    request: veoveo_hub_sdk::registration::ClientRegistrationRequest,
    expected_keys: veoveo_hub_sdk::registration::Ed25519JwkSet,
    signing: ServerKeyPair,
    _directory: tempfile::TempDir,
}

impl AuditRegistrationFixture {
    async fn new(branch: &str) -> Self {
        let db = crate::test_store::TestDb::new().await;
        let store = RegistrationStore::new(db.a.clone());
        let directory = tempfile::tempdir().unwrap();
        let installation = crate::local::generate(directory.path(), "https://hub.test").unwrap();
        let catalog = crate::HubCatalogHandle::new(std::sync::Arc::new(
            crate::HubCatalog::load_json(&installation.config, crate::catalog_fixture::binding())
                .unwrap(),
        ));
        let signing = ServerKeyPair::generate().unwrap();
        let signing_key = veoveo_hub_sdk::HubInternalSigningKey::new(signing.pkcs8_der()).unwrap();
        let expected_keys =
            veoveo_hub_sdk::registration::Ed25519JwkSet::new(vec![signing_key.verification_key()])
                .unwrap();
        let service = crate::registration::RegistrationService::new(
            db.a.clone(),
            catalog.clone(),
            crate::registration::RegistrationIdentity {
                issuer: veoveo_hub_sdk::TokenIssuer::parse("https://hub.test").unwrap(),
                signing_key,
                verification_key: None,
            },
            crate::registration::ServerDatabase {
                url: veoveo_hub_sdk::registration::DatabaseUrl::new(db.a.config().url().clone())
                    .unwrap(),
            },
            Duration::from_secs(60),
            None,
        )
        .unwrap();
        let database = DatabaseName::installation(&db.a).unwrap();
        let instance = crate::registration::create_instance(
            &db.a,
            "0123456789abcdef0123456789abcdef",
            &database,
            &BranchName::parse(branch).unwrap(),
        )
        .await
        .unwrap();
        // Roles are cluster-wide, so frame this probe with its own valid slug.
        let slug =
            ServerSlug::parse(format!("audit-{}", instance.deployment.as_uuid().simple())).unwrap();
        let mut metadata = Candidate::new(
            &signing,
            instance.deployment,
            "00000000000051",
            1,
            Utc::now(),
        )
        .metadata;
        metadata.registration.slug = slug.clone();
        metadata.registration.uri_scheme = ResourceScheme::parse(slug.as_str()).unwrap();
        metadata.registration.endpoint = ServerEndpoint::new(
            format!("https://fixture.invalid/{slug}/mcp")
                .parse()
                .unwrap(),
        )
        .unwrap();
        metadata.check().unwrap();
        let request = veoveo_hub_sdk::registration::ClientRegistrationRequest {
            software_statement: signing
                .sign_statement(&metadata, service.registration_endpoint(), Utc::now())
                .unwrap(),
            metadata,
        };
        let fixture = Self {
            db,
            store,
            service,
            catalog,
            instance,
            database,
            request,
            expected_keys,
            signing,
            _directory: directory,
        };
        assert!(fixture.store.registrations().await.unwrap().is_empty());
        assert!(fixture.logins().await.is_empty());
        assert!(fixture.audit_observation().await.1.is_empty());
        fixture
    }

    async fn register(
        &self,
    ) -> std::result::Result<
        veoveo_hub_sdk::registration::ClientInformation,
        crate::registration::RegistrationError,
    > {
        self.service
            .register(
                super::super::service::Bearer::Presented(
                    self.instance.registration_token.expose_secret(),
                ),
                &serde_json::to_vec(&self.request).unwrap(),
            )
            .await
    }

    async fn logins(&self) -> Vec<String> {
        let group = ServerSchema::new(self.request.metadata.slug())
            .unwrap()
            .group_role();
        member_logins(&self.db.a, &group).await
    }

    async fn audit_observation(&self) -> (Option<Value>, Vec<Value>) {
        let target = format!("veoveo://servers/{}", self.request.metadata.slug());
        let observed = sqlx::query_file!(
            "src/registration/store/tests/audit_observation.sql",
            self.instance.deployment.as_uuid(),
            target,
        )
        .fetch_one(self.db.a.pool())
        .await
        .unwrap();
        (
            observed.instance,
            observed.records.as_array().unwrap().clone(),
        )
    }

    /// Check every ClientInformation field against supplied inputs/native persisted
    /// values. Secrets are checked in memory or by real PG auth, never printed.
    async fn assert_information(
        &self,
        information: &veoveo_hub_sdk::registration::ClientInformation,
        before: DateTime<Utc>,
        after: DateTime<Utc>,
    ) {
        information.check().unwrap();
        assert_eq!(information.metadata, self.request.metadata);
        assert_eq!(
            information.software_statement,
            self.request.software_statement
        );
        assert!(
            before <= information.client_id_issued_at && information.client_id_issued_at <= after
        );
        let prefix = format!("{}-", self.request.metadata.slug());
        let random_id = information
            .client_id
            .as_str()
            .strip_prefix(&prefix)
            .unwrap();
        assert!(
            random_id.len() == 32
                && random_id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert_eq!(
            information.registration_client_uri.as_str(),
            format!("https://hub.test/oauth/register/{}", information.client_id)
        );
        assert_eq!(
            information.access_tokens.deployment,
            self.instance.deployment
        );
        assert_eq!(
            information.access_tokens.issuer.as_str(),
            "https://hub.test"
        );
        assert_eq!(information.access_tokens.jwks, self.expected_keys);
        assert_eq!(
            information.access_tokens.resource.as_str(),
            self.request.metadata.registration.endpoint.url().as_str()
        );
        assert!(information.public_base.is_none() && information.app_origin.is_none());
        let grant = information
            .database
            .as_ref()
            .expect("initial response grants a login");
        assert_eq!(grant.url.as_str(), self.db.a.config().url().as_str());
        let veoveo_hub_sdk::registration::StorageBinding::SharedLogicalSchema { server } =
            &grant.binding;
        assert_eq!(server, self.request.metadata.slug());
        let schema = ServerSchema::new(self.request.metadata.slug()).unwrap();
        let suffix = &information.client_id.as_str()[information.client_id.as_str().len() - 14..];
        assert_eq!(
            grant.username.as_str(),
            format!("{}_{suffix}", schema.schema())
        );
        for secret in [
            information.registration_access_token.expose_secret(),
            grant.password.expose_secret(),
        ] {
            assert!(
                secret.len() == 43
                    && secret
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            );
        }
        let logins = self.logins().await;
        assert_eq!(logins, vec![grant.username.as_str().to_owned()]);
        let snapshot = snapshot_for_server(
            &self.db.a,
            self.instance.deployment,
            self.request.metadata.slug().as_str(),
            &logins,
        )
        .await;
        let rows = snapshot["registrations"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["client_id"], information.client_id.as_str());
        assert_eq!(row["metadata"], json!(information.metadata));
        assert_eq!(row["login"], grant.username.as_str());
        assert_eq!(
            row["software_statement_digest"],
            crate::registration::digest(self.request.software_statement.as_str()).as_str()
        );
        assert_eq!(
            row["access_token_digest"],
            crate::registration::digest(information.registration_access_token.expose_secret())
                .as_str()
        );
        assert_eq!(
            timestamp(&row["issued_at"]),
            information.client_id_issued_at.trunc_subsecs(6)
        );
        let config = veoveo_store::StoreConfig::builder(grant.url.as_str(), grant.password.clone())
            .username(grant.username.as_str())
            .max_connections(1)
            .build()
            .unwrap();
        let connected = PlatformStore::connect(config)
            .await
            .expect("issued grant authenticates against the real fixture PG");
        connected.close().await;
    }
}

/// Intentional removal must retain a successful audit of the ended registration
/// alongside its earlier Register record.
#[tokio::test]
async fn intentional_instance_removal_records_the_ended_registration() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let fixture = AuditRegistrationFixture::new("registration-removal-audit").await;
        let before = Utc::now();
        let registration = fixture.register().await;
        let after = Utc::now();
        let outcome = registration_outcome(&registration);
        assert!(registration.is_ok(), "signed service registration setup failed: {outcome}");
        let information = registration.unwrap();
        fixture.assert_information(&information, before, after).await;
        let before_removal = fixture.audit_observation().await;
        assert_eq!(before_removal.1.len(), 1, "Register audit must be acknowledged before a successful response");
        assert_eq!(before_removal.1[0]["detail"]["activity"], "register");
        assert_eq!(before_removal.1[0]["outcome"], "success");
        let removed = crate::registration::remove_instance(&fixture.db.a, fixture.instance.deployment).await.unwrap();
        let logins = fixture.logins().await;
        let state = snapshot_for_server(&fixture.db.a, fixture.instance.deployment, fixture.request.metadata.slug().as_str(), &[information.database.as_ref().unwrap().username.as_str().to_owned()]).await;
        let after_removal = fixture.audit_observation().await;
        let authority = fixture.store.instance(&crate::registration::digest(fixture.instance.registration_token.expose_secret())).await.unwrap();
        fixture.service.refresh().await.unwrap();
        let route = fixture.catalog.snapshot().catalog().registrations().deployment(&fixture.instance.deployment)
            .map(|server| server.client_id.as_str().to_owned());
        eprintln!("phase=removal-audit response={outcome} removed={removed:?} authority={authority:?} instance_before={:?} instance_after={:?} state={state} logins={logins:?} route={route:?} audit_before={} audit_after={}", before_removal.0, after_removal.0, json!(before_removal.1), json!(after_removal.1));
        assert_eq!(removed, Some(fixture.database));
        assert!(after_removal.0.is_none() && authority.is_none() && logins.is_empty() && route.is_none());
        assert!(state["registrations"].as_array().unwrap().is_empty() && state["logins"].as_array().unwrap().is_empty());
        assert!(!state["deployment"].is_null(), "retained deployment identity is allowed");
        assert!(after_removal.1.starts_with(&before_removal.1));
        assert!(after_removal.1[before_removal.1.len()..].iter().any(|record|
            record["detail"]["activity"] == "deregister" && record["outcome"] == "success"),
            "documented intentional removal has no audit of its ended registration");
    }).await.expect("bounded intentional-instance-removal audit probe");
}

/// Refusing only the post-admission served marker must not leave a committed
/// registration without its required Register audit. Rollback OR atomic audit is
/// allowed by this probe; it does not require unaudited persistence.
#[tokio::test]
async fn a_served_marker_error_cannot_leave_an_unaudited_registration() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let fixture = AuditRegistrationFixture::new("registration-served-audit").await;
        let before = fixture.audit_observation().await;
        sqlx::query_file!("src/registration/store/tests/refuse_served.sql")
            .execute(fixture.db.a.pool()).await.unwrap();
        let registration = fixture.register().await;
        let outcome = registration_outcome(&registration);
        let served_error = matches!(&registration, Err(crate::registration::RegistrationError::Internal(error))
            if error.chain().any(|cause| cause.to_string().contains("fixture refuses postcommit served marker")));
        let logins = fixture.logins().await;
        let state = snapshot_for_server(&fixture.db.a, fixture.instance.deployment, fixture.request.metadata.slug().as_str(), &logins).await;
        let after = fixture.audit_observation().await;
        let authority = fixture.store.instance(&crate::registration::digest(fixture.instance.registration_token.expose_secret())).await.unwrap();
        // Explicit refresh observes persisted rows, not notification convergence;
        // the service reload/information path was never reached. The store's
        // Register audit must already be committed before the served marker.
        fixture.service.refresh().await.unwrap();
        let route = fixture.catalog.snapshot().catalog().registrations().deployment(&fixture.instance.deployment)
            .map(|server| server.client_id.as_str().to_owned());
        let rows = state["registrations"].as_array().unwrap();
        let register_audited = after.1.iter().any(|record|
            record["detail"]["activity"] == "register" && record["outcome"] == "success");
        eprintln!("phase=served-error response={outcome} served_error={served_error} authority={authority:?} instance_before={:?} instance_after={:?} state={state} logins={logins:?} route={route:?} audit_before={} audit_after={}", before.0, after.0, json!(before.1), json!(after.1));
        assert!(served_error, "probe did not reach the real served marker refusal: {outcome}");
        assert_eq!(before.0, after.0, "refused marker changes no instance fields");
        assert_eq!(authority, Some((fixture.instance.deployment, fixture.database)));
        for row in rows {
            assert_eq!(row["metadata"], json!(fixture.request.metadata));
            assert_eq!(row["software_statement_digest"], crate::registration::digest(fixture.request.software_statement.as_str()).as_str());
        }
        assert!(rows.is_empty() || register_audited,
            "a real served error returned after admission committed without the required registration audit");
    }).await.expect("bounded postcommit-served audit probe");
}

async fn audit_records(
    store: &PlatformStore,
    deployment: DeploymentId,
    server: &ServerSlug,
) -> Vec<Value> {
    let target = format!("veoveo://servers/{server}");
    sqlx::query_file!(
        "src/registration/store/tests/audit_observation.sql",
        deployment.as_uuid(),
        target
    )
    .fetch_one(store.pool())
    .await
    .unwrap()
    .records
    .as_array()
    .unwrap()
    .clone()
}

fn audit_activities(records: &[Value]) -> Vec<&str> {
    records
        .iter()
        .map(|record| record["detail"]["activity"].as_str().unwrap())
        .collect()
}

async fn audit_fault(store: &PlatformStore, activity: Option<&str>) {
    // Utility statements defer fixture-table lookup until execution, after this
    // test has installed refuse_audit.sql. Ordinary SQLx/conformance databases
    // need only the real Core and Hub migrations.
    let query = match activity {
        None => sqlx::query_file!("src/registration/store/tests/audit_fault_off.sql"),
        Some("claim_namespace") => {
            sqlx::query_file!("src/registration/store/tests/audit_fault_claim_namespace.sql")
        }
        Some("register") => {
            sqlx::query_file!("src/registration/store/tests/audit_fault_register.sql")
        }
        Some("replace") => {
            sqlx::query_file!("src/registration/store/tests/audit_fault_replace.sql")
        }
        Some("update") => sqlx::query_file!("src/registration/store/tests/audit_fault_update.sql"),
        Some("deregister") => {
            sqlx::query_file!("src/registration/store/tests/audit_fault_deregister.sql")
        }
        Some("expire") => sqlx::query_file!("src/registration/store/tests/audit_fault_expire.sql"),
        Some(other) => panic!("unsupported fixture audit activity: {other}"),
    };
    query.execute(store.pool()).await.unwrap();
}

fn assert_audit_failure(error: &anyhow::Error) {
    assert!(
        error.chain().any(|cause| cause
            .to_string()
            .contains("fixture refuses registration audit")),
        "operation must reach the actual audit refusal: {error:#}"
    );
}

/// A valid typed namespace candidate with unique cluster-wide role names. These
/// store-level probes exercise SQL/audit effects; real service signature/token and
/// issued-material behavior is separately exercised by the service regressions.
fn audited_candidate(
    key: &ServerKeyPair,
    deployment: DeploymentId,
    revision: u8,
    now: DateTime<Utc>,
) -> Candidate {
    let suffix = format!("{revision:014x}");
    let mut candidate = Candidate::new(key, deployment, &suffix, revision, now);
    let slug = ServerSlug::parse(format!("audit-{}", deployment.as_uuid().simple())).unwrap();
    candidate.metadata.registration.slug = slug.clone();
    candidate.metadata.registration.uri_scheme = ResourceScheme::parse(slug.as_str()).unwrap();
    candidate.metadata.check().unwrap();
    candidate.client = OAuthClientId::parse(format!("{slug}-{suffix}")).unwrap();
    candidate.login = ServerSchema::new(&slug).unwrap().login(&suffix).unwrap();
    candidate.statement = key
        .sign_statement(
            &candidate.metadata,
            &"https://registration-probe.example/register"
                .parse()
                .unwrap(),
            now,
        )
        .unwrap();
    candidate
}

/// Audit failure at each actual domain boundary must leave its earlier domain
/// writes and earlier audits uncommitted. Clearing the fault permits a real retry;
/// missing updates/deletes and separate orphan-marker maintenance retain their scope.
#[tokio::test]
async fn namespace_registration_audit_failures_roll_back_and_retry() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let store = RegistrationStore::new(db.a.clone());
        let key = ServerKeyPair::generate().unwrap();
        let deployment = DeploymentId::new();
        let now = Utc::now().trunc_subsecs(6);
        let first = audited_candidate(&key, deployment, 1, now);
        let second = audited_candidate(&key, deployment, 2, now + TimeDelta::seconds(1));
        let third = audited_candidate(&key, deployment, 3, now + TimeDelta::seconds(2));
        let fourth = audited_candidate(&key, deployment, 4, now + TimeDelta::seconds(3));
        let slug = first.metadata.slug();
        let logins = vec![
            first.login.as_str().to_owned(),
            second.login.as_str().to_owned(),
            fourth.login.as_str().to_owned(),
        ];
        sqlx::query_file!("src/registration/store/tests/refuse_audit.sql")
            .execute(db.a.pool())
            .await
            .unwrap();
        let before = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        // Register refusal happens AFTER ClaimNamespace was already appended
        // privately, proving both that earlier audit and namespace/role writes roll back.
        for activity in ["claim_namespace", "register"] {
            audit_fault(&db.a, Some(activity)).await;
            assert_audit_failure(&first.register(&store).await.unwrap_err());
            assert_eq!(
                snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await,
                before
            );
            assert!(audit_records(&db.a, deployment, slug).await.is_empty());
        }
        audit_fault(&db.a, None).await;
        assert_eq!(first.register(&store).await.unwrap().0, Pin::Claimed);
        assert_eq!(
            audit_activities(&audit_records(&db.a, deployment, slug).await),
            ["claim_namespace", "register"]
        );

        let before_replace = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        let prior_audits = audit_records(&db.a, deployment, slug).await;
        audit_fault(&db.a, Some("register")).await;
        assert_audit_failure(&second.register(&store).await.unwrap_err());
        assert_eq!(
            snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await,
            before_replace
        );
        assert_eq!(audit_records(&db.a, deployment, slug).await, prior_audits);
        audit_fault(&db.a, None).await;
        let (pin, ended) = second.register(&store).await.unwrap();
        assert_eq!(pin, Pin::Held);
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].client_id, first.client);
        assert_eq!(
            audit_activities(&audit_records(&db.a, deployment, slug).await),
            ["claim_namespace", "register", "replace", "register"]
        );

        let before_update = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        let prior_audits = audit_records(&db.a, deployment, slug).await;
        audit_fault(&db.a, Some("update")).await;
        assert_audit_failure(
            &store
                .update(&second.client, &third.metadata, &third.statement, third.now)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await,
            before_update
        );
        assert_eq!(audit_records(&db.a, deployment, slug).await, prior_audits);
        audit_fault(&db.a, None).await;
        store
            .update(&second.client, &third.metadata, &third.statement, third.now)
            .await
            .unwrap();
        let updated = store.registration(&second.client).await.unwrap().unwrap();
        assert_eq!(updated.metadata, third.metadata);
        assert_eq!(updated.software_statement, third.statement);
        assert_eq!(updated.issued_at, second.now);
        let before_missing = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        let prior_audits = audit_records(&db.a, deployment, slug).await;
        // Fourth's client has never been admitted. Its pending deployment update
        // must roll back with no audit, retaining the established no-op contract.
        store
            .update(
                &fourth.client,
                &fourth.metadata,
                &fourth.statement,
                fourth.now,
            )
            .await
            .unwrap();
        assert_eq!(
            snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await,
            before_missing
        );
        assert_eq!(audit_records(&db.a, deployment, slug).await, prior_audits);

        audit_fault(&db.a, Some("deregister")).await;
        assert_audit_failure(&store.deregister(&second.client).await.unwrap_err());
        assert_eq!(
            snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await,
            before_missing
        );
        assert_eq!(audit_records(&db.a, deployment, slug).await, prior_audits);
        audit_fault(&db.a, None).await;
        assert_eq!(
            store
                .deregister(&second.client)
                .await
                .unwrap()
                .unwrap()
                .client_id,
            second.client
        );
        let deregistered_audits = audit_records(&db.a, deployment, slug).await;
        assert!(store.deregister(&second.client).await.unwrap().is_none());
        assert_eq!(
            audit_records(&db.a, deployment, slug).await,
            deregistered_audits
        );

        fourth.register(&store).await.unwrap();
        let before_expiry = snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        let prior_audits = audit_records(&db.a, deployment, slug).await;
        let marked = now + TimeDelta::seconds(10);
        audit_fault(&db.a, Some("expire")).await;
        assert_audit_failure(
            &store
                .collect(marked, marked + TimeDelta::seconds(1))
                .await
                .unwrap_err(),
        );
        let mut expiry_failed =
            snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await;
        assert_eq!(
            timestamp(&expiry_failed["registrations"][0]["orphaned_at"]),
            marked
        );
        // Marking is intentionally outside the expired-row removal transaction.
        expiry_failed["registrations"][0]["orphaned_at"] = Value::Null;
        assert_eq!(expiry_failed, before_expiry);
        assert_eq!(audit_records(&db.a, deployment, slug).await, prior_audits);
        audit_fault(&db.a, None).await;
        let expired = store
            .collect(marked, marked + TimeDelta::seconds(1))
            .await
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].client_id, fourth.client);
        assert!(store.registrations().await.unwrap().is_empty());
        let final_audits = audit_records(&db.a, deployment, slug).await;
        assert_eq!(
            audit_activities(&final_audits),
            [
                "claim_namespace",
                "register",
                "replace",
                "register",
                "update",
                "deregister",
                "register",
                "expire"
            ]
        );
        assert!(
            store
                .collect(marked, marked + TimeDelta::seconds(1))
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(audit_records(&db.a, deployment, slug).await, final_audits);
        eprintln!(
            "phase=namespace-audit-rollback-retry activities={:?} final={}",
            audit_activities(&final_audits),
            snapshot_for_server(&db.a, deployment, slug.as_str(), &logins).await
        );
    })
    .await
    .expect("bounded namespace registration audit rollback/retry");
}

#[tokio::test]
async fn instance_registration_and_removal_audit_failures_roll_back_and_retry() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let mut fixture = AuditRegistrationFixture::new("registration-atomic-audit").await;
        sqlx::query_file!("src/registration/store/tests/refuse_audit.sql")
            .execute(fixture.db.a.pool())
            .await
            .unwrap();
        let before_instance = fixture.audit_observation().await;
        let before = snapshot_for_server(
            &fixture.db.a,
            fixture.instance.deployment,
            fixture.request.metadata.slug().as_str(),
            &[],
        )
        .await;
        audit_fault(&fixture.db.a, Some("register")).await;
        let refused = fixture.register().await;
        let outcome = registration_outcome(&refused);
        match refused {
            Err(crate::registration::RegistrationError::Internal(error)) => {
                assert_audit_failure(&error)
            }
            _ => panic!("first instance admission did not reach the audit refusal: {outcome}"),
        }
        assert_eq!(
            snapshot_for_server(
                &fixture.db.a,
                fixture.instance.deployment,
                fixture.request.metadata.slug().as_str(),
                &[]
            )
            .await,
            before
        );
        assert!(fixture.logins().await.is_empty());
        assert_eq!(fixture.audit_observation().await, before_instance);
        audit_fault(&fixture.db.a, None).await;
        let before_time = Utc::now();
        let first = fixture.register().await.unwrap();
        fixture
            .assert_information(&first, before_time, Utc::now())
            .await;

        let prior_audits = fixture.audit_observation().await;
        let old_logins = fixture.logins().await;
        let before_noop = snapshot_for_server(
            &fixture.db.a,
            fixture.instance.deployment,
            fixture.request.metadata.slug().as_str(),
            &old_logins,
        )
        .await;
        let update = veoveo_hub_sdk::registration::ClientUpdateRequest {
            client_id: first.client_id.clone(),
            metadata: fixture.request.metadata.clone(),
            software_statement: fixture
                .signing
                .sign_statement(
                    &fixture.request.metadata,
                    &first.registration_client_uri,
                    Utc::now(),
                )
                .unwrap(),
        };
        let unchanged = fixture
            .service
            .update(
                first.client_id.as_str(),
                super::super::service::Bearer::Presented(
                    first.registration_access_token.expose_secret(),
                ),
                &serde_json::to_vec(&update).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unchanged.client_id, first.client_id);
        assert!(unchanged.database.is_none());
        assert_eq!(
            snapshot_for_server(
                &fixture.db.a,
                fixture.instance.deployment,
                fixture.request.metadata.slug().as_str(),
                &old_logins
            )
            .await,
            before_noop
        );
        assert_eq!(fixture.audit_observation().await, prior_audits);

        fixture.request.metadata.registration.version = ServerVersion::new("2.0.0").unwrap();
        fixture.request.software_statement = fixture
            .signing
            .sign_statement(
                &fixture.request.metadata,
                fixture.service.registration_endpoint(),
                Utc::now(),
            )
            .unwrap();
        audit_fault(&fixture.db.a, Some("replace")).await;
        let refused = fixture.register().await;
        let outcome = registration_outcome(&refused);
        match refused {
            Err(crate::registration::RegistrationError::Internal(error)) => {
                assert_audit_failure(&error)
            }
            _ => panic!("instance replacement did not reach audit refusal: {outcome}"),
        }
        assert_eq!(
            snapshot_for_server(
                &fixture.db.a,
                fixture.instance.deployment,
                fixture.request.metadata.slug().as_str(),
                &old_logins
            )
            .await,
            before_noop
        );
        assert_eq!(fixture.logins().await, old_logins);
        assert_eq!(fixture.audit_observation().await, prior_audits);
        audit_fault(&fixture.db.a, None).await;
        let before_time = Utc::now();
        let second = fixture.register().await.unwrap();
        fixture
            .assert_information(&second, before_time, Utc::now())
            .await;
        assert_ne!(second.client_id, first.client_id);
        let mut all_logins = old_logins;
        all_logins.extend(fixture.logins().await);
        let before_remove = snapshot_for_server(
            &fixture.db.a,
            fixture.instance.deployment,
            fixture.request.metadata.slug().as_str(),
            &all_logins,
        )
        .await;
        let prior_audits = fixture.audit_observation().await;
        audit_fault(&fixture.db.a, Some("deregister")).await;
        assert_audit_failure(
            &crate::registration::remove_instance(&fixture.db.a, fixture.instance.deployment)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            snapshot_for_server(
                &fixture.db.a,
                fixture.instance.deployment,
                fixture.request.metadata.slug().as_str(),
                &all_logins
            )
            .await,
            before_remove
        );
        assert_eq!(fixture.audit_observation().await, prior_audits);
        audit_fault(&fixture.db.a, None).await;
        assert_eq!(
            crate::registration::remove_instance(&fixture.db.a, fixture.instance.deployment)
                .await
                .unwrap(),
            Some(fixture.database.clone())
        );
        assert!(fixture.logins().await.is_empty());
        let final_audits = fixture.audit_observation().await;
        assert!(final_audits.0.is_none());
        assert_eq!(
            audit_activities(&final_audits.1),
            ["register", "replace", "register", "deregister"]
        );
        assert!(
            crate::registration::remove_instance(&fixture.db.a, fixture.instance.deployment)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(fixture.audit_observation().await, final_audits);
        eprintln!(
            "phase=instance-audit-rollback-retry activities={:?} final={}",
            audit_activities(&final_audits.1),
            snapshot_for_server(
                &fixture.db.a,
                fixture.instance.deployment,
                fixture.request.metadata.slug().as_str(),
                &all_logins
            )
            .await
        );
    })
    .await
    .expect("bounded instance registration/removal audit rollback/retry");
}
