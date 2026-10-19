//! Shared upstream browser broker. A typed destination determines downstream
//! code delivery or native device approval; identity proof is the same flow.
use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, internal_error_response, record_oidc_auth_audit,
    },
    http_util::{
        oauth_error_response, pkce_s256_challenge, random_oauth_state, random_oidc_nonce,
        random_pkce_verifier, redirect_response, scope_string,
    },
    runtime::AppState,
};
use axum::{http::StatusCode, response::IntoResponse};
use chrono::{TimeDelta, Utc};
use std::{collections::BTreeSet, time::Instant};
use url::Url;
use veoveo_hub::contract::{AuthOutcome, AuthReasonCode, PkceCodeChallengeMethod};
use veoveo_hub::control_plane::{AuthorizationDestination, HubAuthorizationRequest};
use veoveo_hub::{HubCatalog, RegisteredProtectedResource};
use veoveo_hub_sdk::OAuthClientId;
use veoveo_types::{ScopeName, WorkContextId};
const AUTHORIZATION_REQUEST_TTL_SECONDS: i64 = 600;
pub(super) async fn begin(
    state: &AppState,
    catalog: &HubCatalog,
    profile: RegisteredProtectedResource<'_>,
    client_id: OAuthClientId,
    work_context: WorkContextId,
    scopes: BTreeSet<ScopeName>,
    destination: AuthorizationDestination,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Some(authorization_server) = catalog.authorization_server(profile.authorization_server())
    else {
        return oauth_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "authorization server is unavailable",
        );
    };
    let Some(oidc_client) = profile.oidc_client(catalog) else {
        tracing::error!(resource = %profile.protected_resource(), "protected resource is missing OIDC client registration");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "OIDC client is not configured",
        );
    };
    let Some(identity_provider) = catalog.identity_provider(&oidc_client.identity_provider) else {
        tracing::error!(identity_provider = %oidc_client.identity_provider, "unknown identity provider");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is unavailable",
        );
    };
    let Some(idp_authorization_endpoint) = identity_provider.authorization_endpoint.as_ref() else {
        tracing::error!(identity_provider = %identity_provider.id, "identity provider has no authorization endpoint");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is not configured",
        );
    };

    let idp_state = match random_oauth_state() {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let idp_code_verifier = match random_pkce_verifier() {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let idp_code_challenge = match pkce_s256_challenge(&idp_code_verifier) {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let nonce = match random_oidc_nonce() {
        Ok(value) => value,
        Err(err) => return internal_error_response(err),
    };
    let now = Utc::now();
    let expires_at =
        match now.checked_add_signed(TimeDelta::seconds(AUTHORIZATION_REQUEST_TTL_SECONDS)) {
            Some(value) => value,
            None => return internal_error_response("authorization request expiration overflow"),
        };
    let authorization_request = HubAuthorizationRequest {
        idp_state: idp_state.clone(),
        protected_resource: profile.protected_resource().clone(),
        oauth_client_id: client_id.clone(),
        work_context,
        oidc_client: oidc_client.id.clone(),
        destination,
        requested_scopes: scopes,
        idp_code_verifier,
        idp_code_challenge: idp_code_challenge.clone(),
        idp_code_challenge_method: PkceCodeChallengeMethod::S256,
        nonce: nonce.clone(),
        created_at: now,
        expires_at,
    };
    if let Err(err) = state
        .hub_state
        .record_authorization_request(&authorization_request)
        .await
    {
        tracing::error!("failed to record hub authorization request: {err}");
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::AuthStateUnavailable,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Err(err) = record_oidc_auth_audit(
        &state.hub_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
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

    let mut redirect = match Url::parse(idp_authorization_endpoint.as_str()) {
        Ok(url) => url,
        Err(err) => return internal_error_response(err),
    };
    redirect
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", oidc_client.client_id.as_str())
        .append_pair("redirect_uri", oidc_client.redirect_uri.as_str())
        .append_pair("scope", &scope_string(&oidc_client.scopes))
        .append_pair("state", idp_state.as_str())
        .append_pair("code_challenge", idp_code_challenge.as_str())
        .append_pair("code_challenge_method", "S256")
        .append_pair("nonce", nonce.as_str());
    redirect_response(redirect.as_str())
}
