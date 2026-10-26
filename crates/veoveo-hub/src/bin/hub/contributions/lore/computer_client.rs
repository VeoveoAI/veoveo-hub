//! The token endpoint for a Computer's own Lore client. The gateway refreshes the
//! Computer's Lore token with it, and each token is a Lore token of the Computer's owner.
//! Without a `scope` it is the owner's login token, which names them and carries no
//! repository, for Lore's listing and the requests that name no repository. With a
//! `scope` that names one repository by the deployment's scope template it is that
//! repository's token, which the deployment's public client would obtain by exchange,
//! admitted against the owner's grant on every issue. No token carries more than one
//! repository. The client holds no grant of its own. Its identifier is the Computer's
//! native name, which is never UUID-shaped, while an instance's client is its deployment
//! ID, so neither is taken for the other.
use std::time::Instant;

use axum::{http::StatusCode, response::Response};
use veoveo_hub::{HubCatalog, RegisteredProtectedResource, token_profile::TokenIssueError};
use veoveo_types::OAuthClientId;

use super::{
    Deployment, Lore, Request,
    client_token::{self, Denial, invalid_client},
    person::issue_typed,
};
use crate::{http_util::oauth_error_response, oauth_grants::TokenRequest, runtime::AppState};

/// The answer to a client-credentials request of a Computer's client, or none when the
/// request's client is an installed client or no Computer's native name.
pub(crate) async fn token(
    state: &AppState,
    catalog: &HubCatalog,
    client: &OAuthClientId,
    request: &TokenRequest,
) -> Option<Response> {
    if request.grant_type != "client_credentials"
        || catalog.oauth_client(client).is_some()
        || !veoveo_hub::computers::is_native_name(client.as_str())
    {
        return None;
    }
    Some(answer(state, catalog, client, request).await)
}

/// What a Computer's client asks of the Lore profile: the owner's login token without a
/// scope, or the token of the one repository the scope names by the deployment's scope
/// template. Any other scope is refused.
pub(super) fn requested(
    deployment: &Deployment,
    scope: Option<&str>,
) -> Result<Request, &'static str> {
    match scope {
        None => Ok(Request::Login),
        Some(scope) => deployment
            .scope_template
            .parse(scope)
            .map(|repository| Request::Exchange {
                resource: None,
                scope: Some(deployment.scope_template.render(&repository)),
            })
            .map_err(|_| {
                "a Computer's client requests one repository by the Lore deployment's scope, or no scope"
            }),
    }
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
    let owner = match veoveo_hub::computers::lore_client(
        state.hub_state.hub_store(),
        client.as_str(),
        secret,
    )
    .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return invalid_client(),
        Err(error) => {
            tracing::error!("failed to read a Computer's Lore client: {error:#}");
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "the Computer's client is unavailable",
            );
        }
    };
    // RFC 8707 §2: the client's tokens are for the installation's Lore only.
    if request.resource.is_some() {
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "a Computer's client receives tokens for the installation's Lore only",
        );
    }
    let (deployment, resource) = match deployment(catalog) {
        Ok(found) => found,
        Err(reason) => {
            tracing::error!(%client, "a Computer's Lore token has no deployment: {reason}");
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "the Lore deployment is unavailable",
            );
        }
    };
    let typed = match requested(&deployment, request.scope.as_deref()) {
        Ok(typed) => typed,
        Err(description) => {
            return oauth_error_response(StatusCode::BAD_REQUEST, "invalid_scope", description);
        }
    };
    // RFC 6749 §5.2: a repository the owner holds no grant on exceeds what they hold, as
    // the exchange refuses it.
    let denial = match typed {
        Request::Login => Denial {
            error: "invalid_grant",
            description: "the Computer's owner cannot be issued a Lore token",
        },
        Request::Exchange { .. } => Denial {
            error: "invalid_scope",
            description: "the Computer's owner holds no grant on the repository",
        },
    };
    let issued = issue_typed(
        catalog,
        &deployment.id,
        &owner.tenant,
        &owner.principal,
        &typed,
    )
    .await;
    match &issued {
        Ok(_) => {}
        Err(TokenIssueError::Denied(error)) => {
            tracing::info!(%client, principal = %owner.principal, "the Computer's owner was refused a Lore token: {error:#}");
        }
        Err(TokenIssueError::Unavailable(error)) => {
            tracing::error!(%client, "failed to issue a Computer's Lore token: {error:#}");
        }
    }
    client_token::answer(state, catalog, client, resource, started_at, issued, denial).await
}

/// The installation's one Lore deployment, which every Computer reaches, and its
/// registered resource. A token has one audience, so several deployments leave the
/// Computer's token undetermined.
fn deployment(
    catalog: &HubCatalog,
) -> Result<(Deployment, RegisteredProtectedResource<'_>), &'static str> {
    let section = Lore::of(catalog).map_err(|_| "the Lore section is invalid")?;
    let deployment = section.sole_deployment()?;
    let resource = catalog
        .registered_protected_resource(&deployment.audience)
        .ok_or("the Lore deployment's resource is not registered")?;
    Ok((deployment.clone(), resource))
}
