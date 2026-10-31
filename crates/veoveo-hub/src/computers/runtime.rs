use super::{
    collection::{BrowserRequests, Collection},
    provisioning::Installation,
};
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
    pub(super) installation: Installation,
    pub(super) http: Client,
}
impl ComputerConfiguration {
    /// The Computers section this runtime was prepared from.
    pub(crate) fn config(&self) -> &Arc<Computers> {
        &self.config
    }

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
        let installation = match previous.filter(|old| {
            // Guidance and collection changes do not change native provisioning inputs.
            let mut previous_config = old.config.as_ref().clone();
            previous_config.handbook.clone_from(&config.handbook);
            previous_config.collect_after_seconds = config.collect_after_seconds;
            previous_config == *config
                && old.installation.hub_url() == profile.protected_resource.as_str()
                && catalog.secret_reference(&config.model_provider.secret)
                    == Some(old.installation.model_secret())
        }) {
            Some(old) => old.installation.clone(),
            None => {
                Installation::prepare(
                    config.clone(),
                    previous.map(|v| &v.installation),
                    catalog,
                    public.as_str(),
                )
                .await?
            }
        };
        Ok(Self {
            config,
            installation,
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
    browser: Arc<BrowserRequests>,
    started: chrono::DateTime<chrono::Utc>,
}
impl ComputerService {
    pub fn new(catalog: HubCatalogHandle, hub: HubState, public: &str) -> Result<Self> {
        Ok(Self {
            catalog,
            hub,
            public: Url::parse(public)?,
            browser: Arc::default(),
            started: chrono::Utc::now(),
        })
    }
    /// Collect idle Computers until `stop`, with the browser requests this service
    /// forwards. Computers count as active from the service's creation.
    pub fn collect(&self, stop: tokio_util::sync::CancellationToken) {
        Collection {
            catalog: self.catalog.clone(),
            hub: self.hub.clone(),
            browser: self.browser.clone(),
            started: self.started,
        }
        .spawn(stop);
    }
    /// The browser's routes under `/computer` and the public current version. The
    /// Computer's actions, `GET /computer/status` and `POST /computer/restart` and
    /// `/computer/previous`, admit either the owner's browser session or a bearer of
    /// the Computers' profile, such as the Computer's own Hub credential or a server
    /// calling the Hub for its caller.
    pub fn router(self, auth: crate::http::ProfileAuthState) -> Router {
        let actions = Self::actions().layer(axum::middleware::from_fn_with_state(
            Bearers {
                service: self.clone(),
                auth,
            },
            bearer,
        ));
        Self::routes(actions).with_state(self)
    }
    fn actions() -> Router<Self> {
        Router::new()
            .route("/computer/status", get(super::api::status))
            .route("/computer/restart", post(super::api::restart))
            .route("/computer/previous", post(super::api::previous))
    }
    fn routes(actions: Router<Self>) -> Router<Self> {
        Router::new()
            .route("/computer", get(super::oauth::entry))
            .route("/computer/", get(super::oauth::entry))
            .route("/computer/version", get(super::api::version))
            .route("/computer/callback", get(super::oauth::callback))
            .route("/computer/logout", post(super::oauth::logout))
            .route("/computer/c/{computer}", any(super::proxy::forward))
            .route("/computer/c/{computer}/", any(super::proxy::forward))
            .route("/computer/c/{computer}/{*path}", any(super::proxy::forward))
            .merge(actions)
    }
}

/// What a bearer on a Computer action is admitted with.
#[derive(Clone)]
struct Bearers {
    service: ComputerService,
    auth: crate::http::ProfileAuthState,
}

/// A request with an `Authorization` header is a bearer's: the Hub admits it for the
/// one Computers profile and gives the action its subject. One without is the
/// browser's, which the action admits by its session cookie.
async fn bearer(
    axum::extract::State(bearers): axum::extract::State<Bearers>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if !request
        .headers()
        .contains_key(axum::http::header::AUTHORIZATION)
    {
        return next.run(request).await;
    }
    let catalog = bearers.service.catalog.current();
    let Some(profile) = catalog
        .control_plane()
        .computers
        .as_ref()
        .map(|computers| computers.profile.clone())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    crate::http::authenticate_for_profile(&bearers.auth, &profile, request, next).await
}
#[derive(Clone)]
pub struct ComputerRuntime {
    pub(super) catalog: HubCatalogHandle,
    pub(super) hub: HubState,
    pub(super) config: Arc<Computers>,
    pub(super) public: Url,
    pub(super) http: Client,
    pub(super) installation: Installation,
    /// The last browser request forwarded to each Computer, for collection.
    pub(super) browser: Arc<BrowserRequests>,
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
            installation: config.installation.clone(),
            browser: state.browser.clone(),
        })
    }
}
impl ComputerRuntime {
    pub(super) fn current_http(&self) -> Client {
        self.http.clone()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_action_routes_sit_beside_the_browser_routes_and_the_mounts() {
        // Axum refuses conflicting routes when they are added.
        let _ = super::ComputerService::routes(super::ComputerService::actions());
    }
}
