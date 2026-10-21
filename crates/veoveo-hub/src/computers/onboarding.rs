//! Best-effort public handbook delivery and optional executable-owned onboarding.
//! Neither owns browser custody or auth.
use std::{future::Future, pin::Pin, time::Duration};

use anyhow::{Context, Result};
use axum::{
    Json,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use openshell_sdk::{ExecOptions, ExecResult, raw::proto as p};
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::OAuthClientId;
use veoveo_types::WorkContextId;

use super::{BrowserSession, ComputerRuntime, oauth};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerOnboardingStatus {
    Disabled,
    Preparing,
    AuthenticationRequired,
    Denied,
    Unavailable,
    Conflict,
    Ready,
}

impl ComputerOnboardingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Preparing => "preparing",
            Self::AuthenticationRequired => "authentication_required",
            Self::Denied => "denied",
            Self::Unavailable => "unavailable",
            Self::Conflict => "conflict",
            Self::Ready => "ready",
        }
    }
}

/// Constructed only after current browser authority and native custody admission.
/// The contribution gets no browser bearer or refresh-token material.
#[derive(Clone)]
pub struct ComputerOnboardingContext {
    pub session: BrowserSession,
    pub client: OAuthClientId,
    pub work_context: WorkContextId,
    runtime: ComputerRuntime,
}
impl ComputerOnboardingContext {
    async fn deliver_handbook(&self) -> Result<()> {
        let Some(handbook) = self
            .runtime
            .catalog
            .current()
            .control_plane()
            .computers
            .as_ref()
            .context("Computer handbook configuration unavailable")?
            .handbook
            .clone()
        else {
            return Ok(());
        };
        let result = self
            .exec(
                vec!["/bin/sh".into(), "-ec".into(), INSTALL_HANDBOOK.into()],
                handbook.into_bytes(),
            )
            .await?;
        anyhow::ensure!(result.exit_code == 0, "Computer handbook delivery failed");
        Ok(())
    }
    /// Re-read current browser family/client/owner authority before delayed work
    /// or a later native operation. The contribution cannot extend the session.
    pub async fn recheck(&self) -> Result<()> {
        let family = self
            .runtime
            .hub
            .hub_store()
            .refresh_family(self.session.family())
            .await?
            .context("Computer onboarding family missing")?;
        let owner = self.session.computer();
        let computer = self
            .runtime
            .hub
            .hub_store()
            .computer(&owner.tenant, &owner.principal, &owner.id)
            .await?
            .context("Computer onboarding owner mapping ended")?;
        anyhow::ensure!(
            computer.ready
                && computer.handoff.is_none()
                && computer.family.as_ref() == Some(self.session.family()),
            "Computer onboarding custody changed"
        );
        self.runtime
            .hub
            .admit_computer_family(
                &self.runtime.catalog.current(),
                &self.session.computer().provisioning.config,
                computer,
                family,
            )
            .await?
            .context("Computer onboarding authority ended")?;
        Ok(())
    }
    /// Execute trusted onboarding in this admitted owner's exact native Computer.
    /// No caller-selected workspace, sandbox, provisioning authority, or command
    /// is exposed by the browser routes.
    pub async fn exec(&self, command: Vec<String>, stdin: Vec<u8>) -> Result<ExecResult> {
        self.recheck().await?;
        let computer = self.session.computer();
        let provisioner = self.runtime.provisioner.for_computer(computer)?;
        let mut grpc = provisioner.gateway.client.raw_grpc_fresh().await?;
        let sandbox = tokio::time::timeout(
            super::gateway::RPC_TIMEOUT,
            grpc.get_sandbox(p::GetSandboxRequest {
                workspace_scope: Some(p::workspace_selector(computer.native_name())),
                name: computer.native_name(),
            }),
        )
        .await??
        .into_inner()
        .sandbox
        .context("Computer onboarding sandbox missing")?;
        provisioner.validate_sandbox(computer, &sandbox)?;
        self.recheck().await?;
        let opts = ExecOptions {
            stdin: Some(stdin),
            no_login_shell: true,
            timeout: Some(Duration::from_secs(60)),
            workdir: Some("/sandbox".into()),
            ..Default::default()
        };
        tokio::time::timeout(
            Duration::from_secs(65),
            provisioner
                .gateway
                .client
                .workspace(&computer.native_name())
                .exec(&computer.native_name(), &command, opts),
        )
        .await
        .context("Computer onboarding exec deadline")?
        .context("Computer onboarding exec unavailable")
    }
}

