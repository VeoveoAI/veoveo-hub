//! Revoked JWTs, kept until the revoked token expires.
use crate::control_plane::GatewayJwtRevocation;
use chrono::{DateTime, Utc};
use veoveo_sdk_rust::GatewayProfileId;
use veoveo_sdk_rust::JwtId;
use veoveo_types::TokenIssuer;

use super::admitted;
use super::{GatewayStore, GatewayStoreError};
use veoveo_platform_store::{MaintenanceBatch, Pruned};

impl GatewayStore {
    /// Record a revocation, replacing an earlier one of the same token.
    pub async fn record_jwt_revocation(
        &self,
        revocation: &GatewayJwtRevocation,
    ) -> Result<(), GatewayStoreError> {
        sqlx::query_file!(
            "src/store/revocations/record.sql",
            revocation.profile.as_str(),
            revocation.issuer.as_str(),
            revocation.jwt_id.as_str(),
            revocation.revoked_at,
            revocation.expires_at,
            revocation.reason.as_deref(),
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The revocation of a token that has not yet expired at `now`.
    pub async fn jwt_revocation(
        &self,
        profile: &GatewayProfileId,
        issuer: &TokenIssuer,
        jwt_id: &JwtId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayJwtRevocation>, GatewayStoreError> {
        let row = sqlx::query_file!(
            "src/store/revocations/find.sql",
            profile.as_str(),
            issuer.as_str(),
            jwt_id.as_str(),
            now,
        )
        .fetch_optional(self.pool())
        .await?;
        row.map(|row| {
            Ok(GatewayJwtRevocation {
                profile: admitted(
                    "jwt_revocation.profile",
                    GatewayProfileId::parse(row.profile),
                )?,
                issuer: admitted("jwt_revocation.issuer", TokenIssuer::parse(row.issuer))?,
                jwt_id: admitted("jwt_revocation.jwt_id", JwtId::parse(row.jwt_id))?,
                revoked_at: row.revoked_at,
                expires_at: row.expires_at,
                reason: row.reason,
            })
        })
        .transpose()
    }

    /// Delete the revocations of tokens that expired at or before `now`, in batches.
    pub async fn prune_jwt_revocations(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned, GatewayStoreError> {
        Pruned::in_batches(batch, |limit| async move {
            Ok(
                sqlx::query_file!("src/store/revocations/prune.sql", now, limit)
                    .execute(self.pool())
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }
}
