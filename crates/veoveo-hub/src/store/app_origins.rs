//! Sign-ins on servers' app origins: single-use codes the issuer hands a browser, bound to
//! the state the app origin set in it, and the sessions they open on the app origin.
use chrono::{DateTime, Utc};
use veoveo_hub_sdk::HubRefreshFamilyId;
use veoveo_types::{ServerSlug, Sha256Digest};

use super::{Computer, HubStore, HubStoreError, RefreshFamily, admitted};
use crate::computers::ComputerId;

/// A redeemed code: whose session it opens, and where on the app origin it returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppOriginCode {
    pub computer: ComputerId,
    pub family: HubRefreshFamilyId,
    pub return_path: String,
}

impl HubStore {
    /// Record a code for `server`'s app origin, first pruning a batch of expired ones.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_app_origin_code(
        &self,
        code: &Sha256Digest,
        state: &Sha256Digest,
        server: &ServerSlug,
        computer: &ComputerId,
        family: &HubRefreshFamilyId,
        return_path: &str,
        expires: DateTime<Utc>,
    ) -> Result<(), HubStoreError> {
        sqlx::query_file!("src/store/app_origins/code_prune.sql", Utc::now())
            .execute(self.pool())
            .await?;
        sqlx::query_file!(
            "src/store/app_origins/code_insert.sql",
            code.as_str(),
            state.as_str(),
            server.as_str(),
            computer.as_uuid(),
            family.as_uuid(),
            return_path,
            expires
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Consumes a code only for the browser whose state it is bound to and only on
    /// `server`'s app origin, once and before expiry.
    pub async fn consume_app_origin_code(
        &self,
        code: &Sha256Digest,
        state: &Sha256Digest,
        server: &ServerSlug,
    ) -> Result<Option<AppOriginCode>, HubStoreError> {
        let row = sqlx::query_file!(
            "src/store/app_origins/code_consume.sql",
            code.as_str(),
            state.as_str(),
            server.as_str(),
            Utc::now()
        )
        .fetch_optional(self.pool())
        .await?;
        row.map(|row| {
            Ok(AppOriginCode {
                computer: admitted(
                    "app_origin_code.computer_id",
                    ComputerId::parse(row.computer_id.to_string()),
                )?,
                family: admitted(
                    "app_origin_code.family_id",
                    HubRefreshFamilyId::parse(row.family_id.to_string()),
                )?,
                return_path: row.return_path,
            })
        })
        .transpose()
    }

    pub async fn insert_app_origin_session(
        &self,
        digest: &Sha256Digest,
        server: &ServerSlug,
        computer: &ComputerId,
        family: &HubRefreshFamilyId,
    ) -> Result<(), HubStoreError> {
        sqlx::query_file!(
            "src/store/app_origins/session_insert.sql",
            digest.as_str(),
            server.as_str(),
            computer.as_uuid(),
            family.as_uuid(),
            Utc::now()
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The Computer and family of a session cookie on `server`'s app origin. Admission of
    /// the family, its expiry and revocation included, belongs to the caller.
    pub async fn app_origin_session(
        &self,
        digest: &Sha256Digest,
        server: &ServerSlug,
    ) -> Result<Option<(Computer, RefreshFamily)>, HubStoreError> {
        let row = sqlx::query_file!(
            "src/store/app_origins/session_read.sql",
            digest.as_str(),
            server.as_str()
        )
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id = admitted(
            "app_origin_session.computer_id",
            ComputerId::parse(row.computer_id.to_string()),
        )?;
        let family = admitted(
            "app_origin_session.family_id",
            HubRefreshFamilyId::parse(row.family_id.to_string()),
        )?;
        let Some(family) = self.refresh_family(&family).await? else {
            return Ok(None);
        };
        let principal = &family.principal.principal;
        let Some(tenant) = principal.tenant.as_ref() else {
            return Ok(None);
        };
        Ok(self
            .computer(tenant, &principal.id, &id)
            .await?
            .map(|computer| (computer, family)))
    }
}