const INSTALL_HANDBOOK: &str = "umask 077; mkdir -p /sandbox/home/.config/computer; tmp=$(mktemp /sandbox/home/.config/computer/.instructions.XXXXXX); trap 'rm -f \"$tmp\"' EXIT; cat >\"$tmp\"; chmod 600 \"$tmp\"; mv -f \"$tmp\" /sandbox/home/.config/computer/instructions.md";

pub trait ComputerOnboarding: Send + Sync + 'static {
    fn ensure<'a>(
        &'a self,
        context: ComputerOnboardingContext,
    ) -> Pin<Box<dyn Future<Output = Result<ComputerOnboardingStatus>> + Send + 'a>>;
    /// Persist an interrupted attempt as unavailable, without granting authority.
    fn unavailable<'a>(
        &'a self,
        context: &'a ComputerOnboardingContext,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
    /// Read-only status: no native creation, checkout, or job scheduling.
    fn status<'a>(
        &'a self,
        context: &'a ComputerOnboardingContext,
    ) -> Pin<Box<dyn Future<Output = Result<ComputerOnboardingStatus>> + Send + 'a>>;
}
type Owner = (veoveo_types::TenantId, veoveo_types::PrincipalId);
pub(super) type Jobs = std::sync::Arc<parking_lot::Mutex<std::collections::BTreeSet<Owner>>>;
struct Job {
    jobs: Jobs,
    owner: Owner,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.jobs.lock().remove(&self.owner);
    }
}
impl ComputerRuntime {
    fn onboarding_context(&self, session: BrowserSession) -> ComputerOnboardingContext {
        let config = &session.computer().provisioning.config;
        ComputerOnboardingContext {
            client: config.oauth_client.clone(),
            work_context: config.work_context.clone(),
            session,
            runtime: self.clone(),
        }
    }
    pub(super) fn onboard(&self, session: BrowserSession) {
        let onboarding = self.onboarding.clone();
        let computer = session.computer();
        let owner = (computer.tenant.clone(), computer.principal.clone());
        if !self.onboarding_jobs.lock().insert(owner.clone()) {
            return;
        }
        let job = Job {
            jobs: self.onboarding_jobs.clone(),
            owner,
        };
        let runtime = self.clone();
        tokio::spawn(async move {
            let _job = job;
            let context = runtime.onboarding_context(session);
            if context.deliver_handbook().await.is_err() {
                tracing::warn!(
                    "Computer handbook delivery incomplete; next admitted entry may retry"
                );
            }
            let Some(onboarding) = onboarding else {
                return;
            };
            let result = tokio::time::timeout(Duration::from_secs(180), async {
                // Queueing must not preserve a revoked or replaced browser grant.
                context.recheck().await?;
                onboarding.ensure(context.clone()).await
            })
            .await;
            match result {
                Ok(Ok(_)) => {}
                failure => {
                    match failure {
                        Ok(Err(error)) => {
                            tracing::warn!(%error, "optional Computer onboarding incomplete; Computer custody remains ready")
                        }
                        Err(_) => tracing::warn!(
                            "optional Computer onboarding deadline; next admitted entry may retry"
                        ),
                        Ok(Ok(_)) => unreachable!(),
                    }
                    if !matches!(
                        tokio::time::timeout(
                            super::ADMISSION_READ_TIMEOUT,
                            onboarding.unavailable(&context)
                        )
                        .await,
                        Ok(Ok(()))
                    ) {
                        tracing::warn!("could not record interrupted Computer onboarding status");
                    }
                }
            }
        });
    }
    async fn onboarding_status(&self, session: BrowserSession) -> ComputerOnboardingStatus {
        let Some(onboarding) = &self.onboarding else {
            return ComputerOnboardingStatus::Disabled;
        };
        onboarding
            .status(&self.onboarding_context(session))
            .await
            .unwrap_or(ComputerOnboardingStatus::Unavailable)
    }
}
pub(super) async fn status(runtime: ComputerRuntime, headers: HeaderMap) -> Response {
    let session = async {
        let value =
            oauth::cookie(&headers, oauth::SESSION_COOKIE)?.context("Computer cookie missing")?;
        runtime
            .hub
            .computer_browser_session(
                &runtime.catalog.current(),
                &runtime.config,
                &oauth::digest(&value),
                None,
            )
            .await?
            .context("Computer session unavailable")
    }
    .await;
    match session {
        Ok(session) => {
            oauth::private(Json(runtime.onboarding_status(session).await).into_response())
        }
        Err(_) => oauth::denied(),
    }
}
