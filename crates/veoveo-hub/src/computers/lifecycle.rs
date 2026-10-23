//! One lifecycle path: a Computer has a sandbox at some version, or none. Entry with
//! none creates one at the current version, entry with a ready one serves it whatever
//! its version, and one still starting is awaited. Restart, update and recovery are the
//! same replacement, which runs detached: delete the sandbox keeping its home, create
//! one of the target version, wait until it is ready and record that version on the
//! Computer. A failure records its cause; the Computer keeps the version it had.
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use openshell_sdk::raw::proto as p;
use serde::Serialize;
use uuid::Uuid;

use super::{
    ComputerId, ComputerProvisionSpec, ComputerRuntime,
    provisioning::{Installation, phase, phase_name},
    version::{ComputerFailure, ComputerVersion, UpdateNotice, VersionInfo, WhatsNew, whats_new},
};
use crate::store::{Computer, ComputerOperation, HubStore};

/// How long one replacement may take: the old sandbox's deletion and the new one's
/// start, each bounded on its own.
const OPERATION_DEADLINE: Duration = Duration::from_secs(600);
/// A replacement this old belongs to a worker that ended without recording it, such as
/// a Hub that stopped; entry may begin another.
const OPERATION_STALE: TimeDelta = TimeDelta::seconds(660);
/// How long a restart waits before it deletes the sandbox, so that its answer reaches
/// a caller inside the Computer, such as an agent restarting its own Computer, first.
const ANSWER_FIRST: Duration = Duration::from_secs(3);

/// The version a replacement creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// The installation's current version.
    Current,
    /// The version recorded on the Computer, after a replacement failed.
    Previous,
}

/// Whether a replacement began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Begun {
    Started,
    /// Another replacement runs.
    Running,
}

/// A Computer as entry sees it: its row, read now, its sandbox and whether the Hub can
/// still serve that sandbox safely.
pub(super) struct Observed {
    pub computer: Computer,
    pub sandbox: Option<p::Sandbox>,
    /// Why the Hub can no longer serve the sandbox as the Computer's recorded version.
    pub blocked: Option<String>,
}
impl Observed {
    /// The replacement that runs now, unless its worker ended without recording it.
    pub fn operation(&self) -> Option<&ComputerOperation> {
        running(&self.computer)
    }
    pub fn phase(&self) -> Option<p::SandboxPhase> {
        self.sandbox.as_ref().map(phase)
    }
}

fn running(computer: &Computer) -> Option<&ComputerOperation> {
    computer
        .operation
        .as_ref()
        .filter(|operation| operation.started_at > Utc::now() - OPERATION_STALE)
}

/// What entry does next.
pub(super) enum Entry {
    /// Its sandbox is ready: serve it.
    Ready,
    /// Its sandbox is being created or started: the start page until it is ready,
    /// listing what the replacement brings when one runs.
    Starting(Option<Box<ComputerOperation>>),
    /// A failed replacement left no sandbox, or the Hub can no longer serve the
    /// sandbox safely: the status page, with Restart.
    Attention,
}

/// A replacement that runs now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartStatus {
    pub version: VersionInfo,
    pub since: DateTime<Utc>,
}

/// A Computer's status: the version it runs and the current one, whether an update is
/// available and what it brings, a replacement that runs or the last one's failure,
/// and an update completed since the owner was last told.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerStatus {
    pub computer: ComputerId,
    /// The version its sandbox was last created at.
    pub version: VersionInfo,
    /// Its sandbox's phase, or `none`.
    pub sandbox: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<VersionInfo>,
    /// Why no new sandbox can start now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_unavailable: Option<String>,
    pub update_available: bool,
    /// What the current version brings over the one the Computer runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whats_new: Option<WhatsNew>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<ComputerFailure>,
    /// Why the Hub can no longer serve the sandbox safely. A restart creates it again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    /// An update completed since the owner was last told, told once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<UpdateNotice>,
}

impl ComputerRuntime {
    /// Read the Computer again, and its sandbox.
    pub(super) async fn observe(&self, computer: &Computer) -> Result<Observed> {
        let computer = self
            .hub
            .hub_store()
            .computer(&computer.tenant, &computer.principal, &computer.id)
            .await?
            .context("the Computer no longer exists")?;
        let provisioner = self.installation.for_computer(&computer)?;
        let sandbox = provisioner.sandbox(&computer.native_name()).await?;
        let blocked = sandbox
            .as_ref()
            .filter(|sandbox| phase(sandbox) != p::SandboxPhase::Deleting)
            .and_then(|sandbox| {
                provisioner
                    .validate_sandbox(&computer, sandbox)
                    .err()
                    .map(|error| format!("{error:#}"))
            });
        Ok(Observed {
            computer,
            sandbox,
            blocked,
        })
    }

    /// Entry of the Computer: begin creating a missing sandbox at the current version
    /// and say what to show.
    pub(super) async fn enter(&self, computer: &Computer) -> Result<Entry> {
        let observed = self.observe(computer).await?;
        if let Some(operation) = observed.operation() {
            return Ok(Entry::Starting(Some(Box::new(operation.clone()))));
        }
        if observed.sandbox.is_some() && observed.blocked.is_some() {
            return Ok(Entry::Attention);
        }
        match observed.phase() {
            Some(p::SandboxPhase::Ready) => Ok(Entry::Ready),
            None | Some(p::SandboxPhase::Stopped | p::SandboxPhase::Completed) => {
                if observed.computer.failure.is_some() {
                    return Ok(Entry::Attention);
                }
                // Nothing runs: create a sandbox at the current version.
                self.begin(&observed.computer, Target::Current, false)
                    .await?;
                let observed = self.observe(computer).await?;
                Ok(Entry::Starting(observed.operation().cloned().map(Box::new)))
            }
            // OpenShell is still starting it, restarting it, or removing it.
            Some(_) => Ok(Entry::Starting(None)),
        }
    }

