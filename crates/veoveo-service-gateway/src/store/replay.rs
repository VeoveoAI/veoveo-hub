//! Single-use identifiers of client assertions and identity assertion grants.
use crate::contract::AuthorizationServerId;
use chrono::{DateTime, Utc};
use veoveo_sdk_rust::JwtId;
use veoveo_types::OAuthClientId;

use super::{GatewayStore, GatewayStoreError};
use veoveo_store::{MaintenanceBatch, Pruned};

/// What a replay identifier was carried by.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub enum ReplayKind {
    /// An RFC 7523 client assertion.
    #[vocabulary(rename = "client_assertion")]
    ClientAssertion,
    /// An identity assertion JWT authorization grant.
    #[vocabulary(rename = "id_jag")]
    IdJag,
}

/// One `jti` as its issuer and client scope it, with the expiry of the JWT that
/// carried it.
#[derive(Clone, Copy, Debug)]
pub struct ReplayIdentifier<'a> {
    pub kind: ReplayKind,
    pub authorization_server: &'a AuthorizationServerId,
    pub client_id: &'a OAuthClientId,
    pub jwt_id: &'a JwtId,
    pub expires_at: DateTime<Utc>,
}

impl GatewayStore {
    /// Claim an identifier at `now`. Returns false while an earlier claim is live; an
    /// expired claim passes to exactly one caller.
    pub async fn claim_replay_id(
        &self,
        identifier: ReplayIdentifier<'_>,
        now: DateTime<Utc>,
    ) -> Result<bool, GatewayStoreError> {
        let claimed = sqlx::query_file_scalar!(
            "src/store/replay/claim.sql",
            identifier.kind as ReplayKind,
            identifier.authorization_server.as_str(),
            identifier.client_id.as_str(),
            identifier.jwt_id.as_str(),
            now,
            identifier.expires_at,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(claimed.is_some())
    }

    /// Delete the identifiers of one kind that expired at or before `now`, in batches.
    pub async fn prune_replay_ids(
        &self,
        kind: ReplayKind,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned, GatewayStoreError> {
        Pruned::in_batches(batch, |limit| async move {
            Ok(
                sqlx::query_file!("src/store/replay/prune.sql", kind as ReplayKind, now, limit)
                    .execute(self.pool())
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }
}
