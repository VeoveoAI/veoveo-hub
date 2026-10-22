//! Factory sandboxes against a fake OpenShell gateway (`factory/gateway.rs`) on a
//! disposable database with the Hub's migrations. A new instance gets its registration
//! provider and a sandbox of the pushed image, the default size, the base policy with
//! the repository's rules added, restart policy Always, the server's settings and the
//! fixed start command. A push of the same image sets the policy and stops and starts
//! the sandbox; a push of another image deletes and creates it and keeps its provider.
//! A failed step is recorded until a later one succeeds. Status reports OpenShell's
//! phase, exit code, restart count and conditions. Caps warn above the cap and,
//! enforced, refuse. Deleting an instance whose server registered with the Hub binary
//! removes its sandbox, provider, instance, registration, login and database. No
//! OpenShell, Lore or image is involved: images and revisions are synthetic values.
#[path = "support/database.rs"]
mod database;
#[path = "factory/gateway.rs"]
mod gateway;
#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::{collections::HashMap, future::Future, path::Path, time::Duration};

use database::TestDb;
use gateway::{Call, Gateway};
use openshell_sdk::{OpenShellClient, raw::proto as p};
use prost_types::{Struct, Value, value::Kind};
use secrecy::ExposeSecret;
use serde_json::json;
use veoveo_hub::{
    HubServerHealthState,
    control_plane::FactoryConfiguration,
    data_environment::{DatabaseName, ensure_database},
    execution::BranchName,
    factory::{
        Condition, Factory, Failure, Phase, Push, Revision, Source, Stage, Update,
        image::PinnedImage, sandbox_name,
    },
    registration::{NewInstance, create_instance},
};
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId};

/// How long one of these tests may take.
const BOUND: Duration = Duration::from_secs(120);
const WORKSPACE: &str = "factory";
const REPOSITORY: &str = "grpcs://lore.internal:41337/repository";
const IMAGE_A: &str = "registry.internal:5000/environment@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const IMAGE_B: &str = "registry.internal:5000/environment@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BASE_POLICY: &str = r#"version: 1
filesystem_policy:
  include_workdir: true
  read_only: [/usr, /etc]
  read_write: [/sandbox, /tmp]
