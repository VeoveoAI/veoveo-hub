use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::Router;
use chrono::Utc;
use parking_lot::RwLock;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    ForwardedRequest, GatewayInternalTokenIssuer, GatewayProfileId, InternalTokenError,
    IssuedGatewayInternalToken, ServerSlug,
};
use veoveo_mcp_ext_registration::ResourceIndicator;
use veoveo_platform_store::MaintenanceBatch;
use veoveo_service_gateway::{
    GatewayCatalog, GatewayCatalogHandle, GatewayControlStore, GatewayRefreshDeliveryWindow,
    GatewayState, GatewayUpstreamHttpClientPool, RefreshTokenDeliveryCipher,
};

const REFRESH_DELIVERY_GC_INTERVAL: Duration = Duration::from_secs(60);
/// The rows one retention statement deletes or clears.
const RETENTION_BATCH: MaintenanceBatch = MaintenanceBatch::DEFAULT;

pub(super) type SharedCatalog = GatewayCatalogHandle;
pub(super) type SharedHttpClient = Arc<RwLock<reqwest::Client>>;
pub(super) type ProfileMcpService = Router;
pub(super) type SharedProfileMcpServices =
    Arc<RwLock<BTreeMap<GatewayProfileId, ProfileMcpService>>>;

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: SharedHttpClient,
    pub(super) public_base_url: String,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: GatewayRefreshDeliveryWindow,
}

pub(super) use veoveo_service_gateway::http::ProfileAuthState;

#[derive(Clone)]
pub(super) struct AdminState {
    pub(super) catalog: SharedCatalog,
    pub(super) http: SharedHttpClient,
    pub(super) control_store: GatewayControlStore,
    pub(super) gateway_state: GatewayState,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) artifacts: Option<ArtifactService>,
    pub(super) registrations: veoveo_service_gateway::registration::RegistrationService,
    pub(super) console_stream: crate::admin::ConsoleStreamLimits,
    /// This process's kernel instance, which also reaps the instances of ended processes.
    pub(super) instance: Arc<veoveo_platform_store::Instance>,
}

#[derive(Clone)]
pub(super) struct DynamicMcpState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) upstream_http: GatewayUpstreamHttpClientPool,
    pub(super) allowed_hosts: Arc<Vec<String>>,
    pub(super) cancellation_token: CancellationToken,
    pub(super) services: SharedProfileMcpServices,
}

#[derive(Clone)]
pub(super) struct ArtifactHttpState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: reqwest::Client,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) artifacts: ArtifactService,
}

/// The Artifact service, a platform dependency the installation configures by
/// address. Requests forwarded to it carry an access token whose audience is its URL.
#[derive(Clone, Debug)]
pub(super) struct ArtifactService {
    url: String,
    audience: ResourceIndicator,
    server: ServerSlug,
}

impl ArtifactService {
    pub(super) fn new(url: &str) -> anyhow::Result<Self> {
        let url = url.trim_end_matches('/').to_owned();
        Ok(Self {
            audience: ResourceIndicator::parse(&url)?,
            server: ServerSlug::parse(veoveo_artifact_contract::ARTIFACT_PLANE_SCHEME)?,
            url,
        })
    }

    pub(super) fn url(&self) -> &str {
        &self.url
    }

    pub(super) fn audience(&self) -> &ResourceIndicator {
        &self.audience
    }

    /// The server identity artifact policy targets name.
    pub(super) fn server(&self) -> &ServerSlug {
        &self.server
    }

