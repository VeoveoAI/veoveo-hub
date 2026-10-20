//! Prepared, immutable per-revision hosted resources. No credentials are catalog data.
use crate::{HubCatalog, HubUpstreamHttpClientPool};
use anyhow::Result;
use std::sync::Arc;

pub(crate) struct HostedRuntime {
    pub upstream_http: HubUpstreamHttpClientPool,
    computer_gateway: Option<crate::control_plane::ComputerGateway>,
    pub computers: Option<Arc<crate::computers::runtime::ComputerConfiguration>>,
}
impl std::fmt::Debug for HostedRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedRuntime").finish_non_exhaustive()
    }
}
impl HostedRuntime {
    pub async fn prepare(
        catalog: &HubCatalog,
        previous: Option<&HubCatalog>,
        public: &str,
        http: reqwest::Client,
    ) -> Result<Self> {
        let old = previous.and_then(|catalog| catalog.runtime.as_ref());
        let native = catalog.control_plane().native_upstreams.as_ref();
        let upstream_http = match old {
            Some(old) => old.upstream_http.reconfigured(native)?,
            None => match native {
                Some(config) => HubUpstreamHttpClientPool::native(config).await?,
                None => HubUpstreamHttpClientPool::new(),
            },
        };
        let configured_gateway = catalog
            .control_plane()
            .computers
            .as_ref()
            .map(|config| &config.gateway);
        if let (Some(bootstrap), Some(configured)) = (
            old.and_then(|v| v.computer_gateway.as_ref()),
            configured_gateway,
        ) {
            anyhow::ensure!(
                bootstrap == configured,
                "Computer gateway connection/authentication is process bootstrap; restart the Hub to change it"
            );
        }
        let computer_gateway = old
            .and_then(|v| v.computer_gateway.clone())
            .or_else(|| configured_gateway.cloned());
        let computers = match catalog.control_plane().computers.as_ref() {
            Some(config) => Some(Arc::new(
                crate::computers::runtime::ComputerConfiguration::prepare(
                    config.clone(),
                    old.and_then(|old| old.computers.as_deref()),
                    catalog,
                    public,
                    http,
                )
                .await?,
            )),
            None => None,
        };
        Ok(Self {
            upstream_http,
            computer_gateway,
            computers,
        })
    }
}
