//! Factory sandboxes: each branch of a repository with `openshell/run`, built and served.
//! An instance runs in one OpenShell sandbox, built from its environment image, with the
//! size and driver configuration of the installation's factory template, which mounts
//! Lore's command line ([`LORE`]). Its fixed start command fetches the instance's branch
//! onto the sandbox's persistent disk, and the branch's own `openshell/run` builds and
//! serves it; OpenShell restarts it whenever it exits. Each push is applied with
//! [`Factory::apply`]: the Hub ensures the branch's data environment, builds the image,
//! reads the factory template, then creates, restarts or recreates the sandbox, records
//! what it applied and why its own steps failed, and reads everything else from
//! OpenShell, including the image, policy, size and driver configuration a sandbox was
//! created with. A sandbox's configuration changes only with a new sandbox: the Hub
//! never updates it. [`Factory::remove`] ends an instance. The Hub runs no command inside
//! a sandbox.
pub mod image;
mod record;
mod sandbox;
mod status;

use std::{
    collections::HashMap,
    fmt,
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use openshell_sdk::{OpenShellClient, raw::proto as p};
use secrecy::ExposeSecret;
use serde::Serialize;
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId};

pub use sandbox::{LORE, sandbox_name};
pub use status::{Condition, Failure, InstanceStatus, Phase, SandboxStatus, Stage};

use crate::{
    HubCatalogHandle,
    control_plane::FactoryConfiguration,
    custody_client::CustodyClient,
    data_environment::{self, DatabaseName},
    execution::BranchName,
    registration,
};

/// How long one OpenShell request may take.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a sandbox may take to disappear after its deletion.
const TRANSITION_TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 3 } else { 120 });
/// How long a delete's answer is awaited. OpenShell answers a delete once the sandbox is
/// gone, which a Pod's stop can make longer than one request, and finishes a delete it
/// has taken even when the request ends; after this the sandbox's absence is awaited.
const DELETE_ANSWER: Duration = if cfg!(test) {
    Duration::from_millis(500)
} else {
    RPC_TIMEOUT
};
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// The longest a status read waits for an instance to serve or fail.
pub const MAX_WAIT: Duration = Duration::from_secs(60);

/// A Lore revision by its whole hash signature: 64 lowercase hexadecimal digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Revision(String);

impl Revision {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        ensure!(
            lowercase_hex(&value, 64),
            "a Lore revision hash signature has 64 lowercase hexadecimal digits"
        );
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A Lore repository ID: 32 lowercase hexadecimal digits.
fn repository_id(value: &str) -> Result<()> {
    ensure!(
        lowercase_hex(value, 32),
        "a Lore repository ID has 32 lowercase hexadecimal digits"
    );
    Ok(())
}

fn lowercase_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// One push of the instance's branch: the revision it applies and who pushed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    pub revision: Revision,
    pub pusher: PrincipalId,
}

/// What the pushed revision's `openshell/` holds, read as its pusher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// A checkout of the revision's `openshell/image/`: the environment image's
    /// `Dockerfile` and the files it copies.
    pub image_context: PathBuf,
    /// The revision's `openshell/policy.yaml`, which adds network rules to the
    /// installation's base policy.
    pub policy: Option<String>,
}

/// An instance's own OAuth client. The gateway refreshes the sandbox's Lore token with
/// it; the Hub issues each token for the instance's latest pusher and its repository.
/// Its identifier is the instance's deployment ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceClient {
    pub deployment: DeploymentId,
    /// The instance's latest pusher; none before its first push.
    pub pusher: Option<PrincipalId>,
    /// The Lore repository ID whose branch the instance serves.
    pub repository: String,
}

/// The instance whose client is `client` with this secret, or none when no instance's
/// client has both.
pub async fn instance_client(
    store: &PlatformStore,
    client: &str,
    secret: &str,
) -> Result<Option<InstanceClient>> {
    let Ok(deployment) = DeploymentId::parse(client) else {
        return Ok(None);
    };
    Ok(record::client_of(store, &registration::digest(secret))
        .await?
        .filter(|found| found.deployment == deployment))
}

