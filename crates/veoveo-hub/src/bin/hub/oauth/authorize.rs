use std::time::Instant;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use veoveo_hub::contract::{AuthOutcome, AuthReasonCode};
use veoveo_hub::contract::{
    OAuthRedirectUri, OAuthStateValue, PkceCodeChallenge, PkceCodeChallengeMethod,
};
use veoveo_hub_sdk::OAuthClientId;
use veoveo_types::WorkContextId;

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_oidc_auth_audit},
    http_util::{oauth_error_response, redirect_with_oauth_error},
    oauth_grants::{authorization_code_client_allowed, requested_token_scopes},
    runtime::{AppState, current_catalog},
};

use super::resolve_oauth_resource;

#[derive(Deserialize)]
pub(crate) struct AuthorizationRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    work_context: Option<String>,
}

pub(crate) async fn authorize_endpoint(
    State(state): State<AppState>,
    Query(request): Query<AuthorizationRequest>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let profile = match resolve_oauth_resource(
        &catalog,
        request.client_id.as_str(),
        request.resource.as_deref(),
    ) {
        Ok(profile) => profile,
        Err(response) => return *response,
    };
    let Some(authorization_server) = catalog.authorization_server(profile.authorization_server())
    else {
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            profile,
            AuthAuditRecord {
                authorization_server: None,
                client_id: None,
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnknownAuthorizationServer,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };
    if !profile.supports(veoveo_hub::contract::OAuthGrantType::AuthorizationCodePkce) {
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_response_type",
            "authorization code flow is not enabled for this protected resource",
        );
    }
    if request.response_type != "code" {
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthorizationRequest,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "authorization request is invalid",
        );
    }
    let client_id = match OAuthClientId::parse(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            if let Err(err) = record_oidc_auth_audit(
                &state.hub_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: None,
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClient,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client is not allowed for this protected resource",
            );
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_client",
            "client is not allowed for this protected resource",
        );
    };
    if !authorization_code_client_allowed(profile, client) {
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_client",
            "client is not allowed for this protected resource",
        );
    }
    let work_context = match request.work_context.as_deref() {
        Some(value) => match WorkContextId::parse(value.trim()) {
            Ok(context) if catalog.work_context(&context).is_some() => context,
            _ => {
                if let Err(err) = record_oidc_auth_audit(
                    &state.hub_state,
                    profile,
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::InvalidAuthorizationRequest,
                        started_at,
                    },
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "work_context is not available",
                );
            }
        },
        None => client.default_work_context.clone(),
    };
    let redirect_uri = match OAuthRedirectUri::new(request.redirect_uri.trim()) {
        Ok(redirect_uri) if client.redirect_uris.contains(&redirect_uri) => redirect_uri,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.hub_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationRequest,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "redirect_uri is not registered for this client",
            );
        }
    };
    let client_state = match request
        .state
        .as_deref()
        .map(OAuthStateValue::parse)
        .transpose()
    {
        Ok(state) => state,
        Err(_) => {
            if let Err(err) = record_oidc_auth_audit(
                &state.hub_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationRequest,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return redirect_with_oauth_error(
                &redirect_uri,
                "invalid_request",
                Some("state value is invalid"),
                None,
            );
        }
    };
    let scopes = match requested_token_scopes(&catalog, profile, client, request.scope.as_deref()) {
        Ok(scopes) => scopes,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected authorization scope request: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.hub_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidScope,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return redirect_with_oauth_error(
                &redirect_uri,
                "invalid_scope",
                Some("requested scope is not allowed"),
                client_state.as_ref(),
            );
        }
    };
    let code_challenge = match PkceCodeChallenge::parse(request.code_challenge.trim()) {
        Ok(challenge) if request.code_challenge_method == "S256" => challenge,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.hub_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidPkce,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return redirect_with_oauth_error(
                &redirect_uri,
                "invalid_request",
                Some("PKCE S256 code challenge is required"),
                client_state.as_ref(),
            );
        }
    };
    super::broker::begin(
        &state,
        &catalog,
        profile,
        client_id,
        work_context,
        scopes,
        veoveo_hub::control_plane::AuthorizationDestination::Code {
            redirect_uri,
            client_state,
            code_challenge,
            code_challenge_method: PkceCodeChallengeMethod::S256,
        },
    )
    .await
}
