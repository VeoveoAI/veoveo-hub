//! What the Hub records on an instance about its factory sandbox: the latest pusher,
//! the last revision it applied, whether its server registered or resumed since, the
//! last failure of its own steps and the capacity warning of its creation.
use anyhow::{Context, Result};
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId};

use veoveo_hub_sdk::hub::LinkName;

use super::{
    Failure, InstanceClient, Link, LinkedClient, Revision, Runner, RunnerStatus, Stage,
    TrackedBranch,
};
use crate::{data_environment::DatabaseName, execution::BranchName};

/// The deployment and database of the ordinary instance of `branch` of `repository`.
pub(super) async fn instance(
    store: &PlatformStore,
    repository: &str,
    branch: &BranchName,
) -> Result<Option<(DeploymentId, DatabaseName)>> {
    sqlx::query_file!(
        "src/factory/record/instance.sql",
        repository,
        branch.as_str()
    )
    .fetch_optional(store.pool())
    .await
    .context("failed to look up the instance")?
    .map(|row| {
        Ok((
            DeploymentId::try_from(row.deployment)
                .context("stored instance deployment is invalid")?,
            DatabaseName::parse(row.database).context("stored instance database is invalid")?,
        ))
    })
    .transpose()
}

/// The deployment and database of the runner of `link` of `branch` of `repository`.
pub(super) async fn runner(
    store: &PlatformStore,
    repository: &str,
    branch: &BranchName,
    link: &LinkName,
) -> Result<Option<(DeploymentId, DatabaseName, Link)>> {
    sqlx::query_file!(
        "src/factory/record/runner.sql",
        repository,
        branch.as_str(),
        link.as_str()
    )
    .fetch_optional(store.pool())
    .await
    .context("failed to look up the runner")?
    .map(|row| {
        Ok((
            DeploymentId::try_from(row.deployment)
                .context("stored runner deployment is invalid")?,
            DatabaseName::parse(row.database).context("stored runner database is invalid")?,
            Link {
                name: link.clone(),
                repository: row
                    .linked_repository
                    .context("runner has no linked repository")?,
                tracked: TrackedBranch {
                    id: row.tracked_branch.context("runner has no tracked branch")?,
                    name: row
                        .tracked_branch_name
                        .context("runner has no tracked branch name")?,
                },
            },
        ))
    })
    .transpose()
}

/// A runner's row, as both runner listings read it.
struct RunnerRow {
    repository: String,
    branch: String,
    link: String,
    linked_repository: String,
    tracked_branch: String,
    tracked_branch_name: String,
    revision: Option<String>,
    pusher: Option<String>,
    linked_revision: Option<String>,
    linked_pusher: Option<String>,
}

impl TryFrom<RunnerRow> for Runner {
    type Error = anyhow::Error;
    fn try_from(row: RunnerRow) -> Result<Self> {
        Ok(Self {
            repository: row.repository,
            branch: BranchName::parse(row.branch)
                .context("the stored runner branch is malformed")?,
            link: Link {
                name: LinkName::parse(row.link).context("the stored link is malformed")?,
                repository: row.linked_repository,
                tracked: TrackedBranch {
                    id: row.tracked_branch,
                    name: row.tracked_branch_name,
                },
            },
            revision: row
                .revision
                .map(Revision::parse)
                .transpose()
                .context("the stored runner revision is malformed")?,
            pusher: row
                .pusher
                .map(PrincipalId::parse)
                .transpose()
                .context("the stored runner pusher is malformed")?,
            linked_revision: row
                .linked_revision
                .map(Revision::parse)
                .transpose()
                .context("the stored linked revision is malformed")?,
            linked_pusher: row
                .linked_pusher
                .map(PrincipalId::parse)
                .transpose()
                .context("the stored linked pusher is malformed")?,
        })
    }
}

/// The runners of `branch` of `repository`, by link name.
pub(super) async fn runners(
    store: &PlatformStore,
    repository: &str,
    branch: &BranchName,
) -> Result<Vec<Runner>> {
    sqlx::query_file_as!(
        RunnerRow,
        "src/factory/record/runners.sql",
        repository,
        branch.as_str()
    )
    .fetch_all(store.pool())
    .await
    .context("failed to list the branch's runners")?
    .into_iter()
    .map(Runner::try_from)
    .collect()
}

