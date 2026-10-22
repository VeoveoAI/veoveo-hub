//! Factory sandboxes: each branch of a repository with `openshell/run`, built and served.
//! An instance runs in one OpenShell sandbox, built from its environment image. Its fixed
//! start command fetches the instance's branch onto the sandbox's persistent disk, and
//! the branch's own `openshell/run` builds and serves it; OpenShell restarts it whenever
//! it exits. The Hub creates, restarts, recreates and deletes the sandbox, records what
//! it applied and why its own steps failed, and reads everything else from OpenShell. It
//! runs no command inside a sandbox.
pub mod image;
mod record;
mod sandbox;
mod status;

use std::{collections::HashMap, fmt, future::Future, sync::Arc, time::Duration};

use anyhow::{Context, Result, ensure};
use openshell_sdk::{OpenShellClient, raw::proto as p};
use secrecy::ExposeSecret;
use serde::Serialize;
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId};

pub use sandbox::sandbox_name;
pub use status::{Condition, Failure, InstanceStatus, Phase, SandboxStatus, Stage};

use crate::{
    control_plane::FactoryConfiguration,
    data_environment,
    registration::{self, NewInstance},
};

/// How long one OpenShell request may take.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a sandbox may take to stop or to disappear after its deletion.
const TRANSITION_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

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

/// What the instance's sandbox runs, from the pushed revision of its repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The Lore repository the start command clones the instance's branch from.
    pub repository: url::Url,
    /// The environment image built from the revision's `openshell/image/`.
    pub image: image::PinnedImage,
    /// The revision's `openshell/policy.yaml`, which adds network rules to the
    /// installation's base policy.
    pub policy: Option<String>,
}

/// How a push reached an existing instance's sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    /// The image is unchanged: the policy was set and the sandbox stopped and started.
    Restarted,
    /// The image changed, or the instance had no sandbox: a new sandbox was created,
    /// with an empty disk.
    Recreated,
}

/// The factory sandboxes of one installation, in its factory workspace.
#[derive(Clone)]
pub struct Factory {
    client: OpenShellClient,
    config: Arc<FactoryConfiguration>,
    /// The installation's base policy, canonical.
    policy: Arc<p::sandbox::v1::SandboxPolicy>,
    store: PlatformStore,
}

impl Factory {
    /// Connect to the configured gateway with its bearer reference.
    pub async fn connect(config: FactoryConfiguration, store: PlatformStore) -> Result<Self> {
        let client = crate::computers::connect_gateway(&config.gateway).await?;
        Self::new(config, client, store)
    }

