use super::provisioning::Provisioner;
use crate::{HubCatalog, HubCatalogHandle, HubState, control_plane::Computers};
use anyhow::{Context, Result, ensure};
use axum::{
    Router,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    routing::{any, get, post},
};
use reqwest::Client;
use std::sync::Arc;
use url::Url;

/// A prepared revision owns defaults only, never a live-catalog handle or a user grant.
#[derive(Clone)]
pub(crate) struct ComputerConfiguration {
    pub(super) config: Arc<Computers>,
    pub(super) provisioner: Provisioner,
    pub(super) http: Client,
}
impl ComputerConfiguration {
    pub async fn prepare(
        config: Computers,
        previous: Option<&Self>,
        catalog: &HubCatalog,
        public: &str,
        http: Client,
    ) -> Result<Self> {
        let public = Url::parse(public)?;
        ensure!(
            public.scheme() == "https"
                && public.host_str().is_some()
                && public.path() == "/"
                && public.query().is_none()
                && public.fragment().is_none(),
            "Computer public URL requires HTTPS origin"
        );
        let config = Arc::new(config);
        let profile = catalog
            .profile(&config.profile)
            .context("Computer profile unavailable")?;
        let server = catalog
            .authorization_server(&profile.authorization_server)
            .context("Computer authorization server unavailable")?;
        let callback = public.join("computer/callback")?;
        let client = catalog
            .oauth_client(&config.oauth_client)
            .context("Computer OAuth client unavailable")?;
        ensure!(
            server.issuer.as_str() == public.join("oauth")?.as_str()
                && server.token_endpoint.as_str() == public.join("oauth/token")?.as_str()
                && server
                    .authorization_endpoint
                    .as_ref()
                    .is_some_and(|v| v.as_str()
                        == public.join("oauth/authorize").expect("fixed URL").as_str())
                && client
                    .redirect_uris
                    .iter()
                    .any(|value| value.as_str() == callback.as_str()),
            "Computer client must use this Hub's native OAuth endpoints and exact callback"
        );
        let provisioner = match previous.filter(|old| {
            old.config == config
                && old.provisioner.spec.hub_url == profile.protected_resource.as_str()
                && catalog.secret_reference(&config.model_provider.secret)
                    == Some(&old.provisioner.spec.model_secret)
        }) {
            Some(old) => old.provisioner.clone(),
            None => {
                Provisioner::prepare(
                    config.clone(),
                    previous.map(|v| &v.provisioner),
                    catalog,
                    public.as_str(),
                )
                .await?
            }
        };
        Ok(Self {
            config,
            provisioner,
            http,
        })
    }
}

/// Stable router state. Each request takes one prepared configuration snapshot.
#[derive(Clone)]
pub struct ComputerService {
    catalog: HubCatalogHandle,
    hub: HubState,
    public: Url,
}
impl ComputerService {
    pub fn new(catalog: HubCatalogHandle, hub: HubState, public: &str) -> Result<Self> {
        Ok(Self {
            catalog,
            hub,
            public: Url::parse(public)?,
        })
    }
    pub fn router(self) -> Router {
        Router::new()
            .route("/computer", get(super::oauth::entry))
            .route("/computer/", get(super::oauth::entry))
            .route("/computer/callback", get(super::oauth::callback))
            .route("/computer/logout", post(super::oauth::logout))
            .route("/computer/c/{computer}", any(super::proxy::forward))
            .route("/computer/c/{computer}/", any(super::proxy::forward))
            .route("/computer/c/{computer}/{*path}", any(super::proxy::forward))
            .with_state(self)
    }
}
#[derive(Clone)]
pub struct ComputerRuntime {
    pub(super) catalog: HubCatalogHandle,
    pub(super) hub: HubState,
    pub(super) config: Arc<Computers>,
    pub(super) public: Url,
    pub(super) http: Client,
    pub(super) provisioner: Provisioner,
}
impl FromRequestParts<ComputerService> for ComputerRuntime {
    type Rejection = StatusCode;
    async fn from_request_parts(
        _parts: &mut Parts,
        state: &ComputerService,
    ) -> Result<Self, Self::Rejection> {
        let catalog = state.catalog.current();
        let config = catalog
            .runtime
            .as_ref()
            .and_then(|v| v.computers.as_ref())
            .ok_or(StatusCode::NOT_FOUND)?;
        Ok(Self {
            catalog: state.catalog.clone(),
            hub: state.hub.clone(),
            config: config.config.clone(),
            public: state.public.clone(),
            http: config.http.clone(),
            provisioner: config.provisioner.clone(),
        })
    }
}
impl ComputerRuntime {
    pub(super) fn current_http(&self) -> Client {
        self.http.clone()
    }
}
