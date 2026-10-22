//! The token endpoint for an instance's own client. The gateway refreshes the instance's
//! Lore token with it: each token is the Lore token of the instance's latest pusher for
//! the instance's repository, issued under that person's current grant. The client
//! requests no scope and holds no grant of its own.
use std::time::Instant;

use axum::{http::StatusCode, response::Response};
use veoveo_hub::{
    HubCatalog,
    contract::{AuthMethod, AuthOutcome, AuthReasonCode},
    token_profile::TokenIssueError,
};
use veoveo_types::{DeploymentId as InstanceId, OAuthClientId};

use super::{DeploymentId, Lore, RepositoryId, person::issue};
use crate::{
    audit::{AuthAuditRecord, auth_audit_error_response, record_protocol_auth_audit},
    http_util::{TokenResponse, oauth_error_response, scope_string, token_response},
    oauth_grants::TokenRequest,
    runtime::AppState,
};

/// The answer to a client-credentials request of an instance's client, or none when the
/// request's client is an installed client or names no instance.
pub(crate) async fn token(
    state: &AppState,
    catalog: &HubCatalog,
    client: &OAuthClientId,
    request: &TokenRequest,
) -> Option<Response> {
    if request.grant_type != "client_credentials"
        || catalog.oauth_client(client).is_some()
        || InstanceId::parse(client.as_str()).is_err()
    {
        return None;
    }
    Some(answer(state, catalog, client, request).await)
}

async fn answer(
    state: &AppState,
    catalog: &HubCatalog,
    client: &OAuthClientId,
    request: &TokenRequest,
) -> Response {
    let started_at = Instant::now();
    let invalid_client = || {
        oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_client",
            "client authentication failed",
        )
    };
    let Some(secret) = request.client_secret.as_deref() else {
        return invalid_client();
    };
    if request.client_assertion.is_some() || request.client_assertion_type.is_some() {
        return invalid_client();
    }
    let instance = match veoveo_hub::factory::instance_client(
        state.hub_state.platform_store(),
        client.as_str(),
        secret,
    )
    .await
    {
        Ok(Some(instance)) => instance,
        Ok(None) => return invalid_client(),
        Err(error) => {
            tracing::error!("failed to read an instance's client: {error:#}");
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "the instance's client is unavailable",
            );
        }
    };
    if request.scope.is_some() {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_scope",
            "an instance's client receives its repository's scope and requests none",
        );
    }
    // RFC 8707 §2: the client's tokens are for its repository's Lore only.
    if request.resource.is_some() {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "an instance's client receives tokens for its repository's Lore only",
        );
    }
    let Some(pusher) = instance.pusher else {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unauthorized_client",
            "the instance has no push yet",
        );
    };
    let Some((deployment, id)) = locate(catalog, &instance.repository) else {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unauthorized_client",
            "the instance's repository is no registered Lore repository",
        );
    };
    let Some(resource) = Lore::of(catalog)
        .ok()
        .and_then(|section| {
            section
                .deployments
                .into_iter()
                .find(|candidate| candidate.id == deployment)
        })
        .and_then(|deployment| catalog.registered_protected_resource(&deployment.audience))
    else {
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "the Lore deployment is unavailable",
        );
    };
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
    match issue(catalog, &pusher, &deployment, &id).await {
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
        Err(TokenIssueError::Denied(error)) => {
            tracing::info!(%client, %pusher, "the instance's pusher holds no grant: {error:#}");
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
            // RFC 6749 §5.2: the person the client acts for no longer holds the grant.
            oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "the instance's latest pusher holds no grant on its repository",
            )
        }
        Err(TokenIssueError::Unavailable(error)) => {
            tracing::error!(%client, "failed to issue an instance's Lore token: {error:#}");
            oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "the Lore token is unavailable",
            )
        }
    }
}

/// The one Lore deployment that registers the instance's repository.
fn locate(catalog: &HubCatalog, repository: &str) -> Option<(DeploymentId, RepositoryId)> {
    let repository = RepositoryId::parse(repository).ok()?;
    let section = Lore::of(catalog).ok()?;
    let mut registered = section
        .repositories
        .into_iter()
        .filter(|registered| registered.id == repository);
    let found = registered.next()?;
    registered
        .next()
        .is_none()
        .then_some((found.deployment, found.id))
}