/// The runners whose links name `linked_repository`.
pub(super) async fn tracking(
    store: &PlatformStore,
    linked_repository: &str,
) -> Result<Vec<Runner>> {
    sqlx::query_file_as!(
        RunnerRow,
        "src/factory/record/tracking.sql",
        linked_repository
    )
    .fetch_all(store.pool())
    .await
    .context("failed to list the runners of the linked repository")?
    .into_iter()
    .map(Runner::try_from)
    .collect()
}

/// Record the runner's complete linked target and its latest pusher. A retarget is
/// recorded only after the old sandbox is absent, before its replacement gets credentials.
pub(super) async fn linked(
    store: &PlatformStore,
    deployment: DeploymentId,
    link: &Link,
    pusher: &PrincipalId,
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/linked.sql",
        deployment.as_uuid(),
        link.repository,
        link.tracked.id,
        link.tracked.name,
        pusher.as_str()
    )
    .execute(store.pool())
    .await
    .context("failed to record the runner's tracked branch")?;
    Ok(())
}

pub(super) async fn linked_applied(
    store: &PlatformStore,
    deployment: DeploymentId,
    revision: &Revision,
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/linked_applied.sql",
        deployment.as_uuid(),
        revision.as_str()
    )
    .execute(store.pool())
    .await
    .context("failed to record the runner's linked revision")?;
    Ok(())
}

/// The own tree of its branch and the tracked revision the runner last applied.
pub(super) async fn runner_applied(
    store: &PlatformStore,
    deployment: DeploymentId,
) -> Result<(Option<String>, Option<Revision>)> {
    let row = sqlx::query_file!(
        "src/factory/record/runner_applied.sql",
        deployment.as_uuid()
    )
    .fetch_optional(store.pool())
    .await
    .context("failed to read what the runner applied")?;
    let Some(row) = row else {
        return Ok((None, None));
    };
    Ok((
        row.own_tree,
        row.linked_revision
            .map(Revision::parse)
            .transpose()
            .context("the stored linked revision is malformed")?,
    ))
}

/// Record the own tree of its branch that the Hub applied to the runner's sandbox.
pub(super) async fn own_tree(
    store: &PlatformStore,
    deployment: DeploymentId,
    digest: &str,
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/own_tree.sql",
        deployment.as_uuid(),
        digest
    )
    .execute(store.pool())
    .await
    .context("failed to record the runner's own tree")?;
    Ok(())
}

/// Record `pusher` as the instance's latest pusher. Returns the branch the instance
/// serves.
pub(super) async fn push(
    store: &PlatformStore,
    deployment: DeploymentId,
    pusher: &PrincipalId,
) -> Result<BranchName> {
    let branch = sqlx::query_file_scalar!(
        "src/factory/record/push.sql",
        deployment.as_uuid(),
        pusher.as_str()
    )
    .fetch_optional(store.pool())
    .await
    .context("failed to record the instance's pusher")?
    .with_context(|| format!("no instance has deployment {deployment}"))?;
    BranchName::parse(branch).context("the stored instance branch is malformed")
}

/// Record that the Hub restarts or creates the instance's sandbox: until its server
/// registers or resumes again, the instance serves no revision. A runner whose sandbox
/// keeps serving keeps what it served.
pub(super) async fn unserved(store: &PlatformStore, deployment: DeploymentId) -> Result<()> {
    sqlx::query_file!("src/factory/record/unserved.sql", deployment.as_uuid())
        .execute(store.pool())
        .await
        .context("failed to record that the instance's server stopped serving")?;
    Ok(())
}

/// Record the digest of the secret of the instance's client.
pub(super) async fn client(
    store: &PlatformStore,
    deployment: DeploymentId,
    secret_digest: &veoveo_types::Sha256Digest,
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/client.sql",
        deployment.as_uuid(),
        secret_digest.as_str()
    )
    .execute(store.pool())
    .await
    .context("failed to record the instance's client")?;
    Ok(())
}

