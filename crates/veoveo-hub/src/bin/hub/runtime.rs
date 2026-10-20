use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::Router;
use chrono::Utc;
use parking_lot::RwLock;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_hub::{
    HubCatalog, HubCatalogHandle, HubControlStore, HubRefreshDeliveryWindow, HubState,
    HubUpstreamHttpClientPool, RefreshTokenDeliveryCipher,
};
use veoveo_hub_sdk::{HubInternalTokenIssuer, HubProfileId};
use veoveo_store::MaintenanceBatch;

const REFRESH_DELIVERY_GC_INTERVAL: Duration = Duration::from_secs(60);
/// The rows one retention statement deletes or clears.
const RETENTION_BATCH: MaintenanceBatch = MaintenanceBatch::DEFAULT;

pub(super) type SharedCatalog = HubCatalogHandle;
pub(super) type SharedHttpClient = Arc<RwLock<reqwest::Client>>;
pub(super) type ProfileMcpService = Router;
pub(super) type SharedProfileMcpServices = Arc<RwLock<BTreeMap<HubProfileId, ProfileMcpService>>>;

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) token_profiles: veoveo_hub::token_profile::TokenProfiles,
    pub(super) catalog: SharedCatalog,
    pub(super) hub_state: HubState,
    pub(super) http: SharedHttpClient,
    pub(super) public_base_url: String,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: HubRefreshDeliveryWindow,
    /// The token exchange of calls between registered servers.
    pub(super) delegation: veoveo_hub::delegation::DelegationService,
}

pub(super) use veoveo_hub::http::ProfileAuthState;

#[derive(Clone)]
pub(super) struct AdminState {
    pub(super) public_base_url: String,
    pub(super) publication: Arc<tokio::sync::Mutex<()>>,
    pub(super) catalog: SharedCatalog,
    pub(super) http: SharedHttpClient,
    pub(super) control_store: HubControlStore,
    pub(super) hub_state: HubState,
    pub(super) registrations: veoveo_hub::registration::RegistrationService,
    pub(super) console_stream: crate::admin::ConsoleStreamLimits,
    /// This process's kernel instance, which also reaps the instances of ended processes.
    pub(super) instance: Arc<veoveo_store::Instance>,
}

#[derive(Clone)]
pub(super) struct DynamicMcpState {
    pub(super) catalog: SharedCatalog,
    pub(super) hub_state: HubState,
    pub(super) internal_token_issuer: HubInternalTokenIssuer,
    pub(super) upstream_http: HubUpstreamHttpClientPool,
    pub(super) allowed_hosts: Arc<Vec<String>>,
    pub(super) cancellation_token: CancellationToken,
    pub(super) services: SharedProfileMcpServices,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Readiness {
    pub(super) status: &'static str,
    pub(super) servers: usize,
    pub(super) profiles: usize,
}

pub(super) use veoveo_hub::http::{
    build_http_client, current_catalog, current_http_client, public_authorization_server,
};

pub(super) fn replace_catalog(
    catalog: &SharedCatalog,
    new_catalog: Arc<HubCatalog>,
) -> anyhow::Result<()> {
    catalog.replace(new_catalog)
}

pub(super) fn replace_http_client(http: &SharedHttpClient, new_client: reqwest::Client) {
    *http.write() = new_client;
}

/// Delete expired OAuth state and Task routes. Every prune repeats short batches of at
/// most [`RETENTION_BATCH`] rows, each its own transaction, until one falls short.
pub(super) async fn run_authorization_retention_gc(hub_state: &HubState) -> anyhow::Result<()> {
    let now = Utc::now();
    hub_state.hub_store().prune_computer_logins().await?;
    hub_state
        .hub_store()
        .prune_device_authorizations(now)
        .await?;
    let authorization_records = hub_state
        .prune_expired_authorization_records(now, RETENTION_BATCH)
        .await?;
    let jwt_revocations = hub_state
        .prune_expired_jwt_revocations(now, RETENTION_BATCH)
        .await?;
    let replay = hub_state
        .prune_expired_replay_ids(now, RETENTION_BATCH)
        .await?;
    let refresh = hub_state
        .prune_expired_refresh_tokens(now, RETENTION_BATCH)
        .await?;
    let task_routes = hub_state
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
        "hub retention gc completed"
    );
    Ok(())
}

pub(super) fn spawn_authorization_retention_gc_loop(
    hub_state: HubState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if stop.is_cancelled() {
                return;
            }
            let pause = match run_authorization_retention_gc(&hub_state).await {
                Ok(()) => Duration::from_secs(60 * 60),
                Err(err) => {
                    tracing::error!("hub authorization retention failed: {err}");
                    Duration::from_secs(60)
                }
            };
            tokio::select! { _ = tokio::time::sleep(pause) => {}, _ = stop.cancelled() => return }
        }
    })
}

/// Keep the usage ledger's monthly record partitions three months ahead: at startup,
/// then once a day, and a minute after a failure.
pub(super) fn spawn_usage_partition_loop(
    hub_state: HubState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let pause = match hub_state.prepare_usage_partitions().await {
                Ok(created) => {
                    if created > 0 {
                        tracing::info!(created, "usage ledger partitions prepared");
                    }
                    Duration::from_secs(24 * 60 * 60)
                }
                Err(err) => {
                    tracing::error!("usage ledger partition preparation failed: {err:#}");
                    Duration::from_secs(60)
                }
            };
            tokio::select! { _ = tokio::time::sleep(pause) => {}, _ = stop.cancelled() => return }
        }
    })
}

pub(super) fn spawn_refresh_delivery_gc_loop(
    hub_state: HubState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! { _ = tokio::time::sleep(REFRESH_DELIVERY_GC_INTERVAL) => {}, _ = stop.cancelled() => return }
            match hub_state
                .clear_expired_refresh_delivery_envelopes(Utc::now(), RETENTION_BATCH)
                .await
            {
                Ok(cleared) => tracing::info!(
                    cleared_refresh_delivery_envelopes = cleared.rows,
                    batches = cleared.batches,
                    "hub refresh delivery-envelope gc completed"
                ),
                Err(err) => tracing::error!("hub refresh delivery-envelope gc failed: {err}"),
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
