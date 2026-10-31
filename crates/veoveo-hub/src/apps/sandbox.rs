//! The Apps listener: a request whose `X-Forwarded-Host` names a server's app origin goes
//! to that origin; every other is the sandbox proxy's, at `GET /` and nothing else, whatever
//! its `Host`.
use std::sync::Arc;

use axum::{
    Router,
    extract::{RawQuery, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use tower::ServiceExt as _;

use super::{csp::AppPolicy, origins::AppOrigins, pages, server_origin::ServerOrigins};
use crate::problem::Problem;

/// The sandbox proxy of the installation's Apps, and the servers' app origins beside it.
#[derive(Clone)]
pub struct SandboxProxy {
    origins: Arc<AppOrigins>,
    servers: Option<ServerOrigins>,
}

impl SandboxProxy {
    pub fn new(origins: AppOrigins, servers: Option<ServerOrigins>) -> Self {
        Self {
            origins: Arc::new(origins),
            servers,
        }
    }

    /// The Apps listener's routes: a server's app origin by `X-Forwarded-Host`; otherwise
    /// `GET /`, and 404 for every other path.
    pub fn router(self) -> Router {
        Router::new().fallback(dispatch).with_state(self)
    }
}

async fn dispatch(State(listener): State<SandboxProxy>, request: Request) -> Response {
    if let Some(servers) = &listener.servers
        && let Some(server) = servers.server(request.headers())
    {
        return servers.serve(server, request).await;
    }
    Router::new()
        .route("/", get(proxy))
        .fallback(|| async { Problem::not_found().into_response() })
        .with_state(listener)
        .oneshot(request)
        .await
        .unwrap_or_else(|never| match never {})
}

/// `GET /`: the proxy page under the App policy its query names, validated again.
async fn proxy(State(proxy): State<SandboxProxy>, RawQuery(query): RawQuery) -> Response {
    match AppPolicy::from_query(query.as_deref().unwrap_or(""), &proxy.origins) {
        Ok(policy) => pages::proxy_page(&policy, &proxy.origins),
        Err(error) => Problem::new(
            StatusCode::BAD_REQUEST,
            "apps-sandbox-policy-invalid",
            "Invalid sandbox policy",
        )
        .detail(error.to_string())
        .into_response(),
    }
}
