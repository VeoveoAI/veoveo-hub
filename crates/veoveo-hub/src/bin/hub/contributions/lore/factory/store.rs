//! What the Hub records about watching: each watched repository with its watcher and why
//! the last subscription as them failed, and why a branch's latest push could not be
//! prepared.
use anyhow::{Context, Result};
use chrono::Utc;
use veoveo_hub::{execution::BranchName, factory::Revision};
use veoveo_store::PlatformStore;
use veoveo_types::PrincipalId;

use super::Watched;
use crate::contributions::lore::{DeploymentId, RepositoryId};

/// A watched repository's watcher, and why the last subscription as them failed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchStatus {
    pub watcher: PrincipalId,
    pub failed: Option<String>,
}

/// Why a branch's latest push could not be prepared.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrepareFailure {
    pub revision: Revision,
    pub pusher: PrincipalId,
    pub message: String,
}

#[derive(Clone)]
pub(crate) struct WatchStore(PlatformStore);

impl WatchStore {
    pub fn new(store: PlatformStore) -> Self {
        Self(store)
    }

    pub async fn watch(&self, watched: &Watched, watcher: &PrincipalId) -> Result<()> {
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/factory/watch.sql",
            watched.deployment.as_str(),
            watched.repository.as_str(),
            watcher.as_str(),
            Utc::now()
        )
        .execute(self.0.pool())
        .await
        .context("failed to record the watch")?;
        Ok(())
    }

    /// Every watched repository and its watcher.
    pub async fn watches(&self) -> Result<Vec<(Watched, PrincipalId)>> {
        sqlx::query_file!("src/bin/hub/contributions/lore/factory/watches.sql")
            .fetch_all(self.0.pool())
            .await
            .context("failed to read the watches")?
            .into_iter()
            .map(|row| {
                Ok((
                    Watched {
                        deployment: DeploymentId::parse(row.deployment)?,
                        repository: RepositoryId::parse(row.repository)?,
                    },
                    PrincipalId::parse(row.watcher)?,
                ))
            })
            .collect::<Result<_>>()
            .context("a stored watch is malformed")
    }

    /// The subscription as `watcher` succeeded, or failed with `failure`. Nothing changes
    /// once someone else watches the repository.
    pub async fn subscribed(
        &self,
        watched: &Watched,
        watcher: &PrincipalId,
        failure: Option<&str>,
    ) -> Result<()> {
        let result = match failure {
            None => {
                sqlx::query_file!(
                    "src/bin/hub/contributions/lore/factory/subscribed.sql",
                    watched.deployment.as_str(),
                    watched.repository.as_str(),
                    watcher.as_str(),
                    Utc::now()
                )
                .execute(self.0.pool())
                .await
            }
            Some(failure) => {
                sqlx::query_file!(
                    "src/bin/hub/contributions/lore/factory/unsubscribed.sql",
                    watched.deployment.as_str(),
                    watched.repository.as_str(),
                    watcher.as_str(),
                    failure,
                    Utc::now()
                )
                .execute(self.0.pool())
                .await
            }
        };
        result.context("failed to record the subscription")?;
        Ok(())
    }

    pub async fn status(&self, watched: &Watched) -> Result<Option<WatchStatus>> {
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/factory/status.sql",
            watched.deployment.as_str(),
            watched.repository.as_str()
        )
        .fetch_optional(self.0.pool())
        .await
        .context("failed to read the watch")?
        .map(|row| {
            Ok(WatchStatus {
                watcher: PrincipalId::parse(row.watcher)?,
                failed: row.failed_message,
            })
        })
        .transpose()
    }

    /// The branch's latest push was prepared, or failed to be.
    pub async fn prepared(
        &self,
        watched: &Watched,
        branch: &BranchName,
        failure: Option<(&Revision, &PrincipalId, &str)>,
    ) -> Result<()> {
        let result = match failure {
            None => {
                sqlx::query_file!(
                    "src/bin/hub/contributions/lore/factory/prepared.sql",
                    watched.deployment.as_str(),
                    watched.repository.as_str(),
                    branch.as_str()
                )
                .execute(self.0.pool())
                .await
            }
            Some((revision, pusher, message)) => {
                sqlx::query_file!(
                    "src/bin/hub/contributions/lore/factory/unprepared.sql",
                    watched.deployment.as_str(),
                    watched.repository.as_str(),
                    branch.as_str(),
                    revision.as_str(),
                    pusher.as_str(),
                    message,
                    Utc::now()
                )
                .execute(self.0.pool())
                .await
            }
        };
        result.context("failed to record the preparation")?;
        Ok(())
    }

    pub async fn prepare_failure(
        &self,
        watched: &Watched,
        branch: &BranchName,
    ) -> Result<Option<PrepareFailure>> {
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/factory/prepare_failure.sql",
            watched.deployment.as_str(),
            watched.repository.as_str(),
            branch.as_str()
        )
        .fetch_optional(self.0.pool())
        .await
        .context("failed to read the preparation")?
        .map(|row| {
            Ok(PrepareFailure {
                revision: Revision::parse(row.revision)?,
                pusher: PrincipalId::parse(row.pusher)?,
                message: row.message,
            })
        })
        .transpose()
    }
}
