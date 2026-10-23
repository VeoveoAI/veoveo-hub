//! Factory sandboxes against a fake OpenShell gateway (`factory/gateway.rs`) and a fake
//! image builder, on a disposable database with the Hub's migrations. Each test applies
//! pushes as the Lore watcher will: the first push of a branch creates its instance in
//! its data environment, a registration provider with a new token and a sandbox of the
//! built image, the default size, the base policy with the repository's rules added,
//! restart policy Always, the server's settings and the fixed start command. A push of
//! the same image and policy stops and starts the sandbox, whatever its phase once its
//! program has run, a crash loop included; a push of another image or another policy
//! deletes and creates it with a new token. No push updates a sandbox's configuration,
//! which the fake gateway refuses as it would the Hub's identity. A failed build and a
//! refused repository policy are recorded and change nothing else; a failed provider
//! creation leaves no dead end. Status reports OpenShell's phase, exit
//! code, restart count and conditions. Caps warn and, enforced, refuse. Removing an
//! instance whose server registered with the Hub binary removes its sandbox, provider,
//! instance, registration, login and database. The Hub binary's status route waits for
//! the applied revision to serve or fail, for callers with a grant on the repository
//! only. An instance's own client receives its latest pusher's Lore token, and a
//! Computer's own client its owner's, for every repository the owner is assigned; neither
//! client is taken for the other. No OpenShell, Lore, BuildKit or image is involved:
//! revisions, images and their digests are synthetic, and the fake builder's digest is
//! the SHA-256 of a `Dockerfile`'s text.
#[path = "support/catalog_admission.rs"]
mod catalog_admission;
#[path = "support/database.rs"]
mod database;
#[allow(
    dead_code,
    reason = "these tests use the factory's part of the fake gateway"
)]
#[path = "factory/gateway.rs"]
mod gateway;
#[path = "support/hub.rs"]
mod hub;
#[path = "factory/lore.rs"]
mod lore;
#[path = "support/servers.rs"]
mod servers;

use std::{
    collections::HashMap,
    future::Future,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::ExitStatus,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use database::TestDb;
use futures::future::BoxFuture;
use gateway::{Call, Gateway};
use openshell_sdk::{OpenShellClient, raw::proto as p};
use prost_types::{Struct, Value, value::Kind};
use secrecy::SecretString;
use serde_json::json;
use sha2::{Digest, Sha256};
use veoveo_hub::{
    HubCatalog, HubCatalogHandle, HubRefreshIssueRequest, HubServerHealthState, HubState,
    computers::ComputerProvisionSpec,
    contract::{AuthorizationServerId, PrincipalDisplayName},
    control_plane::FactoryConfiguration,
    data_environment::DatabaseName,
    execution::BranchName,
    factory::{
        self, Condition, Factory, Failure, InstanceClient, InstanceStatus, Phase, Prepared, Push,
        Revision, Stage,
        image::{Build, ImageBuildError, ImageName, PinnedImage},
        sandbox_name,
    },
    registration::NewInstance,
    store::{Computer, HubStore},
};
use veoveo_hub_sdk::{
    OAuthClientId, Principal, PrincipalKind, ProtectedResourceId, TokenIssuer, TokenSubject,
};
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId, Sha256Digest, TenantId, WorkContextId};

/// How long one of these tests may take.
const BOUND: Duration = Duration::from_secs(180);
const WORKSPACE: &str = "factory";
/// The Lore repository every push here belongs to, a synthetic ID.
const REPOSITORY: &str = "0123456789abcdef0123456789abcdef";
const LORE: &str = "grpcs://lore.internal:41337";
/// The installation's base branch, whose instances use the installation database.
const BASE: &str = "dev";
/// The settings variable the Hub binary reads the fake gateway's bearer from.
const GATEWAY_BEARER: &str = "VEOVEO_FACTORY_GATEWAY_TOKEN";
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

/// An image builder that builds nothing: an image's digest is the SHA-256 of the text
/// of its context's `Dockerfile`, so unchanged content keeps its digest as with
/// BuildKit, and a `Dockerfile` that starts with `FAIL` fails with its text as
/// BuildKit's output. It records the names it was asked to build.
#[derive(Default)]
struct Builder {
    built: Mutex<Vec<String>>,
}

impl Build for Builder {
    fn build<'a>(
        &'a self,
        context: &'a Path,
        name: &'a ImageName,
    ) -> BoxFuture<'a, Result<PinnedImage, ImageBuildError>> {
        Box::pin(async move {
            let dockerfile = std::fs::read_to_string(context.join("Dockerfile"))?;
            self.built.lock().unwrap().push(name.as_str().to_owned());
            if dockerfile.starts_with("FAIL") {
                return Err(ImageBuildError::Failed {
                    status: ExitStatus::from_raw(1 << 8),
                    output: dockerfile,
                });
            }
            Ok(built(name.as_str(), &dockerfile))
        })
    }
}

/// The image the fake builder returns for `dockerfile` under `name`.
fn built(name: &str, dockerfile: &str) -> PinnedImage {
    let digest = hex::encode(Sha256::digest(dockerfile.as_bytes()));
    PinnedImage::parse(format!(
        "registry.internal:5000/factory/{name}@sha256:{digest}"
    ))
    .unwrap()
}

struct Fixture {
    db: TestDb,
    gateway: Gateway,
    client: OpenShellClient,
    catalog: HubCatalogHandle,
    builder: Arc<Builder>,
    factory: Factory,
    directory: tempfile::TempDir,
}

impl Fixture {
    async fn start(caps: serde_json::Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("factory-base.yaml"), BASE_POLICY).unwrap();
        let db = TestDb::new().await;
        let (gateway, client) = Gateway::start().await;
        let catalog = catalog();
        let builder = Arc::new(Builder::default());
        let factory = Factory::new(
            configuration(directory.path(), caps),
            catalog.clone(),
            client.clone(),
            db.a.clone(),
        )
        .unwrap()
        .with_builder(builder.clone());
        Self {
            db,
            gateway,
            client,
            catalog,
            builder,
            factory,
            directory,
        }
    }

    /// Another factory on the same gateway, database and builder, with other caps.
    fn with_caps(&self, caps: serde_json::Value) -> Factory {
        Factory::new(
            configuration(self.directory.path(), caps),
            self.catalog.clone(),
            self.client.clone(),
            self.db.a.clone(),
        )
        .unwrap()
        .with_builder(self.builder.clone())
    }

    /// A revision's `openshell/` whose image `Dockerfile` is `dockerfile`.
    fn prepared(&self, dockerfile: &str, policy: Option<&str>) -> Prepared {
        let context = tempfile::tempdir_in(self.directory.path()).unwrap().keep();
        std::fs::write(context.join("Dockerfile"), dockerfile).unwrap();
        Prepared {
            image_context: context,
            policy: policy.map(str::to_owned),
        }
    }

    async fn apply(&self, branch: &str, push: Push, prepared: Prepared) -> anyhow::Result<()> {
        self.factory
            .apply(REPOSITORY, &branch_name(branch), push, prepared)
            .await
    }

    async fn status(&self, branch: &str) -> Option<InstanceStatus> {
        self.factory
            .status(REPOSITORY, &branch_name(branch), Duration::ZERO)
            .await
            .unwrap()
    }

    fn sandbox(&self, deployment: DeploymentId) -> Option<p::Sandbox> {
        self.gateway.sandbox(WORKSPACE, &sandbox_name(deployment))
    }

    /// The registration token in the instance's registration provider.
    fn token(&self, deployment: DeploymentId) -> String {
        let name = format!("{}-registration", sandbox_name(deployment));
        self.gateway.credentials(WORKSPACE, &name).unwrap()["VEOVEO_REGISTRATION_SECRET"].clone()
    }

    /// The instance as its server would register: its deployment and the token its
    /// provider holds.
    fn instance(&self, deployment: DeploymentId) -> NewInstance {
        NewInstance {
            deployment,
            registration_token: SecretString::from(self.token(deployment)),
        }
    }
}

/// The installation's configuration, with the base branch set.
fn catalog() -> HubCatalogHandle {
    let directory = tempfile::tempdir().unwrap();
    let installation = veoveo_hub::local::generate(directory.path(), "http://127.0.0.1:1").unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&installation.config).unwrap()).unwrap();
    config["execution"]["baseBranch"] = json!(BASE);
    let plane = serde_json::from_value(config).unwrap();
    HubCatalogHandle::new(Arc::new(
        HubCatalog::from_control_plane(plane, catalog_admission::binding()).unwrap(),
    ))
}

