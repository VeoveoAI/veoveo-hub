//! The token endpoint of the Hub's own clients that act for a person in Lore: a factory
//! instance's and a Computer's. Each authenticates with the secret the gateway holds in
//! its refresh custody (`client_secret_post`), requests no scope or resource, and
//! receives the person's Lore token, audited against the Lore deployment.
use std::time::Instant;

use axum::{http::StatusCode, response::Response};
use veoveo_hub::{
    HubCatalog, RegisteredProtectedResource,
    contract::{AuthMethod, AuthOutcome, AuthReasonCode},
    issuance::IssuedAccessToken,
    token_profile::TokenIssueError,
};
use veoveo_types::OAuthClientId;

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_protocol_auth_audit},
    http_util::{TokenResponse, oauth_error_response, scope_string, token_response},
    oauth_grants::TokenRequest,
    runtime::AppState,
};

pub(super) fn invalid_client() -> Response {
    oauth_error_response(
        StatusCode::BAD_REQUEST,
        "invalid_client",
        "client authentication failed",
    )
}

/// The secret the client posted, unless the request authenticates otherwise.
pub(super) fn secret(request: &TokenRequest) -> Option<&str> {
    if request.client_assertion.is_some() || request.client_assertion_type.is_some() {
        return None;
    }
    request.client_secret.as_deref()
}

/// The OAuth error a client receives when the person it acts for holds no grant.
pub(super) struct Denial {
    pub error: &'static str,
    pub description: &'static str,
}

/// The answer with the token `issued` for the client: the token, or `denied` when the
/// person the client acts for holds no grant, each audited for `resource`.
pub(super) async fn answer(
    state: &AppState,
    catalog: &HubCatalog,
    client: &OAuthClientId,
    resource: RegisteredProtectedResource<'_>,
    started_at: Instant,
    issued: Result<IssuedAccessToken, TokenIssueError>,
    denied: Denial,
) -> Response {
    let authorization_server = catalog.authorization_server(resource.authorization_server());
    let audit = |outcome, reason| AuthAuditRecord {
        authorization_server,
        client_id: Some(client),
        principal: None,
        outcome,
        reason,
        started_at,
    };
    let method = AuthMethod::ClientCredentialsSecretPost;
    match issued {
        Ok(token) => {
            if let Err(error) = record_protocol_auth_audit(
                &state.hub_state,
                resource.audit_target(),
                audit(AuthOutcome::Allow, AuthReasonCode::AuthAllow),
                method,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            token_response(TokenResponse {
                expires_in: token.expires_in(),
                scope: scope_string(&token.scopes),
                access_token: token.access_token,
                token_type: "Bearer",
                refresh_token: None,
                refresh_token_expires_in: None,
            })
        }
        Err(TokenIssueError::Denied(_)) => {
            if let Err(error) = record_protocol_auth_audit(
                &state.hub_state,
                resource.audit_target(),
                audit(AuthOutcome::Deny, AuthReasonCode::PolicyDenied),
                method,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            // RFC 6749 §5.2: the person the client acts for holds no grant.
            oauth_error_response(StatusCode::BAD_REQUEST, denied.error, denied.description)
        }
        Err(TokenIssueError::Unavailable(_)) => oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "the Lore token is unavailable",
        ),
    }
}
