//! Cleanup of the routes the Hub keeps for the upstream Tasks it projected. The
//! Hub creates and reads routes with its own statements.
use chrono::{DateTime, Utc};

use super::{HubStore, HubStoreError};
use veoveo_store::{MaintenanceBatch, Pruned};

impl HubStore {
    /// Delete the Task routes that expired at or before `now`, in batches. The Hub
    /// schedules it; replicas that run it at once take different routes.
    pub async fn prune_task_routes(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned, HubStoreError> {
        Pruned::in_batches(batch, |limit| async move {
            Ok(
                sqlx::query_file!("src/store/task_routes/prune.sql", now, limit)
                    .execute(self.pool())
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }
}