/// What the instance's sandbox runs, from the pushed revision of its repository.
struct Source {
    /// The Lore repository the start command clones the instance's branch from.
    remote: url::Url,
    /// The environment image built from the revision's `openshell/image/`.
    image: image::PinnedImage,
    /// The installation's base policy with the network rules of the revision's
    /// `openshell/policy.yaml` added, canonical.
    policy: p::sandbox::v1::SandboxPolicy,
    /// The size and driver configuration of the installation's factory template.
    shape: sandbox::Shape,
}

/// The factory sandboxes of one installation, in its factory workspace.
#[derive(Clone)]
pub struct Factory {
    client: OpenShellClient,
    config: Arc<FactoryConfiguration>,
    /// The installation's base policy, canonical.
    policy: Arc<p::sandbox::v1::SandboxPolicy>,
    builder: Arc<dyn image::Build>,
    /// The installation's current configuration, which names the base branch.
    catalog: HubCatalogHandle,
    store: PlatformStore,
}

impl Factory {
    /// Connect to the configured gateway with its bearer reference.
    pub async fn connect(
        config: FactoryConfiguration,
        catalog: HubCatalogHandle,
        store: PlatformStore,
    ) -> Result<Self> {
        let client = crate::computers::connect_gateway(&config.gateway).await?;
        Self::new(config, catalog, client, store)
    }

    /// The factory on `client`, with the base policy read from the configured file and
    /// images built by the configured image builder.
    pub fn new(
        config: FactoryConfiguration,
        catalog: HubCatalogHandle,
        client: OpenShellClient,
        store: PlatformStore,
    ) -> Result<Self> {
        let policy =
            openshell_policy::parse_sandbox_policy_file(&config.policy_file).map_err(|error| {
                anyhow::anyhow!(
                    "the factory base policy {} is invalid: {error}",
                    config.policy_file.display()
                )
            })?;
        let policy = sandbox::policy(&policy, None)?;
        Ok(Self {
            client,
            builder: Arc::new(config.image_builder.clone()),
            config: Arc::new(config),
            policy: Arc::new(policy),
            catalog,
            store,
        })
    }

    /// The same factory building images with `builder`.
    pub fn with_builder(self, builder: Arc<dyn image::Build>) -> Self {
        Self { builder, ..self }
    }

    /// Apply a push of `branch` of the Lore repository `repository`, whose
    /// `openshell/` was read as `prepared`. Ensures the branch's data environment and
    /// the instance, builds the image, reads the factory template, then creates the
    /// sandbox on the first push, restarts it when it runs the built image under the
    /// revision's policy with the template's size and driver configuration and
    /// recreates it otherwise, and records the revision and its pusher. A failed step
    /// is recorded on the instance as `failed{stage, message}` and returned; a failed
    /// build, a repository policy the Hub refuses or a factory template it cannot use
    /// leaves the sandbox as it was. Pushes to one instance are applied one at a time
    /// by the caller.
    pub async fn apply(
        &self,
        repository: &str,
        branch: &BranchName,
        push: Push,
        prepared: Prepared,
    ) -> Result<()> {
        repository_id(repository)?;
        let (deployment, database) = match record::instance(&self.store, repository, branch).await?
        {
            Some(instance) => instance,
            None => {
                let database = self.database(branch)?;
                let instance =
                    registration::create_instance(&self.store, repository, &database, branch)
                        .await?;
                (instance.deployment, database)
            }
        };
        if !data_environment::is_installation(&self.store, &database) {
            self.recorded(
                deployment,
                Stage::Database,
                data_environment::ensure_database(&self.store, &database),
            )
            .await?;
        }
        let name = sandbox_name(deployment);
        let image = self
            .recorded(deployment, Stage::Build, async {
                let image = image::ImageName::parse(name.clone())?;
                Ok(self.builder.build(&prepared.image_context, &image).await?)
            })
            .await?;
        let policy = self
            .recorded(deployment, Stage::Create, async {
                sandbox::policy(&self.policy, prepared.policy.as_deref())
            })
            .await?;
        let shape = self
            .recorded(deployment, Stage::Create, self.shape())
            .await?;
        let mut remote = self.config.lore_server.clone();
        remote.set_path(&format!("/{repository}"));
        let source = Source {
            remote,
            image,
            policy,
            shape,
        };
        let branch = record::push(&self.store, deployment, &push.pusher).await?;
        let current = self
            .recorded(deployment, Stage::Create, self.sandbox(&name))
            .await?;
        match current {
            Some(sandbox) if sandbox::runs(&sandbox, &source) => {
                self.recorded(deployment, Stage::Restart, self.restart(&name, &sandbox))
                    .await?;
            }
            Some(_) => {
                self.recorded(deployment, Stage::Create, async {
                    self.delete_sandbox(&name).await?;
                    self.create(deployment, &branch, &source).await
                })
                .await?;
            }
            None => {
                self.admit(deployment, &push.pusher).await?;
                self.recorded(
                    deployment,
                    Stage::Create,
                    self.create(deployment, &branch, &source),
                )
                .await?;
            }
        }
        record::applied(&self.store, deployment, &push.revision).await
    }