fn configuration_json(
    directory: &Path,
    gateway: &str,
    authority: &Path,
    caps: serde_json::Value,
) -> serde_json::Value {
    json!({
        "gateway": {
            "endpoint": gateway,
            "certificateAuthority": authority,
            "bearerEnvironment": GATEWAY_BEARER,
        },
        "workspace": WORKSPACE,
        "policyFile": directory.join("factory-base.yaml"),
        "cpu": "8",
        "memory": "16Gi",
        "port": 8800,
        "hubIssuer": "https://hub.example",
        "loreServer": LORE,
        "registrationProfile": "vv-factory-registration",
        "loreProfile": "vv-factory-lore",
        "imageBuilder": {
            "buildkit": "tcp://buildkitd.internal:1234",
            "registry": "registry.internal:5000",
            "lore": format!("registry.internal:5000/lore@sha256:{}", "a".repeat(64)),
        },
        "caps": caps,
    })
}

fn configuration(directory: &Path, caps: serde_json::Value) -> FactoryConfiguration {
    serde_json::from_value(configuration_json(
        directory,
        "https://gateway.example/",
        Path::new("/unused/ca.pem"),
        caps,
    ))
    .unwrap()
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

fn branch_name(branch: &str) -> BranchName {
    BranchName::parse(branch).unwrap()
}

fn scope() -> Option<p::WorkspaceSelector> {
    Some(p::WorkspaceSelector {
        selection: Some(p::workspace_selector::Selection::Workspace(
            WORKSPACE.into(),
        )),
    })
}

/// The digest the Hub keeps of a registration token.
fn digest(token: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(token.as_bytes())))
}

async fn bounded(test: impl Future<Output = ()>) {
    tokio::time::timeout(BOUND, test)
        .await
        .expect("the factory test exceeded its bound");
}

#[tokio::test]
async fn a_first_push_creates_the_instance_in_its_data_environment() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        fixture
            .apply(
                "dev/jose/x",
                push('1', "alice"),
                fixture.prepared("FROM a", Some(REPOSITORY_POLICY)),
            )
            .await
            .unwrap();
        let status = fixture.status("dev/jose/x").await.unwrap();
        let deployment = status.deployment;
        let name = sandbox_name(deployment);
        assert_eq!(
            *fixture.builder.built.lock().unwrap(),
            std::slice::from_ref(&name)
        );

        let calls = fixture.gateway.take_calls();
        let [
            Call::DeleteProvider(previous),
            Call::CreateProvider(provider),
            Call::DeleteProvider(previous_lore),
            Call::CreateProvider(lore),
            Call::ConfigureProviderRefresh(refresh),
            Call::RotateProviderCredential(rotate),
            Call::CreateSandbox(sandbox),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        // The Lore provider starts without a credential: the gateway refreshes it with
        // the instance's own client, whose identifier is the deployment, and mints its
        // first token before the sandbox starts.
        let lore_provider = format!("{name}-lore");
        assert_eq!(
            (previous_lore.name.as_str(), previous_lore.allow_missing),
            (lore_provider.as_str(), true)
        );
        assert_eq!(lore.workspace_scope, scope());
        let lore = lore.provider.as_ref().unwrap();
        assert_eq!(lore.metadata.as_ref().unwrap().name, lore_provider);
        assert_eq!(lore.r#type, "vv-factory-lore");
        assert!(lore.credentials.is_empty());
        assert_eq!(
            (
                refresh.workspace_scope.clone(),
                refresh.provider.as_str(),
                refresh.credential_key.as_str(),
                refresh.strategy,
                refresh.secret_material_keys.as_slice(),
            ),
            (
                scope(),
                lore_provider.as_str(),
                "LORE_ACCESS_TOKEN",
                p::ProviderCredentialRefreshStrategy::Oauth2ClientCredentials as i32,
                ["client_secret".to_owned()].as_slice(),
            )
        );
        assert_eq!(refresh.material["client_id"], deployment.to_string());
        let secret = &refresh.material["client_secret"];
        assert_eq!(
            factory::instance_client(&fixture.db.a, &deployment.to_string(), secret)
                .await
                .unwrap(),
            Some(InstanceClient {
                deployment,
                pusher: Some(PrincipalId::parse("alice").unwrap()),
                repository: REPOSITORY.into(),
            })
        );
        for (client, secret) in [
            (DeploymentId::new().to_string(), secret.as_str()),
            (deployment.to_string(), "another-secret"),
        ] {
            assert_eq!(
                factory::instance_client(&fixture.db.a, &client, secret)
                    .await
                    .unwrap(),
                None
            );
        }
        assert_eq!(
            (rotate.provider.as_str(), rotate.credential_key.as_str()),
            (lore_provider.as_str(), "LORE_ACCESS_TOKEN")
        );
        let registration = format!("{name}-registration");
        assert_eq!(
            (previous.name.as_str(), previous.allow_missing),
            (registration.as_str(), true)
        );
        assert_eq!(provider.workspace_scope, scope());
        let provider = provider.provider.as_ref().unwrap();
        assert_eq!(provider.metadata.as_ref().unwrap().name, registration);
        assert_eq!(provider.r#type, "vv-factory-registration");
        let token = fixture.token(deployment);
        assert_eq!(
            provider.credentials,
            HashMap::from([("VEOVEO_REGISTRATION_SECRET".to_owned(), token.clone())])
        );
        assert_eq!(
            token_digest(&fixture.db.a, deployment).await,
            digest(&token)
        );

        assert_eq!(sandbox.workspace_scope, scope());
        assert_eq!(sandbox.name, name);
        let spec = sandbox.spec.as_ref().unwrap();
        let template = spec.template.as_ref().unwrap();
        assert_eq!(template.image, built(&name, "FROM a").as_str());
        assert_eq!(template.resources, Some(limits("8", "16Gi")));
        assert_eq!(spec.restart_policy, p::SandboxRestartPolicy::Always as i32);
        assert_eq!(spec.providers, [registration, lore_provider]);
        assert_eq!(
            spec.environment,
            HashMap::from([
                ("VEOVEO_DEPLOYMENT_ID".to_owned(), deployment.to_string()),
                ("LORE_IDENTITY".to_owned(), deployment.to_string()),
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
                &format!("{LORE}/{REPOSITORY}"),
                "dev/jose/x",
            ]
        );
        let policy = spec.policy.as_ref().unwrap();
        let base = openshell_policy::parse_sandbox_policy(BASE_POLICY).unwrap();
        assert_eq!(policy.filesystem, base.filesystem);
        let mut rules = policy.network_policies.keys().collect::<Vec<_>>();
        rules.sort();
        assert_eq!(rules, ["hub", "internet", "packages"]);

        assert_eq!(status.workspace, WORKSPACE);
        assert_eq!(status.sandbox_name, name);
        assert_eq!(status.sandbox_id, Some(format!("{name}-id")));
        assert_eq!(status.applied_revision, Some(revision_of('1')));
        assert_eq!(status.serving_revision, None);
        assert_eq!(status.failed, None);
        assert_eq!(status.warning, None);
        assert_eq!(status.sandbox.unwrap().phase, Phase::Provisioning);

        // The branch's data environment has a database of its own, and the base
        // branch's instances use the installation database.
        let environment = DatabaseName::of_branch(&branch_name("dev/jose/x"));
        assert_eq!(
            instance_database(&fixture.db.a, deployment).await,
            environment
        );
        assert!(database_exists(&fixture.db.a, &environment).await);
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let base = fixture.status(BASE).await.unwrap();
        assert_eq!(
            instance_database(&fixture.db.a, base.deployment).await,
            DatabaseName::installation(&fixture.db.a).unwrap()
        );
        assert!(fixture.status("dev/jose/y").await.is_none());
        assert!(
            fixture
                .factory
                .apply(
                    "not-a-repository",
                    &branch_name(BASE),
                    push('2', "alice"),
                    fixture.prepared("FROM a", None)
                )
                .await
                .is_err()
        );
    })
    .await;
}

#[tokio::test]
async fn a_push_restarts_an_unchanged_image_and_recreates_a_changed_one() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        let prepared = |dockerfile| fixture.prepared(dockerfile, Some(REPOSITORY_POLICY));
        fixture
            .apply(BASE, push('1', "alice"), prepared("FROM a"))
            .await
            .unwrap();
        let deployment = fixture.status(BASE).await.unwrap().deployment;
        let name = sandbox_name(deployment);
        fixture.gateway.take_calls();

        // A sandbox still provisioning has not run its start command: it fetches the
        // latest revision when it does, so only its Lore token is refreshed, and the
        // sandbox starts as the new pusher.
        fixture
            .apply(BASE, push('2', "bob"), prepared("FROM a"))
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [Call::RotateProviderCredential(rotate)] = calls.as_slice() else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(rotate.provider, format!("{name}-lore"));
        assert_eq!(
            fixture.status(BASE).await.unwrap().applied_revision,
            Some(revision_of('2'))
        );

        fixture
            .gateway
            .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
        served(&fixture.db.a, deployment).await;
        let serving = fixture.status(BASE).await.unwrap();
        assert_eq!(serving.serving_revision, Some(revision_of('2')));
        assert!(serving.settled());
        fixture
            .apply(BASE, push('3', "bob"), prepared("FROM a"))
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [
            Call::RotateProviderCredential(_),
            Call::StopSandbox(stop),
            Call::StartSandbox(start),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(
            (stop.workspace_scope.clone(), stop.name.as_str()),
            (scope(), name.as_str())
        );
        assert_eq!(start.name, name);
        let restarted = fixture.status(BASE).await.unwrap();
        assert_eq!(restarted.applied_revision, Some(revision_of('3')));
        assert_eq!(restarted.failed, None);
        assert_eq!(restarted.sandbox.unwrap().phase, Phase::Starting);
        // The restart ended what the server served, until it registers or resumes.
        assert_eq!(restarted.serving_revision, None);
        let first_token = fixture.token(deployment);

        fixture
            .apply(BASE, push('4', "bob"), prepared("FROM b"))
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [
            Call::DeleteSandbox(delete),
            Call::DeleteProvider(previous),
            Call::CreateProvider(_),
            Call::DeleteProvider(previous_lore),
            Call::CreateProvider(_),
            Call::ConfigureProviderRefresh(_),
            Call::RotateProviderCredential(_),
            Call::CreateSandbox(create),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(previous_lore.name, format!("{name}-lore"));
        assert_eq!(
            (delete.name.as_str(), delete.allow_missing),
            (name.as_str(), true)
        );
        assert_eq!(previous.name, format!("{name}-registration"));
        assert_eq!(create.name, name);
        let created = fixture.sandbox(deployment).unwrap();
        assert_eq!(
            created.spec.unwrap().template.unwrap().image,
            built(&name, "FROM b").as_str()
        );
        // The new sandbox's disk is empty, and its server registers with a new token.
        let token = fixture.token(deployment);
        assert_ne!(token, first_token);
        assert_eq!(
            token_digest(&fixture.db.a, deployment).await,
            digest(&token)
        );
        assert_eq!(
            fixture.gateway.providers(WORKSPACE),
            [format!("{name}-lore"), format!("{name}-registration")]
        );
        let recreated = fixture.status(BASE).await.unwrap();
        assert_eq!(recreated.applied_revision, Some(revision_of('4')));
        assert_eq!(recreated.sandbox.unwrap().phase, Phase::Provisioning);
    })
    .await;
}

