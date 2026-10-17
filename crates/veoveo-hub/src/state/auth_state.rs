use crate::contract::AuthorizationServerId;
use crate::contract::{OAuthAuthorizationCode, OAuthStateValue};
use crate::control_plane::{HubAuthorizationCodeRecord, HubAuthorizationRequest, HubJwtRevocation};
use crate::store::{ReplayIdentifier, ReplayKind};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use veoveo_hub_sdk::{HubProfileId, JwtId, OAuthClientId, TokenIssuer};
use veoveo_store::{MaintenanceBatch, Pruned};

use super::HubState;

/// The expired replay identifiers one retention pass deleted, by the token kind that
/// carried them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubReplayRetentionSummary {
    pub client_assertion_jtis: Pruned,
    pub id_jag_jtis: Pruned,
}

impl HubState {
    pub async fn record_jwt_revocation(&self, revocation: &HubJwtRevocation) -> Result<()> {
        self.store
            .record_jwt_revocation(revocation)
            .await
            .context("failed to persist hub JWT revocation")
    }

    pub async fn jwt_revocation(
        &self,
        profile: &HubProfileId,
        issuer: &TokenIssuer,
        jwt_id: &JwtId,
        now: DateTime<Utc>,
    ) -> Result<Option<HubJwtRevocation>> {
        self.store
            .jwt_revocation(profile, issuer, jwt_id, now)
            .await
            .context("failed to read hub JWT revocation")
    }

    /// Delete the revocations of expired tokens, one short transaction per batch.
    pub async fn prune_expired_jwt_revocations(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned> {
        self.store
            .prune_jwt_revocations(now, batch)
            .await
            .context("failed to prune expired hub JWT revocations")
    }

    pub async fn record_client_assertion_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.claim_replay_id(
            ReplayIdentifier {
                kind: ReplayKind::ClientAssertion,
                authorization_server,
                client_id,
                jwt_id,
                expires_at,
            },
            now,
        )
        .await
    }

    pub async fn record_id_jag_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.claim_replay_id(
            ReplayIdentifier {
                kind: ReplayKind::IdJag,
                authorization_server,
                client_id,
                jwt_id,
                expires_at,
            },
            now,
        )
        .await
    }

    /// Delete expired replay identifiers of each kind, one short transaction per batch.
    pub async fn prune_expired_replay_ids(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<HubReplayRetentionSummary> {
        let client_assertion_jtis = self
            .store
            .prune_replay_ids(ReplayKind::ClientAssertion, now, batch)
            .await
            .context("failed to prune client assertion replay identifiers")?;
        let id_jag_jtis = self
            .store
            .prune_replay_ids(ReplayKind::IdJag, now, batch)
            .await
            .context("failed to prune ID-JAG replay identifiers")?;
        Ok(HubReplayRetentionSummary {
            client_assertion_jtis,
            id_jag_jtis,
        })
    }

    pub async fn record_authorization_request(
        &self,
        request: &HubAuthorizationRequest,
    ) -> Result<()> {
        self.store
            .record_authorization_request(request)
            .await
            .context("failed to persist OAuth authorization request")
    }

    pub async fn consume_authorization_request(
        &self,
        idp_state: &OAuthStateValue,
        now: DateTime<Utc>,
    ) -> Result<Option<HubAuthorizationRequest>> {
        self.store
            .consume_authorization_request(idp_state, now)
            .await
            .context("failed to consume OAuth authorization request")
    }

    pub async fn record_authorization_code(&self, code: &HubAuthorizationCodeRecord) -> Result<()> {
        self.store
            .record_authorization_code(code)
            .await
            .context("failed to persist OAuth authorization code")
    }

    pub async fn consume_authorization_code(
        &self,
        code: &OAuthAuthorizationCode,
        now: DateTime<Utc>,
    ) -> Result<Option<HubAuthorizationCodeRecord>> {
        self.store
            .consume_authorization_code(code, now)
            .await
            .context("failed to consume OAuth authorization code")
    }

    /// Delete expired authorization requests and codes, one short transaction per batch.
    pub async fn prune_expired_authorization_records(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned> {
        self.store
            .prune_authorization_records(now, batch)
            .await
            .context("failed to prune expired OAuth authorization records")
    }

    async fn claim_replay_id(
        &self,
        identifier: ReplayIdentifier<'_>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.store
            .claim_replay_id(identifier, now)
            .await
            .context("failed to atomically register hub replay identifier")
    }
}
