use std::time::Instant;
use veoveo_hub::contract::SecretPurpose;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{TimeDelta, Utc};
use serde::Deserialize;
use veoveo_hub::contract::OAuthStateValue;
use veoveo_hub::contract::{AuthOutcome, AuthReasonCode};
use veoveo_hub::control_plane::{
    AuthorizationDestination, HubAuthorizationCodeRecord, HubAuthorizationRequest,
};
use veoveo_hub::{HubSecretResolver, OidcIdTokenConfig, OidcIdTokenVerifier, person::Person};

use crate::{
    audit::{
        AuthAuditRecord, auth_audit_error_response, internal_error_response, record_oidc_auth_audit,
    },
    http_util::{
        OidcTokenExchangeRequest, allowed_hub_jwt_algorithms, exchange_oidc_authorization_code,
        load_jwks, oauth_error_response, random_authorization_code,
        redirect_with_authorization_code, redirect_with_oauth_error,
    },
    runtime::{AppState, current_catalog, current_http_client},
};

const AUTHORIZATION_CODE_TTL_SECONDS: i64 = 5 * 60;

#[derive(Deserialize)]
pub(crate) struct AuthorizationCallback {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default, rename = "error_description")]
    _error_description: Option<String>,
}

pub(crate) async fn authorization_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(callback): Query<AuthorizationCallback>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Some(raw_state) = callback.state.as_deref() else {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "state is required",
        );
    };
    let idp_state = match OAuthStateValue::parse(raw_state.trim()) {
        Ok(value) => value,
        Err(_) => {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "state is invalid",
            );
        }
    };
    let authorization_request = match state
        .hub_state
        .consume_authorization_request(&idp_state, Utc::now())
        .await
    {
        Ok(Some(request)) => request,
        Ok(_) => {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "authorization state is invalid or expired",
            );
        }
        Err(err) => {
            tracing::error!("failed to consume hub authorization state: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if let AuthorizationDestination::Device { browser, .. } = &authorization_request.destination
        && super::device::browser_digest(&headers).ok().as_ref() != Some(browser)
    {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "initiating browser is required",
        );
    }
    let catalog = current_catalog(&state.catalog);
    let Some(profile) =
        catalog.registered_protected_resource(&authorization_request.protected_resource)
    else {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "authorization state references an unknown protected resource",
        );
    };
    let Some(authorization_server) = catalog
        .authorization_server(profile.authorization_server())
        .cloned()
    else {
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };
    if callback.error.is_some() {
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            &profile,
            AuthAuditRecord {
                authorization_server: Some(&authorization_server),
                client_id: Some(&authorization_request.oauth_client_id),
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
        return delivery_error(
            &state,
            &authorization_request,
            "access_denied",
            Some("upstream authorization was denied"),
        )
        .await;
    }
    let Some(idp_code) = callback
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return delivery_error(
            &state,
            &authorization_request,
            "invalid_request",
            Some("authorization code is required"),
        )
        .await;
    };
    let idp_code = idp_code.to_string();
    let Some(oidc_client) = catalog
        .oidc_client(&authorization_request.oidc_client)
        .cloned()
    else {
        tracing::error!(oidc_client = %authorization_request.oidc_client, "unknown OIDC client registration");
        return delivery_error(
            &state,
            &authorization_request,
            "server_error",
            Some("OIDC client is unavailable"),
        )
        .await;
    };
    if profile.oidc_client(&catalog).map(|client| &client.id) != Some(&oidc_client.id) {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "authorization broker registration changed",
        );
    }
    let Some(identity_provider) = catalog
        .identity_provider(&oidc_client.identity_provider)
        .cloned()
    else {
        tracing::error!(identity_provider = %oidc_client.identity_provider, "unknown identity provider");
        return delivery_error(
            &state,
            &authorization_request,
            "server_error",
            Some("identity provider is unavailable"),
        )
        .await;
    };
    let Some(token_endpoint) = identity_provider
        .token_endpoint
        .as_ref()
        .map(ToString::to_string)
    else {
        tracing::error!(identity_provider = %identity_provider.id, "identity provider has no token endpoint");
        return delivery_error(
            &state,
            &authorization_request,
            "server_error",
            Some("identity provider is not configured"),
        )
        .await;
    };
    let client_secret = match HubSecretResolver::new()
        .resolve_string(
            &catalog,
            &oidc_client.credential_secret,
            SecretPurpose::OAuthClientSecret,
        )
        .await
    {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!("failed to resolve OIDC client secret: {err}");
            return delivery_error(
                &state,
                &authorization_request,
                "server_error",
                Some("OIDC client secret is unavailable"),
            )
            .await;
        }
    };
    let idp_jwks_source = identity_provider.jwks.clone();
    let idp_issuer = identity_provider.issuer.clone();
    let idp_claim_mapping = identity_provider.claim_mapping.clone();
    let oidc_client_id = oidc_client.client_id.clone();
    let oidc_client_record_id = oidc_client.id.clone();
    let token_exchange = OidcTokenExchangeRequest {
        token_endpoint,
        client_id: oidc_client.client_id.to_string(),
        client_secret,
        auth_method: oidc_client.auth_method,
        redirect_uri: oidc_client.redirect_uri.to_string(),
        code_verifier: authorization_request.idp_code_verifier.to_string(),
    };
    drop(identity_provider);
    drop(oidc_client);
    let http = current_http_client(&state.http);
    let token_response =
        match exchange_oidc_authorization_code(&http, token_exchange, idp_code).await {
            Ok(response) => response,
            Err(err) => {
                tracing::warn!("OIDC token exchange failed: {err}");
                if let Err(err) = record_oidc_auth_audit(
                    &state.hub_state,
                    &profile,
                    AuthAuditRecord {
                        authorization_server: Some(&authorization_server),
                        client_id: Some(&authorization_request.oauth_client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::IdentityProviderUnavailable,
                        started_at,
                    },
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return delivery_error(
                    &state,
                    &authorization_request,
                    "server_error",
                    Some("identity provider token exchange failed"),
                )
                .await;
            }
        };
    let idp_jwks = match load_jwks(&http, &idp_jwks_source).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load identity provider JWKS for OIDC: {err}");
            return delivery_error(
                &state,
                &authorization_request,
                "server_error",
                Some("identity provider keys are unavailable"),
            )
            .await;
        }
    };
    let verifier = match OidcIdTokenConfig::new_with_claim_mapping(
        idp_issuer,
        oidc_client_id,
        authorization_request.nonce.clone(),
        allowed_hub_jwt_algorithms(),
        idp_claim_mapping,
    ) {
        Ok(config) => OidcIdTokenVerifier::new(config, idp_jwks),
        Err(err) => return internal_error_response(err),
    };
    let verified_identity = match verifier.verify(&token_response.id_token) {
        Ok(identity) => identity,
        Err(err) => {
            tracing::warn!("rejected OIDC ID token: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.hub_state,
                &profile,
                AuthAuditRecord {
                    authorization_server: Some(&authorization_server),
                    client_id: Some(&authorization_request.oauth_client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidOidcIdToken,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return delivery_error(
                &state,
                &authorization_request,
                "invalid_grant",
                Some("identity token could not be validated"),
            )
            .await;
        }
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) =
        catalog.registered_protected_resource(&authorization_request.protected_resource)
    else {
        return delivery_error(
            &state,
            &authorization_request,
            "access_denied",
            Some("resource registration changed"),
        )
        .await;
    };
    let Some(client) = catalog.oauth_client(&authorization_request.oauth_client_id) else {
        return delivery_error(
            &state,
            &authorization_request,
            "access_denied",
            Some("client registration changed"),
        )
        .await;
    };
    if client.authorization_server != *profile.authorization_server()
        || !client
            .allowed_resources
            .contains(profile.protected_resource())
        || !authorization_request
            .requested_scopes
            .is_subset(&client.allowed_scopes)
        || !authorization_request
            .requested_scopes
            .is_subset(&profile.supported_scopes(&catalog))
        || profile.oidc_client(&catalog).map(|c| &c.id) != Some(&oidc_client_record_id)
    {
        return delivery_error(
            &state,
            &authorization_request,
            "access_denied",
            Some("authorization registration changed"),
        )
        .await;
    }
    if profile.profile().is_some()
        && let Err(err) = catalog.work_context_membership(
            &authorization_request.oauth_client_id,
            &authorization_request.work_context,
            &verified_identity.principal,
        )
    {
        tracing::warn!("rejected Work Context selection: {err}");
        if let Err(err) = record_oidc_auth_audit(
            &state.hub_state,
            &profile,
            AuthAuditRecord {
                authorization_server: Some(&authorization_server),
                client_id: Some(&authorization_request.oauth_client_id),
                principal: Some(&verified_identity.principal),
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthorizationRequest,
                started_at,
            },
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return delivery_error(
            &state,
            &authorization_request,
            "access_denied",
            Some("Work Context membership is required"),
        )
        .await;
    }
    // Every sign-in records its person as the identity provider names them now, so they
    // can be named by email.
    if let Some(tenant) = &verified_identity.principal.tenant {
        let person = Person {
            tenant: tenant.clone(),
            principal: verified_identity.principal.id.clone(),
            name: verified_identity.principal_display_name.clone(),
            email: verified_identity.email.clone(),
        };
        if let Err(error) = state
            .hub_state
            .hub_store()
            .record_person(&person, Utc::now())
            .await
        {
            return internal_error_response(format!("{error:#}"));
        }
    }
    match &authorization_request.destination {
        AuthorizationDestination::Device { request, browser } => {
            match state
                .hub_state
                .hub_store()
                .approve_device_authorization(
                    *request,
                    browser,
                    &verified_identity.principal,
                    &verified_identity.principal_display_name,
                    Utc::now(),
                )
                .await
            {
                Ok(true) => axum::response::Html("Device approved. You may return to your client.")
                    .into_response(),
                Ok(false) => oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "device authorization is expired or already completed",
                ),
                Err(error) => internal_error_response(error),
            }
        }
        AuthorizationDestination::Code {
            redirect_uri,
            client_state,
            code_challenge,
            code_challenge_method,
        } => {
            let hub_code = match random_authorization_code() {
                Ok(code) => code,
                Err(error) => return internal_error_response(error),
            };
            let now = Utc::now();
            let Some(expires_at) =
                now.checked_add_signed(TimeDelta::seconds(AUTHORIZATION_CODE_TTL_SECONDS))
            else {
                return internal_error_response("authorization code expiration overflow");
            };
            let code_record = HubAuthorizationCodeRecord {
                code: hub_code.clone(),
                protected_resource: profile.protected_resource().clone(),
                oauth_client_id: authorization_request.oauth_client_id.clone(),
                work_context: authorization_request.work_context.clone(),
                oidc_client: oidc_client_record_id,
                redirect_uri: redirect_uri.clone(),
                client_state: client_state.clone(),
                scopes: authorization_request.requested_scopes.clone(),
                code_challenge: code_challenge.clone(),
                code_challenge_method: *code_challenge_method,
                principal: verified_identity.principal.clone(),
                principal_display_name: verified_identity.principal_display_name.clone(),
                issued_at: now,
                expires_at,
                consumed_at: None,
            };
            if let Err(error) = state
                .hub_state
                .record_authorization_code(&code_record)
                .await
            {
                return internal_error_response(error);
            }
            if let Err(error) = record_oidc_auth_audit(
                &state.hub_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(&authorization_server),
                    client_id: Some(&authorization_request.oauth_client_id),
                    principal: Some(&verified_identity.principal),
                    outcome: AuthOutcome::Allow,
                    reason: AuthReasonCode::AuthAllow,
                    started_at,
                },
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            redirect_with_authorization_code(redirect_uri, &hub_code, client_state.as_ref())
        }
    }
}

async fn delivery_error(
    state: &AppState,
    request: &HubAuthorizationRequest,
    code: &'static str,
    description: Option<&'static str>,
) -> axum::response::Response {
    match &request.destination {
        AuthorizationDestination::Code {
            redirect_uri,
            client_state,
            ..
        } => redirect_with_oauth_error(redirect_uri, code, description, client_state.as_ref()),
        AuthorizationDestination::Device { request, .. } => {
            if let Err(error) = state
                .hub_state
                .hub_store()
                .deny_device_authorization(*request)
                .await
            {
                return internal_error_response(error);
            }
            oauth_error_response(
                StatusCode::BAD_REQUEST,
                code,
                description.unwrap_or("device approval denied"),
            )
        }
    }
}