#[tokio::test]
async fn a_push_recreates_a_sandbox_whose_policy_changed() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status(BASE).await.unwrap().deployment;
        let name = sandbox_name(deployment);
        let rules = || {
            let sandbox = fixture.sandbox(deployment).unwrap();
            let mut rules = sandbox
                .spec
                .unwrap()
                .policy
                .unwrap()
                .network_policies
                .into_keys()
                .collect::<Vec<_>>();
            rules.sort();
            rules
        };
        assert_eq!(rules(), ["hub", "internet"]);
        let ready = async || {
            fixture
                .gateway
                .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
            served(&fixture.db.a, deployment).await;
            fixture.gateway.take_calls();
        };
        ready().await;
        let first_token = fixture.token(deployment);

        // The same image with the repository's rules added: OpenShell's identity for the
        // Hub cannot update the sandbox's policy, so the Hub replaces the sandbox, whose
        // disk starts empty and whose server registers with a new token.
        fixture
            .apply(
                BASE,
                push('2', "bob"),
                fixture.prepared("FROM a", Some(REPOSITORY_POLICY)),
            )
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [
            Call::DeleteSandbox(delete),
            Call::DeleteProvider(_),
            Call::CreateProvider(_),
            Call::DeleteProvider(_),
            Call::CreateProvider(_),
            Call::ConfigureProviderRefresh(_),
            Call::RotateProviderCredential(_),
            Call::CreateSandbox(create),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!((delete.name.as_str(), create.name.as_str()), (name.as_str(), name.as_str()));
        assert_eq!(
            create.spec.as_ref().unwrap().template.as_ref().unwrap().image,
            built(&name, "FROM a").as_str()
        );
        assert_eq!(rules(), ["hub", "internet", "packages"]);
        assert_ne!(fixture.token(deployment), first_token);
        let recreated = fixture.status(BASE).await.unwrap();
        assert_eq!(recreated.applied_revision, Some(revision_of('2')));
        assert_eq!(recreated.serving_revision, None);
        assert_eq!(recreated.failed, None);
        assert_eq!(recreated.sandbox.unwrap().phase, Phase::Provisioning);

        // The same repository policy again compares equal to the one OpenShell recorded
        // in its canonical form: the sandbox restarts.
        ready().await;
        fixture
            .apply(
                BASE,
                push('3', "bob"),
                fixture.prepared("FROM a", Some(REPOSITORY_POLICY)),
            )
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [
            Call::RotateProviderCredential(_),
            Call::StopSandbox(_),
            Call::StartSandbox(_),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };

        // Removing the repository's rules replaces the sandbox again.
        ready().await;
        fixture
            .apply(BASE, push('4', "bob"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [Call::DeleteSandbox(_), .., Call::CreateSandbox(_)] = calls.as_slice() else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(rules(), ["hub", "internet"]);

        // A repository policy the Hub refuses is recorded and changes nothing else.
        ready().await;
        let token = fixture.token(deployment);
        let error = fixture
            .apply(
                BASE,
                push('5', "bob"),
                fixture.prepared(
                    "FROM a",
                    Some("version: 1\nnetwork_policies:\n  internet:\n    endpoints:\n      - {host: packages.example, port: 443}\n    binaries:\n      - path: /**\n"),
                ),
            )
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("redefines"), "{error:#}");
        assert_eq!(fixture.gateway.take_calls(), []);
        let status = fixture.status(BASE).await.unwrap();
        let Some(Failure { stage, message }) = &status.failed else {
            panic!("the refused policy is recorded");
        };
        assert_eq!(*stage, Stage::Create);
        assert!(
            message.contains("the installation's network policy `internet`"),
            "{message}"
        );
        assert_eq!(status.applied_revision, Some(revision_of('4')));
        assert_eq!(status.serving_revision, Some(revision_of('4')));
        assert_eq!(fixture.token(deployment), token);
        assert_eq!(rules(), ["hub", "internet"]);
    })
    .await;
}

#[tokio::test]
async fn a_push_restarts_a_crash_looping_sandbox() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status(BASE).await.unwrap().deployment;
        let name = sandbox_name(deployment);
        // A failed program waiting out its backoff, and one OpenShell is restarting.
        for (revision, phase, restarts) in [
            ('2', p::SandboxPhase::Error, 3),
            ('3', p::SandboxPhase::Starting, 2),
        ] {
            fixture.gateway.set_status(
                WORKSPACE,
                &name,
                p::SandboxStatus {
                    phase: phase as i32,
                    exit_code: Some(101),
                    restart_count: restarts,
                    ..Default::default()
                },
            );
            assert!(fixture.status(BASE).await.unwrap().settled());
            fixture.gateway.take_calls();
            fixture
                .apply(
                    BASE,
                    push(revision, "alice"),
                    fixture.prepared("FROM a", None),
                )
                .await
                .unwrap();
            let calls = fixture.gateway.take_calls();
            let [
                Call::RotateProviderCredential(_),
                Call::StopSandbox(_),
                Call::StartSandbox(_),
            ] = calls.as_slice()
            else {
                panic!("unexpected requests in {phase:?}: {calls:?}");
            };
            let status = fixture.status(BASE).await.unwrap();
            let sandbox = status.sandbox.clone().unwrap();
            assert_eq!((sandbox.phase, sandbox.restart_count), (Phase::Starting, 0));
            assert_eq!(status.applied_revision, Some(revision_of(revision)));
            assert_eq!(status.failed, None);
            assert!(
                !status.settled(),
                "the restarted program has not failed yet"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn a_failed_build_leaves_the_serving_instance_untouched() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status(BASE).await.unwrap().deployment;
        let name = sandbox_name(deployment);
        fixture
            .gateway
            .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
        served(&fixture.db.a, deployment).await;
        let token = fixture.token(deployment);
        fixture.gateway.take_calls();

        let error = fixture
            .apply(
                BASE,
                push('2', "bob"),
                fixture.prepared("FAIL: unknown instruction", None),
            )
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("FAIL: unknown instruction"),
            "{error:#}"
        );
        assert_eq!(fixture.gateway.take_calls(), []);
        let status = fixture.status(BASE).await.unwrap();
        let Some(Failure { stage, message }) = &status.failed else {
            panic!("the failed build is recorded");
        };
        assert_eq!(*stage, Stage::Build);
        assert!(message.contains("BuildKit failed the build"), "{message}");
        assert!(message.contains("FAIL: unknown instruction"), "{message}");
        assert_eq!(status.applied_revision, Some(revision_of('1')));
        assert_eq!(status.serving_revision, Some(revision_of('1')));
        assert_eq!(status.sandbox.as_ref().unwrap().phase, Phase::Ready);
        assert!(status.settled());
        assert_eq!(fixture.token(deployment), token);
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["failed"]["stage"], "build");

        // The next push that builds is applied and clears the failure.
        fixture
            .apply(BASE, push('3', "bob"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let status = fixture.status(BASE).await.unwrap();
        assert_eq!(status.failed, None);
        assert_eq!(status.applied_revision, Some(revision_of('3')));

        // A first push that fails to build leaves an instance without a sandbox that
        // shows why.
        fixture
            .apply(
                "dev/broken",
                push('1', "bob"),
                fixture.prepared("FAIL: no base image", None),
            )
            .await
            .unwrap_err();
        let broken = fixture.status("dev/broken").await.unwrap();
        assert_eq!(broken.failed.unwrap().stage, Stage::Build);
        assert_eq!((broken.sandbox, broken.applied_revision), (None, None));
        assert!(fixture.sandbox(broken.deployment).is_none());
    })
    .await;
}

