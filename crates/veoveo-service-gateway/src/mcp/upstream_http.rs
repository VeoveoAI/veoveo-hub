use std::sync::Arc;

use rmcp::model::ErrorData as McpError;
use tokio::sync::OnceCell;

use crate::mcp_support::mcp_internal;

const UPSTREAM_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The transport clients for gateway-to-server traffic.
///
/// Registered servers share one connection pool. Each request still carries its own
/// short-lived access token, so the pool holds no authority. A second client keeps no
/// idle connection, so each of its requests opens a new one. Discovery retries through
/// it: when a server behind a port forward restarts, every idle pooled connection to it
/// is as stale as the one that just failed.
#[derive(Debug, Clone, Default)]
pub struct GatewayUpstreamHttpClientPool {
    pooled: Arc<OnceCell<reqwest::Client>>,
    fresh: Arc<OnceCell<reqwest::Client>>,
}

/// The shared pooled client, and the client whose every request opens a new connection.
#[derive(Debug, Clone)]
pub(super) struct UpstreamHttpClients {
    pub(super) pooled: reqwest::Client,
    pub(super) fresh: reqwest::Client,
}

impl GatewayUpstreamHttpClientPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) async fn clients(&self) -> Result<UpstreamHttpClients, McpError> {
        Ok(UpstreamHttpClients {
            pooled: built(&self.pooled, upstream_client_builder).await?,
            fresh: built(&self.fresh, || {
                upstream_client_builder().pool_max_idle_per_host(0)
            })
            .await?,
        })
    }
}

async fn built(
    client: &OnceCell<reqwest::Client>,
    builder: impl FnOnce() -> reqwest::ClientBuilder,
) -> Result<reqwest::Client, McpError> {
    client
        .get_or_try_init(|| async {
            builder()
                .build()
                .map_err(|err| mcp_internal(format!("failed to build upstream HTTP client: {err}")))
        })
        .await
        .cloned()
}

pub fn upstream_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        // `subscriptions/listen` keeps an SSE response open for the request's
        // lifetime. A total request timeout would tear that stream down and
        // create notification gaps, so only connection establishment is bounded.
        .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
}
