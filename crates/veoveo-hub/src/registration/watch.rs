//! Every replica follows registrations through notifications on the `hub` channel
//! and retains every deployment, selecting its newest live process registration. A
//! housekeeping loop removes the registrations whose logins hold no live kernel
//! instance for the grace period, except an instance's, which only its next
//! registration replaces.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use crate::audit::RegistrationActivity;
use chrono::{TimeDelta, Utc};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use veoveo_hub_sdk::registration::ClientMetadata;
use veoveo_store::ChangeSignal;
use veoveo_types::{DeploymentId, OAuthClientId};

use super::{
    RegisteredServer, RegistrationService, RegistrationSnapshot, ServerCapabilities,
    store::StoredRegistration,
};
use crate::{HubServerHealth, HubServerHealthState};

/// How often a replica checks which logins hold a live instance and collects the
/// registrations that held none for the grace period.
pub const HOUSEKEEPING_INTERVAL: Duration = Duration::from_secs(5);

/// The table whose changes reload registrations.
const REGISTRATION_TABLE: &str = "registration";

#[derive(Default)]
pub(crate) struct LiveRegistrations {
    rows: BTreeMap<OAuthClientId, StoredRegistration>,
    /// The client and metadata of every routed server, to replace the catalog only
    /// when routing changes.
    routed: Vec<(OAuthClientId, ClientMetadata)>,
}

impl RegistrationService {
    /// The registered metadata of a live registration, by its client identifier. Every
    /// live registration of a server authenticates, not only the one that routes.
    pub fn registered_client(&self, client_id: &OAuthClientId) -> Option<ClientMetadata> {
        self.inner
            .live
            .read()
            .rows
            .get(client_id)
            .map(|row| row.metadata.clone())
    }

    /// Reload every registration and retain the newest process of each deployment.
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let rows = self.inner.store.registrations().await?;
        self.inner.live.write().rows = rows
            .into_iter()
            .map(|row| (row.client_id.clone(), row))
            .collect();
        self.route();
        Ok(())
    }

    /// Reload the one registration a change names, or forget it when its row is gone,
    /// and route again.
    pub(super) async fn reload(&self, client_id: &OAuthClientId) -> anyhow::Result<()> {
        let row = self.inner.store.registration(client_id).await?;
        {
            let mut live = self.inner.live.write();
            match row {
                Some(row) => live.rows.insert(client_id.clone(), row),
                None => live.rows.remove(client_id),
            };
        }
        self.route();
        Ok(())
    }

    /// Apply one notification: a change that names its registration reloads that row,
    /// and a reconciliation or a change without a usable key reloads every row.
    async fn apply(&self, signal: ChangeSignal) -> anyhow::Result<()> {
        match signal {
            ChangeSignal::Changed(change) if change.table == REGISTRATION_TABLE => {
                match change
                    .key
                    .as_deref()
                    .and_then(|key| OAuthClientId::parse(key).ok())
                {
                    Some(client_id) => self.reload(&client_id).await,
                    None => self.refresh().await,
                }
            }
            ChangeSignal::Changed(_) => Ok(()),
            ChangeSignal::Reconcile => self.refresh().await,
        }
    }

    /// Retain every deployment. A process can resume the same deployment;
    /// a different deployment never takes over another environment's logical route.
    fn route(&self) {
        let mut live = self.inner.live.write();
        let mut chosen: BTreeMap<DeploymentId, &StoredRegistration> = BTreeMap::new();
        for row in live.rows.values() {
            let deployment = row.metadata.registration.deployment;
            match chosen.get(&deployment) {
                Some(current) if current.issued_at >= row.issued_at => {}
                _ => {
                    chosen.insert(deployment, row);
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

    /// Mark the registrations whose logins lost their live instance, clear the mark of
    /// those that regained one, and remove the ones orphaned for the grace period with
    /// their logins. An instance's registration is never marked or removed. A round in
    /// which no liveness changed writes nothing.
    pub async fn collect_abandoned(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        let grace = TimeDelta::from_std(self.inner.grace)?;
        for ended in self.inner.store.collect(now, now - grace).await? {
            self.ended(&ended, RegistrationActivity::Expire).await?;
        }
        Ok(())
    }

    /// Health of every routed server except instances: healthy while its login holds a
    /// live kernel instance, degraded since housekeeping found it without one until the
    /// grace period removes it. An instance's health is its factory sandbox's
    /// ([`crate::factory::InstanceStatus::health`]); its server may hold its kernel
    /// instance in a data environment's database, which liveness does not see.
    pub async fn health(&self) -> anyhow::Result<BTreeMap<DeploymentId, HubServerHealth>> {
        let routed = self
            .inner
            .live
            .read()
            .routed
            .iter()
            .map(|(client_id, _)| client_id.clone())
            .collect::<Vec<_>>();
        let now = Utc::now();
        Ok(self
            .inner
            .store
            .liveness(&routed)
            .await?
            .into_iter()
            .map(|liveness| {
                let health = if liveness.alive {
                    HubServerHealth {
                        state: HubServerHealthState::Healthy,
                        checked_at: now,
                    }
                } else {
                    HubServerHealth {
                        state: HubServerHealthState::Degraded,
                        checked_at: liveness.orphaned_at.unwrap_or(now),
                    }
                };
                (liveness.deployment, health)
            })
            .collect())
    }

    /// Follow registration changes until `stop` fires. The stream asks for a full
    /// reload once subscribed and after every reconnection.
    pub fn spawn_watcher(&self, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut changes = service
                .inner
                .store
                .platform()
                .listen(vec![crate::store::channel()]);
            loop {
                let signal = tokio::select! {
                    () = stop.cancelled() => return,
                    signal = changes.next() => signal,
                };
                match signal {
                    Some(Ok(signal)) => {
                        if let Err(error) = service.apply(signal).await {
                            tracing::error!("registration reload failed: {error:#}");
                        }
                    }
                    Some(Err(error)) => {
                        tracing::warn!("registration notifications interrupted: {error}");
                    }
                    None => return,
                }
            }
        })
    }

    /// Collect abandoned registrations every [`HOUSEKEEPING_INTERVAL`] until `stop`
    /// fires.
    pub fn spawn_housekeeping(&self, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(HOUSEKEEPING_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = stop.cancelled() => return,
                    _ = tick.tick() => {}
                }
                if let Err(error) = service.collect_abandoned().await {
                    tracing::error!("registration housekeeping failed: {error:#}");
                }
            }
        })
    }
}

fn registered(row: &StoredRegistration) -> RegisteredServer {
    let registration = &row.metadata.registration;
    RegisteredServer {
        deployment: row.metadata.registration.deployment,
        slug: registration.slug.clone(),
        uri_scheme: registration.uri_scheme.clone(),
        endpoint: registration.endpoint.clone(),
        version: registration.version.clone(),
        client_id: row.client_id.clone(),
        authorization: Arc::new(row.metadata.authorization.clone()),
        capabilities: ServerCapabilities::from_declaration(&row.metadata.authorization),
        branch: row.branch.clone(),
    }
}