#[tokio::test]
async fn a_failed_provider_creation_gets_a_new_token_on_the_next_push() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        fixture.gateway.refuse_next(
            "CreateProvider",
            tonic::Status::unavailable("the gateway is restarting"),
        );
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap_err();
        let status = fixture.status(BASE).await.unwrap();
        let deployment = status.deployment;
        let Some(Failure { stage, message }) = status.failed else {
            panic!("the failed creation is recorded");
        };
        assert_eq!(stage, Stage::Create);
        assert!(message.contains("the gateway is restarting"), "{message}");
        assert!(fixture.sandbox(deployment).is_none());
        let lost = token_digest(&fixture.db.a, deployment).await;
        fixture.gateway.take_calls();

        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [
            Call::DeleteProvider(_),
            Call::CreateProvider(_),
            Call::DeleteProvider(_),
            Call::CreateProvider(_),
            Call::ConfigureProviderRefresh(_),
            Call::RotateProviderCredential(_),
            Call::CreateSandbox(_),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        let token = fixture.token(deployment);
        let current = token_digest(&fixture.db.a, deployment).await;
        assert_eq!(current, digest(&token));
        assert_ne!(current, lost, "the token no provider received is replaced");
        let status = fixture.status(BASE).await.unwrap();
        assert_eq!(status.failed, None);
        assert_eq!(status.applied_revision, Some(revision_of('1')));
    })
    .await;
}

#[tokio::test]
async fn status_reports_the_sandbox_as_openshell_does() {
    bounded(async {
        let fixture = Fixture::start(json!(null)).await;
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status(BASE).await.unwrap().deployment;
        let name = sandbox_name(deployment);
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
        let status = fixture.status(BASE).await.unwrap();
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
        assert_eq!(json["deployment"], deployment.to_string());
        assert_eq!(json["workspace"], WORKSPACE);
        assert_eq!(json["sandboxName"], name);
        assert_eq!(json["sandbox"]["phase"], "provisioning");
        assert_eq!(json["sandbox"]["exitCode"], 101);
        assert_eq!(json["sandbox"]["restartCount"], 2);
        assert_eq!(json["sandbox"]["conditions"][0]["type"], "PodScheduled");
        assert_eq!(json["appliedRevision"], "1".repeat(64));
        assert_eq!(json["servingRevision"], serde_json::Value::Null);

        for (phase, health) in [
            (p::SandboxPhase::Ready, HubServerHealthState::Healthy),
            (p::SandboxPhase::Error, HubServerHealthState::Offline),
        ] {
            fixture.gateway.set_phase(WORKSPACE, &name, phase);
            let status = fixture.status(BASE).await.unwrap();
            assert_eq!(status.health().state, health);
        }
    })
    .await;
}

#[tokio::test]
async fn caps_warn_above_the_cap_and_refuse_when_enforced() {
    bounded(async {
        let fixture = Fixture::start(json!({"instancesPerUser": 1})).await;
        for (branch, pusher) in [
            ("dev/alice/a", "alice"),
            ("dev/alice/b", "alice"),
            ("dev/bob/a", "bob"),
        ] {
            fixture
                .apply(branch, push('1', pusher), fixture.prepared("FROM a", None))
                .await
                .unwrap();
        }
        let fixture = &fixture;
        let warning = |branch| async move { fixture.status(branch).await.unwrap().warning };
        assert_eq!(warning("dev/alice/a").await, None);
        assert_eq!(
            warning("dev/alice/b").await.as_deref(),
            Some("alice is the latest pusher of 2 instances, above the cap of 1")
        );
        assert_eq!(warning("dev/bob/a").await, None);

        let enforced = fixture.with_caps(json!({"instancesPerUser": 1, "enforce": true}));
        let error = enforced
            .apply(
                REPOSITORY,
                &branch_name("dev/alice/c"),
                push('1', "alice"),
                fixture.prepared("FROM a", None),
            )
            .await
            .unwrap_err();
        let message = "alice is the latest pusher of 3 instances, above the cap of 1";
        assert_eq!(error.to_string(), message);
        let status = fixture.status("dev/alice/c").await.unwrap();
        assert_eq!(
            status.failed,
            Some(Failure {
                stage: Stage::Capacity,
                message: message.into()
            })
        );
        assert_eq!((status.sandbox, status.applied_revision), (None, None));
        assert!(fixture.sandbox(status.deployment).is_none());
    })
    .await;
}

#[tokio::test]
async fn removing_an_instance_deletes_its_sandbox_providers_registrations_and_database() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let installation = tempfile::tempdir().unwrap();
        let hub = hub::Hub::start(installation.path(), &fixture.db, Duration::from_secs(60)).await;
        let http = reqwest::Client::new();
        let store = &fixture.db.a;
        fixture
            .apply(
                "feature",
                push('1', "alice"),
                fixture.prepared("FROM a", None),
            )
            .await
            .unwrap();
        let deployment = fixture.status("feature").await.unwrap().deployment;
        let name = sandbox_name(deployment);
        let environment = instance_database(store, deployment).await;
        assert_eq!(
            environment,
            DatabaseName::of_branch(&branch_name("feature"))
        );
        // The fixture server stands in for the sandbox's server and registers with the
        // token of the instance's provider.
        let served = servers::serve_instance(
            &hub,
            &http,
            "ledger",
            ledger::authorization().unwrap(),
            &fixture.instance(deployment),
            async |registration, _, _| {
                servers::hosted::<LedgerDomain>(registration, &http)
                    .handler(|| ledger::hosted(LedgerTasks::default()))
                    .build()
            },
        )
        .await;
        fixture
            .gateway
            .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
        assert_eq!(
            fixture.status("feature").await.unwrap().serving_revision,
            Some(revision_of('1')),
            "the registration serves the applied revision"
        );
        let registered = logins(store, deployment).await;
        assert_eq!(registered.len(), 1);
        assert!(role_exists(store, &registered[0]).await);
        fixture.gateway.take_calls();

        fixture
            .factory
            .remove(REPOSITORY, &branch_name("feature"))
            .await
            .unwrap();
        let calls = fixture.gateway.take_calls();
        let [
            Call::DeleteSandbox(sandbox),
            Call::DeleteProvider(provider),
            Call::DeleteProvider(lore),
        ] = calls.as_slice()
        else {
            panic!("unexpected requests: {calls:?}");
        };
        assert_eq!(
            (sandbox.workspace_scope.clone(), sandbox.name.as_str()),
            (scope(), name.as_str())
        );
        assert_eq!(provider.name, format!("{name}-registration"));
        assert_eq!(lore.name, format!("{name}-lore"));
        assert!(fixture.sandbox(deployment).is_none());
        assert!(fixture.gateway.providers(WORKSPACE).is_empty());
        assert!(logins(store, deployment).await.is_empty());
        assert!(!role_exists(store, &registered[0]).await);
        assert!(fixture.status("feature").await.is_none());
        assert!(!database_exists(store, &environment).await);
        // Removing what is gone changes nothing.
        fixture
            .factory
            .remove(REPOSITORY, &branch_name("feature"))
            .await
            .unwrap();
        assert_eq!(fixture.gateway.take_calls(), []);
        drop(served);
    })
    .await;
}

