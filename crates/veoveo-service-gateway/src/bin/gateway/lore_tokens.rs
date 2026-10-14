//! Lore access tokens. A client the installation grants Lore repositories receives an
//! RFC 9068 access token for the Lore server whose `lore_resources` claim carries those
//! grants in the shape Lore's per-repository authorizer reads. The token carries no
//! scope and no Veoveo invocation claims, because Lore reads neither.

use std::time::Instant;

use anyhow::anyhow;
use axum::http::StatusCode;
use chrono::{TimeDelta, Utc};
use jsonwebtoken::encode;
use serde::Serialize;
use veoveo_sdk_rust::{JwtId, OAuthClientId};
use veoveo_service_gateway::GatewayCatalog;
use veoveo_service_gateway::contract::{AuthOutcome, AuthReasonCode};
use veoveo_service_gateway::control_plane::{
    LoreGrant, LoreResource, LoreResourceGrant, ResourceAuthorizationServer,
};
use veoveo_service_gateway::http::auth_support::{AuthAuditTarget, access_token_signing_key};

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_token_auth_audit},
    http_util::{TokenResponse, oauth_error_response, token_response},
    runtime::AppState,
    tokens::{ACCESS_TOKEN_TTL_SECONDS, IssuedAccessToken},
};

#[derive(Serialize)]
struct LoreAccessTokenClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    client_id: &'a str,
    aud: &'a str,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    lore_resources: Vec<LoreResourceGrant>,
    /// Lore lets a service account push to protected branches.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_service_account: bool,
}

/// Answer a client-credentials request for the Lore resource from a client that has
/// already authenticated.
pub(super) async fn lore_token_response(
    state: &AppState,
    catalog: &GatewayCatalog,
    lore: &LoreResource,
    authorization_server: &ResourceAuthorizationServer,
    client_id: &OAuthClientId,
    scope: Option<&str>,
    started_at: Instant,
) -> axum::response::Response {
    let deny = |reason| AuthAuditRecord {
        authorization_server: Some(authorization_server),
        client_id: Some(client_id),
        principal: None,
        outcome: AuthOutcome::Deny,
        reason,
        started_at,
    };
    let target = || AuthAuditTarget {
        profile: None,
        protected_resource: &lore.resource,
    };
    if scope.is_some_and(|scope| !scope.trim().is_empty()) {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            target(),
            deny(AuthReasonCode::InvalidScope),
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_scope",
            "Lore tokens carry repository grants and no scope",
        );
    }
    // Validation gives every client that may request the resource one grant; a
    // catalog revision that removed it leaves the client nothing to receive.
    let Some(grant) = lore.grant(client_id) else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            target(),
            deny(AuthReasonCode::InvalidClient),
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "the installation grants this client no Lore repositories",
        );
    };
    let token = match issue_lore_access_token(catalog, authorization_server, lore, client_id, grant)
        .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue a Lore access token: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                target(),
                deny(AuthReasonCode::TokenSigningKeyUnavailable),
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    if let Err(err) = record_token_auth_audit(
        &state.gateway_state,
        target(),
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(client_id),
            principal: None,
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    )
    .await
    {
        return auth_audit_error_response(err);
    }
    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: String::new(),
        refresh_token: None,
        refresh_token_expires_in: None,
    })
}

async fn issue_lore_access_token(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
    lore: &LoreResource,
    client_id: &OAuthClientId,
    grant: &LoreGrant,
) -> anyhow::Result<IssuedAccessToken> {
    let signing_key = access_token_signing_key(catalog, authorization_server).await?;
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::seconds(ACCESS_TOKEN_TTL_SECONDS))
        .ok_or_else(|| anyhow!("access token expiration overflow"))?;
    let seconds = |at: chrono::DateTime<Utc>| {
        u64::try_from(at.timestamp()).map_err(|_| anyhow!("timestamp before Unix epoch"))
    };
    let jwt_id = JwtId::parse(uuid::Uuid::new_v4().to_string())?;
    let issuer = authorization_server.issuer.to_string();
    let claims = LoreAccessTokenClaims {
        iss: &issuer,
        sub: client_id.as_str(),
        client_id: client_id.as_str(),
        aud: lore.resource.as_str(),
        exp: seconds(expires_at)?,
        nbf: seconds(now)?,
        iat: seconds(now)?,
        jti: jwt_id.to_string(),
        lore_resources: grant.resource_grants(),
        is_service_account: grant.service_account,
    };
    let access_token = encode(&signing_key.header(), &claims, signing_key.encoding_key())?;
    Ok(IssuedAccessToken {
        access_token,
        jwt_id,
    })
}
