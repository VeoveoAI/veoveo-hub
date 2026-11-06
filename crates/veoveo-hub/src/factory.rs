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
    collections::{HashMap, HashSet},
    fmt,
    future::Future,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use openshell_sdk::{OpenShellClient, raw::proto as p};
use secrecy::ExposeSecret;
use serde::Serialize;
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId};

pub use sandbox::{LINKED_CHECKOUT, LORE, check_linked_policy, sandbox_name};
pub use status::{Condition, Failure, InstanceStatus, Phase, RunnerStatus, SandboxStatus, Stage};
use veoveo_hub_sdk::{ServerSlug, hub::LinkName};

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
    /// The revision's `openshell/providers.yaml`, which requests the installation's
    /// providers by name.
    pub providers: Option<String>,
}

/// An instance's own OAuth client. The gateway refreshes the sandbox's Lore token with
/// it; the Hub issues each token for the instance's latest pusher and its repository, and
/// a runner's for its linked repository as its tracked branch's latest pusher. Its
/// identifier is the instance's deployment ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceClient {
    pub deployment: DeploymentId,
    /// The instance's latest pusher; none before its first push.
    pub pusher: Option<PrincipalId>,
    /// The Lore repository ID whose branch the instance serves.
    pub repository: String,
    /// For a runner, the linked repository and its tracked branch's latest pusher.
    pub linked: Option<LinkedClient>,
}

/// A runner's linked repository, as its client issues tokens for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedClient {
    /// The linked repository's Lore ID.
    pub repository: String,
    /// The tracked branch's latest pusher; none before the Hub applied a revision of it.
    pub pusher: Option<PrincipalId>,
}

/// A link at the top level of a branch's revision, which one runner serves: its name, the
/// Lore repository it names and the branch of that repository it tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub name: LinkName,
    /// The linked repository's Lore ID.
    pub repository: String,
    pub tracked: TrackedBranch,
}

/// The branch of a linked repository that a link tracks: the parent branch's ID for a
/// link that follows the parent's branch, else the branch of its pinned revision. People
/// push to it to change what the runner serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedBranch {
    /// The branch's Lore ID, 32 lowercase hexadecimal digits.
    pub id: String,
    /// Its name, as the Hub last read it.
    pub name: String,
}

/// What the tracked branch's head holds in `openshell/`, read as its pusher: the network
/// rules its `policy.yaml` adds and the providers its `providers.yaml` requests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkedPrepared {
    pub policy: Option<String>,
    pub providers: Option<String>,
}

/// What a runner serves beside its branch's push: its link, the head of the tracked branch
/// with its pusher and what it holds, and the branch's own tree, without its links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPush {
    pub link: Link,
    pub linked: Push,
    pub prepared: LinkedPrepared,
    pub own: OwnTree,
}

/// A branch revision's own tree, without the links at its top level: the SHA-256 of
/// every entry Lore lists of it, which a link's addition or removal leaves as it is, and
/// its top-level entries, which a runner's checkout of the branch includes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnTree {
    /// 64 lowercase hexadecimal digits.
    pub digest: String,
    /// The names at the top level that are no links, sorted.
    pub entries: Vec<String>,
}

/// A runner as the Hub records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    /// The parent repository's Lore ID.
    pub repository: String,
    pub branch: BranchName,
    pub link: Link,
    /// The parent branch's revision the Hub last applied, and its pusher.
    pub revision: Option<Revision>,
    pub pusher: Option<PrincipalId>,
    /// The tracked branch's revision the Hub last applied, and its pusher.
    pub linked_revision: Option<Revision>,
    pub linked_pusher: Option<PrincipalId>,
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
    /// The installation's providers the revision requested that are granted to its
    /// repository, sorted; for a runner, those its linked repository's tracked branch
    /// requested that are granted to the linked repository.
    providers: Vec<String>,
    /// For a runner, what its start command checks out beside the branch.
    runner: Option<RunnerSource>,
}

/// What a runner's start command checks out beside its branch.
struct RunnerSource {
    /// The linked repository's Lore URL.
    linked: url::Url,
    /// The tracked branch's ID.
    tracked: String,
    /// The top-level entries of the branch's own tree, which its checkout includes.
    entries: Vec<String>,
}

