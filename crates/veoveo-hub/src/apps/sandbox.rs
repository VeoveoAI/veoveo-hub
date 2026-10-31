//! The Apps listener: the sandbox proxy at `GET /` and nothing else. The listener a
//! request arrives on decides what serves it, so this router reads no `Host`.
use std::sync::Arc;

use axum::{
    Router,
    extract::{RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};

use super::{csp::AppPolicy, origins::AppOrigins, pages};
use crate::problem::Problem;

/// The sandbox proxy of the installation's Apps.
#[derive(Debug, Clone)]
pub struct SandboxProxy {
    origins: Arc<AppOrigins>,
}

impl SandboxProxy {
    pub fn new(origins: AppOrigins) -> Self {
        Self {
            origins: Arc::new(origins),
        }
    }

    /// The Apps listener's routes: `GET /`, and 404 for every other path.
    pub fn router(self) -> Router {
        Router::new()
            .route("/", get(proxy))
            .fallback(|| async { Problem::not_found().into_response() })
            .with_state(self)
    }
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
