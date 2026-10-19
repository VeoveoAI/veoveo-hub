use std::sync::Arc;

use super::provisioning::Provisioner;
use crate::{HubCatalogHandle, HubState, control_plane::Computers};
use anyhow::{Context, Result, ensure};
use axum::{
    Router,
    routing::{any, get, post},
};
use reqwest::Client;
use url::Url;

#[derive(Clone)]
pub struct ComputerRuntime {
    pub(super) catalog: HubCatalogHandle,
    pub(super) hub: HubState,
    pub(super) config: Arc<Computers>,
    pub(super) public: Url,
    pub(super) http: Client,
    pub(super) provisioner: Provisioner,
}
impl ComputerRuntime {
    /// Installation setup is immutable for the life of the process. Current
    /// catalog membership, client and family authority remain live on every use.
    pub async fn new(
        catalog: HubCatalogHandle,
        hub: HubState,
        public: &str,
        http: Client,
    ) -> Result<Option<Self>> {
        let initial = catalog.current();
        let Some(config) = initial.control_plane().computers.clone() else {
            return Ok(None);
        };
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
        let profile = initial
            .profile(&config.profile)
            .context("Computer profile unavailable")?;
        let server = initial
            .authorization_server(&profile.authorization_server)
            .context("Computer authorization server unavailable")?;
        let callback = public.join("computer/callback")?;
        let client = initial
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
        let provisioner = Provisioner::new(config.clone(), &initial, public.as_str()).await?;
        Ok(Some(Self {
            catalog,
            hub,
            config,
            public,
            http,
            provisioner,
        }))
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
    pub(super) fn current_http(&self) -> Client {
        self.http.clone()
    }
}
