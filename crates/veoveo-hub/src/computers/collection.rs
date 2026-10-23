//! Collection: the Hub deletes the sandbox of a Computer idle for
//! `computers.collectAfterSeconds`, keeping its home, as one operation claimed on the
//! Computer like a restart, and records the Computer as collected. A collected Computer
//! has no sandbox, so its next entry creates one at the current version and tells of an
//! update as any creation does.
//!
//! Idle means no browser request through this Hub and no network traffic from the
//! sandbox, OpenShell's `last_activity_time`, for that long. The Hub keeps its browser
//! requests in memory, so both count from no earlier than this Hub's start: a Hub that
//! restarts treats every Computer as active from then, and never collects in a burst.
//! A sandbox that is not ready, such as one being created, and a Computer whose
//! replacement runs are never collected.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use openshell_sdk::raw::proto as p;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    ComputerId,
    lifecycle::{OPERATION_DEADLINE, OPERATION_STALE, running},
    provisioning::{Installation, phase},
};
use crate::{HubCatalogHandle, HubState, store::Computer};

/// How often the Hub looks for idle Computers. Collection waits hours.
const SWEEP_INTERVAL: Duration = Duration::from_secs(300);

/// The last browser request the Hub forwarded to each Computer, since this Hub started.
#[derive(Debug, Default)]
pub(crate) struct BrowserRequests(Mutex<HashMap<Uuid, DateTime<Utc>>>);
impl BrowserRequests {
    pub(super) fn record(&self, computer: &ComputerId, at: DateTime<Utc>) {
        self.0
            .lock()
            .expect("browser requests lock")
            .insert(computer.as_uuid(), at);
    }
    pub(super) fn last(&self, computer: &ComputerId) -> Option<DateTime<Utc>> {
        self.0
            .lock()
            .expect("browser requests lock")
            .get(&computer.as_uuid())
            .copied()
    }
}

/// How long a Computer has been idle, each counted from no earlier than the Hub's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Idle {
    /// Since the last browser request through this Hub.
    pub browser: TimeDelta,
    /// Since the sandbox's last network traffic.
    pub sandbox: TimeDelta,
}

/// The time a timestamp names.
fn time(timestamp: &prost_types::Timestamp) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(timestamp.seconds, u32::try_from(timestamp.nanos).ok()?)
}

/// Whether to collect `computer`, whose sandbox is `sandbox`, at `now`: no replacement
/// runs, its sandbox is ready, and both its last browser request and its sandbox's last
/// network traffic are at least `after` old, each counted from no earlier than
/// `started`. A sandbox with no recorded traffic counts from its creation; one with
/// neither time is active.
pub(super) fn idle(
    now: DateTime<Utc>,
    started: DateTime<Utc>,
    after: TimeDelta,
    computer: &Computer,
    browser: Option<DateTime<Utc>>,
    sandbox: &p::Sandbox,
) -> Option<Idle> {
    if running(computer).is_some() || phase(sandbox) != p::SandboxPhase::Ready {
        return None;
    }
    let traffic = sandbox
        .status
        .as_ref()
        .and_then(|status| status.last_activity_time.as_ref())
        .or_else(|| {
            sandbox
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.created_time.as_ref())
        })
        .and_then(time)?;
    let idle = Idle {
        browser: now - browser.map_or(started, |at| at.max(started)),
        sandbox: now - traffic.max(started),
    };
    (idle.browser >= after && idle.sandbox >= after).then_some(idle)
}

/// The sweep that collects idle Computers.
#[derive(Clone)]
pub(crate) struct Collection {
    pub(super) catalog: HubCatalogHandle,
    pub(super) hub: HubState,
    pub(super) browser: Arc<BrowserRequests>,
    /// When this Hub started: nothing is idle from before.
    pub(super) started: DateTime<Utc>,
}
impl Collection {
    /// Sweep every [`SWEEP_INTERVAL`] until `stop`.
    pub(super) fn spawn(self, stop: CancellationToken) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(SWEEP_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = stop.cancelled() => return,
                    _ = tick.tick() => {}
                }
                self.tick(Utc::now()).await;
            }
        });
    }
    /// One sweep with the current Computer settings, when they collect.
    async fn tick(&self, now: DateTime<Utc>) {
        let catalog = self.catalog.current();
        let Some(computers) = catalog
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.computers.as_ref())
        else {
            return;
        };
        let Some(after) = computers.config.collect_after_seconds else {
            return;
        };
        let after = i64::try_from(after.get())
            .ok()
            .and_then(TimeDelta::try_seconds)
            .unwrap_or(TimeDelta::MAX);
        self.sweep(&computers.installation, after, now).await;
    }
    /// Collect every Computer idle for `after` at `now`.
    pub(super) async fn sweep(
        &self,
        installation: &Installation,
        after: TimeDelta,
        now: DateTime<Utc>,
    ) {
        let computers = match self
            .hub
            .hub_store()
            .collectable_computers(now - OPERATION_STALE)
            .await
        {
            Ok(computers) => computers,
            Err(error) => {
                tracing::warn!("Computer collection could not list Computers: {error:#}");
                return;
            }
        };
        for computer in computers {
            if let Err(error) = self.consider(installation, &computer, after, now).await {
                tracing::warn!(computer = %computer.id, "Computer collection failed: {error:#}");
            }
        }
    }
    async fn consider(
        &self,
        installation: &Installation,
        computer: &Computer,
        after: TimeDelta,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let provisioner = installation.for_computer(computer)?;
        let Some(sandbox) = provisioner.sandbox(&computer.native_name()).await? else {
            return Ok(());
        };
        let browser = self.browser.last(&computer.id);
        let Some(idle) = idle(now, self.started, after, computer, browser, &sandbox) else {
            return Ok(());
        };
        let store = self.hub.hub_store();
        let operation = Uuid::now_v7();
        let claimed = Utc::now();
        if !store
            .claim_computer_operation(
                computer,
                operation,
                &computer.provisioning,
                claimed,
                claimed - OPERATION_STALE,
            )
            .await?
        {
            return Ok(());
        }
        let version = computer.provisioning.version()?;
        // A browser request since the decision keeps the Computer.
        let collected = if self.browser.last(&computer.id) == browser {
            let started = std::time::Instant::now();
            let removed =
                tokio::time::timeout(OPERATION_DEADLINE, provisioner.remove(computer, operation))
                    .await
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "the sandbox was not gone within {} s",
                            OPERATION_DEADLINE.as_secs()
                        ))
                    });
            let seconds = started.elapsed().as_secs_f64();
            match removed {
                Ok(()) => {
                    tracing::info!(
                        computer = %computer.id,
                        %version,
                        browser_idle_seconds = idle.browser.num_seconds(),
                        sandbox_idle_seconds = idle.sandbox.num_seconds(),
                        seconds,
                        "Computer collected"
                    );
                    Some(Utc::now())
                }
                Err(error) => {
                    tracing::warn!(computer = %computer.id, %version, seconds, "Computer collection failed: {error:#}");
                    None
                }
            }
        } else {
            None
        };
        store
            .end_computer_collection(computer, operation, collected)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
