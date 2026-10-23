use super::auth_support::{
    allowed_hub_jwt_algorithms, auth_audit_error_response,
    authorization_server_jwks_from_signing_key, bearer_challenge, load_jwks, record_auth_audit,
};
use super::{ProfileAuthState, current_catalog, current_http_client, public_authorization_server};
use crate::contract::{AuthOutcome, AuthReasonCode};
use crate::control_plane::ResourceAuthorizationServer;
use crate::delegation::ExchangedAdmission;
use crate::{
    AuthError, AuthenticatedSubject, BearerError, BearerToken, HubCatalog, JwtAuthConfig,
    JwtVerifier, problem::Problem,
};
use axum::{
    extract::{MatchedPath, OriginalUri, Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use chrono::Utc;
use jsonwebtoken::jwk::JwkSet;
use std::time::Instant;
use veoveo_hub_sdk::{HubProfileId, PrincipalKind};
use veoveo_store::PrincipalKind as StorePrincipalKind;

/// Capture the raw spelling at the registered named profile position. Decoded
/// path extractors cannot establish whether the caller submitted an encoded alias.
pub fn profile_from_route(matched: &str, raw: &str) -> Option<HubProfileId> {
    let template: Vec<_> = matched.split('/').collect();
    let input: Vec<_> = raw.split('/').collect();
    let position = template.iter().position(|part| *part == "{profile}")?;
    for (expected, actual) in template.iter().zip(&input).take(position) {
        if !expected.starts_with('{') && expected != actual {
            return None;
        }
    }
    let raw = *input.get(position)?;
    if raw.contains('%') {
        return None;
    }
    HubProfileId::parse(raw).ok()
}
pub fn profile_from_request(request: &Request) -> Option<HubProfileId> {
    let matched = request.extensions().get::<MatchedPath>()?;
    let raw = request
        .extensions()
        .get::<OriginalUri>()
        .map_or(request.uri(), |uri| &uri.0);
    profile_from_route(matched.as_str(), raw.path())
}
pub async fn authenticate_profile(
    State(state): State<ProfileAuthState>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let Some(profile_id) = profile_from_request(&request) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    authenticate_for_profile(&state, &profile_id, request, next).await
}
/// Admit the request's bearer for `profile_id`, as [`authenticate_profile`] does for
/// the profile its route names, for a route whose profile is fixed by configuration.
pub async fn authenticate_for_profile(
    state: &ProfileAuthState,
    profile_id: &HubProfileId,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        if let Err(err) = record_auth_audit(
            state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::UnknownAuthorizationServer,
            None,
            started_at,
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return bearer_challenge(state, profile, None, "unknown authorization server");
    };

    let Some(header) = request.headers().get(AUTHORIZATION) else {
        if let Err(err) = record_auth_audit(
            state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::MissingAuthorizationHeader,
            None,
            started_at,
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return bearer_challenge(state, profile, None, "missing authorization header");
    };
    let token = match header
        .to_str()
        .map_err(|_| AuthError::InvalidAuthorizationHeader)
        .and_then(BearerToken::from_authorization_header)
    {
        Ok(token) => token,
        Err(err) => {
            tracing::warn!("rejected hub request: {err}");
            if let Err(err) = record_auth_audit(
                state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthorizationHeader,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            // RFC 6750 §3.1: an unsupported scheme carries no bearer credential, and
            // a malformed bearer credential is an invalid request.
            let error = match err {
                AuthError::InvalidAuthorizationScheme => None,
                _ => Some(BearerError::InvalidRequest),
            };
            return bearer_challenge(state, profile, error, "invalid authorization header");
        }
    };

    // A registered server calling another server presents the token the Hub issued
    // it in a token exchange. The Hub admits it whatever authorization servers the
    // profile names.
    match state.delegation.admit(token.as_str(), profile).await {
        ExchangedAdmission::NotExchanged => {}
        ExchangedAdmission::Admitted(subject) => {
            return admit_exchanged(state, profile, *subject, request, next, started_at).await;
        }
        ExchangedAdmission::Refused(reason) => {
            tracing::warn!(profile = %profile.id, reason, "rejected an exchanged token");
            if let Err(err) = record_auth_audit(
                state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidBearerToken,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return bearer_challenge(state, profile, Some(BearerError::InvalidToken), reason);
        }
        ExchangedAdmission::Unavailable(error) => {
            tracing::error!(%error, "failed to judge an exchanged token");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }

    let http = current_http_client(&state.auth_http);
    let jwks = match load_resource_authorization_jwks(
        &catalog,
        authorization_server,
        state.deployment.base_url(),
        &http,
    )
    .await
    {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load resource authorization server JWKS: {err}");
            if let Err(err) = record_auth_audit(
                state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::AuthorizationServerUnavailable,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            // The token was not judged, so the answer names the unavailable keys
            // instead of challenging the caller.
            return Problem::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "authorization-server-unavailable",
                "Authorization server unavailable",
            )
            .detail("the keys of the profile's authorization server cannot be loaded")
            .into_response();
        }
    };
    let auth_config = match JwtAuthConfig::new(
        authorization_server.issuer.clone(),
        profile.protected_resource.clone(),
        profile.required_scopes.iter().cloned().collect(),
        allowed_hub_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid hub auth config: {err}");
            if let Err(err) = record_auth_audit(
                state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthConfig,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let verified = match state
        .hub_state
        .token_extension_registry()
        .map_err(|_| {
            AuthError::Extension(veoveo_types::ExtensionError::new(
                "JWT extension profile is unbound",
            ))
        })
        .and_then(|registry| JwtVerifier::new(auth_config, jwks).with_extensions(registry))
        .and_then(|verifier| verifier.verify(&token))
    {
        Ok(verified) => verified,
        Err(err) => {
            tracing::warn!("rejected hub token: {err}");
            // The verifier checks scope after the signature and every other claim, so
            // only a valid token lacks scope.
            let (reason, error, detail) = match err {
                AuthError::MissingRequiredScope => (
                    AuthReasonCode::InvalidScope,
                    BearerError::InsufficientScope,
                    "the access token lacks a scope the profile requires",
                ),
                _ => (
                    AuthReasonCode::InvalidBearerToken,
                    BearerError::InvalidToken,
                    "invalid bearer token",
                ),
            };
            if let Err(err) =
                record_auth_audit(state, profile, AuthOutcome::Deny, reason, None, started_at).await
            {
                return auth_audit_error_response(err);
            }
            return bearer_challenge(state, profile, Some(error), detail);
        }
    };
    let subject = match state
        .hub_state
        .resolve_authenticated_subject(&catalog, verified)
        .await
    {
        Ok(subject) => subject,
        Err(err) => {
            tracing::warn!("rejected hub authority: {err}");
            if let Err(err) = record_auth_audit(
                state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidBearerToken,
                None,
                started_at,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return bearer_challenge(
                state,
                profile,
                Some(BearerError::InvalidToken),
                "invalid invocation authority",
            );
        }
    };
    match state
        .hub_state
        .access_token_session_valid(
            &profile.protected_resource,
            &authorization_server.id,
            &subject.access_token,
            &subject.principal,
        )
        .await
    {
        Ok(true) => {}
        outcome => {
            let (reason, response) = match outcome {
                Ok(false) => (
                    AuthReasonCode::TokenRevoked,
                    bearer_challenge(
                        state,
                        profile,
                        Some(BearerError::InvalidToken),
                        "session revoked or expired",
                    ),
                ),
                Err(error) => {
                    tracing::error!(%error, "failed to read current access-token session");
                    (
                        AuthReasonCode::AuthStateUnavailable,
                        StatusCode::SERVICE_UNAVAILABLE.into_response(),
                    )
                }
                Ok(true) => unreachable!(),
            };
            if let Err(error) = record_auth_audit(
                state,
                profile,
                AuthOutcome::Deny,
                reason,
                Some(&subject),
                started_at,
            )
            .await
            {
                return auth_audit_error_response(error);
            }
            return response;
        }
    }
    if let Some(jwt_id) = &subject.access_token.jwt_id {
        match state
            .hub_state
            .jwt_revocation(
                &profile.id,
                &subject.access_token.issuer,
                jwt_id,
                Utc::now(),
            )
            .await
        {
            Ok(Some(_revocation)) => {
                tracing::warn!(
                    profile = %profile.id,
                    issuer = %subject.access_token.issuer,
                    jwt_id = %jwt_id,
                    "rejected revoked hub token"
                );
                if let Err(err) = record_auth_audit(
                    state,
                    profile,
                    AuthOutcome::Deny,
                    AuthReasonCode::TokenRevoked,
                    Some(&subject),
                    started_at,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return bearer_challenge(
                    state,
                    profile,
                    Some(BearerError::InvalidToken),
                    "token revoked",
                );
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!("failed to check hub token revocation state: {err}");
                if let Err(err) = record_auth_audit(
                    state,
                    profile,
                    AuthOutcome::Deny,
                    AuthReasonCode::AuthStateUnavailable,
                    Some(&subject),
                    started_at,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    if let Err(err) = sync_principal_directory(state, &subject).await {
        tracing::error!(%err, principal = %subject.principal.id, "failed to synchronize authenticated principal display metadata");
        if let Err(audit_error) = record_auth_audit(
            state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::AuthStateUnavailable,
            Some(&subject),
            started_at,
        )
        .await
        {
            return auth_audit_error_response(audit_error);
        }
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Err(err) = record_auth_audit(
        state,
        profile,
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        Some(&subject),
        started_at,
    )
    .await
    {
        return auth_audit_error_response(err);
    }

    request
        .extensions_mut()
        .insert::<AuthenticatedSubject>(subject);
    next.run(request).await
}

/// Admit the subject of an exchanged token: its caller as the Hub authenticated it
/// for the call that reached the calling server, narrowed to the token's permissions.
async fn admit_exchanged(
    state: &ProfileAuthState,
    profile: &crate::control_plane::HubProfile,
    subject: AuthenticatedSubject,
    mut request: Request,
    next: Next,
    started_at: Instant,
) -> axum::response::Response {
    if let Err(err) = sync_principal_directory(state, &subject).await {
        tracing::error!(%err, principal = %subject.principal.id, "failed to synchronize a delegated caller");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Err(err) = record_auth_audit(
        state,
        profile,
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        Some(&subject),
        started_at,
    )
    .await
    {
        return auth_audit_error_response(err);
    }
    request
        .extensions_mut()
        .insert::<AuthenticatedSubject>(subject);
    next.run(request).await
}

async fn sync_principal_directory(
    state: &ProfileAuthState,
    subject: &AuthenticatedSubject,
) -> Result<(), veoveo_store::StoreError> {
    let store = state.hub_state.platform_store();
    let tenant = subject.authority.tenant.as_str();
    let principal = &subject.principal;
    let kind = match principal.kind {
        PrincipalKind::User => StorePrincipalKind::User,
        PrincipalKind::Service => StorePrincipalKind::Service,
    };
    match &subject.principal_display_name {
        Some(display_name) => {
            store
                .ensure_named_identity(
                    tenant,
                    principal.id.as_str(),
                    principal.issuer.as_str(),
                    principal.subject.as_str(),
                    kind,
                    display_name.as_str(),
                )
                .await?;
        }
        None => {
            store
                .ensure_identity(
                    tenant,
                    principal.id.as_str(),
                    principal.issuer.as_str(),
                    principal.subject.as_str(),
                    kind,
                )
                .await?;
        }
    }
    if subject.actor.id != principal.id {
        let actor = &subject.actor;
        store
            .ensure_identity(
                tenant,
                actor.id.as_str(),
                actor.issuer.as_str(),
                actor.subject.as_str(),
                match actor.kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
    }
    Ok(())
}

pub async fn load_resource_authorization_jwks(
    catalog: &HubCatalog,
    authorization_server: &ResourceAuthorizationServer,
    public_base_url: &str,
    http: &reqwest::Client,
) -> anyhow::Result<JwkSet> {
    if public_authorization_server(catalog, public_base_url)
        .is_some_and(|hosted| hosted.id == authorization_server.id)
    {
        authorization_server_jwks_from_signing_key(catalog, authorization_server).await
    } else {
        load_jwks(http, &authorization_server.jwks).await
    }
}
