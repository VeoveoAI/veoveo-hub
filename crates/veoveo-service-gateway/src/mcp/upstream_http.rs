use std::sync::Arc;

use rmcp::model::ErrorData as McpError;
use tokio::sync::OnceCell;

use crate::mcp_support::mcp_internal;

const UPSTREAM_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// One shared transport client for gateway-to-server traffic.
///
/// Registered servers share one connection pool. Each request still carries its own
/// short-lived access token, so the pool holds no authority.
#[derive(Debug, Clone, Default)]
pub struct GatewayUpstreamHttpClientPool {
    client: Arc<OnceCell<reqwest::Client>>,
}

impl GatewayUpstreamHttpClientPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn client(&self) -> Result<reqwest::Client, McpError> {
        self.client
            .get_or_try_init(|| async {
                upstream_client_builder().build().map_err(|err| {
                    mcp_internal(format!("failed to build upstream HTTP client: {err}"))
                })
            })
            .await
            .cloned()
    }
}

pub fn upstream_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        // `subscriptions/listen` keeps an SSE response open for the request's
        // lifetime. A total request timeout would tear that stream down and
        // create notification gaps, so only connection establishment is bounded.
        .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
}