    /// Begin replacing the Computer's sandbox with one of `target`. Answers at once;
    /// the replacement runs detached, and a restart waits [`ANSWER_FIRST`] before it
    /// deletes the sandbox.
    pub(super) async fn begin(
        &self,
        computer: &Computer,
        target: Target,
        restart: bool,
    ) -> Result<Begun> {
        let spec = match target {
            Target::Current => self.installation.current().await?.spec,
            Target::Previous => computer.provisioning.clone(),
        };
        let version = spec.version()?;
        let operation = Uuid::now_v7();
        let now = Utc::now();
        if !self
            .hub
            .hub_store()
            .claim_computer_operation(computer, operation, &spec, now, now - OPERATION_STALE)
            .await?
        {
            return Ok(Begun::Running);
        }
        // The version it replaces is the one recorded when the claim succeeded.
        let claimed = self
            .hub
            .hub_store()
            .computer(&computer.tenant, &computer.principal, &computer.id)
            .await?
            .context("the Computer no longer exists")?;
        let worker = Worker {
            store: self.hub.hub_store().clone(),
            installation: self.installation.clone(),
            computer: claimed,
            operation,
            spec,
            version,
            wait: restart.then_some(ANSWER_FIRST),
        };
        tokio::spawn(worker.run());
        Ok(Begun::Started)
    }

    /// The Computer's status. With `tell`, an update completed since the owner was last
    /// told is returned once.
    pub(super) async fn status(&self, computer: &Computer, tell: bool) -> Result<ComputerStatus> {
        let observed = self.observe(computer).await?;
        let computer = &observed.computer;
        let version = computer.provisioning.info()?;
        let (current, current_unavailable) = match self.installation.current().await {
            Ok(current) => (Some(current.spec), None),
            Err(unavailable) => (None, Some(unavailable.0)),
        };
        let current_info = current
            .as_ref()
            .map(ComputerProvisionSpec::info)
            .transpose()?;
        let update_available = current_info
            .as_ref()
            .is_some_and(|current| current.version != version.version);
        let whats_new = match &current {
            Some(current) if update_available => Some(whats_new(&computer.provisioning, current)?),
            _ => None,
        };
        let restart = observed
            .operation()
            .map(|operation| {
                Ok::<_, anyhow::Error>(RestartStatus {
                    version: operation.provisioning.info()?,
                    since: operation.started_at,
                })
            })
            .transpose()?;
        let updated = if tell {
            self.hub
                .hub_store()
                .take_computer_update_notice(computer)
                .await?
        } else {
            computer.update_notice.clone()
        };
        Ok(ComputerStatus {
            computer: computer.id.clone(),
            version,
            sandbox: observed.phase().map_or("none", phase_name),
            current: current_info,
            current_unavailable,
            update_available,
            whats_new,
            restart,
            failure: computer.failure.clone(),
            blocked: observed.blocked.clone(),
            updated,
        })
    }
}

/// One replacement, detached from the request that began it.
struct Worker {
    store: HubStore,
    installation: Installation,
    /// The Computer as the replacement began: the version it replaces.
    computer: Computer,
    operation: Uuid,
    spec: ComputerProvisionSpec,
    version: ComputerVersion,
    wait: Option<Duration>,
}
impl Worker {
    async fn run(self) {
        if let Some(wait) = self.wait {
            tokio::time::sleep(wait).await;
        }
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(OPERATION_DEADLINE, async {
            self.installation
                .for_spec(self.spec.clone())?
                .replace(&self.computer, self.operation)
                .await
        })
        .await
        .unwrap_or_else(|_| {
            Err(anyhow::anyhow!(
                "the replacement did not finish within {} s",
                OPERATION_DEADLINE.as_secs()
            ))
        });
        let seconds = started.elapsed().as_secs_f64();
        let version = &self.version;
        let recorded = match result {
            Ok(()) => {
                tracing::info!(computer = %self.computer.id, %version, seconds, "Computer sandbox replaced");
                self.store
                    .finish_computer_operation(
                        &self.computer,
                        self.operation,
                        self.notice().as_ref(),
                    )
                    .await
            }
            Err(error) => {
                let cause = format!("{error:#}");
                tracing::warn!(computer = %self.computer.id, %version, seconds, %cause, "Computer sandbox replacement failed");
                self.store
                    .fail_computer_operation(
                        &self.computer,
                        self.operation,
                        &ComputerFailure {
                            cause,
                            at: Utc::now(),
                            version: version.clone(),
                        },
                    )
                    .await
            }
        };
        if let Err(error) = recorded {
            tracing::error!(computer = %self.computer.id, "a Computer replacement's end was not recorded: {error:#}");
        }
    }
    /// The notice of this replacement's update, when it changes the version: from the
    /// version the owner was last told of, to this one.
    fn notice(&self) -> Option<UpdateNotice> {
        let from = self.computer.provisioning.version().ok()?;
        let to = self.version.clone();
        let from = self
            .computer
            .update_notice
            .as_ref()
            .map_or(from, |earlier| earlier.from.clone());
        if from == to {
            return None;
        }
        let whats_new = whats_new(&self.computer.provisioning, &self.spec).unwrap_or_default();
        Some(UpdateNotice {
            from,
            to,
            whats_new,
        })
    }
}

#[cfg(test)]
mod tests;