    /// Stop serving `branch` of `repository`: delete the instance's sandbox and
    /// providers, then the instance and its registrations, then its data environment's
    /// database when no other instance names it. Nothing happens without an instance.
    pub async fn remove(&self, repository: &str, branch: &BranchName) -> Result<()> {
        repository_id(repository)?;
        match record::instance(&self.store, repository, branch).await? {
            Some((deployment, _)) => self.delete(deployment).await,
            None => Ok(()),
        }
    }

    /// The status of the instance of `branch` of `repository`, or none without one.
    /// With a nonzero `wait`, at most [`MAX_WAIT`], it returns as soon as the instance
    /// settles ([`InstanceStatus::settled`]), or at the end of the wait.
    pub async fn status(
        &self,
        repository: &str,
        branch: &BranchName,
        wait: Duration,
    ) -> Result<Option<InstanceStatus>> {
        repository_id(repository)?;
        let Some((deployment, _)) = record::instance(&self.store, repository, branch).await? else {
            return Ok(None);
        };
        let deadline = Instant::now() + wait.min(MAX_WAIT);
        loop {
            let Some(status) = self.instance_status(deployment).await? else {
                return Ok(None);
            };
            let now = Instant::now();
            if status.settled() || now >= deadline {
                return Ok(Some(status));
            }
            tokio::time::sleep(POLL_INTERVAL.min(deadline - now)).await;
        }
    }

    /// The instance's status: its sandbox as OpenShell reports it, and what the Hub
    /// last applied and why its last step failed.
    async fn instance_status(&self, deployment: DeploymentId) -> Result<Option<InstanceStatus>> {
        let Some(record) = record::read(&self.store, deployment).await? else {
            return Ok(None);
        };
        let name = sandbox_name(deployment);
        let found = self.sandbox(&name).await?;
        let sandbox_id = found
            .as_ref()
            .and_then(|sandbox| sandbox.metadata.as_ref())
            .map(|metadata| metadata.id.clone())
            .filter(|id| !id.is_empty());
        let sandbox = found.as_ref().map(SandboxStatus::of);
        let ready = sandbox
            .as_ref()
            .is_some_and(|sandbox| sandbox.phase == Phase::Ready);
        Ok(Some(InstanceStatus {
            deployment,
            workspace: self.config.workspace.clone(),
            sandbox_name: name,
            sandbox_id,
            sandbox,
            serving_revision: record.revision.clone().filter(|_| ready && record.served),
            applied_revision: record.revision,
            failed: record.failed,
            warning: record.warning,
        }))
    }

