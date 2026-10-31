//! A registered server's public routes: `/instances/{deployment}/{slug}/...` on the
//! issuer's origin reaches that deployment within the route prefix it declared, with no
//! Hub authentication, at most the prefix's body size and its rate.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use parking_lot::Mutex;
use veoveo_hub_sdk::registration::PublicRouteDeclaration;
use veoveo_types::DeploymentId;

use super::{plain_path, strip_hop};
use crate::{
    HubCatalogHandle, HubUpstreamHttpClientPool, problem::Problem, registration::INSTANCES_PATH,
};

/// The public routes of every registered deployment.
#[derive(Clone)]
pub struct PublicRoutes {
    catalog: HubCatalogHandle,
    upstream: HubUpstreamHttpClientPool,
    /// The issuer's host authority, the host of every instance's public base, which the
    /// server admits for its public routes.
    host: HeaderValue,
    buckets: Arc<Mutex<HashMap<(DeploymentId, String), Bucket>>>,
}

/// A token bucket of one route prefix of one deployment: as many requests as its rate
/// at once, refilled at its rate.
struct Bucket {
    tokens: f64,
    at: Instant,
}

impl PublicRoutes {
    pub fn new(
        catalog: HubCatalogHandle,
        upstream: HubUpstreamHttpClientPool,
        host: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            catalog,
            upstream,
            host: HeaderValue::from_str(host)?,
            buckets: Arc::default(),
        })
    }

    /// Every method of `/instances/{deployment}/{path}`.
    pub fn router(self) -> Router {
        Router::new()
            .route(
                &format!("/{INSTANCES_PATH}/{{deployment}}/{{*path}}"),
                any(forward),
            )
            .with_state(self)
    }

    /// Whether a request for `route` of `deployment` is within the route's rate, and
    /// takes its token when it is.
    fn admit(&self, deployment: DeploymentId, route: &PublicRouteDeclaration) -> bool {
        let rate = f64::from(route.requests_per_minute.requests_per_minute());
        let now = Instant::now();
        let mut buckets = self.buckets.lock();
        let bucket = buckets
            .entry((deployment, route.prefix.as_str().to_owned()))
            .or_insert(Bucket {
                tokens: rate,
                at: now,
            });
        let refill = now.duration_since(bucket.at).as_secs_f64() * rate / 60.0;
        bucket.tokens = (bucket.tokens + refill).min(rate);
        bucket.at = now;
        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

/// The deployment and the server's own path, `/{slug}/...`, of a request path.
fn split(path: &str) -> Option<(DeploymentId, &str)> {
    let rest = path.strip_prefix(&format!("/{INSTANCES_PATH}/"))?;
    let (deployment, server_path) = rest.split_at(rest.find('/')?);
    Some((DeploymentId::parse(deployment).ok()?, server_path))
}

fn refused(status: StatusCode, code: &'static str, title: &'static str) -> Response {
    Problem::new(status, code, title).into_response()
}

async fn forward(State(routes): State<PublicRoutes>, request: Request) -> Response {
    let not_found = || Problem::not_found().into_response();
    let raw = request.uri().path().to_owned();
    let Some((deployment, path)) = split(&raw) else {
        return not_found();
    };
    if !plain_path(path) {
        return not_found();
    }
    let catalog = routes.catalog.current();
    let Some(server) = catalog.registrations().deployment(&deployment).cloned() else {
        return not_found();
    };
    let Some(route) = server.surfaces.as_ref().and_then(|surfaces| {
        surfaces
            .public_routes
            .iter()
            .find(|route| path.starts_with(route.prefix.as_str()))
            .cloned()
    }) else {
        return not_found();
    };
    if !routes.admit(deployment, &route) {
        let mut response = refused(
            StatusCode::TOO_MANY_REQUESTS,
            "public-route-rate",
            "This route's request rate is exceeded",
        );
        let retry = Duration::from_secs_f64(
            60.0 / f64::from(route.requests_per_minute.requests_per_minute()),
        );
        response.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from(retry.as_secs().max(1)),
        );
        return response;
    }
    let limit = usize::try_from(route.max_body_bytes.bytes()).unwrap_or(usize::MAX);
    let too_large = || {
        refused(
            StatusCode::PAYLOAD_TOO_LARGE,
            "public-route-body",
            "The request body exceeds this route's limit",
        )
    };
    let (parts, body) = request.into_parts();
    let declared = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared.is_some_and(|length| length > route.max_body_bytes.bytes()) {
        return too_large();
    }
    let Ok(body) = axum::body::to_bytes(body, limit).await else {
        return too_large();
    };
    let client = match routes.upstream.http_client(
        server.deployment,
        &server.endpoint,
        server.branch.is_some(),
    ) {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%deployment, error = %error, "no transport to a public route's deployment");
            return refused(
                StatusCode::BAD_GATEWAY,
                "public-route-unreachable",
                "The server is unreachable",
            );
        }
    };
    let mut url = server.endpoint.url().clone();
    url.set_path(path);
    url.set_query(parts.uri.query());
    let mut headers = parts.headers;
    strip_hop(&mut headers);
    // The issuer's cookies are the Hub's sessions; a public route never receives them.
    headers.remove(header::COOKIE);
    headers.insert(header::HOST, routes.host.clone());
    let answer = match client
        .request(parts.method, url)
        .headers(headers)
        .body(body)
        .send()
        .await
    {
        Ok(answer) => answer,
        Err(error) => {
            tracing::warn!(%deployment, error = %error, "a public route's server did not answer");
            return refused(
                StatusCode::BAD_GATEWAY,
                "public-route-unreachable",
                "The server is unreachable",
            );
        }
    };
    let mut response = Response::builder().status(answer.status());
    if let Some(headers) = response.headers_mut() {
        *headers = answer.headers().clone();
        strip_hop(headers);
        // A server sets no cookie on the issuer's origin.
        headers.remove(header::SET_COOKIE);
    }
    response
        .body(Body::from_stream(answer.bytes_stream()))
        .unwrap_or_else(|_| Problem::internal().into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_path_names_a_deployment_and_the_servers_own_path() {
        let deployment = DeploymentId::new();
        let path = format!("/instances/{deployment}/n8n/hooks/a");
        assert_eq!(split(&path), Some((deployment, "/n8n/hooks/a")));
        assert_eq!(split("/instances/not-a-deployment/n8n/hooks/a"), None);
        assert_eq!(split(&format!("/instances/{deployment}")), None);
        assert_eq!(split("/apps/time/now"), None);
    }
}
