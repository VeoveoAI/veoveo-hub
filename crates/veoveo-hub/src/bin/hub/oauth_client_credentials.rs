use std::time::Instant;
use veoveo_hub::RegisteredProtectedResource;

use axum::{http::StatusCode, response::IntoResponse};
use chrono::Utc;
use veoveo_hub::contract::{AuthMethod, AuthOutcome, AuthReasonCode};
use veoveo_hub::contract::{OAuthClientAuthMethod, OAuthGrantType};
use veoveo_hub::control_plane::ResourceAuthorizationServer;
use veoveo_hub::oauth_clients::EffectiveOAuthClient;
use veoveo_hub::{ClientAssertionConfig, ClientAssertionVerifier, HubCatalog};
use veoveo_oauth::CLIENT_ASSERTION_TYPE;
use veoveo_types::WorkContextId;

use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_protocol_auth_audit},
    http_util::{
        TokenResponse, allowed_hub_jwt_algorithms, load_jwks, oauth_error_response, token_response,
        work_context_refused,
    },
    oauth_grants::{TokenRequest, requested_client_credentials_scopes},
    runtime::{AppState, current_http_client},
    tokens::{client_credentials_principal, issue_client_credentials_access_token},
};

pub(super) async fn token_endpoint_client_credentials(
    state: &AppState,
    catalog: &HubCatalog,
    resource: RegisteredProtectedResource<'_>,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    effective: EffectiveOAuthClient,
    started_at: Instant,
) -> axum::response::Response {
    let method = if effective
        .registration
        .auth_methods
        .contains(&OAuthClientAuthMethod::ClientSecretPost)
    {
        AuthMethod::ClientCredentialsSecretPost
    } else {
        AuthMethod::ClientCredentialsPrivateKeyJwt
    };
    if request.grant_type != "client_credentials"
        || !resource.supports(OAuthGrantType::ClientCredentials)
    {
        if let Err(err) = record_protocol_auth_audit(
            &state.hub_state,
            resource.audit_target(),
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
            method,
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "grant type is not supported for this hub resource",
        );
    }

    let client_id = effective.registration.id.clone();
    let client = &effective.registration;
    if &client.authorization_server != resource.authorization_server()
        || !client
            .allowed_resources
            .contains(resource.protected_resource())
        || !(client.auth_methods
            == std::collections::BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt])
            || client.auth_methods
                == std::collections::BTreeSet::from([OAuthClientAuthMethod::ClientSecretPost]))
    {
        if let Err(err) = record_protocol_auth_audit(
            &state.hub_state,
            resource.audit_target(),
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
            method,
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_client",
            "client authentication failed",
        );
    }

    if method == AuthMethod::ClientCredentialsSecretPost {
        if request.client_assertion.is_some() || request.client_assertion_type.is_some() {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client authentication failed",
            );
        }
        let authenticated = async {
            let supplied = request
                .client_secret
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing client credential"))?;
            let reference = client
                .credential_secret
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing client credential binding"))?;
            let secret = veoveo_hub::HubSecretResolver::new()
                .resolve_string(
                    catalog,
                    reference,
                    veoveo_hub::contract::SecretPurpose::OAuthClientSecret,
                )
                .await?;
            aws_lc_rs::constant_time::verify_slices_are_equal(
                supplied.as_bytes(),
                secret.expose_secret().as_bytes(),
            )
            .map_err(|_| anyhow::anyhow!("client authentication failed"))
        }
        .await;
        if authenticated.is_err() {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client authentication failed",
            );
        }
    } else {
        if request.client_secret.is_some() {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client authentication failed",
            );
        }
        if request.client_assertion_type.as_deref() != Some(CLIENT_ASSERTION_TYPE) {
            if let Err(err) = record_protocol_auth_audit(
                &state.hub_state,
                resource.audit_target(),
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClientAssertion,
                    started_at,
                },
                method,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client authentication failed",
            );
        }
        let Some(assertion) = request.client_assertion.as_deref() else {
            if let Err(err) = record_protocol_auth_audit(
                &state.hub_state,
                resource.audit_target(),
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClientAssertion,
                    started_at,
                },
                method,
            )
            .await
            {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client authentication failed",
            );
        };

        let client_jwks = if let Some(keys) = &effective.public_keys {
            keys.clone()
        } else {
            let Some(client_jwks_source) = client.jwks.as_ref() else {
                tracing::error!(client = %client_id, "private-key JWT client is missing JWKS source");
                if let Err(err) = record_protocol_auth_audit(
                    &state.hub_state,
                    resource.audit_target(),
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::InvalidAuthConfig,
                        started_at,
                    },
                    method,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    "client authentication is not configured",
                );
            };
            let http = current_http_client(&state.http);
            match load_jwks(&http, client_jwks_source).await {
                Ok(jwks) => jwks,
                Err(err) => {
                    tracing::warn!(client = %client_id, "failed to load OAuth client JWKS: {err}");
                    if let Err(err) = record_protocol_auth_audit(
                        &state.hub_state,
                        resource.audit_target(),
                        AuthAuditRecord {
                            authorization_server: Some(authorization_server),
                            client_id: Some(&client_id),
                            principal: None,
                            outcome: AuthOutcome::Deny,
                            reason: AuthReasonCode::AuthorizationServerUnavailable,
                            started_at,
                        },
                        method,
                    )
                    .await
                    {
                        return auth_audit_error_response(err);
                    }
                    return oauth_error_response(
                        StatusCode::BAD_REQUEST,
                        "invalid_client",
                        "client authentication failed",
                    );
                }
            }
        };
        let assertion_config = match ClientAssertionConfig::new(
            client_id.clone(),
            authorization_server.token_endpoint.as_str(),
            allowed_hub_jwt_algorithms(),
        ) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("invalid client assertion verifier config: {err}");
                if let Err(err) = record_protocol_auth_audit(
                    &state.hub_state,
                    resource.audit_target(),
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::InvalidAuthConfig,
                        started_at,
                    },
                    method,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    "client authentication is not configured",
                );
            }
        };
        let verified_assertion =
            match ClientAssertionVerifier::new(assertion_config, client_jwks).verify(assertion) {
                Ok(verified) => verified,
                Err(err) => {
                    tracing::warn!(client = %client_id, "rejected OAuth client assertion: {err}");
                    if let Err(err) = record_protocol_auth_audit(
                        &state.hub_state,
                        resource.audit_target(),
                        AuthAuditRecord {
                            authorization_server: Some(authorization_server),
                            client_id: Some(&client_id),
                            principal: None,
                            outcome: AuthOutcome::Deny,
                            reason: AuthReasonCode::InvalidClientAssertion,
                            started_at,
                        },
                        method,
                    )
                    .await
                    {
                        return auth_audit_error_response(err);
                    }
                    return oauth_error_response(
                        StatusCode::BAD_REQUEST,
                        "invalid_client",
                        "client authentication failed",
                    );
                }
            };
        match state
            .hub_state
            .record_client_assertion_jti(
                &authorization_server.id,
                &client_id,
                &verified_assertion.jwt_id,
                verified_assertion.expires_at,
                Utc::now(),
            )
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(
                    client = %client_id,
                    jwt_id = %verified_assertion.jwt_id,
                    "rejected replayed OAuth client assertion"
                );
                if let Err(err) = record_protocol_auth_audit(
                    &state.hub_state,
                    resource.audit_target(),
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::ClientAssertionReplay,
                        started_at,
                    },
                    method,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_client",
                    "client authentication failed",
                );
            }
            Err(err) => {
                tracing::error!("failed to record OAuth client assertion replay state: {err}");
                if let Err(err) = record_protocol_auth_audit(
                    &state.hub_state,
                    resource.audit_target(),
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::AuthStateUnavailable,
                        started_at,
                    },
                    method,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    // RFC 6749 §5.2: an authenticated client without this grant is unauthorized.
    if !client
        .grant_types
        .contains(&OAuthGrantType::ClientCredentials)
    {
        if let Err(err) = record_protocol_auth_audit(
            &state.hub_state,
            resource.audit_target(),
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
            method,
        )
        .await
        {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unauthorized_client",
            "the client may not use the client_credentials grant",
        );
    }

    let supported_scopes = resource.supported_scopes(catalog);
    let scopes = if resource.profile().is_none() {
        supported_scopes.clone()
    } else {
        match requested_client_credentials_scopes(
            &supported_scopes,
            client,
            request.scope.as_deref(),
        ) {
            Ok(scopes) => scopes,
            Err(err) => {
                tracing::warn!(client = %client_id, "rejected token scope request: {err}");
                if let Err(err) = record_protocol_auth_audit(
                    &state.hub_state,
                    resource.audit_target(),
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::InvalidScope,
                        started_at,
                    },
                    method,
                )
                .await
                {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_scope",
                    "requested scope is not allowed",
                );
            }
        }
    };
    let work_context = match request.work_context.as_deref() {
        Some(value) => match WorkContextId::parse(value.trim()) {
            Ok(context) => context,
            Err(_) => {
                return oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "work_context is not a valid Work Context ID",
                );
            }
        },
        None => client.default_work_context.clone(),
    };
    let Some(tenant) = client.tenant.as_ref() else {
        tracing::error!(client = %client_id, "automated OAuth client has no tenant");
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "this client is not configured to act in any tenant; ask an administrator to finish its setup",
        );
    };
    let mut service_principal = match client_credentials_principal(
        authorization_server,
        &client_id,
        tenant,
        &scopes,
    ) {
        Ok(principal) => principal,
        Err(err) => {
            tracing::error!("failed to establish automated principal: {err}");
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "this client is not configured to act in any tenant; ask an administrator to finish its setup",
            );
        }
    };
    // RFC 6749 §5.2 defines no `access_denied` for the token endpoint. A client whose
    // own authority cannot act is unauthorized for the grant; a Work Context it asked
    // for and may not act in is an invalid request.
    if let Err(error) = effective.apply_service_roles(&mut service_principal) {
        tracing::warn!(%error, "managed service roles are unavailable");
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unauthorized_client",
            "the client's authority is unavailable",
        );
    }
    if resource.profile().is_some()
        && let Err(err) = effective.membership(catalog, &work_context, &service_principal)
    {
        tracing::warn!("rejected Work Context selection: {err}");
        return work_context_refused(
            request.work_context.is_some(),
            "unauthorized_client",
            "the client is no member of its default Work Context",
        );
    }

    let binding = match effective.token_extensions() {
        Ok(binding) => binding,
        Err(_) => {
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "unauthorized_client",
                "the client's authority is unavailable",
            );
        }
    };
    let token = match if resource.profile().is_none() {
        async {
            let context = veoveo_hub::token_profile::TokenContext::admit(
                catalog,
                resource.protected_resource(),
                &client_id,
                &service_principal,
                &work_context,
                &scopes,
                None,
            )?;
            state
                .token_profiles
                .issue(
                    context,
                    veoveo_hub::token_profile::ProfileRequest::Exchange {
                        resource: None,
                        scope: request.scope.as_deref(),
                    },
                    None,
                )
                .await
        }
        .await
    } else {
        issue_client_credentials_access_token(
            catalog,
            authorization_server,
            resource.protected_resource(),
            &client_id,
            &service_principal,
            crate::tokens::ServiceTokenAuthority {
                work_context,
                extensions: binding,
            },
            &scopes,
        )
        .await
        .map_err(veoveo_hub::token_profile::TokenIssueError::Unavailable)
    } {
        Ok(token) => token,
        Err(err) => {
            if err.is_denied() {
                if let Err(error) = record_protocol_auth_audit(
                    &state.hub_state,
                    resource,
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: Some(&service_principal),
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::PolicyDenied,
                        started_at,
                    },
                    method,
                )
                .await
                {
                    return auth_audit_error_response(error);
                }
                return oauth_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_scope",
                    "requested resource authority is denied",
                );
            }
            tracing::error!("failed to issue client credentials access token: {err}");
            if let Err(err) = record_protocol_auth_audit(
                &state.hub_state,
                resource.audit_target(),
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
                method,
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
    if let Err(err) = record_protocol_auth_audit(
        &state.hub_state,
        resource.audit_target(),
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: None,
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
        method,
    )
    .await
    {
        return auth_audit_error_response(err);
    }

    token_response(TokenResponse {
        expires_in: token.expires_in(),
        scope: crate::http_util::scope_string(&token.scopes),
        access_token: token.access_token,
        token_type: "Bearer",
        refresh_token: None,
        refresh_token_expires_in: None,
    })
}