    /// The database of `branch`'s data environment: the installation database for the
    /// installation's current base branch, else the branch's own.
    fn database(&self, branch: &BranchName) -> Result<DatabaseName> {
        let catalog = self.catalog.current();
        let base = catalog
            .control_plane()
            .execution
            .as_ref()
            .and_then(|execution| execution.base_branch.as_ref());
        if base == Some(branch) {
            DatabaseName::installation(&self.store)
        } else {
            Ok(DatabaseName::of_branch(branch))
        }
    }

    /// Serve a push whose image and policy the sandbox runs: stop and start it, whose
    /// start command fetches the branch's latest revision. OpenShell stops a sandbox in
    /// any phase in which its program has run, including a crash loop, and resets its
    /// restart count; a sandbox still provisioning or starting for the first time has
    /// not run the start command yet and fetches the latest revision when it does, so
    /// it is left to start.
    async fn restart(&self, name: &str, sandbox: &p::Sandbox) -> Result<()> {
        self.refresh_lore(name).await?;
        let status = SandboxStatus::of(sandbox);
        let started = match status.phase {
            Phase::Provisioning => false,
            Phase::Starting => status.restart_count > 0,
            _ => true,
        };
        if !started {
            return Ok(());
        }
        // OpenShell answers once the sandbox stopped, and starts its program at once.
        let mut grpc = self.client.raw_grpc_fresh().await?;
        call(
            "stop the sandbox",
            grpc.stop_sandbox(p::StopSandboxRequest {
                workspace_scope: scope(&self.config.workspace),
                name: name.into(),
                ..Default::default()
            }),
        )
        .await?;
        call(
            "start the sandbox",
            grpc.start_sandbox(p::StartSandboxRequest {
                workspace_scope: scope(&self.config.workspace),
                name: name.into(),
                ..Default::default()
            }),
        )
        .await
    }