/// The Hub binary with the factory on the fake gateway, over TLS, and a Lore grant on
/// the repository for its `member` caller only.
async fn factory_hub(fixture: &Fixture, directory: &Path) -> hub::Hub {
    let (gateway, authority) = fixture.gateway.serve_tls().await;
    let authority_file = directory.join("gateway-ca.pem");
    std::fs::write(&authority_file, authority).unwrap();
    let factory = configuration_json(
        fixture.directory.path(),
        gateway.as_str(),
        &authority_file,
        json!(null),
    );
    let installation = directory.join("installation");
    std::fs::create_dir(&installation).unwrap();
    hub::Hub::start_with(
        &installation,
        &fixture.db,
        Duration::from_secs(60),
        &[(GATEWAY_BEARER, "synthetic-gateway-bearer".to_owned())],
        |config| {
            let issuer = config["authorizationServers"][0]["issuer"]
                .as_str()
                .unwrap()
                .to_owned();
            config["execution"]["baseBranch"] = json!(BASE);
            config["nativeUpstreams"] = json!({
                "gateway": gateway.as_str(),
                "certificateAuthority": authority_file,
                "bearerEnvironment": GATEWAY_BEARER,
                "bindings": [],
            });
            config["factory"] = factory;
            config["lore"] = json!({
                "deployments": [{
                    "id": "source", "resourceName": "native-source",
                    "audience": "https://lore.internal/native", "endpoint": LORE,
                    "authorizationServer": "local", "policyVersion": "local",
                    "loginScopes": ["native:login", "offline_access"],
                    "scopeTemplate": "source-{id}", "resourceIdTemplate": "{id}",
                    "tokenLifetimeSeconds": 120,
                }],
                "repositories": [
                    {"deployment": "source", "id": REPOSITORY, "tenant": "local", "title": "Served"},
                ],
                "assignments": [{
                    "deployment": "source", "repository": REPOSITORY, "tenant": "local",
                    "principal": format!("{issuer}#acceptance-member"), "actions": [],
                }],
            });
        },
    )
    .await
}

#[tokio::test]
async fn the_status_route_waits_for_the_applied_revision_to_serve_or_fail() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let directory = tempfile::tempdir().unwrap();
        let hub = factory_hub(&fixture, directory.path()).await;
        let http = reqwest::Client::new();
        fixture
            .apply(BASE, push('1', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status(BASE).await.unwrap().deployment;
        let name = sandbox_name(deployment);

        let member = hub.access_token(&http, hub.caller("member")).await;
        let admin = hub.access_token(&http, hub.caller("admin")).await;
        let (code, body) = get_status(&http, &hub.base, &admin, BASE, 0).await;
        assert_eq!(code, reqwest::StatusCode::FORBIDDEN, "{body}");
        assert_eq!(
            body["type"],
            "https://veoveo.ai/problems/instance-forbidden"
        );
        let (code, body) = get_status(&http, &hub.base, &member, "dev/none", 0).await;
        assert_eq!(code, reqwest::StatusCode::NOT_FOUND, "{body}");
        assert!(
            body["detail"]
                .as_str()
                .unwrap()
                .ends_with("; nobody watches the repository"),
            "{body}"
        );
        let (code, body) = get_status(&http, &hub.base, &member, BASE, 0).await;
        assert_eq!(code, reqwest::StatusCode::OK, "{body}");
        assert_eq!(body["deployment"], deployment.to_string());
        assert_eq!(body["sandboxName"], name);
        assert_eq!(body["workspace"], WORKSPACE);
        assert_eq!(body["sandbox"]["phase"], "provisioning");
        assert_eq!(body["appliedRevision"], "1".repeat(64));
        assert_eq!(body["servingRevision"], serde_json::Value::Null);
        // Nobody watches the repository and no push failed to be prepared.
        assert_eq!(body["watch"], serde_json::Value::Null);
        assert_eq!(body["prepareFailure"], serde_json::Value::Null);

        // The wait ends once the sandbox is ready and its server registered.
        let started = Instant::now();
        let waiting = tokio::spawn({
            let (base, http, member) = (hub.base.clone(), http.clone(), member.clone());
            async move { get_status(&http, &base, &member, BASE, 30).await }
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!waiting.is_finished(), "nothing serves yet");
        fixture
            .gateway
            .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
        let served = servers::serve_instance(
            &hub,
            &http,
            "ledger",
            ledger::authorization().unwrap(),
            &fixture.instance(deployment),
            async |registration, _, _| {
                servers::hosted::<LedgerDomain>(registration, &http)
                    .handler(|| ledger::hosted(LedgerTasks::default()))
                    .build()
            },
        )
        .await;
        let (code, body) = waiting.await.unwrap();
        assert_eq!(code, reqwest::StatusCode::OK, "{body}");
        assert_eq!(body["servingRevision"], "1".repeat(64));
        assert_eq!(body["sandbox"]["phase"], "ready");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{:?}",
            started.elapsed()
        );

        // A push restarts the sandbox, and the wait ends when its program exits, as a
        // broken push's would.
        fixture
            .apply(BASE, push('2', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let started = Instant::now();
        let waiting = tokio::spawn({
            let (base, http, member) = (hub.base.clone(), http.clone(), member.clone());
            async move { get_status(&http, &base, &member, BASE, 30).await }
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            !waiting.is_finished(),
            "the restarted program has not exited"
        );
        fixture.gateway.set_status(
            WORKSPACE,
            &name,
            p::SandboxStatus {
                phase: p::SandboxPhase::Starting as i32,
                exit_code: Some(101),
                restart_count: 1,
                ..Default::default()
            },
        );
        let (code, body) = waiting.await.unwrap();
        assert_eq!(code, reqwest::StatusCode::OK, "{body}");
        assert_eq!(body["appliedRevision"], "2".repeat(64));
        assert_eq!(body["servingRevision"], serde_json::Value::Null);
        assert_eq!(body["sandbox"]["exitCode"], 101);
        assert_eq!(body["sandbox"]["restartCount"], 1);
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{:?}",
            started.elapsed()
        );

        // The fix is pushed; the restarted server resumes its saved registration, as the
        // SDK does on start, and the wait ends.
        fixture
            .apply(BASE, push('3', "alice"), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let waiting = tokio::spawn({
            let (base, http, member) = (hub.base.clone(), http.clone(), member.clone());
            async move { get_status(&http, &base, &member, BASE, 30).await }
        });
        fixture
            .gateway
            .set_phase(WORKSPACE, &name, p::SandboxPhase::Ready);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!waiting.is_finished(), "the server has not resumed");
        served.client.read(&served.registration).await.unwrap();
        let (code, body) = waiting.await.unwrap();
        assert_eq!(code, reqwest::StatusCode::OK, "{body}");
        assert_eq!(body["servingRevision"], "3".repeat(64));
        assert_eq!(body["failed"], serde_json::Value::Null);
        drop(served);
    })
    .await;
}

/// `GET /factory/local/instance` with `token` for `branch` of the repository.
async fn get_status(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    branch: &str,
    wait: u64,
) -> (reqwest::StatusCode, serde_json::Value) {
    let mut url = url::Url::parse(&format!("{base}/factory/local/instance")).unwrap();
    url.query_pairs_mut()
        .append_pair("repository", REPOSITORY)
        .append_pair("branch", branch)
        .append_pair("wait", &wait.to_string());
    let response = http.get(url).bearer_auth(token).send().await.unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
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

async fn instance_database(store: &PlatformStore, deployment: DeploymentId) -> DatabaseName {
    let database =
        sqlx::query_file_scalar!("tests/factory/instance_database.sql", deployment.as_uuid())
            .fetch_one(store.pool())
            .await
            .unwrap();
    DatabaseName::parse(database).unwrap()
}

async fn token_digest(store: &PlatformStore, deployment: DeploymentId) -> String {
    sqlx::query_file_scalar!("tests/factory/token_digest.sql", deployment.as_uuid())
        .fetch_one(store.pool())
        .await
        .unwrap()
}

/// What the Hub records when the instance's server registers or resumes.
async fn served(store: &PlatformStore, deployment: DeploymentId) {
    sqlx::query_file!("tests/factory/served.sql", deployment.as_uuid())
        .execute(store.pool())
        .await
        .unwrap();
}

/// A Lore deployment whose repository Alice and Bob may use, and its public client.
fn native_section(config: &mut serde_json::Value) {
    config["lore"] = json!({
        "deployments": [{
            "id": "source", "resourceName": "native-source", "audience": NATIVE_RESOURCE,
            "endpoint": LORE, "authorizationServer": "local",
            "policyVersion": "local", "loginScopes": ["native:login", "offline_access"],
            "scopeTemplate": "source-{id}", "resourceIdTemplate": "{id}",
            "tokenLifetimeSeconds": 120
        }],
        "repositories": [
            {"deployment": "source", "id": REPOSITORY, "tenant": "local", "title": "Served"}
        ],
        "assignments": [
            {"deployment": "source", "repository": REPOSITORY, "tenant": "local", "principal": ALICE, "actions": []},
            {"deployment": "source", "repository": REPOSITORY, "tenant": "local", "principal": BOB, "actions": []}
        ]
    });
    config["oauthClients"].as_array_mut().unwrap().push(json!({
        "id": "native-cli", "authorizationServer": "local", "defaultWorkContext": "local",
        "invocationMode": "direct", "allowedResources": [NATIVE_RESOURCE],
        "grantTypes": ["device_authorization", "refresh_token", "token_exchange"],
        "authMethods": ["none"], "allowedScopes": ["native:login", "offline_access"]
    }));
}

const NATIVE_RESOURCE: &str = "https://native.example/source";
const ALICE: &str = "https://identity.example#alice";
const BOB: &str = "https://identity.example#bob";

/// The claims of a token, as the Lore server reads them after verifying it.
fn claims(token: &str) -> serde_json::Value {
    use base64::Engine as _;
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .unwrap(),
    )
    .unwrap()
}

/// A token request of an instance's client, as the gateway's refresh sends it.
async fn refresh(
    http: &reqwest::Client,
    hub: &hub::Hub,
    pairs: &[(&str, &str)],
) -> (reqwest::StatusCode, serde_json::Value) {
    let form = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().copied())
        .finish();
    let response = http
        .post(format!("{}/oauth/token", hub.base))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}

