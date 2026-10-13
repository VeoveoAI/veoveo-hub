//! Cleanup of the routes the Gateway keeps for the upstream Tasks it projected. The
//! Gateway creates and reads routes with its own statements.
use chrono::{DateTime, Utc};

use super::{GatewayStore, GatewayStoreError};
use veoveo_platform_store::{MaintenanceBatch, Pruned};

impl GatewayStore {
    /// Delete the Task routes that expired at or before `now`, in batches. The Gateway
    /// schedules it; replicas that run it at once take different routes.
    pub async fn prune_task_routes(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned, GatewayStoreError> {
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
