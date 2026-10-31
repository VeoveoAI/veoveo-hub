//! An administrator ends every session of a person.
use chrono::{DateTime, Utc};
use veoveo_types::{PrincipalId, TenantId};

use super::{HubStore, HubStoreError, RefreshRevocation};

impl HubStore {
    /// End every session of `principal` in `tenant` at `now`, in one transaction: clear
    /// their Computer's Lore client, so it mints no further Lore token, then revoke each of
    /// their unexpired families that is not revoked yet, with its successor deliveries.
    /// Answers how many families it revoked.
    pub async fn end_person_sessions(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        now: DateTime<Utc>,
    ) -> Result<u64, HubStoreError> {
        let mut transaction = self.pool().begin().await?;
        sqlx::query_file!(
            "src/store/sessions/clear_lore_client.sql",
            tenant.as_str(),
            principal.as_str()
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let mut families = Vec::new();
        loop {
            let batch = sqlx::query_file!(
                "src/store/sessions/revoke_families.sql",
                tenant.as_str(),
                principal.as_str(),
                now,
                RefreshRevocation::AdministratorRevocation as RefreshRevocation,
            )
            .fetch_all(&mut *transaction)
            .await?;
            let done = batch.len() < 1000;
            families.extend(batch);
            if done {
                break;
            }
        }
        for family in &families {
            while sqlx::query_file!("src/store/computers/clear_deliveries.sql", family.id)
                .execute(&mut *transaction)
                .await?
                .rows_affected()
                > 0
            {}
        }
        transaction.commit().await?;
        Ok(u64::try_from(families.len()).unwrap_or(u64::MAX))
    }
}

#[cfg(test)]
mod tests;