#[tokio::test]
async fn an_instances_lore_token_acts_as_its_latest_pusher_under_their_grant() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let installation = tempfile::tempdir().unwrap();
        let hub = hub::Hub::start_with(
            installation.path(),
            &fixture.db,
            Duration::from_secs(60),
            &[],
            native_section,
        )
        .await;
        fixture
            .gateway
            .set_token_url(format!("{}/oauth/token", hub.base));
        let branch = "dev/alice/x";
        let lore = |deployment| format!("{}-lore", sandbox_name(deployment));
        let token = |deployment| {
            fixture
                .gateway
                .credentials(WORKSPACE, &lore(deployment))
                .unwrap()["LORE_ACCESS_TOKEN"]
                .clone()
        };

        // The first push mints the instance's first token, for its pusher, before its
        // sandbox starts.
        fixture
            .apply(branch, push('1', ALICE), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status(branch).await.unwrap().deployment;
        let first = claims(&token(deployment));
        assert_eq!(first["identity"], ALICE);
        assert_eq!(first["aud"], NATIVE_RESOURCE);
        assert_eq!(
            first["resources"],
            json!([{"resource_id": REPOSITORY, "permission": []}])
        );
        assert_eq!(first["root_domains"], json!(["lore.internal"]));

        // Each push refreshes the token for its pusher.
        fixture
            .apply(branch, push('2', BOB), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        assert_eq!(claims(&token(deployment))["identity"], BOB);

        // A pusher who lost the grant gets no token; the push is recorded as failed.
        let mut plane = hub.control_plane().await;
        plane["lore"]["assignments"]
            .as_array_mut()
            .unwrap()
            .retain(|assignment| assignment["principal"] != BOB);
        hub.publish_control_plane(&plane).await;
        let material = fixture
            .gateway
            .refresh_material(WORKSPACE, &lore(deployment), "LORE_ACCESS_TOKEN")
            .unwrap();
        let http = reqwest::Client::new();
        let client = [
            ("grant_type", "client_credentials"),
            ("client_id", material["client_id"].as_str()),
            ("client_secret", material["client_secret"].as_str()),
        ];
        assert_eq!(material["client_id"], deployment.to_string());
        let (status, refused) = refresh(&http, &hub, &client).await;
        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
        assert_eq!(refused["error"], "invalid_grant");
        assert!(
            fixture
                .apply(branch, push('3', BOB), fixture.prepared("FROM a", None))
                .await
                .is_err()
        );
        let Some(Failure { stage, message }) = fixture.status(branch).await.unwrap().failed else {
            panic!("the refused refresh is recorded");
        };
        assert_eq!(stage, Stage::Restart);
        assert!(
            message.contains("refresh the instance's Lore token")
                && message.contains("invalid_grant"),
            "{message}"
        );

        // A push by someone who holds the grant serves again.
        fixture
            .apply(branch, push('4', ALICE), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        assert_eq!(claims(&token(deployment))["identity"], ALICE);

        // The client authenticates with its own secret and requests nothing else.
        let wrong = [
            ("grant_type", "client_credentials"),
            ("client_id", material["client_id"].as_str()),
            ("client_secret", "another-secret"),
        ];
        assert_eq!(
            refresh(&http, &hub, &wrong).await.1["error"],
            "invalid_client"
        );
        let scoped = [client.as_slice(), &[("scope", "native:login")]].concat();
        assert_eq!(
            refresh(&http, &hub, &scoped).await.1["error"],
            "invalid_scope"
        );
        let (status, issued) = refresh(&http, &hub, &client).await;
        assert_eq!(status, reqwest::StatusCode::OK);
        assert_eq!(issued["scope"], format!("source-{REPOSITORY}"));
        assert_eq!(
            claims(issued["access_token"].as_str().unwrap())["identity"],
            ALICE
        );

        // A new image gives the client a new secret with its new sandbox, and the client
        // goes with the instance.
        fixture
            .apply(branch, push('5', ALICE), fixture.prepared("FROM b", None))
            .await
            .unwrap();
        assert_eq!(
            refresh(&http, &hub, &client).await.1["error"],
            "invalid_client"
        );
        let renewed = fixture
            .gateway
            .refresh_material(WORKSPACE, &lore(deployment), "LORE_ACCESS_TOKEN")
            .unwrap();
        let renewed = [
            ("grant_type", "client_credentials"),
            ("client_id", renewed["client_id"].as_str()),
            ("client_secret", renewed["client_secret"].as_str()),
        ];
        assert_eq!(
            refresh(&http, &hub, &renewed).await.0,
            reqwest::StatusCode::OK
        );
        fixture
            .factory
            .remove(REPOSITORY, &branch_name(branch))
            .await
            .unwrap();
        assert_eq!(
            refresh(&http, &hub, &renewed).await.1["error"],
            "invalid_client"
        );
    })
    .await;
}

/// A second repository of the Lore deployment, which only Alice may use.
const SECOND: &str = "fedcba9876543210fedcba9876543210";
/// A person with no Lore assignment.
const CAROL: &str = "https://identity.example#carol";

/// The digest the Hub keeps of a Computer's Lore client secret.
fn secret_digest(secret: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(secret.as_bytes()).into())
}

/// The Computer of `owner` with its own Lore client's `secret`, as the beginning of a
/// custody handoff records it. Its provisioning defaults are synthetic: no native
/// workload exists.
async fn computer(store: &HubStore, owner: &str, secret: &str) -> Computer {
    let gateway = json!({"endpoint": "https://gateway.example/", "certificateAuthority": "/unused/ca", "bearerEnvironment": "UNUSED"});
    computer_on(store, owner, secret, gateway).await.0
}

/// [`computer`] provisioned on `gateway`, and its pending handoff.
async fn computer_on(
    store: &HubStore,
    owner: &str,
    secret: &str,
    gateway: serde_json::Value,
) -> (Computer, uuid::Uuid) {
    let provisioning: ComputerProvisionSpec = serde_json::from_value(json!({
        "config": {
            "gateway": gateway,
            "template": {"workspace": "installation", "name": "computer"},
            "policyFile": "/unused/policy", "command": ["/unused/app"],
            "oauthClient": "computer-browser", "profile": "computer", "workContext": "local", "port": 4096,
            "hubProvider": {"profile": "vv-computer-hub", "credentialKey": "COMPUTER_HUB_REFERENCE"},
            "modelProvider": {"profile": "vv-computer-model", "credentialKey": "DEEPINFRA_API_KEY", "secret": "unused"},
            "loreProvider": {"profile": "vv-lore", "credentialKey": "LORE_ACCESS_TOKEN"}
        },
        "modelSecret": {
            "id": "unused", "source": "env", "purpose": "provider_api_key",
            "locator": "UNUSED_FIXTURE_KEY", "owner": {"kind": "hub"}, "metadata": {}
        },
        "hubUrl": "https://hub.example/mcp/computer",
        "backend": "kubernetes",
        "policy": [],
        "template": []
    }))
    .unwrap();
    let computer = store
        .ensure_computer(
            &TenantId::parse("local").unwrap(),
            &PrincipalId::parse(owner).unwrap(),
            &provisioning,
        )
        .await
        .unwrap();
    let handoff = store
        .begin_computer_handoff(&computer, &secret_digest(secret))
        .await
        .unwrap();
    (computer, handoff)
}