impl Source {
    /// The sandbox's fixed start command for `branch`.
    fn command(&self, branch: &BranchName) -> Vec<String> {
        match &self.runner {
            None => sandbox::start_command(&self.remote, branch),
            Some(runner) => sandbox::runner_start_command(
                &self.remote,
                branch,
                &runner.linked,
                &runner.tracked,
                &runner.entries,
            ),
        }
    }
}

/// The factory sandboxes of one installation, in its factory workspace.
/// An instance's place in the factory and its status.
#[derive(Debug, Clone)]
pub struct InventoryEntry {
    /// The server, once its first registration named it.
    pub server: Option<ServerSlug>,
    pub repository: String,
    pub branch: BranchName,
    pub database: String,
    pub created_at: DateTime<Utc>,
    /// When its server registered or resumed, since its sandbox last started.
    pub served_at: Option<DateTime<Utc>>,
    pub pusher: Option<PrincipalId>,
    pub linked_pusher: Option<PrincipalId>,
    pub status: InstanceStatus,
}

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
    /// The instances a push or a removal is being applied to, which may be without a
    /// sandbox meanwhile. One Hub serves the factory, so its memory is the whole record.
    busy: Arc<Mutex<HashSet<DeploymentId>>>,
}

/// An instance marked busy until this is dropped.
struct Busy {
    busy: Arc<Mutex<HashSet<DeploymentId>>>,
    deployment: DeploymentId,
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.busy
            .lock()
            .expect("the busy instances lock")
            .remove(&self.deployment);
    }
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
            busy: Arc::default(),
        })
    }

    /// Mark the instance busy while a push or a removal is applied to it.
    fn busy(&self, deployment: DeploymentId) -> Busy {
        self.busy
            .lock()
            .expect("the busy instances lock")
            .insert(deployment);
        Busy {
            busy: self.busy.clone(),
            deployment,
        }
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
        self.apply_to(repository, branch, push, prepared, None)
            .await
    }

    /// [`Self::apply`] to the runner of one link of `branch`: the image and run of the
    /// branch's `openshell/`, with the network rules of the parent's and then the linked
    /// repository's `policy.yaml`, the providers the tracked branch's `providers.yaml`
    /// requests that are granted to the linked repository, and a database of its own. Its
    /// start command checks the branch out without its links and the linked repository at
    /// the tracked branch. Records the tracked branch's name, its revision and its pusher,
    /// whose token the runner's client issues for the linked repository.
    pub async fn apply_runner(
        &self,
        repository: &str,
        branch: &BranchName,
        push: Push,
        prepared: Prepared,
        runner: RunnerPush,
    ) -> Result<()> {
        self.apply_to(repository, branch, push, prepared, Some(runner))
            .await
    }

    async fn apply_to(
        &self,
        repository: &str,
        branch: &BranchName,
        push: Push,
        prepared: Prepared,
        runner: Option<RunnerPush>,
    ) -> Result<()> {
        repository_id(repository)?;
        let (found, retargeted) = match &runner {
            None => (
                record::instance(&self.store, repository, branch).await?,
                false,
            ),
            Some(runner) => {
                repository_id(&runner.link.repository)?;
                repository_id(&runner.link.tracked.id)?;
                let found =
                    record::runner(&self.store, repository, branch, &runner.link.name).await?;
                let retargeted = found.as_ref().is_some_and(|(_, _, previous)| {
                    previous.repository != runner.link.repository
                        || previous.tracked.id != runner.link.tracked.id
                });
                (
                    found.map(|(deployment, database, _)| (deployment, database)),
                    retargeted,
                )
            }
        };
        let (deployment, database) = match found {
            Some(instance) => instance,
            None => {
                let database = match &runner {
                    None => self.database(branch)?,
                    Some(runner) => runner_database(branch, &runner.link.name)?,
                };
                let instance = match &runner {
                    None => {
                        registration::create_instance(&self.store, repository, &database, branch)
                            .await?
                    }
                    Some(runner) => {
                        registration::create_runner(
                            &self.store,
                            repository,
                            &database,
                            branch,
                            &runner.link,
                        )
                        .await?
                    }
                };
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
        let _busy = self.busy(deployment);
        let name = sandbox_name(deployment);
        let image = self
            .recorded(deployment, Stage::Build, async {
                let image = image::ImageName::parse(name.clone())?;
                let cache = image::CacheNamespace::of(repository, branch);
                Ok(self
                    .builder
                    .build(&prepared.image_context, &image, &cache)
                    .await?)
            })
            .await?;
        let policy = self
            .recorded(deployment, Stage::Create, async {
                sandbox::policies(
                    &self.policy,
                    &[
                        ("repository", prepared.policy.as_deref()),
                        (
                            "linked repository",
                            runner
                                .as_ref()
                                .and_then(|runner| runner.prepared.policy.as_deref()),
                        ),
                    ],
                )
            })
            .await?;
        let shape = self
            .recorded(deployment, Stage::Create, self.shape())
            .await?;
        // A runner's providers are its linked repository's; the branch's own requests
        // apply to no runner.
        let (providers_file, granted_to) = match &runner {
            None => (prepared.providers.as_deref(), repository),
            Some(runner) => (
                runner.prepared.providers.as_deref(),
                runner.link.repository.as_str(),
            ),
        };
        let requested = self
            .recorded(deployment, Stage::Create, async {
                sandbox::requested_providers(providers_file)
            })
            .await?;
        let (providers, ungranted) = self.granted_providers(granted_to, &requested);
        if !ungranted.is_empty() {
            tracing::warn!(%deployment, ?ungranted, "the revision requests providers its repository is not granted");
        }
        record::ungranted(&self.store, deployment, &ungranted).await?;
        let mut remote = self.config.lore_server.clone();
        remote.set_path(&format!("/{repository}"));
        let runner_source = runner.as_ref().map(|runner| {
            let mut linked = self.config.lore_server.clone();
            linked.set_path(&format!("/{}", runner.link.repository));
            RunnerSource {
                linked,
                tracked: runner.link.tracked.id.clone(),
                entries: runner.own.entries.clone(),
            }
        });
        let source = Source {
            remote,
            image,
            policy,
            shape,
            providers,
            runner: runner_source,
        };
        let branch = record::push(&self.store, deployment, &push.pusher).await?;
        if !retargeted && let Some(runner) = &runner {
            record::linked(&self.store, deployment, &runner.link, &runner.linked.pusher).await?;
        }
        let current = self
            .recorded(deployment, Stage::Create, self.sandbox(&name))
            .await?;
        // A runner whose branch's own tree and tracked revision are the ones it applied
        // serves the same program and content: a link added to or removed from the branch
        // changes nothing for it, neither its sandbox nor its program.
        let unchanged = match &runner {
            Some(runner) => {
                let (own, linked) = record::runner_applied(&self.store, deployment).await?;
                own.as_deref() == Some(runner.own.digest.as_str())
                    && linked.as_ref() == Some(&runner.linked.revision)
            }
            None => false,
        };
        // A runner that keeps serving keeps what its server served; a restart or creation
        // ends it until the server registers or resumes again, so a server that registered
        // since serves the revision the Hub applied.
        match current {
            Some(sandbox)
                if !retargeted
                    && unchanged
                    && sandbox::runs_beside_image(&name, &sandbox, &source, &branch) =>
            {
                tracing::info!(%deployment, "the runner's own tree and tracked revision are unchanged: its sandbox keeps serving");
            }
            Some(sandbox) if !retargeted && sandbox::runs(&name, &sandbox, &source, &branch) => {
                record::unserved(&self.store, deployment).await?;
                self.recorded(deployment, Stage::Restart, self.restart(&name, &sandbox))
                    .await?;
            }
            Some(_) => {
                record::unserved(&self.store, deployment).await?;
                self.recorded(deployment, Stage::Create, async {
                    self.delete_sandbox(&name).await?;
                    if retargeted && let Some(runner) = &runner {
                        record::linked(
                            &self.store,
                            deployment,
                            &runner.link,
                            &runner.linked.pusher,
                        )
                        .await?;
                    }
                    self.create(deployment, &branch, &source).await
                })
                .await?;
            }
            None => {
                record::unserved(&self.store, deployment).await?;
                self.admit(deployment, &push.pusher).await?;
                self.recorded(deployment, Stage::Create, async {
                    if retargeted && let Some(runner) = &runner {
                        record::linked(
                            &self.store,
                            deployment,
                            &runner.link,
                            &runner.linked.pusher,
                        )
                        .await?;
                    }
                    self.create(deployment, &branch, &source).await
                })
                .await?;
            }
        }
        if let Some(runner) = &runner {
            record::linked_applied(&self.store, deployment, &runner.linked.revision).await?;
            record::own_tree(&self.store, deployment, &runner.own.digest).await?;
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

    /// Stop serving the link `link` of `branch` of `repository`: delete its runner, as
    /// [`Self::remove`] deletes an instance. Nothing happens without a runner.
    pub async fn remove_runner(
        &self,
        repository: &str,
        branch: &BranchName,
        link: &LinkName,
    ) -> Result<()> {
        repository_id(repository)?;
        match record::runner(&self.store, repository, branch, link).await? {
            Some((deployment, _, _)) => self.delete(deployment).await,
            None => Ok(()),
        }
    }

    /// The complete current runner of `link` of `branch`, or none after removal.
    pub async fn runner(
        &self,
        repository: &str,
        branch: &BranchName,
        link: &LinkName,
    ) -> Result<Option<Runner>> {
        repository_id(repository)?;
        record::current_runner(&self.store, repository, branch, link).await
    }

    /// The runners of `branch` of `repository`, by link name.
    pub async fn runners(&self, repository: &str, branch: &BranchName) -> Result<Vec<Runner>> {
        repository_id(repository)?;
        record::runners(&self.store, repository, branch).await
    }

    /// The runners whose links name the Lore repository `linked`, of every repository and
    /// branch.
    pub async fn tracking(&self, linked: &str) -> Result<Vec<Runner>> {
        repository_id(linked)?;
        record::tracking(&self.store, linked).await
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
        self.settled(deployment, wait).await
    }

    /// [`Self::status`] of the runner of `link` of `branch` of `repository`.
    pub async fn runner_status(
        &self,
        repository: &str,
        branch: &BranchName,
        link: &LinkName,
        wait: Duration,
    ) -> Result<Option<InstanceStatus>> {
        repository_id(repository)?;
        let Some((deployment, _, _)) =
            record::runner(&self.store, repository, branch, link).await?
        else {
            return Ok(None);
        };
        self.settled(deployment, wait).await
    }

    /// The instance's status once it settles, or at the end of `wait`.
    async fn settled(
        &self,
        deployment: DeploymentId,
        wait: Duration,
    ) -> Result<Option<InstanceStatus>> {
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

    /// Every instance of the factory with its status, from one listing of the workspace's
    /// sandboxes.
    pub async fn inventory(&self) -> Result<Vec<InventoryEntry>> {
        let rows = record::inventory(&self.store).await?;
        let sandboxes = self.sandboxes().await?;
        rows.into_iter()
            .map(|row| {
                let name = sandbox_name(row.deployment);
                let found = sandboxes.get(&name);
                Ok(InventoryEntry {
                    server: row
                        .server
                        .map(ServerSlug::parse)
                        .transpose()
                        .context("the stored server is malformed")?,
                    repository: row.repository,
                    branch: BranchName::parse(row.branch)
                        .context("the stored branch is malformed")?,
                    database: row.database,
                    created_at: row.created_at,
                    served_at: row.served_at,
                    pusher: row
                        .pusher
                        .map(PrincipalId::parse)
                        .transpose()
                        .context("the stored pusher is malformed")?,
                    linked_pusher: row
                        .linked_pusher
                        .map(PrincipalId::parse)
                        .transpose()
                        .context("the stored pusher is malformed")?,
                    status: self.composed_status(row.deployment, row.record, found),
                })
            })
            .collect()
    }

    /// Whether `deployment` is an instance of the factory.
    pub async fn is_instance(&self, deployment: DeploymentId) -> Result<bool> {
        record::is_instance(&self.store, deployment).await
    }

    /// The name of the installation's database, the base branch's data environment.
    pub fn installation_database(&self) -> Result<String> {
        Ok(DatabaseName::installation(&self.store)?.as_str().to_owned())
    }

    /// The size in bytes of each of the databases that exists.
    pub async fn database_sizes(&self, names: &[String]) -> Result<Vec<(String, i64)>> {
        record::database_sizes(&self.store, names).await
    }

    /// The instance's status: its sandbox as OpenShell reports it, and what the Hub
    /// last applied and why its last step failed.
    async fn instance_status(&self, deployment: DeploymentId) -> Result<Option<InstanceStatus>> {
        let Some(record) = record::read(&self.store, deployment).await? else {
            return Ok(None);
        };
        let found = self.sandbox(&sandbox_name(deployment)).await?;
        Ok(Some(self.composed_status(
            deployment,
            record,
            found.as_ref(),
        )))
    }

    fn composed_status(
        &self,
        deployment: DeploymentId,
        record: record::Record,
        found: Option<&p::Sandbox>,
    ) -> InstanceStatus {
        let name = sandbox_name(deployment);
        let sandbox_id = found
            .and_then(|sandbox| sandbox.metadata.as_ref())
            .map(|metadata| metadata.id.clone())
            .filter(|id| !id.is_empty());
        let sandbox = found.map(SandboxStatus::of);
        let ready = sandbox
            .as_ref()
            .is_some_and(|sandbox| sandbox.phase == Phase::Ready);
        // An applied instance has a sandbox unless a push or removal is replacing it, or
        // OpenShell removed it, as it does one that never becomes ready.
        let gone = found.is_none()
            && record.revision.is_some()
            && !self
                .busy
                .lock()
                .expect("the busy instances lock")
                .contains(&deployment);
        let failed = record.failed.or_else(|| {
            gone.then(|| Failure {
                stage: Stage::Create,
                message: "OpenShell no longer has the instance's sandbox: it removes one that \
                          never becomes ready. The branch's next push creates it again."
                    .into(),
            })
        });
        InstanceStatus {
            deployment,
            workspace: self.config.workspace.clone(),
            sandbox_name: name,
            sandbox_id,
            sandbox,
            serving_revision: record.revision.clone().filter(|_| ready && record.served),
            applied_revision: record.revision,
            failed,
            warning: record.warning,
            ungranted_providers: record.ungranted_providers,
            runner: record.runner,
        }
    }

    /// The providers among `requested` that the active control plane's `factory.providers`
    /// grants to `repository`, and those it does not. Grants change while the Hub runs, as
    /// teams do, so each push reads them; a control plane without `factory` grants none.
    fn granted_providers(
        &self,
        repository: &str,
        requested: &[String],
    ) -> (Vec<String>, Vec<String>) {
        let catalog = self.catalog.current();
        let grants = catalog
            .control_plane()
            .factory
            .as_ref()
            .map_or(&[][..], |factory| factory.providers.as_slice());
        crate::control_plane::granted_providers(grants, repository, requested)
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
        let _busy = self.busy(deployment);
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

    /// The factory workspace's sandboxes by name.
    async fn sandboxes(&self) -> Result<HashMap<String, p::Sandbox>> {
        let mut sandboxes = HashMap::new();
        let mut page_token = String::new();
        loop {
            let mut grpc = self.client.raw_grpc_fresh().await?;
            let answer = tokio::time::timeout(
                RPC_TIMEOUT,
                grpc.list_sandboxes(p::ListSandboxesRequest {
                    workspace_scope: scope(&self.config.workspace),
                    page_size: 1000,
                    page_token: page_token.clone(),
                    label_selector: String::new(),
                }),
            )
            .await
            .context("OpenShell did not list the sandboxes in time")?
            .map_err(|status| refused("list the sandboxes", &status))?
            .into_inner();
            for sandbox in answer.sandboxes {
                if let Some(name) = sandbox
                    .metadata
                    .as_ref()
                    .map(|metadata| metadata.name.clone())
                {
                    sandboxes.insert(name, sandbox);
                }
            }
            if answer.next_page_token.is_empty() {
                return Ok(sandboxes);
            }
            page_token = answer.next_page_token;
        }
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

/// The database of a runner's data environment: that of a branch named by the branch's
/// name and the link's joined by a hyphen, on the base branch too, so runners of one
/// program never share a schema.
fn runner_database(branch: &BranchName, link: &LinkName) -> Result<DatabaseName> {
    let joined = BranchName::parse(format!("{branch}-{link}"))
        .context("the branch and link names do not name a data environment")?;
    Ok(DatabaseName::of_branch(&joined))
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
