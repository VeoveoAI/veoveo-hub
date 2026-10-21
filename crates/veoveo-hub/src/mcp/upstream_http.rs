use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use rmcp::model::ErrorData as McpError;
use veoveo_hub_sdk::DeploymentId;

use crate::mcp_support::mcp_internal;

const UPSTREAM_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The transport clients for hub-to-server traffic, one per immutable deployment and endpoint.
///
/// Each deployment client pools only its endpoint's connections. Each request still carries its
/// own short-lived access token, so a pool holds no authority. A server reached through a
/// port forward can restart while the forward keeps the Hub's side of every pooled
/// connection open; those connections look idle and fail only when the next request goes
/// out. A connection-level failure therefore discards the server's client, and the next
/// request opens new connections through a new one. Other servers keep their pools.
#[derive(Debug, Clone)]
pub struct HubUpstreamHttpClientPool {
    servers: Arc<Mutex<HashMap<PoolKey, Arc<Pooled>>>>,
    generations: Arc<AtomicU64>,
    builder: fn() -> reqwest::ClientBuilder,
    native: Option<Arc<super::native_upstreams::NativeTransport>>,
}

impl Default for HubUpstreamHttpClientPool {
    fn default() -> Self {
        Self::with_builder(upstream_client_builder)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolKey {
    deployment: DeploymentId,
    endpoint: url::Url,
}

impl PoolKey {
    fn new(
        deployment: DeploymentId,
        endpoint: &veoveo_hub_sdk::registration::ServerEndpoint,
    ) -> Self {
        Self {
            deployment,
            endpoint: endpoint.url().clone(),
        }
    }
}

#[derive(Debug)]
struct Pooled {
    endpoint: veoveo_hub_sdk::registration::ServerEndpoint,
    generation: u64,
    discarded: AtomicBool,
    client: reqwest::Client,
}

impl HubUpstreamHttpClientPool {
    /// Provisioning probe only: native placement must already be authoritative.
    /// This is the SDK's existing anonymous application readiness endpoint, not
    /// an authenticated MCP initialize or a replacement for selected-session tests.
    pub(crate) async fn application_ready(
        &self,
        deployment: DeploymentId,
        endpoint: &veoveo_hub_sdk::registration::ServerEndpoint,
    ) -> anyhow::Result<bool> {
        anyhow::ensure!(
            self.native.is_some(),
            "candidate readiness requires native placement"
        );
        let transport = self
            .transport(deployment, endpoint)
            .map_err(|_| anyhow::anyhow!("candidate transport unavailable"))?;
        let response = transport
            .client()
            .get(endpoint.url().join("/readyz")?)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await;
        match response {
            Ok(response) => Ok(response.status() == reqwest::StatusCode::OK),
            Err(_) => {
                transport.discard();
                Ok(false)
            }
        }
    }
    /// Reuse the installation-admitted native connection for trusted provisioning.
    /// This grants no workload/user API; native RPC authorization still applies.
    pub(crate) fn native_control_client(&self) -> Option<openshell_sdk::OpenShellClient> {
        self.native.as_ref().map(|native| native.control_client())
    }
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

    /// Prepare a new immutable routing generation, sharing only unchanged pools.
    /// Existing request transports keep their prior generation alive until drained.
    pub(crate) fn reconfigured(
        &self,
        config: Option<&crate::control_plane::NativeUpstreams>,
    ) -> anyhow::Result<Self> {
        let native = match (&self.native, config) {
            (Some(previous), Some(config)) => Some(Arc::new(previous.reconfigured(config)?)),
            (None, None) => None,
            _ => anyhow::bail!("native gateway bootstrap cannot change during runtime adoption"),
        };
        let servers = self
            .servers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, pooled)| match (&self.native, &native) {
                (Some(left), Some(right)) => left.same_endpoint(right, &pooled.endpoint),
                (None, None) => true,
                _ => false,
            })
            .map(|(slug, pooled)| (slug.clone(), pooled.clone()))
            .collect();
        Ok(Self {
            servers: Arc::new(Mutex::new(servers)),
            generations: self.generations.clone(),
            builder: self.builder,
            native,
        })
    }

    /// The current transport to `server`, building its client on first use.
    pub(super) fn transport(
        &self,
        deployment: DeploymentId,
        endpoint: &veoveo_hub_sdk::registration::ServerEndpoint,
    ) -> Result<UpstreamTransport, McpError> {
        let key = PoolKey::new(deployment, endpoint);
        let mut servers = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
        if servers.get(&key).is_some_and(|pooled| {
            &pooled.endpoint != endpoint || pooled.discarded.load(Ordering::Acquire)
        }) {
            servers.remove(&key);
        }
        if !servers.contains_key(&key) {
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
                key.clone(),
                Arc::new(Pooled {
                    endpoint: endpoint.clone(),
                    generation,
                    discarded: AtomicBool::new(false),
                    client,
                }),
            );
        }
        let pooled = &servers[&key];
        Ok(UpstreamTransport {
            pool: self.clone(),
            endpoint: endpoint.clone(),
            key,
            generation: pooled.generation,
            client: pooled.client.clone(),
        })
    }

    /// Discard `server`'s client if it is still `generation`, so concurrent failures on
    /// one generation discard it once and never a newer one.
    fn discard(&self, key: &PoolKey, generation: u64) {
        let mut servers = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
        if servers
            .get(key)
            .is_some_and(|pooled| pooled.generation == generation)
        {
            servers[key].discarded.store(true, Ordering::Release);
            servers.remove(key);
        }
    }
}

/// One generation of a server's pooled client.
#[derive(Debug, Clone)]
pub(super) struct UpstreamTransport {
    pool: HubUpstreamHttpClientPool,
    endpoint: veoveo_hub_sdk::registration::ServerEndpoint,
    key: PoolKey,
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
        self.pool.discard(&self.key, self.generation);
    }

    /// The server's current transport, after this one was discarded.
    pub(super) fn current(&self) -> Result<Self, McpError> {
        self.pool.transport(self.key.deployment, &self.endpoint)
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

#[cfg(test)]
mod live_tests {
    use super::*;
    use veoveo_hub_sdk::registration::ServerEndpoint;
    #[test]
    fn live_pool_generations_share_unaffected_clients_and_propagate_discard() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let first = HubUpstreamHttpClientPool::new();
        let a = DeploymentId::new();
        let b = DeploymentId::new();
        let endpoint =
            ServerEndpoint::new(url::Url::parse("http://fixture.internal/mcp").unwrap()).unwrap();
        let old_a = first.transport(a, &endpoint).unwrap();
        let old_b = first.transport(b, &endpoint).unwrap();
        let next = first.reconfigured(None).unwrap();
        assert_eq!(
            next.transport(a, &endpoint).unwrap().generation,
            old_a.generation
        );
        assert_eq!(
            next.transport(b, &endpoint).unwrap().generation,
            old_b.generation
        );
        old_a.discard();
        assert_ne!(
            next.transport(a, &endpoint).unwrap().generation,
            old_a.generation
        );
        assert_eq!(
            next.transport(b, &endpoint).unwrap().generation,
            old_b.generation
        );
        // A late failure from the prior generation cannot discard its replacement.
        let replacement = next.transport(a, &endpoint).unwrap();
        old_a.discard();
        assert_eq!(
            next.transport(a, &endpoint).unwrap().generation,
            replacement.generation
        );
    }
}