#[tokio::test]
async fn a_computers_lore_token_acts_as_its_owner_for_every_assigned_repository() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let installation = tempfile::tempdir().unwrap();
        let hub = hub::Hub::start_with(
            installation.path(),
            &fixture.db,
            Duration::from_secs(60),
            &[],
            |config| {
                native_section(config);
                config["lore"]["repositories"].as_array_mut().unwrap().push(
                    json!({"deployment": "source", "id": SECOND, "tenant": "local", "title": "Second"}),
                );
                config["lore"]["assignments"].as_array_mut().unwrap().push(json!({
                    "deployment": "source", "repository": SECOND, "tenant": "local",
                    "principal": ALICE, "actions": ["owner"]
                }));
            },
        )
        .await;
        let store = HubStore::new(fixture.db.a.clone());
        let http = reqwest::Client::new();
        let request = |client: &str, secret: &str| {
            [
                ("grant_type", "client_credentials".to_owned()),
                ("client_id", client.to_owned()),
                ("client_secret", secret.to_owned()),
            ]
        };
        let issue = async |client: &str, secret: &str, extra: &[(&str, &str)]| {
            let pairs = request(client, secret);
            let mut form = pairs
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect::<Vec<_>>();
            form.extend_from_slice(extra);
            refresh(&http, &hub, &form).await
        };
        let alice = computer(&store, ALICE, "alice-secret").await;
        let bob = computer(&store, BOB, "bob-secret").await;
        let carol = computer(&store, CAROL, "carol-secret").await;

        // One token as the owner, with one resource entry per assigned repository.
        let (status, issued) = issue(&alice.native_name(), "alice-secret", &[]).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{issued}");
        assert_eq!(
            issued["scope"],
            format!("source-{REPOSITORY} source-{SECOND}")
        );
        let token = claims(issued["access_token"].as_str().unwrap());
        assert_eq!(token["identity"], ALICE);
        assert_eq!(token["aud"], NATIVE_RESOURCE);
        assert_eq!(token["root_domains"], json!(["lore.internal"]));
        assert_eq!(
            token["resources"],
            json!([
                {"resource_id": REPOSITORY, "permission": []},
                {"resource_id": SECOND, "permission": ["owner"]}
            ])
        );
        assert_eq!(token["https://veoveo.ai/claims/token-purpose"], "resource");
        let (_, issued) = issue(&bob.native_name(), "bob-secret", &[]).await;
        assert_eq!(
            claims(issued["access_token"].as_str().unwrap())["resources"],
            json!([{"resource_id": REPOSITORY, "permission": []}])
        );

        // An owner without assignments gets a valid token without resources.
        let (status, issued) = issue(&carol.native_name(), "carol-secret", &[]).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{issued}");
        let token = claims(issued["access_token"].as_str().unwrap());
        assert_eq!(token["identity"], CAROL);
        assert_eq!(token["resources"], json!([]));
        assert_eq!(token["https://veoveo.ai/claims/token-purpose"], "resource");

        // The client authenticates with its own secret and requests nothing else.
        for (client, secret) in [
            (alice.native_name(), "another-secret"),
            (alice.native_name(), "bob-secret"),
        ] {
            assert_eq!(
                issue(&client, secret, &[]).await.1["error"],
                "invalid_client"
            );
        }
        let alice_client = alice.native_name();
        let scoped = issue(&alice_client, "alice-secret", &[("scope", "native:login")]);
        assert_eq!(scoped.await.1["error"], "invalid_scope");
        let targeted = issue(&alice_client, "alice-secret", &[("resource", NATIVE_RESOURCE)]);
        assert_eq!(targeted.await.1["error"], "invalid_target");

        // A later handoff revokes the earlier secret.
        store
            .begin_computer_handoff(&alice, &secret_digest("alice-renewed"))
            .await
            .unwrap();
        assert_eq!(
            issue(&alice_client, "alice-secret", &[]).await.1["error"],
            "invalid_client"
        );
        assert_eq!(
            issue(&alice_client, "alice-renewed", &[]).await.0,
            reqwest::StatusCode::OK
        );

        // An instance's client and a Computer's are never taken for each other.
        fixture
            .gateway
            .set_token_url(format!("{}/oauth/token", hub.base));
        fixture
            .apply("dev/alice/y", push('1', ALICE), fixture.prepared("FROM a", None))
            .await
            .unwrap();
        let deployment = fixture.status("dev/alice/y").await.unwrap().deployment;
        let material = fixture
            .gateway
            .refresh_material(
                WORKSPACE,
                &format!("{}-lore", sandbox_name(deployment)),
                "LORE_ACCESS_TOKEN",
            )
            .unwrap();
        let instance = deployment.to_string();
        assert_eq!(
            issue(&instance, "alice-renewed", &[]).await.1["error"],
            "invalid_client"
        );
        assert_eq!(
            issue(&alice_client, &material["client_secret"], &[]).await.1["error"],
            "invalid_client"
        );
        let (status, issued) = issue(&instance, &material["client_secret"], &[]).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{issued}");
        assert_eq!(issued["scope"], format!("source-{REPOSITORY}"));
        assert_eq!(
            claims(issued["access_token"].as_str().unwrap())["resources"],
            json!([{"resource_id": REPOSITORY, "permission": []}])
        );
    })
    .await;
}

