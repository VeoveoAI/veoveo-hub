//! Per-principal selection: the branch whose instance serves each server a principal
//! names.
use std::collections::BTreeMap;

use anyhow::{Context, Result};
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
}
