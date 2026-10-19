//! Public resource exchange is distinct from registered-server delegation. Only
//! a current session login token can request the registered owner's typed target.
use crate::{
    audit::{AuthAuditRecord, internal_error_response, record_protocol_auth_audit},
    http_util::{allowed_hub_jwt_algorithms, load_jwks, oauth_error_response, scope_string},
    oauth_grants::TokenRequest,
    runtime::{AppState, current_http_client},
};
use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::time::Instant;
use veoveo_hub::{
    BearerToken, HubCatalog, JwtAuthConfig, JwtVerifier, RegisteredProtectedResource,
    contract::{AuthMethod, AuthOutcome, AuthReasonCode, OAuthGrantType},
    control_plane::ResourceAuthorizationServer,
    token_profile::{ProfileRequest, TokenContext, TokenPurpose},
};
const ACCESS: &str = "urn:ietf:params:oauth:token-type:access_token";
#[derive(Serialize)]
struct ExchangeResponse {
    access_token: String,
    issued_token_type: &'static str,
    token_type: &'static str,
    expires_in: u64,
    scope: String,
}
pub(super) async fn token(
    state: &AppState,
    catalog: &HubCatalog,
    resource: RegisteredProtectedResource<'_>,
    server: &ResourceAuthorizationServer,
    request: TokenRequest,
) -> Response {
    let started_at = Instant::now();
    let Ok(client_id) = veoveo_hub_sdk::OAuthClientId::parse(&request.client_id) else {
        return refuse("invalid_client");
    };
    let Some(client) = catalog
        .oauth_client(&client_id)
        .filter(|c| super::device::public_client(resource, c, OAuthGrantType::TokenExchange))
    else {
        return refuse("invalid_client");
    };
    if request.client_secret.is_some()
        || request.client_assertion.is_some()
        || request.client_assertion_type.is_some()
        || request.actor_token.is_some()
        || request.audience.is_some()
        || request.authorization_details.is_some()
        || request.subject_token_type.as_deref() != Some(ACCESS)
        || request
            .requested_token_type
            .as_deref()
            .is_some_and(|v| v != ACCESS)
    {
        return refuse("invalid_request");
    }
    let Some(raw) = request.subject_token.as_deref() else {
        return refuse("invalid_request");
    };
    let verified = match async {
        let keys = load_jwks(&current_http_client(&state.http), &server.jwks).await?;
        let verifier = JwtVerifier::new(
            JwtAuthConfig::new(
                server.issuer.clone(),
                resource.protected_resource().clone(),
                Default::default(),
                allowed_hub_jwt_algorithms(),
            )?,
            keys,
        )
        .with_extensions(state.token_profiles.claim_registry())?;
        let bearer = BearerToken::from_authorization_header(&format!("Bearer {raw}"))?;
        anyhow::Ok(verifier.verify(&bearer)?)
    }
    .await
    {
        Ok(v) => v,
        Err(_) => return refuse("invalid_grant"),
    };
    if verified.purpose != Some(TokenPurpose::Login)
        || verified.access_token.oauth_client_id != client.id
    {
        return refuse("invalid_grant");
    }
    let Some(family_id) = verified.access_token.session_family.as_ref() else {
        return refuse("invalid_grant");
    };
    let family = match state.hub_state.hub_store().refresh_family(family_id).await {
        Ok(Some(f)) => f,
        Ok(None) => return refuse("invalid_grant"),
        Err(e) => return internal_error_response(e),
    };
    // Use the same current family admission as other hosted callers. No new
    // Work Context membership requirement is imposed on contributed resources.
    match state
        .hub_state
        .access_token_session_valid(
            resource.protected_resource(),
            &server.id,
            &verified.access_token,
            &verified.principal,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => return refuse("invalid_grant"),
        Err(e) => return internal_error_response(e),
    }
    if !family.scopes.is_subset(&client.allowed_scopes)
        || family.scopes != resource.supported_scopes(catalog)
    {
        return refuse("invalid_grant");
    }
    let issued = match async {
        let context = TokenContext::admit(
            catalog,
            resource.protected_resource(),
            &client_id,
            &family.principal.principal,
            &family.work_context,
            &family.scopes,
            Some(family_id),
        )?
        .expires_before(family.expires_at.min(verified.access_token.expires_at));
        state
            .token_profiles
            .issue(
                context,
                ProfileRequest::Exchange {
                    resource: request.resource.as_deref(),
                    scope: request.scope.as_deref(),
                },
                Some(&family.principal.display_name),
            )
            .await
    }
    .await
    {
        Ok(t) => t,
        Err(error) if error.is_denied() => {
            if let Err(audit_error) = record_protocol_auth_audit(
                &state.hub_state,
                resource,
                AuthAuditRecord {
                    authorization_server: Some(server),
                    client_id: Some(&client_id),
                    principal: Some(&family.principal.principal),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::PolicyDenied,
                    started_at,
                },
                AuthMethod::PublicTokenExchange,
            )
            .await
            {
                return internal_error_response(audit_error);
            }
            return refuse("invalid_scope");
        }
        Err(error) => {
            tracing::error!(%error, "resource token issuance unavailable");
            if let Err(audit_error) = record_protocol_auth_audit(
                &state.hub_state,
                resource,
                AuthAuditRecord {
                    authorization_server: Some(server),
                    client_id: Some(&client_id),
                    principal: Some(&family.principal.principal),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
                AuthMethod::PublicTokenExchange,
            )
            .await
            {
                return internal_error_response(audit_error);
            }
            return oauth_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "token issuer is unavailable",
            );
        }
    };
    if let Err(e) = record_protocol_auth_audit(
        &state.hub_state,
        resource,
        AuthAuditRecord {
            authorization_server: Some(server),
            client_id: Some(&client_id),
            principal: Some(&family.principal.principal),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
        AuthMethod::PublicTokenExchange,
    )
    .await
    {
        return internal_error_response(e);
    }
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(ExchangeResponse {
            expires_in: issued.expires_in(),
            scope: scope_string(&issued.scopes),
            access_token: issued.access_token,
            issued_token_type: ACCESS,
            token_type: "Bearer",
        }),
    )
        .into_response()
}
fn refuse(code: &'static str) -> Response {
    oauth_error_response(
        StatusCode::BAD_REQUEST,
        code,
        "token exchange could not be authorized",
    )
}
