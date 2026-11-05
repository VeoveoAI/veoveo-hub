//! Tokens on the content domain: stored by digest, each naming one item of one owner for one
//! browser session of a Computer's family.
use chrono::{DateTime, Utc};
use veoveo_hub_sdk::HubRefreshFamilyId;
use veoveo_types::Sha256Digest;

use super::{HubStore, HubStoreError, admitted};
use crate::computers::ComputerId;

/// A token's row: whose it is, what it names, and until when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentToken {
    pub owner: String,
    pub item: String,
    pub computer: ComputerId,
    pub family: HubRefreshFamilyId,
    pub expires_at: DateTime<Utc>,
}

/// A token to record.
#[derive(Debug, Clone, Copy)]
pub struct NewContentToken<'a> {
    pub digest: &'a Sha256Digest,
    pub nonce: &'a [u8],
    pub session: &'a Sha256Digest,
    pub owner: &'a str,
    pub item: &'a str,
    pub computer: &'a ComputerId,
    pub family: &'a HubRefreshFamilyId,
    pub expires_at: DateTime<Utc>,
    pub hard_expires_at: DateTime<Utc>,
}

impl HubStore {
    /// The nonce of the token that `session` holds on `item` of `owner` under `family`, while
    /// it lives.
    pub async fn live_content_token(
        &self,
        session: &Sha256Digest,
        owner: &str,
        item: &str,
        family: &HubRefreshFamilyId,
        now: DateTime<Utc>,
    ) -> Result<Option<Vec<u8>>, HubStoreError> {
        Ok(sqlx::query_file!(
            "src/store/content_tokens/live.sql",
            session.as_str(),
            owner,
            item,
            family.as_uuid(),
            now
        )
        .fetch_optional(self.pool())
        .await?
        .map(|row| row.nonce))
    }

    /// Extend a token to `until`, within its hard limit.
    pub async fn extend_content_token(
        &self,
        digest: &Sha256Digest,
        until: DateTime<Utc>,
    ) -> Result<(), HubStoreError> {
        sqlx::query_file!(
            "src/store/content_tokens/extend.sql",
            digest.as_str(),
            until
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Record a token, first pruning a batch of those that expired at or before `prune`.
    pub async fn insert_content_token(
        &self,
        token: NewContentToken<'_>,
        prune: DateTime<Utc>,
    ) -> Result<(), HubStoreError> {
        sqlx::query_file!("src/store/content_tokens/prune.sql", prune)
            .execute(self.pool())
            .await?;
        sqlx::query_file!(
            "src/store/content_tokens/insert.sql",
            token.digest.as_str(),
            token.nonce,
            token.session.as_str(),
            token.owner,
            token.item,
            token.computer.as_uuid(),
            token.family.as_uuid(),
            token.expires_at,
            token.hard_expires_at
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The row of a token. Its expiry, and the admission of its family, belong to the caller.
    pub async fn content_token(
        &self,
        digest: &Sha256Digest,
    ) -> Result<Option<ContentToken>, HubStoreError> {
        let row = sqlx::query_file!("src/store/content_tokens/read.sql", digest.as_str())
            .fetch_optional(self.pool())
            .await?;
        row.map(|row| {
            Ok(ContentToken {
                owner: row.owner,
                item: row.item,
                computer: admitted(
                    "content_token.computer_id",
                    ComputerId::parse(row.computer_id.to_string()),
                )?,
                family: admitted(
                    "content_token.family_id",
                    HubRefreshFamilyId::parse(row.family_id.to_string()),
                )?,
                expires_at: row.expires_at,
            })
        })
        .transpose()
    }
}