    /// The factory on `client`, with the base policy read from the configured file.
    pub fn new(
        config: FactoryConfiguration,
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
            config: Arc::new(config),
            policy: Arc::new(policy),
            store,
        })
    }

    /// Serve a new instance: create its registration provider, which holds the
    /// instance's registration token, then its sandbox, unless an enforced cap refuses
    /// it. `push` names the instance's first pusher and revision.
    pub async fn create(&self, instance: &NewInstance, source: &Source, push: &Push) -> Result<()> {
        let deployment = instance.deployment;
        let branch = record::push(&self.store, deployment, &push.pusher).await?;
        let name = sandbox_name(deployment);
        self.recorded(deployment, Stage::Create, async {
            let mut grpc = self.client.raw_grpc_fresh().await?;
            call(
                "create the registration provider",
                grpc.create_provider(p::CreateProviderRequest {
                    workspace_scope: scope(&self.config.workspace),
                    provider: Some(p::Provider {
                        metadata: Some(p::ObjectMeta {
                            name: sandbox::registration_provider(&name),
                            workspace: self.config.workspace.clone(),
                            ..Default::default()
                        }),
                        r#type: self.config.registration_profile.clone(),
                        credentials: HashMap::from([(
                            sandbox::REGISTRATION_SECRET.into(),
                            instance.registration_token.expose_secret().into(),
                        )]),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )
            .await
        })
        .await?;
        self.serve(deployment, &branch, source, push).await
    }

    /// Apply a push to an existing instance: restart its sandbox when it runs the
    /// pushed image, otherwise recreate it.
    pub async fn update(
        &self,
        deployment: DeploymentId,
        source: &Source,
        push: &Push,
    ) -> Result<Update> {
        let current = self.sandbox(&sandbox_name(deployment)).await?;
        match current {
            Some(sandbox) if sandbox::runs(&sandbox, &source.image) => {
                self.restart(deployment, source.policy.as_deref(), push)
                    .await?;
                Ok(Update::Restarted)
            }
            Some(_) => {
                self.recreate(deployment, source, push).await?;
                Ok(Update::Recreated)
            }
            None => {
                let branch = record::push(&self.store, deployment, &push.pusher).await?;
                self.serve(deployment, &branch, source, push).await?;
                Ok(Update::Recreated)
            }
        }
    }

    /// Serve a push whose image is unchanged: set the sandbox's policy, which OpenShell
    /// keeps when unchanged, then stop and start the sandbox, whose start command
    /// fetches the branch's latest revision. `policy` is the revision's
    /// `openshell/policy.yaml`.
    pub async fn restart(
        &self,
        deployment: DeploymentId,
        policy: Option<&str>,
        push: &Push,
    ) -> Result<()> {
        record::push(&self.store, deployment, &push.pusher).await?;
        let name = sandbox_name(deployment);
        self.recorded(deployment, Stage::Restart, async {
            let policy = sandbox::policy(&self.policy, policy)?;
            let mut grpc = self.client.raw_grpc_fresh().await?;
            call(
                "set the sandbox's policy",
                grpc.update_config(p::UpdateConfigRequest {
                    workspace_scope: scope(&self.config.workspace),
                    sandbox: name.clone(),
                    policy: Some(policy),
                    ..Default::default()
                }),
            )
            .await?;
            call(
                "stop the sandbox",
                grpc.stop_sandbox(p::StopSandboxRequest {
                    workspace_scope: scope(&self.config.workspace),
                    name: name.clone(),
                    ..Default::default()
                }),
            )
            .await?;
            // OpenShell starts a sandbox once its main program has stopped, completed
            // or failed.
            self.wait(&name, "stop", |sandbox| {
                sandbox.is_none_or(|sandbox| {
                    !matches!(Phase::of(sandbox), Phase::Ready | Phase::Stopping)
                })
            })
            .await?;
            call(
                "start the sandbox",
                grpc.start_sandbox(p::StartSandboxRequest {
                    workspace_scope: scope(&self.config.workspace),
                    name: name.clone(),
                    ..Default::default()
                }),
            )
            .await
        })
        .await?;
        record::applied(&self.store, deployment, &push.revision).await
    }

    /// Serve a push whose image changed: delete the sandbox and create it again from the
    /// new image. The new sandbox's disk starts empty; its providers stay.
    pub async fn recreate(
        &self,
        deployment: DeploymentId,
        source: &Source,
        push: &Push,
    ) -> Result<()> {
        let branch = record::push(&self.store, deployment, &push.pusher).await?;
        self.recorded(deployment, Stage::Create, async {
            self.delete_sandbox(deployment).await?;
            self.create_sandbox(deployment, &branch, source).await
        })
        .await?;
        record::applied(&self.store, deployment, &push.revision).await
    }

    /// Stop serving an instance: delete its sandbox and providers, then the instance
    /// and its registrations, then its data environment's database when no other
    /// instance names it.
    pub async fn delete(&self, deployment: DeploymentId) -> Result<()> {
        let name = sandbox_name(deployment);
        self.recorded(deployment, Stage::Delete, async {
            self.delete_sandbox(deployment).await?;
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

    /// The instance's status: its sandbox as OpenShell reports it, and what the Hub
    /// last applied and why its last step failed.
    pub async fn status(&self, deployment: DeploymentId) -> Result<InstanceStatus> {
        let record = record::read(&self.store, deployment)
            .await?
            .with_context(|| format!("no instance has deployment {deployment}"))?;
        let sandbox = self.sandbox(&sandbox_name(deployment)).await?;
        Ok(InstanceStatus {
            sandbox: sandbox.as_ref().map(SandboxStatus::of),
            revision: record.revision,
            failed: record.failed,
            warning: record.warning,
        })
    }

    /// Create the sandbox of an instance that has none, unless an enforced cap refuses
    /// it, and record the pushed revision.
    async fn serve(
        &self,
        deployment: DeploymentId,
        branch: &crate::execution::BranchName,
        source: &Source,
        push: &Push,
    ) -> Result<()> {
        self.admit(deployment, &push.pusher).await?;
        self.recorded(
            deployment,
            Stage::Create,
            self.create_sandbox(deployment, branch, source),
        )
        .await?;
        record::applied(&self.store, deployment, &push.revision).await
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

    async fn create_sandbox(
        &self,
        deployment: DeploymentId,
        branch: &crate::execution::BranchName,
        source: &Source,
    ) -> Result<()> {
        let policy = sandbox::policy(&self.policy, source.policy.as_deref())?;
        let mut grpc = self.client.raw_grpc_fresh().await?;
        call(
            "create the sandbox",
            grpc.create_sandbox(sandbox::create_request(
                &self.config,
                deployment,
                branch,
                source,
                policy,
            )),
        )
        .await
    }

    /// Delete the instance's sandbox and wait until OpenShell no longer has it.
    async fn delete_sandbox(&self, deployment: DeploymentId) -> Result<()> {
        let name = sandbox_name(deployment);
        let mut grpc = self.client.raw_grpc_fresh().await?;
        call(
            "delete the sandbox",
            grpc.delete_sandbox(p::DeleteSandboxRequest {
                workspace_scope: scope(&self.config.workspace),
                name: name.clone(),
                allow_missing: true,
                ..Default::default()
            }),
        )
        .await?;
        self.wait(&name, "disappear", |sandbox| sandbox.is_none())
            .await
    }

    async fn sandbox(&self, name: &str) -> Result<Option<p::Sandbox>> {
        let mut grpc = self.client.raw_grpc_fresh().await?;
        let answer = tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_sandbox(p::GetSandboxRequest {
                workspace_scope: scope(&self.config.workspace),
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

    /// Wait until the sandbox, or its absence, satisfies `done`.
    async fn wait(
        &self,
        name: &str,
        transition: &str,
        done: impl Fn(Option<&p::Sandbox>) -> bool,
    ) -> Result<()> {
        tokio::time::timeout(TRANSITION_TIMEOUT, async {
            loop {
                if done(self.sandbox(name).await?.as_ref()) {
                    return Ok(());
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        })
        .await
        .with_context(|| {
            format!("the sandbox did not {transition} within {TRANSITION_TIMEOUT:?}")
        })?
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
