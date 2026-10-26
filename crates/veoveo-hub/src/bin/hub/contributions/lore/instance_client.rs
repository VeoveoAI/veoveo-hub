//! The token endpoint for an instance's own client. The gateway refreshes the instance's
//! Lore token with it: each token is the Lore token of the instance's latest pusher for
//! the instance's repository, issued under that person's current grant. The client
//! requests no scope and holds no grant of its own.
use std::time::Instant;

use axum::{http::StatusCode, response::Response};
use veoveo_hub::{HubCatalog, token_profile::TokenIssueError};
use veoveo_types::{DeploymentId as InstanceId, OAuthClientId};

use super::{
    DeploymentId, Lore, RepositoryId,
    client_token::{self, Denial, invalid_client},
    person::issue,
};
use crate::{http_util::oauth_error_response, oauth_grants::TokenRequest, runtime::AppState};

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
    let Some(secret) = client_token::secret(request) else {
        return invalid_client();
    };
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
    let issued = issue(catalog, &pusher, &deployment, &id).await;
    match &issued {
        Ok(_) => {}
        Err(TokenIssueError::Denied(error)) => {
            tracing::info!(%client, %pusher, "the instance's pusher holds no grant: {error:#}");
        }
        Err(TokenIssueError::Unavailable(error)) => {
            tracing::error!(%client, "failed to issue an instance's Lore token: {error:#}");
        }
    }
    client_token::answer(
        state,
        catalog,
        client,
        resource,
        started_at,
        issued,
        Denial {
            error: "invalid_grant",
            description: "the instance's latest pusher holds no grant on its repository",
        },
    )
    .await
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
