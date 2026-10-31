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

use super::{Lore, RepositoryId, factory::watch::Watches};

#[derive(Clone)]
struct Instances {
    factory: Factory,
    watches: Watches,
    catalog: HubCatalogHandle,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    repository: RepositoryId,
    branch: BranchName,
    /// The link of a linked branch whose runner to report on.
    #[serde(default)]
    link: Option<veoveo_hub_sdk::hub::LinkName>,
    /// Seconds to wait for the applied revision to serve or fail.
    #[serde(default)]
    wait: u64,
}

pub(crate) fn router(
    factory: Factory,
    watches: Watches,
    catalog: HubCatalogHandle,
    auth: ProfileAuthState,
) -> Router {
    Router::new()
        .route("/factory/{profile}/instance", get(status))
        .with_state(Instances {
            factory,
            watches,
            catalog,
        })
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
    // A runner's status follows the caller's grant on its linked repository, which the
    // people who push to its tracked branch hold; a grant on the parent reaches no runner.
    let linked = match &request.link {
        None => None,
        Some(link) => match linked_repository(&instances, &subject, &request, link).await {
            Ok(linked) => linked,
            Err(error) => {
                tracing::error!("the runner's linked repository is unknown: {error:#}");
                return Problem::internal().into_response();
            }
        },
    };
    let checked = linked.as_ref().unwrap_or(&request.repository);
    let granted = match Lore::of(&instances.catalog.current()) {
        Ok(lore) => lore.grants(&subject.principal, checked),
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
        .detail(if linked.is_some() {
            "the caller holds no grant on the runner's linked Lore repository"
        } else {
            "the caller holds no grant on the instance's Lore repository"
        })
        .into_response();
    }
    let wait = Duration::from_secs(request.wait);
    let status = match &request.link {
        None => {
            instances
                .factory
                .status(request.repository.as_str(), &request.branch, wait)
                .await
        }
        Some(link) => {
            instances
                .factory
                .runner_status(request.repository.as_str(), &request.branch, link, wait)
                .await
        }
    };
    let report = instances
        .watches
        .report(
            &subject.principal,
            &request.repository,
            &request.branch,
            request.link.as_ref(),
        )
        .await;
    let linked_watch = match &linked {
        Some(linked) => {
            instances
                .watches
                .linked_watch(&subject.principal, &request.repository, linked)
                .await
        }
        None => Ok(None),
    };
    match status.and_then(|status| Ok((status, report?, linked_watch?))) {
        Ok((Some(status), report, linked_watch)) => {
            let mut body = match serde_json::to_value(status) {
                Ok(body) => body,
                Err(error) => {
                    tracing::error!("instance status is not JSON: {error}");
                    return Problem::internal().into_response();
                }
            };
            body["watch"] = serde_json::json!(report.watch);
            body["prepareFailure"] = serde_json::json!(report.prepare_failure);
            if request.link.is_some() {
                body["linkedWatch"] = serde_json::json!(linked_watch);
                body["runnerFailure"] = serde_json::json!(report.runner_failure);
            }
            let mut response = Json(body).into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Ok((None, report, linked_watch)) => {
            let mut detail = match &request.link {
                None => "no instance serves that branch of the repository".to_owned(),
                Some(link) => format!("no runner serves the link {link} of that branch"),
            };
            if let Some(failure) = &report.runner_failure {
                detail.push_str(&format!(
                    "; the link could not run at {}: {}",
                    failure.revision, failure.message
                ));
            }
            if linked.is_some() && linked_watch.is_none() {
                detail.push_str("; nobody watches the linked repository");
            }
            if let Some(failure) = &report.prepare_failure {
                detail.push_str(&format!(
                    "; its push of {} could not be prepared: {}",
                    failure.revision, failure.message
                ));
            }
            match &report.watch {
                None => detail.push_str("; nobody watches the repository"),
                Some(watch) => {
                    if let Some(failed) = &watch.failed {
                        detail.push_str(&format!(
                            "; watching the repository as {} failed: {failed}",
                            watch.watcher
                        ));
                    }
                }
            }
            Problem::new(StatusCode::NOT_FOUND, "instance-not-found", "No instance")
                .detail(detail)
                .into_response()
        }
        Err(error) => {
            tracing::error!("instance status failed: {error:#}");
            Problem::internal().into_response()
        }
    }
}

/// The linked repository of the runner the request names: its runner's, or the one its
/// failure names when the Hub could not create it.
async fn linked_repository(
    instances: &Instances,
    subject: &AuthenticatedSubject,
    request: &Request,
    link: &veoveo_hub_sdk::hub::LinkName,
) -> anyhow::Result<Option<RepositoryId>> {
    let runner = instances
        .factory
        .runner_status(
            request.repository.as_str(),
            &request.branch,
            link,
            Duration::ZERO,
        )
        .await?
        .and_then(|status| status.runner);
    if let Some(runner) = runner {
        return Ok(Some(RepositoryId::parse(runner.linked_repository)?));
    }
    Ok(instances
        .watches
        .runner_failure(
            &subject.principal,
            &request.repository,
            &request.branch,
            link,
        )
        .await?
        .map(|failure| failure.linked_repository))
}