/// A ready Computer of `owner` on the fake gateway: its custody handoff completed and
/// configured its Lore provider's refresh there with its own client's `secret`.
async fn ready_computer(
    fixture: &Fixture,
    store: &HubStore,
    gateway: &serde_json::Value,
    owner: &str,
    secret: &str,
) -> Computer {
    let (computer, handoff) = computer_on(store, owner, secret, gateway.clone()).await;
    let principal = Principal {
        id: PrincipalId::parse(owner).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://identity.example").unwrap(),
        subject: TokenSubject::parse(owner.rsplit('#').next().unwrap()).unwrap(),
        tenant: Some(TenantId::parse("local").unwrap()),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: Default::default(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let family = HubState::new(fixture.db.a.clone())
        .issue_refresh_token(HubRefreshIssueRequest {
            authorization_server: &AuthorizationServerId::parse("local").unwrap(),
            protected_resource: &ProtectedResourceId::parse("https://hub.example/mcp/computer")
                .unwrap(),
            oauth_client_id: &OAuthClientId::parse("computer-browser").unwrap(),
            work_context: &WorkContextId::parse("local").unwrap(),
            principal: &principal,
            principal_display_name: &PrincipalDisplayName::new("Owner").unwrap(),
            scopes: &Default::default(),
            now: chrono::Utc::now(),
        })
        .await
        .unwrap()
        .grant
        .family_id;
    store
        .lock_computer_handoff(&computer, handoff)
        .await
        .unwrap()
        .unwrap()
        .complete(&family, &secret_digest(&format!("{secret}-cookie")))
        .await
        .unwrap();
    let name = computer.native_name();
    let workspace = Some(p::workspace_selector(name.clone()));
    let mut grpc = fixture.client.raw_grpc_fresh().await.unwrap();
    grpc.create_provider(p::CreateProviderRequest {
        workspace_scope: workspace.clone(),
        provider: Some(p::Provider {
            metadata: Some(p::ObjectMeta {
                name: "computer-lore".into(),
                workspace: name.clone(),
                ..Default::default()
            }),
            r#type: "vv-lore".into(),
            ..Default::default()
        }),
        ..Default::default()
    })
    .await
    .unwrap();
    grpc.configure_provider_refresh(p::ConfigureProviderRefreshRequest {
        workspace_scope: workspace,
        provider: "computer-lore".into(),
        credential_key: "LORE_ACCESS_TOKEN".into(),
        strategy: p::ProviderCredentialRefreshStrategy::Oauth2ClientCredentials as i32,
        material: HashMap::from([
            ("client_id".into(), name),
            ("client_secret".into(), secret.into()),
        ]),
        secret_material_keys: vec!["client_secret".into()],
        ..Default::default()
    })
    .await
    .unwrap();
    computer
}

/// Wait until the Computer's Lore credential carries `resources`, as the gateway minted
/// it at the Hub.
async fn renewed(fixture: &Fixture, computer: &Computer, resources: serde_json::Value) {
    let name = computer.native_name();
    for _ in 0..100 {
        let token = fixture
            .gateway
            .credentials(&name, "computer-lore")
            .and_then(|credentials| credentials.get("LORE_ACCESS_TOKEN").cloned());
        if token.is_some_and(|token| claims(&token)["resources"] == resources) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{name}'s Lore credential never carried {resources}");
}

/// The workspaces whose provider credentials the Hub rotated since the last call.
fn rotated(fixture: &Fixture) -> Vec<String> {
    fixture
        .gateway
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::RotateProviderCredential(rotation) => {
                match rotation.workspace_scope?.selection? {
                    p::workspace_selector::Selection::Workspace(name) => Some(name),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_assignment_change_renews_its_owners_computer_lore_access_at_once() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let installation = tempfile::tempdir().unwrap();
        let (endpoint, authority) = fixture.gateway.serve_tls().await;
        let authority_file = installation.path().join("gateway-ca.pem");
        std::fs::write(&authority_file, authority).unwrap();
        let gateway = json!({
            "endpoint": endpoint.as_str(),
            "certificateAuthority": authority_file,
            "bearerEnvironment": GATEWAY_BEARER,
        });
        let store = HubStore::new(fixture.db.a.clone());
        let alice = ready_computer(&fixture, &store, &gateway, ALICE, "alice-secret").await;
        let bob = ready_computer(&fixture, &store, &gateway, BOB, "bob-secret").await;
        let _ = fixture.gateway.take_calls();

        // A Hub that starts on a revision whose activation no Hub claimed yet, such as
        // one imported while no Hub ran, renews the Computers of the owners it assigns.
        let hub_directory = installation.path().join("hub");
        std::fs::create_dir(&hub_directory).unwrap();
        let hub = hub::Hub::start_with(
            &hub_directory,
            &fixture.db,
            Duration::from_secs(60),
            &[(GATEWAY_BEARER, "synthetic-gateway-bearer".to_owned())],
            |config| {
                native_section(config);
                config["lore"]["repositories"].as_array_mut().unwrap().push(
                    json!({"deployment": "source", "id": SECOND, "tenant": "local", "title": "Second"}),
                );
            },
        )
        .await;
        fixture
            .gateway
            .set_token_url(format!("{}/oauth/token", hub.base));
        let mut started = Vec::new();
        for _ in 0..100 {
            started.extend(rotated(&fixture));
            if started.len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        started.sort();
        let mut both_computers = vec![alice.native_name(), bob.native_name()];
        both_computers.sort();
        assert_eq!(started, both_computers);

        let first = json!([{"resource_id": REPOSITORY, "permission": []}]);
        let both = json!([
            {"resource_id": REPOSITORY, "permission": []},
            {"resource_id": SECOND, "permission": []}
        ]);
        let assign = |plane: &mut serde_json::Value, principal: &str| {
            plane["lore"]["assignments"].as_array_mut().unwrap().push(json!({
                "deployment": "source", "repository": SECOND, "tenant": "local",
                "principal": principal, "actions": []
            }));
        };

        // An assignment published through the admin route renews its owner's Computer
        // at once, and only that one.
        let mut plane = hub.control_plane().await;
        assign(&mut plane, ALICE);
        hub.publish_control_plane(&plane).await;
        renewed(&fixture, &alice, both.clone()).await;
        assert_eq!(rotated(&fixture), [alice.native_name()]);

        // Its removal renews it too.
        plane["lore"]["assignments"]
            .as_array_mut()
            .unwrap()
            .retain(|assignment| assignment["repository"] != SECOND);
        hub.publish_control_plane(&plane).await;
        renewed(&fixture, &alice, first).await;
        assert_eq!(rotated(&fixture), [alice.native_name()]);

        // A publication that changes no assignment renews nothing: after it and one that
        // assigns Bob, only Bob's Computer renewed.
        plane["metadata"] = json!({"note": "unrelated"});
        hub.publish_control_plane(&plane).await;
        assign(&mut plane, BOB);
        hub.publish_control_plane(&plane).await;
        renewed(&fixture, &bob, both).await;
        assert_eq!(rotated(&fixture), [bob.native_name()]);
    })
    .await;
}

/// The answer to a repository creation by `caller` with `body`.
async fn create_repository(
    http: &reqwest::Client,
    hub: &hub::Hub,
    caller: &str,
    body: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let token = hub.access_token(http, hub.caller(caller)).await;
    let response = http
        .post(format!("{}/lore/local/repositories", hub.base))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}

#[tokio::test]
async fn a_person_creates_a_repository_and_owns_it_at_once() {
    bounded(async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let fixture = Fixture::start(json!(null)).await;
        let installation = tempfile::tempdir().unwrap();
        let (endpoint, authority) = fixture.gateway.serve_tls().await;
        let authority_file = installation.path().join("gateway-ca.pem");
        std::fs::write(&authority_file, authority).unwrap();
        let gateway = json!({
            "endpoint": endpoint.as_str(),
            "certificateAuthority": authority_file,
            "bearerEnvironment": GATEWAY_BEARER,
        });
        // The Hub trusts the fake Lore's authority as its platform roots.
        let (lore, lore_endpoint, lore_authority) = lore::FakeLore::serve().await;
        let lore_authority_file = installation.path().join("lore-ca.pem");
        std::fs::write(&lore_authority_file, lore_authority).unwrap();
        let hub_directory = installation.path().join("hub");
        std::fs::create_dir(&hub_directory).unwrap();
        let endpoint_text = lore_endpoint.to_string();
        let hub = hub::Hub::start_with(
            &hub_directory,
            &fixture.db,
            Duration::from_secs(60),
            &[
                (GATEWAY_BEARER, "synthetic-gateway-bearer".to_owned()),
                ("SSL_CERT_FILE", lore_authority_file.display().to_string()),
            ],
            |config| {
                native_section(config);
                config["lore"]["deployments"][0]["endpoint"] = json!(endpoint_text);
            },
        )
        .await;
        fixture
            .gateway
            .set_token_url(format!("{}/oauth/token", hub.base));
        let issuer = hub.control_plane().await["authorizationServers"][0]["issuer"]
            .as_str()
            .unwrap()
            .to_owned();
        let member = format!("{issuer}#acceptance-member");
        let store = HubStore::new(fixture.db.a.clone());
        let computer = ready_computer(&fixture, &store, &gateway, &member, "member-secret").await;
        let _ = fixture.gateway.take_calls();
        let http = reqwest::Client::new();

        // A person creates a repository: Lore records them as its creator, and the Hub
        // answers once their Computer's token carries it as theirs.
        let (status, created) =
            create_repository(&http, &hub, "member", json!({"name": "alpha"})).await;
        assert_eq!(status, reqwest::StatusCode::CREATED, "{created}");
        let id = created["id"].as_str().unwrap().to_owned();
        assert_eq!(created["name"], "alpha");
        assert_eq!(created["remote"], format!("{lore_endpoint}/{id}"));
        assert_eq!(lore.creator(&id), Some(member.clone()));
        let token = &lore.tokens()[0];
        assert_eq!(token["identity"], member);
        assert_eq!(
            token["resources"],
            json!([]),
            "a login token carries no repository"
        );
        assert_eq!(rotated(&fixture), [computer.native_name()]);
        let credential = fixture
            .gateway
            .credentials(&computer.native_name(), "computer-lore")
            .unwrap()["LORE_ACCESS_TOKEN"]
            .clone();
        let resources = claims(&credential)["resources"].clone();
        assert!(
            resources
                .as_array()
                .unwrap()
                .contains(&json!({"resource_id": id, "permission": ["owner"]})),
            "{resources}"
        );
        let owners = |plane: &serde_json::Value, id: &str| {
            plane["lore"]["assignments"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|a| a["repository"] == id)
                .map(|a| {
                    (
                        a["principal"].as_str().unwrap().to_owned(),
                        a["actions"].clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let plane = hub.control_plane().await;
        assert_eq!(owners(&plane, &id), [(member.clone(), json!(["owner"]))]);

        // Another person cannot claim it through its ID, and owns nothing of it.
        let (status, refused) =
            create_repository(&http, &hub, "admin", json!({"name": "alpha", "id": id})).await;
        assert_eq!(status, reqwest::StatusCode::CONFLICT, "{refused}");
        assert_eq!(
            refused["type"],
            "https://veoveo.ai/problems/repository-claimed"
        );
        assert_eq!(
            owners(&hub.control_plane().await, &id),
            [(member.clone(), json!(["owner"]))]
        );

        // Its creator calling again changes nothing.
        let (status, again) =
            create_repository(&http, &hub, "member", json!({"name": "alpha", "id": id})).await;
        assert_eq!(
            (status, again["id"].clone()),
            (reqwest::StatusCode::OK, json!(id))
        );

        // A repository Lore holds as the caller's creation but nobody registered, as
        // after a publication that failed, registers with its ID; another person's
        // does not.
        let unregistered = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";
        lore.seed(unregistered, "beta", &member);
        let (status, refused) = create_repository(
            &http,
            &hub,
            "admin",
            json!({"name": "beta", "id": unregistered}),
        )
        .await;
        assert_eq!(status, reqwest::StatusCode::CONFLICT, "{refused}");
        let (status, registered) = create_repository(
            &http,
            &hub,
            "member",
            json!({"name": "beta", "id": unregistered}),
        )
        .await;
        assert_eq!(status, reqwest::StatusCode::CREATED, "{registered}");
        assert_eq!(
            owners(&hub.control_plane().await, unregistered),
            [(member.clone(), json!(["owner"]))]
        );

        // Lore refuses a name another repository holds.
        let (status, taken) =
            create_repository(&http, &hub, "member", json!({"name": "alpha"})).await;
        assert_eq!(status, reqwest::StatusCode::CONFLICT, "{taken}");
        assert_eq!(
            taken["type"],
            "https://veoveo.ai/problems/repository-exists"
        );
    })
    .await;
}