network_policies:
  hub:
    endpoints:
      - {host: hub.example, port: 443}
    binaries:
      - path: /sandbox/src/**
  internet:
    endpoints:
      - {host: "**.*.*", ports: [80, 443]}
    binaries:
      - path: /**
"#;
const REPOSITORY_POLICY: &str = r#"version: 1
network_policies:
  packages:
    endpoints:
      - {host: packages.example, port: 443}
    binaries:
      - path: /**
"#;

struct Fixture {
    db: TestDb,
    gateway: Gateway,
    client: OpenShellClient,
    factory: Factory,
    directory: tempfile::TempDir,
}

impl Fixture {
    async fn start(caps: serde_json::Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("factory-base.yaml"), BASE_POLICY).unwrap();
        let db = TestDb::new().await;
        let (gateway, client) = Gateway::start().await;
        let factory = Factory::new(
            configuration(directory.path(), caps),
            client.clone(),
            db.a.clone(),
        )
        .unwrap();
        Self {
            db,
            gateway,
            client,
            factory,
            directory,
        }
    }

    /// Another factory on the same gateway and database, with other caps.
    fn with_caps(&self, caps: serde_json::Value) -> Factory {
        Factory::new(
            configuration(self.directory.path(), caps),
            self.client.clone(),
            self.db.a.clone(),
        )
        .unwrap()
    }

    /// A new instance of `branch` in the installation database.
    async fn instance(&self, branch: &str) -> NewInstance {
        let database = DatabaseName::installation(&self.db.a).unwrap();
        create_instance(&self.db.a, &database, &BranchName::parse(branch).unwrap())
            .await
            .unwrap()
    }

    fn sandbox(&self, deployment: DeploymentId) -> Option<p::Sandbox> {
        self.gateway.sandbox(WORKSPACE, &sandbox_name(deployment))
    }
}

fn configuration(directory: &Path, caps: serde_json::Value) -> FactoryConfiguration {
    serde_json::from_value(json!({
        "gateway": {
            "endpoint": "https://gateway.example/",
            "certificateAuthority": "/unused/ca.pem",
            "bearerEnvironment": "UNUSED"
        },
        "workspace": WORKSPACE,
        "policyFile": directory.join("factory-base.yaml"),
        "cpu": "8",
        "memory": "16Gi",
        "port": 8800,
        "hubIssuer": "https://hub.example",
        "registrationProfile": "vv-factory-registration",
        "imageBuilder": {
            "buildkit": "tcp://buildkitd.internal:1234",
            "registry": "registry.internal:5000",
            "lore": format!("registry.internal:5000/lore@{}", &IMAGE_A[IMAGE_A.len() - 71..]),
        },
        "caps": caps,
    }))
    .unwrap()
}

fn source(image: &str, policy: Option<&str>) -> Source {
    Source {
        repository: url::Url::parse(REPOSITORY).unwrap(),
        image: PinnedImage::parse(image).unwrap(),
        policy: policy.map(str::to_owned),
    }
}

/// A synthetic revision, the digit repeated, pushed by `pusher`.
fn push(revision: char, pusher: &str) -> Push {
    Push {
        revision: revision_of(revision),
        pusher: PrincipalId::parse(pusher).unwrap(),
    }
}

fn revision_of(digit: char) -> Revision {
    Revision::parse(digit.to_string().repeat(64)).unwrap()
}

fn scope() -> Option<p::WorkspaceSelector> {
    Some(p::WorkspaceSelector {
        selection: Some(p::workspace_selector::Selection::Workspace(
            WORKSPACE.into(),
        )),
    })
}

async fn bounded(test: impl Future<Output = ()>) {
    tokio::time::timeout(BOUND, test)
        .await
        .expect("the factory test exceeded its bound");
}

#[tokio::test]
async fn a_new_instance_gets_its_registration_provider_and_sandbox() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        let instance = fixture.instance("dev/jose/x").await;
        let name = sandbox_name(instance.deployment);
        fixture
            .factory
            .create(
                &instance,
                &source(IMAGE_A, Some(REPOSITORY_POLICY)),
                &push('1', "alice"),
            )
            .await
            .unwrap();

        let calls = fixture.gateway.take_calls();
        let [Call::CreateProvider(provider), Call::CreateSandbox(sandbox)] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(provider.workspace_scope, scope());
        let provider = provider.provider.as_ref().unwrap();
        let registration = format!("{name}-registration");
        assert_eq!(provider.metadata.as_ref().unwrap().name, registration);
        assert_eq!(provider.r#type, "vv-factory-registration");
        assert_eq!(
            provider.credentials,
            HashMap::from([(
                "VEOVEO_REGISTRATION_SECRET".to_owned(),
                instance.registration_token.expose_secret().to_owned()
            )])
        );

        assert_eq!(sandbox.workspace_scope, scope());
        assert_eq!(sandbox.name, name);
        let spec = sandbox.spec.as_ref().unwrap();
        let template = spec.template.as_ref().unwrap();
        assert_eq!(template.image, IMAGE_A);
        assert_eq!(template.resources, Some(limits("8", "16Gi")));
        assert_eq!(spec.restart_policy, p::SandboxRestartPolicy::Always as i32);
        assert_eq!(spec.providers, [registration]);
        assert_eq!(
            spec.environment,
            HashMap::from([
                (
                    "VEOVEO_DEPLOYMENT_ID".to_owned(),
                    instance.deployment.to_string()
                ),
                (
                    "VEOVEO_PUBLIC_BASE_URL".to_owned(),
                    format!("http://{name}.internal:8800")
                ),
                ("VEOVEO_PORT".to_owned(), "8800".to_owned()),
                (
                    "VEOVEO_HUB_ISSUER".to_owned(),
                    "https://hub.example".to_owned()
                ),
            ])
        );
        assert_eq!(
            spec.command,
            [
                "/bin/sh",
                "-c",
                "set -e\n\
                 [ -d /sandbox/src/.lore ] || lore clone \"$1\" /sandbox/src --branch \"$2\"\n\
                 cd /sandbox/src\n\
                 lore sync --reset \"$2@LATEST\"\n\
                 exec ./openshell/run",
                "factory",
                REPOSITORY,
                "dev/jose/x",
            ]
        );
        let policy = spec.policy.as_ref().unwrap();
        let base = openshell_policy::parse_sandbox_policy(BASE_POLICY).unwrap();
        assert_eq!(policy.filesystem, base.filesystem);
        let mut rules = policy.network_policies.keys().collect::<Vec<_>>();
        rules.sort();
        assert_eq!(rules, ["hub", "internet", "packages"]);

        let status = fixture.factory.status(instance.deployment).await.unwrap();
        assert_eq!(status.revision, Some(revision_of('1')));
        assert_eq!(status.failed, None);
        assert_eq!(status.warning, None);
        assert_eq!(status.sandbox.unwrap().phase, Phase::Provisioning);
    })
    .await;
}

#[tokio::test]
async fn a_push_restarts_an_unchanged_image_and_recreates_a_changed_one() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        let instance = fixture.instance("dev").await;
        let deployment = instance.deployment;
        let name = sandbox_name(deployment);
        fixture
            .factory
            .create(&instance, &source(IMAGE_A, None), &push('1', "alice"))
            .await
            .unwrap();
        fixture.gateway.take_calls();

        // OpenShell stops only a ready sandbox. The Hub records why its step failed and
        // keeps the revision it last applied.
        let pushed = source(IMAGE_A, Some(REPOSITORY_POLICY));
        assert!(
            fixture
                .factory
                .update(deployment, &pushed, &push('2', "bob"))
                .await
                .is_err()
        );
        let status = fixture.factory.status(deployment).await.unwrap();
        assert_eq!(status.revision, Some(revision_of('1')));
        let Some(Failure { stage, message }) = status.failed else {
            panic!("the failed restart is recorded");
        };
        assert_eq!(stage, Stage::Restart);
        assert!(
            message.contains("OpenShell refused to stop the sandbox: FailedPrecondition"),
            "{message}"
        );
        fixture.gateway.take_calls();

        fixture
            .gateway
            .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
        assert_eq!(
            fixture
                .factory
                .update(deployment, &pushed, &push('2', "bob"))
                .await
                .unwrap(),
            Update::Restarted
        );
        let calls = fixture.gateway.take_calls();
        let [
            Call::UpdateConfig(update),
            Call::StopSandbox(stop),
            Call::StartSandbox(start),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(
            (update.workspace_scope.clone(), update.sandbox.as_str()),
            (scope(), name.as_str())
        );
        assert!(
            update
                .policy
                .as_ref()
                .unwrap()
                .network_policies
                .contains_key("packages")
        );
        assert_eq!(stop.name, name);
        assert_eq!(start.name, name);
        let status = fixture.factory.status(deployment).await.unwrap();
        assert_eq!(status.revision, Some(revision_of('2')));
        assert_eq!(status.failed, None);
        assert_eq!(status.sandbox.unwrap().phase, Phase::Ready);

        assert_eq!(
            fixture
                .factory
                .update(deployment, &source(IMAGE_B, None), &push('3', "bob"))
                .await
                .unwrap(),
            Update::Recreated
        );
        let calls = fixture.gateway.take_calls();
        let [Call::DeleteSandbox(delete), Call::CreateSandbox(create)] = calls.as_slice() else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(
            (delete.name.as_str(), delete.allow_missing),
            (name.as_str(), true)
        );
        assert_eq!(create.name, name);
        let created = fixture.sandbox(deployment).unwrap();
        assert_eq!(created.spec.unwrap().template.unwrap().image, IMAGE_B);
        assert_eq!(
            fixture.gateway.providers(WORKSPACE),
            [format!("{name}-registration")]
        );
        let status = fixture.factory.status(deployment).await.unwrap();
        assert_eq!(status.revision, Some(revision_of('3')));
        assert_eq!(status.sandbox.unwrap().phase, Phase::Provisioning);
    })
    .await;
}

#[tokio::test]
async fn status_reports_the_sandbox_as_openshell_does() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        let instance = fixture.instance("dev").await;
        let name = sandbox_name(instance.deployment);
        fixture
            .factory
            .create(&instance, &source(IMAGE_A, None), &push('1', "alice"))
            .await
            .unwrap();
        let unschedulable = "0/1 nodes are available: 1 Insufficient cpu.";
        fixture.gateway.set_status(
            WORKSPACE,
            &name,
            p::SandboxStatus {
                phase: p::SandboxPhase::Provisioning as i32,
                exit_code: Some(101),
                restart_count: 2,
                conditions: vec![p::SandboxCondition {
                    r#type: "PodScheduled".into(),
                    status: "False".into(),
                    reason: "Unschedulable".into(),
                    message: unschedulable.into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let status = fixture.factory.status(instance.deployment).await.unwrap();
        let sandbox = status.sandbox.clone().unwrap();
        assert_eq!(sandbox.phase, Phase::Provisioning);
        assert_eq!(sandbox.exit_code, Some(101));
        assert_eq!(sandbox.restart_count, 2);
        assert_eq!(
            sandbox.conditions,
            [Condition {
                kind: "PodScheduled".into(),
                status: "False".into(),
                reason: "Unschedulable".into(),
                message: unschedulable.into(),
            }]
        );
        assert_eq!(status.health().state, HubServerHealthState::Degraded);
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["sandbox"]["phase"], "provisioning");
        assert_eq!(json["sandbox"]["exitCode"], 101);
        assert_eq!(json["sandbox"]["conditions"][0]["type"], "PodScheduled");
        assert_eq!(json["revision"], "1".repeat(64));

        for (phase, health) in [
            (p::SandboxPhase::Ready, HubServerHealthState::Healthy),
            (p::SandboxPhase::Error, HubServerHealthState::Offline),
        ] {
            fixture.gateway.set_phase(WORKSPACE, &name, phase);
            let status = fixture.factory.status(instance.deployment).await.unwrap();
            assert_eq!(status.health().state, health);
        }
        assert!(fixture.factory.status(DeploymentId::new()).await.is_err());
    })
    .await;
}

#[tokio::test]
async fn caps_warn_above_the_cap_and_refuse_when_enforced() {
    bounded(async {
        let fixture = Fixture::start(json!({"instancesPerUser": 1})).await;
        let first = fixture.instance("dev/alice/a").await;
        let second = fixture.instance("dev/alice/b").await;
        let other = fixture.instance("dev/bob/a").await;
        for (instance, pusher) in [(&first, "alice"), (&second, "alice"), (&other, "bob")] {
            fixture
                .factory
                .create(instance, &source(IMAGE_A, None), &push('1', pusher))
                .await
                .unwrap();
            assert!(fixture.sandbox(instance.deployment).is_some());
        }
        let factory = &fixture.factory;
        let warning = |deployment| async move { factory.status(deployment).await.unwrap().warning };
        assert_eq!(warning(first.deployment).await, None);
        assert_eq!(
            warning(second.deployment).await.as_deref(),
            Some("alice is the latest pusher of 2 instances, above the cap of 1")
        );
        assert_eq!(warning(other.deployment).await, None);

        let enforced = fixture.with_caps(json!({"instancesPerUser": 1, "enforce": true}));
        let refused = fixture.instance("dev/alice/c").await;
        let error = enforced
            .create(&refused, &source(IMAGE_A, None), &push('1', "alice"))
            .await
            .unwrap_err();
        let message = "alice is the latest pusher of 3 instances, above the cap of 1";
        assert_eq!(error.to_string(), message);
        let status = enforced.status(refused.deployment).await.unwrap();
        assert_eq!(
            status.failed,
            Some(Failure {
                stage: Stage::Capacity,
                message: message.into()
            })
        );
        assert_eq!(status.sandbox, None);
        assert_eq!(status.revision, None);
        assert!(fixture.sandbox(refused.deployment).is_none());
    })
    .await;
}

#[tokio::test]
async fn deleting_an_instance_removes_its_sandbox_providers_registrations_and_database() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let installation = tempfile::tempdir().unwrap();
        let hub = hub::Hub::start(installation.path(), &fixture.db, Duration::from_secs(60)).await;
        let http = reqwest::Client::new();
        let store = &fixture.db.a;
        let environment =
            DatabaseName::parse(format!("factory_{}", uuid::Uuid::new_v4().simple())).unwrap();
        ensure_database(store, &environment).await.unwrap();
        let instance = create_instance(store, &environment, &BranchName::parse("feature").unwrap())
            .await
            .unwrap();
        let deployment = instance.deployment;
        let name = sandbox_name(deployment);
        fixture
            .factory
            .create(&instance, &source(IMAGE_A, None), &push('1', "alice"))
            .await
            .unwrap();
        // The fixture server stands in for the sandbox's server and registers with the
        // instance's token.
        let served = servers::serve_instance(
            &hub,
            &http,
            "ledger",
            ledger::authorization().unwrap(),
            &instance,
            async |registration, _, _| {
                servers::hosted::<LedgerDomain>(registration, &http)
                    .handler(|| ledger::hosted(LedgerTasks::default()))
                    .build()
            },
        )
        .await;
        let registered = logins(store, deployment).await;
        assert_eq!(registered.len(), 1);
        assert!(role_exists(store, &registered[0]).await);
        fixture.gateway.take_calls();

        fixture.factory.delete(deployment).await.unwrap();
        let calls = fixture.gateway.take_calls();
        let [Call::DeleteSandbox(sandbox), Call::DeleteProvider(provider)] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(
            (sandbox.workspace_scope.clone(), sandbox.name.as_str()),
            (scope(), name.as_str())
        );
        assert_eq!(provider.name, format!("{name}-registration"));
        assert!(fixture.sandbox(deployment).is_none());
        assert!(fixture.gateway.providers(WORKSPACE).is_empty());
        assert!(logins(store, deployment).await.is_empty());
        assert!(!role_exists(store, &registered[0]).await);
        assert!(fixture.factory.status(deployment).await.is_err());
        assert!(!database_exists(store, &environment).await);
        drop(served);
    })
    .await;
}

/// Kubernetes resource limits, as OpenShell's command line sends them.
fn limits(cpu: &str, memory: &str) -> Struct {
    let text = |value: &str| Value {
        kind: Some(Kind::StringValue(value.into())),
    };
    Struct {
        fields: [(
            "limits".to_owned(),
            Value {
                kind: Some(Kind::StructValue(Struct {
                    fields: [
                        ("cpu".to_owned(), text(cpu)),
                        ("memory".to_owned(), text(memory)),
                    ]
                    .into(),
                })),
            },
        )]
        .into(),
    }
}

async fn logins(store: &PlatformStore, deployment: DeploymentId) -> Vec<String> {
    sqlx::query_file_scalar!("tests/factory/logins.sql", deployment.as_uuid())
        .fetch_all(store.pool())
        .await
        .unwrap()
}

async fn role_exists(store: &PlatformStore, role: &str) -> bool {
    sqlx::query_file_scalar!("tests/factory/role_exists.sql", role)
        .fetch_one(store.pool())
        .await
        .unwrap()
}

async fn database_exists(store: &PlatformStore, database: &DatabaseName) -> bool {
    sqlx::query_file_scalar!("tests/factory/database_exists.sql", database.as_str())
        .fetch_one(store.pool())
        .await
        .unwrap()
}
