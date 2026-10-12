//! Every replica observes registrations through a SurrealDB LIVE query and routes to
//! the servers whose leases are current. A lease that lapses leaves routing at the
//! next tick, on every replica, whether or not its row was already removed.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use chrono::{DateTime, TimeDelta, Utc};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use veoveo_audit_contract::RegistrationActivity;
use veoveo_mcp_ext_registration::ClientMetadata;
use veoveo_platform_store::{ObservationReplay, ObservationTable};
use veoveo_types::{OAuthClientId, ServerSlug};

use super::{
    RegisteredServer, RegistrationService, RegistrationSnapshot, ServerCapabilities,
    store::{REGISTRATION_TABLE, StoredRegistration},
};
use crate::{GatewayServerHealth, GatewayServerHealthState};

const TICK: Duration = Duration::from_secs(1);
const RECONNECT: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct LiveRegistrations {
    rows: BTreeMap<OAuthClientId, StoredRegistration>,
    /// The client and metadata of every routed server, to replace the catalog only
    /// when routing changes rather than on every renewal.
    routed: Vec<(OAuthClientId, ClientMetadata)>,
}

impl RegistrationService {
    /// Reload every registration and route to the current ones.
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let rows = self.inner.store.registrations().await?;
        self.inner.live.write().rows = rows
            .into_iter()
            .map(|row| (row.client_id.clone(), row))
            .collect();
        self.route(Utc::now());
        Ok(())
    }

    /// Route to the newest current registration of each slug.
    fn route(&self, now: DateTime<Utc>) {
        let mut live = self.inner.live.write();
        let mut chosen: BTreeMap<ServerSlug, &StoredRegistration> = BTreeMap::new();
        for row in live.rows.values().filter(|row| row.lease_expires_at > now) {
            let slug = row.metadata.slug().clone();
            match chosen.get(&slug) {
                Some(current) if current.issued_at >= row.issued_at => {}
                _ => {
                    chosen.insert(slug, row);
                }
            }
        }
        let routed: Vec<(OAuthClientId, ClientMetadata)> = chosen
            .values()
            .map(|row| (row.client_id.clone(), row.metadata.clone()))
            .collect();
        if routed == live.routed {
            return;
        }
        let snapshot = RegistrationSnapshot::new(chosen.values().map(|row| registered(row)));
        tracing::info!(
            servers = ?snapshot.iter().map(|server| server.slug.as_str()).collect::<Vec<_>>(),
            "registered servers changed"
        );
        live.routed = routed;
        drop(live);
        self.inner.catalog.replace_registrations(snapshot);
    }

    /// Remove lapsed registrations from the store and from routing.
    async fn sweep(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        for expired in self.inner.store.expire(now).await? {
            self.release(&expired, RegistrationActivity::Expire).await?;
        }
        self.route(now);
        Ok(())
    }

    /// Lease health of every routed server: healthy while it renews on schedule,
    /// degraded once a renewal is late.
    pub fn health(&self) -> BTreeMap<ServerSlug, GatewayServerHealth> {
        let renew = TimeDelta::seconds(i64::from(
            self.inner
                .catalog
                .current()
                .control_plane()
                .registration
                .as_ref()
                .map(|settings| settings.lease.renew_every_seconds)
                .unwrap_or(10),
        ));
        let now = Utc::now();
        let live = self.inner.live.read();
        live.routed
            .iter()
            .filter_map(|(client_id, _)| live.rows.get(client_id))
            .map(|row| {
                let state = if row.lease_expires_at <= now {
                    GatewayServerHealthState::Offline
                } else if now - row.updated_at > renew + renew / 2 {
                    GatewayServerHealthState::Degraded
                } else {
                    GatewayServerHealthState::Healthy
                };
                (
                    row.metadata.slug().clone(),
                    GatewayServerHealth {
                        state,
                        checked_at: row.updated_at,
                    },
                )
            })
            .collect()
    }

    /// Follow registration changes until `stop` fires.
    pub fn spawn_watcher(&self, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let table = ObservationTable::new(
                veoveo_modules::TableName::new(REGISTRATION_TABLE).expect("registration table"),
                ObservationReplay::LiveOnly,
            );
            loop {
                let stream = service
                    .inner
                    .store
                    .platform()
                    .live::<surrealdb::types::Value>(table.clone())
                    .await;
                match stream {
                    Ok(mut stream) => {
                        // Rows written before the LIVE query registered arrive here.
                        if let Err(error) = service.refresh().await {
                            tracing::error!("registration reload failed: {error:#}");
                        }
                        let mut tick = tokio::time::interval(TICK);
                        loop {
                            tokio::select! {
                                () = stop.cancelled() => return,
                                event = stream.next() => match event {
                                    Some(Ok(_)) => {
                                        if let Err(error) = service.refresh().await {
                                            tracing::error!("registration reload failed: {error:#}");
                                        }
                                    }
                                    Some(Err(error)) => {
                                        tracing::warn!("registration LIVE query ended: {error}");
                                        break;
                                    }
                                    None => break,
                                },
                                _ = tick.tick() => {
                                    if let Err(error) = service.sweep().await {
                                        tracing::error!("registration expiry failed: {error:#}");
                                    }
                                }
                            }
                        }
                    }
                    Err(error) => tracing::warn!("registration LIVE query failed: {error}"),
                }
                // Leases keep expiring from routing while the query reconnects.
                service.route(Utc::now());
                tokio::select! {
                    () = stop.cancelled() => return,
                    () = tokio::time::sleep(RECONNECT) => {}
                }
            }
        })
    }
}

fn registered(row: &StoredRegistration) -> RegisteredServer {
    let registration = &row.metadata.registration;
    RegisteredServer {
        slug: registration.slug.clone(),
        uri_scheme: registration.uri_scheme.clone(),
        endpoint: registration.endpoint.clone(),
        version: registration.version.clone(),
        client_id: row.client_id.clone(),
        authorization: Arc::new(row.metadata.authorization.clone()),
        capabilities: ServerCapabilities::from_declaration(&row.metadata.authorization),
    }
}