    /// An access token that lets the Artifact service act for the caller.
    pub(super) fn issue(
        &self,
        issuer: &GatewayInternalTokenIssuer,
        profile: GatewayProfileId,
        subject: &veoveo_service_gateway::AuthenticatedSubject,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<IssuedGatewayInternalToken, InternalTokenError> {
        issuer.issue(ForwardedRequest {
            profile,
            server: self.server.clone(),
            audience: self.audience.clone(),
            client: subject.access_token.oauth_client_id.clone(),
            actor: subject.actor.clone(),
            authority: subject.authority.clone(),
            permissions: Default::default(),
            request_context: Some(subject.request_context()),
            expires_at,
        })
    }
}

/// The Artifact service of an admin request, or 404 when none is configured.
pub(super) fn artifact_service(
    state: &AdminState,
) -> Result<&ArtifactService, Box<axum::response::Response>> {
    use axum::response::IntoResponse;
    state.artifacts.as_ref().ok_or_else(|| {
        Box::new(veoveo_service_gateway::problem::Problem::not_found().into_response())
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Readiness {
    pub(super) status: &'static str,
    pub(super) servers: usize,
    pub(super) profiles: usize,
}

pub(super) use veoveo_service_gateway::http::{
    build_http_client, current_catalog, current_http_client, public_authorization_server,
};

pub(super) fn replace_catalog(
    catalog: &SharedCatalog,
    new_catalog: Arc<GatewayCatalog>,
) -> anyhow::Result<()> {
    catalog.replace(new_catalog)
}

pub(super) fn replace_http_client(http: &SharedHttpClient, new_client: reqwest::Client) {
    *http.write() = new_client;
}

/// Delete expired OAuth state and Task routes. Every prune repeats short batches of at
/// most [`RETENTION_BATCH`] rows, each its own transaction, until one falls short.
pub(super) async fn run_authorization_retention_gc(
    gateway_state: &GatewayState,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let authorization_records = gateway_state
        .prune_expired_authorization_records(now, RETENTION_BATCH)
        .await?;
    let jwt_revocations = gateway_state
        .prune_expired_jwt_revocations(now, RETENTION_BATCH)
        .await?;
    let replay = gateway_state
        .prune_expired_replay_ids(now, RETENTION_BATCH)
        .await?;
    let refresh = gateway_state
        .prune_expired_refresh_tokens(now, RETENTION_BATCH)
        .await?;
    let task_routes = gateway_state
        .prune_expired_task_routes(now, RETENTION_BATCH)
        .await?;
    let batches = [
        authorization_records,
        jwt_revocations,
        replay.client_assertion_jtis,
        replay.id_jag_jtis,
        refresh.delivery_envelopes_cleared,
        refresh.tokens_deleted,
        refresh.families_deleted,
        task_routes,
    ]
    .into_iter()
    .map(|pruned| pruned.batches)
    .sum::<u64>();
    tracing::info!(
        deleted_authorization_records = authorization_records.rows,
        deleted_jwt_revocations = jwt_revocations.rows,
        deleted_client_assertion_replay_ids = replay.client_assertion_jtis.rows,
        deleted_id_jag_replay_ids = replay.id_jag_jtis.rows,
        deleted_refresh_tokens = refresh.tokens_deleted.rows,
        deleted_refresh_families = refresh.families_deleted.rows,
        cleared_refresh_delivery_envelopes = refresh.delivery_envelopes_cleared.rows,
        deleted_task_routes = task_routes.rows,
        batches,
        "gateway retention gc completed"
    );
    Ok(())
}

pub(super) fn spawn_authorization_retention_gc_loop(
    gateway_state: GatewayState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if stop.is_cancelled() {
                return;
            }
            let pause = match run_authorization_retention_gc(&gateway_state).await {
                Ok(()) => Duration::from_secs(60 * 60),
                Err(err) => {
                    tracing::error!("gateway authorization retention failed: {err}");
                    Duration::from_secs(60)
                }
            };
            tokio::select! { _ = tokio::time::sleep(pause) => {}, _ = stop.cancelled() => return }
        }
    })
}

pub(super) fn spawn_refresh_delivery_gc_loop(
    gateway_state: GatewayState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! { _ = tokio::time::sleep(REFRESH_DELIVERY_GC_INTERVAL) => {}, _ = stop.cancelled() => return }
            match gateway_state
                .clear_expired_refresh_delivery_envelopes(Utc::now(), RETENTION_BATCH)
                .await
            {
                Ok(cleared) => tracing::info!(
                    cleared_refresh_delivery_envelopes = cleared.rows,
                    batches = cleared.batches,
                    "gateway refresh delivery-envelope gc completed"
                ),
                Err(err) => tracing::error!("gateway refresh delivery-envelope gc failed: {err}"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delivery_envelope_gc_cadence_is_bounded_for_the_max_window() {
        assert!(REFRESH_DELIVERY_GC_INTERVAL <= Duration::from_secs(2 * 30));
    }
}
