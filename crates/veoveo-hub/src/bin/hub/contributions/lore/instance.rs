//! The status of a factory instance, for its repository's grant holders. `GET
//! /factory/{profile}/instance?repository=…&branch=…&wait=…` answers the instance of
//! that branch of that Lore repository, waiting up to `wait` seconds, at most 60, for its
//! applied revision to serve or fail. The caller's own current grant on the repository is
//! the one check, whether it calls directly or through a server that calls the Hub for
//! it. The profile only authenticates the caller.
use std::time::Duration;

use axum::{
    Extension, Json, Router,
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use veoveo_hub::{
    AuthenticatedSubject, HubCatalogHandle, execution::BranchName, factory::Factory,
    http::ProfileAuthState, problem::Problem,
};

use super::{Lore, RepositoryId};

#[derive(Clone)]
struct Instances {
    factory: Factory,
    catalog: HubCatalogHandle,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    repository: RepositoryId,
    branch: BranchName,
    /// Seconds to wait for the applied revision to serve or fail.
    #[serde(default)]
    wait: u64,
}

pub(crate) fn router(
    factory: Factory,
    catalog: HubCatalogHandle,
    auth: ProfileAuthState,
) -> Router {
    Router::new()
        .route("/factory/{profile}/instance", get(status))
        .with_state(Instances { factory, catalog })
        .layer(axum::middleware::from_fn_with_state(
            auth,
            veoveo_hub::http::authenticate_profile,
        ))
}

async fn status(
    State(instances): State<Instances>,
    Extension(subject): Extension<AuthenticatedSubject>,
    request: Result<Query<Request>, QueryRejection>,
) -> Response {
    let request = match request {
        Ok(Query(request)) => request,
        Err(rejection) => {
            return Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid-instance",
                "Invalid instance",
            )
            .detail(rejection.body_text())
            .into_response();
        }
    };
    let granted = match Lore::of(&instances.catalog.current()) {
        Ok(lore) => lore.grants(&subject.principal, &request.repository),
        Err(error) => {
            tracing::error!("the Lore configuration is unreadable: {error:#}");
            return Problem::internal().into_response();
        }
    };
    if !granted {
        return Problem::new(
            StatusCode::FORBIDDEN,
            "instance-forbidden",
            "No grant on the repository",
        )
        .detail("the caller holds no grant on the instance's Lore repository")
        .into_response();
    }
    let wait = Duration::from_secs(request.wait);
    match instances
        .factory
        .status(request.repository.as_str(), &request.branch, wait)
        .await
    {
        Ok(Some(status)) => {
            let mut response = Json(status).into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Ok(None) => Problem::new(StatusCode::NOT_FOUND, "instance-not-found", "No instance")
            .detail("no instance serves that branch of the repository")
            .into_response(),
        Err(error) => {
            tracing::error!("instance status failed: {error:#}");
            Problem::internal().into_response()
        }
    }
}
