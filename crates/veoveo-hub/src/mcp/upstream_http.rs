use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rmcp::model::ErrorData as McpError;
use veoveo_hub_sdk::ServerSlug;

use crate::mcp_support::mcp_internal;

const UPSTREAM_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The transport clients for hub-to-server traffic, one per registered server.
///
/// Each server's client pools that server's connections. Each request still carries its
/// own short-lived access token, so a pool holds no authority. A server reached through a
/// port forward can restart while the forward keeps the Hub's side of every pooled
/// connection open; those connections look idle and fail only when the next request goes
/// out. A connection-level failure therefore discards the server's client, and the next
/// request opens new connections through a new one. Other servers keep their pools.
#[derive(Debug, Clone)]
pub struct HubUpstreamHttpClientPool {
    servers: Arc<Mutex<HashMap<ServerSlug, Pooled>>>,
    generations: Arc<AtomicU64>,
    builder: fn() -> reqwest::ClientBuilder,
    native: Option<Arc<super::native_upstreams::NativeTransport>>,
}

impl Default for HubUpstreamHttpClientPool {
    fn default() -> Self {
        Self::with_builder(upstream_client_builder)
    }
}

#[derive(Debug)]
struct Pooled {
    endpoint: veoveo_hub_sdk::registration::ServerEndpoint,
    generation: u64,
    client: reqwest::Client,
}

impl HubUpstreamHttpClientPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// A pool whose clients come from `builder`.
    pub(super) fn with_builder(builder: fn() -> reqwest::ClientBuilder) -> Self {
        Self {
            servers: Arc::default(),
            generations: Arc::default(),
            builder,
            native: None,
        }
    }

    /// Install trusted native placements; missing endpoints never use direct TCP.
    pub async fn native(config: &crate::control_plane::NativeUpstreams) -> anyhow::Result<Self> {
        Ok(Self {
            native: Some(Arc::new(
                super::native_upstreams::NativeTransport::start(config).await?,
            )),
            ..Self::new()
        })
    }

    /// The current transport to `server`, building its client on first use.
    pub(super) fn transport(
        &self,
        server: &ServerSlug,
        endpoint: &veoveo_hub_sdk::registration::ServerEndpoint,
    ) -> Result<UpstreamTransport, McpError> {
        let mut servers = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
        if servers
            .get(server)
            .is_some_and(|pooled| &pooled.endpoint != endpoint)
        {
            servers.remove(server);
        }
        if !servers.contains_key(server) {
            let mut builder = (self.builder)();
            if let Some(native) = &self.native {
                builder = builder
                    .unix_socket(
                        native
                            .socket(endpoint)
                            .map_err(|error| mcp_internal(error.to_string()))?,
                    )
                    .no_proxy();
            }
            let client = builder.build().map_err(|err| {
                mcp_internal(format!("failed to build upstream HTTP client: {err}"))
            })?;
            let generation = self.generations.fetch_add(1, Ordering::Relaxed);
            servers.insert(
                server.clone(),
                Pooled {
                    endpoint: endpoint.clone(),
                    generation,
                    client,
                },
            );
        }
        let pooled = &servers[server];
        Ok(UpstreamTransport {
            pool: self.clone(),
            endpoint: endpoint.clone(),
            server: server.clone(),
            generation: pooled.generation,
            client: pooled.client.clone(),
        })
    }

    /// Discard `server`'s client if it is still `generation`, so concurrent failures on
    /// one generation discard it once and never a newer one.
    fn discard(&self, server: &ServerSlug, generation: u64) {
        let mut servers = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
        if servers
            .get(server)
            .is_some_and(|pooled| pooled.generation == generation)
        {
            servers.remove(server);
        }
    }
}

/// One generation of a server's pooled client.
#[derive(Debug, Clone)]
pub(super) struct UpstreamTransport {
    pool: HubUpstreamHttpClientPool,
    endpoint: veoveo_hub_sdk::registration::ServerEndpoint,
    server: ServerSlug,
    generation: u64,
    client: reqwest::Client,
}

impl UpstreamTransport {
    pub(super) fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Stop reusing this generation's idle connections. The client closes them once
    /// the requests still holding it end.
    pub(super) fn discard(&self) {
        self.pool.discard(&self.server, self.generation);
    }

    /// The server's current transport, after this one was discarded.
    pub(super) fn current(&self) -> Result<Self, McpError> {
        self.pool.transport(&self.server, &self.endpoint)
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