/// The instance whose client's secret has this digest.
pub(super) async fn client_of(
    store: &PlatformStore,
    secret_digest: &veoveo_types::Sha256Digest,
) -> Result<Option<InstanceClient>> {
    sqlx::query_file!("src/factory/record/client_read.sql", secret_digest.as_str())
        .fetch_optional(store.pool())
        .await
        .context("failed to read the instance's client")?
        .map(|row| {
            Ok(InstanceClient {
                deployment: DeploymentId::try_from(row.deployment)
                    .context("the stored instance deployment is invalid")?,
                pusher: row
                    .pusher
                    .map(PrincipalId::parse)
                    .transpose()
                    .context("the stored instance pusher is malformed")?,
                repository: row.repository,
                linked: row
                    .linked_repository
                    .map(|repository| {
                        Ok::<_, anyhow::Error>(LinkedClient {
                            repository,
                            pusher: row
                                .linked_pusher
                                .map(PrincipalId::parse)
                                .transpose()
                                .context("the stored linked pusher is malformed")?,
                        })
                    })
                    .transpose()?,
            })
        })
        .transpose()
}

/// How many instances name `pusher` as their latest pusher.
pub(super) async fn pushed(store: &PlatformStore, pusher: &PrincipalId) -> Result<u64> {
    let instances = sqlx::query_file_scalar!("src/factory/record/pushed.sql", pusher.as_str())
        .fetch_one(store.pool())
        .await
        .context("failed to count the pusher's instances")?;
    u64::try_from(instances).context("a count is never negative")
}

pub(super) async fn warn(
    store: &PlatformStore,
    deployment: DeploymentId,
    warning: Option<&str>,
) -> Result<()> {
    sqlx::query_file!("src/factory/record/warn.sql", deployment.as_uuid(), warning)
        .execute(store.pool())
        .await
        .context("failed to record the instance's warning")?;
    Ok(())
}

/// Record the providers the revision requested that its repository is not granted.
pub(super) async fn ungranted(
    store: &PlatformStore,
    deployment: DeploymentId,
    providers: &[String],
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/ungranted.sql",
        deployment.as_uuid(),
        providers
    )
    .execute(store.pool())
    .await
    .context("failed to record the instance's ungranted providers")?;
    Ok(())
}

pub(super) async fn applied(
    store: &PlatformStore,
    deployment: DeploymentId,
    revision: &Revision,
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/applied.sql",
        deployment.as_uuid(),
        revision.as_str()
    )
    .execute(store.pool())
    .await
    .context("failed to record the instance's revision")?;
    Ok(())
}

pub(super) async fn failed(
    store: &PlatformStore,
    deployment: DeploymentId,
    stage: Stage,
    message: &str,
) -> Result<()> {
    sqlx::query_file!(
        "src/factory/record/failed.sql",
        deployment.as_uuid(),
        stage.as_str(),
        message
    )
    .execute(store.pool())
    .await
    .context("failed to record the instance's failure")?;
    Ok(())
}

/// The instance's record, when the instance exists.
pub(super) struct Record {
    /// For a runner, its link and what it applied of the tracked branch.
    pub runner: Option<RunnerStatus>,
    pub revision: Option<Revision>,
    /// Whether the instance's server registered or resumed since the Hub last restarted
    /// or created its sandbox.
    pub served: bool,
    pub failed: Option<Failure>,
    pub warning: Option<String>,
    pub ungranted_providers: Vec<String>,
}

pub(super) async fn read(
    store: &PlatformStore,
    deployment: DeploymentId,
) -> Result<Option<Record>> {
    let Some(row) = sqlx::query_file!("src/factory/record/read.sql", deployment.as_uuid())
        .fetch_optional(store.pool())
        .await
        .context("failed to read the instance")?
    else {
        return Ok(None);
    };
    let failed = match (row.failed_stage, row.failed_message) {
        (Some(stage), Some(message)) => Some(Failure {
            stage: Stage::parse(&stage)?,
            message,
        }),
        _ => None,
    };
    let runner = match (
        row.link,
        row.linked_repository,
        row.tracked_branch,
        row.tracked_branch_name,
    ) {
        (Some(link), Some(repository), Some(id), Some(name)) => Some(RunnerStatus {
            link: LinkName::parse(link).context("the stored link is malformed")?,
            linked_repository: repository,
            tracked_branch: TrackedBranch { id, name },
            linked_revision: row
                .linked_revision
                .map(Revision::parse)
                .transpose()
                .context("the stored linked revision is malformed")?,
        }),
        _ => None,
    };
    Ok(Some(Record {
        runner,
        revision: row
            .revision
            .map(Revision::parse)
            .transpose()
            .context("the stored instance revision is malformed")?,
        served: row.served,
        failed,
        warning: row.warning,
        ungranted_providers: row.ungranted_providers,
    }))
}
