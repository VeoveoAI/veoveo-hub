//! Per-principal selection: the branch whose instance serves each server a principal
//! names. Changes notify on the `hub` channel, keyed by principal.
use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::Utc;
use veoveo_hub_sdk::ServerSlug;
use veoveo_types::PrincipalId;

use crate::execution::BranchName;

impl crate::store::HubStore {
    /// The branch `principal` selected for each server it names.
    pub async fn selection(
        &self,
        principal: &PrincipalId,
    ) -> Result<BTreeMap<ServerSlug, BranchName>> {
        sqlx::query_file!("src/store/selection/read.sql", principal.as_str())
            .fetch_all(self.pool())
            .await
            .context("read the principal's selection")?
            .into_iter()
            .map(|row| {
                Ok((
                    ServerSlug::parse(row.server).context("stored selection names no server")?,
                    BranchName::parse(row.branch).context("stored selection names no branch")?,
                ))
            })
            .collect()
    }

    /// Serve `server` to `principal` from the instance of `branch`.
    pub async fn select(
        &self,
        principal: &PrincipalId,
        server: &ServerSlug,
        branch: &BranchName,
    ) -> Result<()> {
        sqlx::query_file!(
            "src/store/selection/select.sql",
            principal.as_str(),
            server.as_str(),
            branch.as_str(),
            Utc::now(),
        )
        .execute(self.pool())
        .await
        .context("record the selection")?;
        Ok(())
    }

    /// Serve `server` to `principal` from the instance of the base branch again.
    pub async fn clear_selection(
        &self,
        principal: &PrincipalId,
        server: &ServerSlug,
    ) -> Result<()> {
        sqlx::query_file!(
            "src/store/selection/clear.sql",
            principal.as_str(),
            server.as_str()
        )
        .execute(self.pool())
        .await
        .context("clear the selection")?;
        Ok(())
    }
}
