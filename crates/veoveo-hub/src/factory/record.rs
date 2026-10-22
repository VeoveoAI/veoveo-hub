//! What the Hub records on an instance about its factory sandbox: the latest pusher,
//! the last revision it applied, whether its server registered or resumed since, the
//! last failure of its own steps and the capacity warning of its creation.
use anyhow::{Context, Result};
use veoveo_store::PlatformStore;
use veoveo_types::{DeploymentId, PrincipalId};

use super::{Failure, Revision, Stage};
use crate::{data_environment::DatabaseName, execution::BranchName};

/// The deployment and database of the instance of `branch` of `repository`.
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

/// Record `pusher` as the instance's latest pusher, before the Hub restarts or creates
/// its sandbox; until its server registers or resumes again, the instance serves no
/// revision. Returns the branch the instance serves.
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
    pub revision: Option<Revision>,
    /// Whether the instance's server registered or resumed since the Hub last restarted
    /// or created its sandbox.
    pub served: bool,
    pub failed: Option<Failure>,
    pub warning: Option<String>,
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
    Ok(Some(Record {
        revision: row
            .revision
            .map(Revision::parse)
            .transpose()
            .context("the stored instance revision is malformed")?,
        served: row.served,
        failed,
        warning: row.warning,
    }))
}