    /// Create the sandbox of an instance that has none. The instance gets a new
    /// registration token in a new registration provider first, so a sandbox always
    /// starts with a token the Hub holds, whatever became of an earlier provider: its
    /// server registers again with it whenever its disk holds no registration. Its
    /// client likewise gets a new secret in a new Lore provider, whose first token the
    /// gateway mints for the latest pusher before the sandbox starts.
    async fn create(
        &self,
        deployment: DeploymentId,
        branch: &BranchName,
        source: &Source,
    ) -> Result<()> {
        let name = sandbox_name(deployment);
        let token = registration::reissue_token(&self.store, deployment).await?;
        let mut grpc = self.client.raw_grpc_fresh().await?;
        let provider = sandbox::registration_provider(&name);
        call(
            "delete the previous registration provider",
            grpc.delete_provider(p::DeleteProviderRequest {
                workspace_scope: scope(&self.config.workspace),
                name: provider.clone(),
                allow_missing: true,
                ..Default::default()
            }),
        )
        .await?;
        call(
            "create the registration provider",
            grpc.create_provider(p::CreateProviderRequest {
                workspace_scope: scope(&self.config.workspace),
                provider: Some(p::Provider {
                    metadata: Some(p::ObjectMeta {
                        name: provider,
                        workspace: self.config.workspace.clone(),
                        ..Default::default()
                    }),
                    r#type: self.config.registration_profile.clone(),
                    credentials: HashMap::from([(
                        sandbox::REGISTRATION_SECRET.into(),
                        token.expose_secret().into(),
                    )]),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )
        .await?;
        self.lore_provider(deployment, &name).await?;
        self.refresh_lore(&name).await?;
        let mut grpc = self.client.raw_grpc_fresh().await?;
        call(
            "create the sandbox",
            grpc.create_sandbox(sandbox::create_request(
                &self.config,
                deployment,
                branch,
                source,
            )),
        )
        .await
    }

    /// Give the instance's client a new secret, in a new Lore provider without a
    /// credential: the gateway resolves the token at runtime, refreshing it with the
    /// client.
    async fn lore_provider(&self, deployment: DeploymentId, name: &str) -> Result<()> {
        let client = CustodyClient::new(deployment.to_string())?;
        record::client(&self.store, deployment, &client.digest()).await?;
        let provider = sandbox::lore_provider(name);
        let mut grpc = self.client.raw_grpc_fresh().await?;
        call(
            "delete the previous Lore provider",
            grpc.delete_provider(p::DeleteProviderRequest {
                workspace_scope: scope(&self.config.workspace),
                name: provider.clone(),
                allow_missing: true,
                ..Default::default()
            }),
        )
        .await?;
        call(
            "create the Lore provider",
            grpc.create_provider(p::CreateProviderRequest {
                workspace_scope: scope(&self.config.workspace),
                provider: Some(p::Provider {
                    metadata: Some(p::ObjectMeta {
                        name: provider.clone(),
                        workspace: self.config.workspace.clone(),
                        ..Default::default()
                    }),
                    r#type: self.config.lore_profile.clone(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )
        .await?;
        call(
            "configure the refresh of the Lore provider",
            grpc.configure_provider_refresh(client.refresh(
                scope(&self.config.workspace),
                provider,
                sandbox::LORE_TOKEN.into(),
            )),
        )
        .await
    }

    /// Refresh the instance's Lore token now, so that the sandbox's next start reads as
    /// the latest pusher instead of the pusher of the token the gateway holds.
    async fn refresh_lore(&self, name: &str) -> Result<()> {
        let mut grpc = self.client.raw_grpc_fresh().await?;
        call(
            "refresh the instance's Lore token",
            grpc.rotate_provider_credential(p::RotateProviderCredentialRequest {
                workspace_scope: scope(&self.config.workspace),
                provider: sandbox::lore_provider(name),
                credential_key: sandbox::LORE_TOKEN.into(),
                ..Default::default()
            }),
        )
        .await
    }

    /// What the installation's factory template gives the instance's sandbox, read now.
    async fn shape(&self) -> Result<sandbox::Shape> {
        let template = &self.config.template;
        let named = format!(
            "the factory template `{}` in workspace `{}`",
            template.name, template.workspace
        );
        let mut grpc = self.client.raw_grpc_fresh().await?;
        let answer = tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_sandbox_template(p::GetSandboxTemplateRequest {
                workspace_scope: scope(&template.workspace),
                name: template.name.clone(),
            }),
        )
        .await
        .with_context(|| format!("OpenShell did not answer a read of {named} in time"))?;
        let found = match answer {
            Ok(response) => response
                .into_inner()
                .template
                .with_context(|| format!("OpenShell answered without {named}"))?,
            Err(status) if status.code() == tonic::Code::NotFound => {
                anyhow::bail!("{named} does not exist")
            }
            Err(status) => return Err(refused(&format!("read {named}"), &status)),
        };
        sandbox::Shape::of(&found).with_context(|| format!("{named} is not usable"))
    }

    /// Check the per-user cap before a sandbox is created for an instance: above the
    /// cap, record a warning, or with `enforce` refuse and record the failure.
    async fn admit(&self, deployment: DeploymentId, pusher: &PrincipalId) -> Result<()> {
        let Some(caps) = &self.config.caps else {
            return Ok(());
        };
        let instances = record::pushed(&self.store, pusher).await?;
        let warning = (instances > u64::from(caps.instances_per_user)).then(|| {
            format!(
                "{pusher} is the latest pusher of {instances} instances, above the cap of {}",
                caps.instances_per_user
            )
        });
        if let (Some(message), true) = (&warning, caps.enforce) {
            record::failed(&self.store, deployment, Stage::Capacity, message).await?;
            anyhow::bail!("{message}");
        }
        record::warn(&self.store, deployment, warning.as_deref()).await
    }

    /// Delete an instance: its sandbox and providers, then the instance and its
    /// registrations, then its data environment's database when no other instance
    /// names it.
    async fn delete(&self, deployment: DeploymentId) -> Result<()> {
        let name = sandbox_name(deployment);
        self.recorded(deployment, Stage::Delete, async {
            self.delete_sandbox(&name).await?;
            let mut grpc = self.client.raw_grpc_fresh().await?;
            for provider in sandbox::providers(&name) {
                call(
                    "delete the provider",
                    grpc.delete_provider(p::DeleteProviderRequest {
                        workspace_scope: scope(&self.config.workspace),
                        name: provider,
                        allow_missing: true,
                        ..Default::default()
                    }),
                )
                .await?;
            }
            Ok(())
        })
        .await?;
        if let Some(database) = registration::remove_instance(&self.store, deployment).await? {
            data_environment::drop_database_if_unused(&self.store, &database).await?;
        }
        Ok(())
    }

    /// Delete the sandbox and wait until OpenShell no longer has it.
    async fn delete_sandbox(&self, name: &str) -> Result<()> {
        delete_sandbox(&self.client, &self.config.workspace, name).await
    }

    async fn sandbox(&self, name: &str) -> Result<Option<p::Sandbox>> {
        read_sandbox(&self.client, &self.config.workspace, name).await
    }

    /// Run one step for the instance; when it fails, record why before returning the
    /// error.
    async fn recorded<T>(
        &self,
        deployment: DeploymentId,
        stage: Stage,
        step: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let result = step.await;
        if let Err(error) = &result {
            let message = format!("{error:#}");
            tracing::warn!(%deployment, stage = stage.as_str(), "factory step failed: {message}");
            if let Err(error) = record::failed(&self.store, deployment, stage, &message).await {
                tracing::error!(%deployment, "failed to record the factory failure: {error:#}");
            }
        }
        result
    }
}

/// Delete the sandbox and wait until it is gone, for at most [`TRANSITION_TIMEOUT`];
/// past it, say what OpenShell still reports. An unanswered delete is no failure: see
/// [`DELETE_ANSWER`].
async fn delete_sandbox(client: &OpenShellClient, workspace: &str, name: &str) -> Result<()> {
    let mut grpc = client.raw_grpc_fresh().await?;
    let answer = tokio::time::timeout(
        DELETE_ANSWER,
        grpc.delete_sandbox(p::DeleteSandboxRequest {
            workspace_scope: scope(workspace),
            name: name.into(),
            allow_missing: true,
            ..Default::default()
        }),
    )
    .await;
    match answer {
        Ok(answer) => {
            answer.map_err(|status| refused("delete the sandbox", &status))?;
        }
        Err(_) => tracing::info!(
            sandbox = %name,
            "OpenShell has not answered the delete within {DELETE_ANSWER:?}; waiting for the sandbox to go"
        ),
    }
    let deadline = tokio::time::Instant::now() + TRANSITION_TIMEOUT;
    loop {
        let Some(sandbox) = read_sandbox(client, workspace, name).await? else {
            return Ok(());
        };
        if tokio::time::Instant::now() >= deadline {
            let pod = sandbox
                .status
                .as_ref()
                .map(|status| status.agent_pod.as_str())
                .filter(|pod| !pod.is_empty())
                .map(|pod| format!(" on Pod {pod}"))
                .unwrap_or_default();
            anyhow::bail!(
                "the sandbox did not disappear within {TRANSITION_TIMEOUT:?}; it is still{pod}: {}",
                serde_json::to_string(&status::SandboxStatus::of(&sandbox))?
            );
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn read_sandbox(
    client: &OpenShellClient,
    workspace: &str,
    name: &str,
) -> Result<Option<p::Sandbox>> {
    let mut grpc = client.raw_grpc_fresh().await?;
    let answer = tokio::time::timeout(
        RPC_TIMEOUT,
        grpc.get_sandbox(p::GetSandboxRequest {
            workspace_scope: scope(workspace),
            name: name.into(),
        }),
    )
    .await
    .context("OpenShell did not answer a sandbox read in time")?;
    match answer {
        Ok(response) => Ok(Some(
            response
                .into_inner()
                .sandbox
                .context("OpenShell answered without the sandbox")?,
        )),
        Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
        Err(status) => Err(refused("read the sandbox", &status)),
    }
}

fn scope(workspace: &str) -> Option<p::WorkspaceSelector> {
    Some(p::WorkspaceSelector {
        selection: Some(p::workspace_selector::Selection::Workspace(
            workspace.into(),
        )),
    })
}

/// Send one request to OpenShell and take its answer.
async fn call<T>(
    operation: &str,
    request: impl Future<Output = Result<tonic::Response<T>, tonic::Status>>,
) -> Result<()> {
    tokio::time::timeout(RPC_TIMEOUT, request)
        .await
        .with_context(|| format!("OpenShell did not answer in time to {operation}"))?
        .map_err(|status| refused(operation, &status))?;
    Ok(())
}

/// OpenShell's refusal, with its code and message, which name the cause.
fn refused(operation: &str, status: &tonic::Status) -> anyhow::Error {
    anyhow::anyhow!(
        "OpenShell refused to {operation}: {:?}: {}",
        status.code(),
        status.message()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_gateway as fake;

    const WORKSPACE: &str = "vv-factory";

    /// A fake gateway holding one ready sandbox named `name`.
    async fn gateway_with(name: &str) -> (fake::Gateway, OpenShellClient) {
        let (fake, client) = fake::Gateway::start().await;
        fake.ready_on_start();
        client
            .raw_grpc_fresh()
            .await
            .unwrap()
            .create_sandbox(p::CreateSandboxRequest {
                workspace_scope: scope(WORKSPACE),
                name: name.into(),
                ..Default::default()
            })
            .await
            .unwrap();
        (fake, client)
    }

    #[tokio::test]
    async fn a_delete_answered_after_its_deadline_waits_for_the_sandbox_to_go() {
        let (fake, client) = gateway_with("server").await;
        // As while the Pod stops, OpenShell answers once the sandbox is gone, three
        // times later than the factory awaits the answer.
        fake.delete_slowly(Some(Duration::from_millis(1500)));
        let started = std::time::Instant::now();
        delete_sandbox(&client, WORKSPACE, "server").await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(1500));
        assert!(fake.sandbox(WORKSPACE, "server").is_none());
    }

    #[tokio::test]
    async fn a_sandbox_that_does_not_go_fails_the_delete_with_what_openshell_reports() {
        let (fake, client) = gateway_with("server").await;
        fake.set_status(
            WORKSPACE,
            "server",
            p::SandboxStatus {
                phase: p::SandboxPhase::Ready as i32,
                agent_pod: "server-pod".into(),
                conditions: vec![p::SandboxCondition {
                    r#type: "Ready".into(),
                    status: "False".into(),
                    reason: "Terminating".into(),
                    message: "the workload has not exited".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        fake.delete_slowly(None);
        let error = delete_sandbox(&client, WORKSPACE, "server")
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "the sandbox did not disappear within 3s; it is still on Pod server-pod: \
             {\"phase\":\"deleting\",\"exitCode\":null,\"restartCount\":0,\"conditions\":\
             [{\"type\":\"Ready\",\"status\":\"False\",\"reason\":\"Terminating\",\
             \"message\":\"the workload has not exited\"}]}"
        );
    }

    #[test]
    fn revisions_are_whole_hash_signatures() {
        let signature = "0123456789abcdef".repeat(4);
        assert_eq!(
            Revision::parse(signature.clone()).unwrap().as_str(),
            signature
        );
        assert!(Revision::parse(signature.to_uppercase()).is_err());
        assert!(Revision::parse(&signature[1..]).is_err());
        assert!(Revision::parse("17").is_err());
    }
}
