//! Why an instance serves or does not: OpenShell's view of its sandbox, and what the
//! Hub last applied to it and why that failed.
use chrono::Utc;
use openshell_sdk::raw::proto as p;
use serde::Serialize;

use super::Revision;
use crate::{HubServerHealth, HubServerHealthState};

/// An instance's status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    /// The sandbox as OpenShell reports it; none while OpenShell has no sandbox for the
    /// instance.
    pub sandbox: Option<SandboxStatus>,
    /// The revision the Hub last applied to the sandbox.
    pub revision: Option<Revision>,
    /// Why the Hub's last step for the instance failed, until a later one succeeds.
    pub failed: Option<Failure>,
    /// Why the instance was created above a cap.
    pub warning: Option<String>,
}

/// The sandbox's lifecycle as OpenShell reports it. OpenShell restarts its main program
/// whenever it exits, with backoff, so a failed fetch, build or server start shows as
/// its exit code and restart count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxStatus {
    pub phase: Phase,
    /// The main program's last exit code; signals count as 128 plus the signal number.
    /// OpenShell clears it once a restarted program becomes ready.
    pub exit_code: Option<i32>,
    /// Consecutive restarts in the current crash loop.
    pub restart_count: u32,
    /// OpenShell's readiness and lifecycle conditions, such as a scheduling failure.
    pub conditions: Vec<Condition>,
}

/// OpenShell's lifecycle phase of a sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Provisioning,
    Starting,
    Ready,
    Stopping,
    Stopped,
    Completed,
    Error,
    Deleting,
    Unknown,
}

/// One OpenShell condition: its type, status, reason and message, as OpenShell or the
/// compute platform reported them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub reason: String,
    pub message: String,
}

/// The Hub's step that failed, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub stage: Stage,
    pub message: String,
}

/// The Hub's steps for an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Creating the instance would exceed an enforced cap.
    Capacity,
    /// Creating the instance's provider or sandbox.
    Create,
    /// Setting the sandbox's policy, stopping and starting it.
    Restart,
    /// Deleting the sandbox, its providers and the instance.
    Delete,
}

impl Stage {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Capacity => "capacity",
            Self::Create => "create",
            Self::Restart => "restart",
            Self::Delete => "delete",
        }
    }

    pub(super) fn parse(value: &str) -> anyhow::Result<Self> {
        Ok(match value {
            "capacity" => Self::Capacity,
            "create" => Self::Create,
            "restart" => Self::Restart,
            "delete" => Self::Delete,
            other => anyhow::bail!("unknown factory stage `{other}`"),
        })
    }
}

impl Phase {
    pub(super) fn of(sandbox: &p::Sandbox) -> Self {
        let phase = sandbox.status.as_ref().map_or(0, |status| status.phase);
        match p::SandboxPhase::try_from(phase).unwrap_or(p::SandboxPhase::Unknown) {
            p::SandboxPhase::Provisioning => Self::Provisioning,
            p::SandboxPhase::Starting => Self::Starting,
            p::SandboxPhase::Ready => Self::Ready,
            p::SandboxPhase::Stopping => Self::Stopping,
            p::SandboxPhase::Stopped => Self::Stopped,
            p::SandboxPhase::Completed => Self::Completed,
            p::SandboxPhase::Error => Self::Error,
            p::SandboxPhase::Deleting => Self::Deleting,
            p::SandboxPhase::Unknown | p::SandboxPhase::Unspecified => Self::Unknown,
        }
    }
}

impl SandboxStatus {
    pub(super) fn of(sandbox: &p::Sandbox) -> Self {
        let status = sandbox.status.as_ref();
        Self {
            phase: Phase::of(sandbox),
            exit_code: status.and_then(|status| status.exit_code),
            restart_count: status.map_or(0, |status| status.restart_count),
            conditions: status
                .map(|status| {
                    status
                        .conditions
                        .iter()
                        .map(|condition| Condition {
                            kind: condition.r#type.clone(),
                            status: condition.status.clone(),
                            reason: condition.reason.clone(),
                            message: condition.message.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

impl InstanceStatus {
    /// The instance's health in the Hub's catalog: healthy while its sandbox is ready
    /// and the Hub's last step for it succeeded, degraded while the sandbox is on its
    /// way to ready or still serves after a failed step, offline otherwise.
    pub fn health(&self) -> HubServerHealth {
        let phase = self.sandbox.as_ref().map(|sandbox| sandbox.phase);
        let state = match (phase, &self.failed) {
            (Some(Phase::Ready), None) => HubServerHealthState::Healthy,
            (Some(Phase::Ready | Phase::Provisioning | Phase::Starting | Phase::Stopping), _) => {
                HubServerHealthState::Degraded
            }
            _ => HubServerHealthState::Offline,
        };
        HubServerHealth {
            state,
            checked_at: Utc::now(),
        }
    }
}
