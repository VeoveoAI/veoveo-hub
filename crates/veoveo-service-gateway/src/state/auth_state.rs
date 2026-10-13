use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use veoveo_gateway_contract::AuthorizationServerId;
use veoveo_mcp_contract::{
    GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest, GatewayJwtRevocation,
    GatewayProfileId, JwtId, OAuthAuthorizationCode, OAuthClientId, OAuthStateValue, TokenIssuer,
};
use veoveo_platform_store::{
    MaintenanceBatch, Pruned,
    gateway::{ReplayIdentifier, ReplayKind},
};

use super::GatewayState;

/// The expired replay identifiers one retention pass deleted, by the token kind that
/// carried them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayReplayRetentionSummary {
    pub client_assertion_jtis: Pruned,
    pub id_jag_jtis: Pruned,
}

impl GatewayState {
    pub async fn record_jwt_revocation(&self, revocation: &GatewayJwtRevocation) -> Result<()> {
        self.platform
            .record_jwt_revocation(revocation)
            .await
            .context("failed to persist gateway JWT revocation")
    }

    pub async fn jwt_revocation(
        &self,
        profile: &GatewayProfileId,
        issuer: &TokenIssuer,
        jwt_id: &JwtId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayJwtRevocation>> {
        self.platform
            .jwt_revocation(profile, issuer, jwt_id, now)
            .await
            .context("failed to read gateway JWT revocation")
    }

    /// Delete the revocations of expired tokens, one short transaction per batch.
    pub async fn prune_expired_jwt_revocations(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned> {
        self.platform
            .prune_jwt_revocations(now, batch)
            .await
            .context("failed to prune expired gateway JWT revocations")
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
    ) -> Result<GatewayReplayRetentionSummary> {
        let client_assertion_jtis = self
            .platform
            .prune_replay_ids(ReplayKind::ClientAssertion, now, batch)
            .await
            .context("failed to prune client assertion replay identifiers")?;
        let id_jag_jtis = self
            .platform
            .prune_replay_ids(ReplayKind::IdJag, now, batch)
            .await
            .context("failed to prune ID-JAG replay identifiers")?;
        Ok(GatewayReplayRetentionSummary {
            client_assertion_jtis,
            id_jag_jtis,
        })
    }

    pub async fn record_authorization_request(
        &self,
        request: &GatewayAuthorizationRequest,
    ) -> Result<()> {
        self.platform
            .record_authorization_request(request)
            .await
            .context("failed to persist OAuth authorization request")
    }

    pub async fn consume_authorization_request(
        &self,
        idp_state: &OAuthStateValue,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationRequest>> {
        self.platform
            .consume_authorization_request(idp_state, now)
            .await
            .context("failed to consume OAuth authorization request")
    }

    pub async fn record_authorization_code(
        &self,
        code: &GatewayAuthorizationCodeRecord,
    ) -> Result<()> {
        self.platform
            .record_authorization_code(code)
            .await
            .context("failed to persist OAuth authorization code")
    }

    pub async fn consume_authorization_code(
        &self,
        code: &OAuthAuthorizationCode,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationCodeRecord>> {
        self.platform
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
        self.platform
            .prune_authorization_records(now, batch)
            .await
            .context("failed to prune expired OAuth authorization records")
    }

    async fn claim_replay_id(
        &self,
        identifier: ReplayIdentifier<'_>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.platform
            .claim_replay_id(identifier, now)
            .await
            .context("failed to atomically register gateway replay identifier")
    }
}
