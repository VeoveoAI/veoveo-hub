//! Gateway-owned conversion from admitted identity and policy into audit records.
mod vocabulary;

pub use vocabulary::*;

use crate::{
    AuthenticatedSubject, GatewayState,
    request_observation::{RequestStage, StageTimer},
};
use anyhow::{Context, Result};
use veoveo_audit_contract::*;
use veoveo_sdk_rust::PrincipalKind;

use crate::control_plane::{PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyTarget};

impl AuthenticatedSubject {
    pub fn audit_context(&self) -> Result<AuditContext> {
        Ok(AuditContext {
            actor: self.audit_actor()?,
            authority: self.audit_authority(),
            request: self.audit.clone(),
        })
    }
    pub fn audit_actor(&self) -> Result<AuditActor> {
        Ok(AuditActor {
            principal: self.actor.id.clone(),
            kind: match self.actor.kind {
                PrincipalKind::User => AuditPrincipalKind::User,
                PrincipalKind::Service => AuditPrincipalKind::Service,
            },
            tenant: self.actor.tenant.clone(),
            oauth_client: Some(self.access_token.oauth_client_id.clone()),
            delegating_principal: (self.principal.id != self.actor.id)
                .then(|| self.principal.id.clone()),
        })
    }
    pub fn audit_authority(&self) -> AuditAuthority {
        AuditAuthority {
            work_context: Some(self.authority.work_context.clone()),
            policy_revision: Some(self.authority.policy_revision.clone()),
            scopes: self.actor.scopes.clone(),
            data_labels: self.actor.data_labels.clone(),
        }
    }
    pub fn audit_draft(
        &self,
        target: AuditTarget,
        detail: AuditDetail,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> Result<AuditDraft> {
        Ok(
            AuditDraft::builder(self.audit.clone(), target, detail, outcome, reason)
                .actor(self.audit_actor()?)
                .authority(self.audit_authority())
                .build()?,
        )
    }
}
/// MCP projections use typed resource addresses. Domain-only policy targets are
/// recorded by their producer with its own target, never erased to a gateway label.
pub fn mcp_audit_target(target: &PolicyTarget) -> Result<AuditTarget> {
    let audit = gateway_audit();
    Ok(match target {
        PolicyTarget::Gateway => AuditTarget::Installation,
        PolicyTarget::Server { server } => AuditTarget::Server {
            server: server.clone(),
        },
        PolicyTarget::Tool { server, tool } => audit.tool(server.clone(), tool.clone())?,
        PolicyTarget::Resource { uri, .. } => AuditTarget::Resource { uri: uri.clone() },
        PolicyTarget::ResourceTemplate { server, uri } => {
            audit.resource_template(server.clone(), uri.clone())?
        }
        PolicyTarget::Prompt { server, prompt } => audit.prompt(server.clone(), prompt.clone())?,
        PolicyTarget::Task { server, task_id } => {
            audit.task_route(server.clone(), task_id.clone())?
        }
        PolicyTarget::PlatformTask { task_id, .. } => AuditTarget::Task { task: *task_id },
        PolicyTarget::Owner(_) => anyhow::bail!("owner target audit requires its catalog registry"),
        PolicyTarget::Unadmitted(_) => {
            anyhow::bail!("unadmitted owner policy target cannot produce audit authority")
        }
    })
}
/// Owner targets expose an audit resource only through their admitted catalog binding.
pub fn policy_audit_target(
    registry: &crate::contract::CatalogRegistry,
    target: &PolicyTarget,
) -> Result<AuditTarget> {
    if let PolicyTarget::Owner(target) = target {
        registry.check_target(target)?;
        return Ok(AuditTarget::Resource {
            uri: target.audit().uri.clone(),
        });
    }
    mcp_audit_target(target)
}
impl GatewayState {
    pub fn set_audit_health(&self, health: veoveo_audit::AuditHealth) -> Result<()> {
        self.audit_health
            .set(health)
            .map_err(|_| anyhow::anyhow!("audit service already registered"))
    }
    pub fn audit_ready(&self) -> bool {
        self.audit_health
            .get()
            .is_some_and(veoveo_audit::AuditHealth::ready)
            && self
                .audit_writer
                .get()
                .is_some_and(veoveo_audit::AuditWriter::is_running)
    }
    pub async fn audit_writer(&self) -> &veoveo_audit::AuditWriter {
        self.audit_writer
            .get_or_init(|| async { veoveo_audit::AuditWriter::start(self.audit.clone()) })
            .await
    }
    pub async fn record_audit(&self, draft: AuditDraft) -> Result<()> {
        let _timer = StageTimer::start(RequestStage::Audit);
        self.audit_writer()
            .await
            .record(draft)
            .await
            .context("required audit commit failed")
    }
}

/// Audit records that commit in the same transaction as a Gateway state change. A
/// record the audit log refuses rolls the change back.
pub(crate) struct AuditAppend(AuditTransactionWrite);

impl AuditAppend {
    pub(crate) fn new(write: AuditTransactionWrite) -> Self {
        Self(write)
    }
}

impl crate::store::TransactionAppend for AuditAppend {
    async fn append(
        &self,
        connection: &mut sqlx::PgConnection,
    ) -> Result<(), crate::store::GatewayStoreError> {
        self.0
            .append(connection)
            .await
            .map_err(|error| crate::store::GatewayStoreError::Append(Box::new(error)))
    }
}

/// The audit detail of an MCP request the Gateway admits or refuses: the MCP vocabulary
/// for tool calls and reads, core's Task classes for Task changes, and the Gateway's
/// own class for Task status reads. Other actions have their own producers.
pub fn mcp_action_detail(action: crate::contract::GatewayAction) -> Result<AuditDetail> {
    use crate::contract::GatewayAction;
    use veoveo_audit_contract::platform::{TaskCancel, TaskUpdate};
    use veoveo_sdk_rust::audit::{McpReadMethod, Read, ToolAdmission};
    let audit = gateway_audit();
    let read = |method| audit.detail(&Read { method });
    match action {
        GatewayAction::ToolsCall => audit.detail(&ToolAdmission {}),
        GatewayAction::ResourcesRead => read(McpReadMethod::ResourceRead),
        GatewayAction::PromptsGet => read(McpReadMethod::PromptGet),
        GatewayAction::CompletionComplete => read(McpReadMethod::Completion),
        GatewayAction::SubscriptionsListen => read(McpReadMethod::Subscription),
        GatewayAction::TasksGet => audit.detail(&TaskStatusRead {}),
        GatewayAction::TasksUpdate => audit.detail(&TaskUpdate {}),
        GatewayAction::TasksCancel => audit.detail(&TaskCancel {}),
        _ => anyhow::bail!("action `{action:?}` requires its own audit producer"),
    }
}

/// The audit reason for a policy decision. An unknown profile, tool or prompt is a
/// target the caller named that does not exist.
pub fn policy_reason(reason: PolicyReasonCode) -> AuditReason {
    match reason {
        PolicyReasonCode::PolicyAllow => AuditReason::Accepted,
        PolicyReasonCode::PolicyDeny
        | PolicyReasonCode::MissingPermission
        | PolicyReasonCode::UndeclaredTarget => AuditReason::PolicyDenied,
        PolicyReasonCode::UnknownProfile
        | PolicyReasonCode::UnknownTool
        | PolicyReasonCode::UnknownPrompt => AuditReason::NotFound,
        PolicyReasonCode::UnknownServer => AuditReason::UnknownServer,
        PolicyReasonCode::UnknownResource => AuditReason::UnknownResource,
        PolicyReasonCode::UnknownTask => AuditReason::UnknownTask,
        PolicyReasonCode::UnknownPrincipal => AuditReason::UnknownPrincipal,
        PolicyReasonCode::UnknownScope => AuditReason::UnknownScope,
        PolicyReasonCode::UnknownDataLabel => AuditReason::UnknownDataLabel,
        PolicyReasonCode::UnknownTenant => AuditReason::UnknownTenant,
        PolicyReasonCode::UnknownTokenIssuer => AuditReason::UnknownTokenIssuer,
        PolicyReasonCode::MissingPrincipal => AuditReason::MissingPrincipal,
        PolicyReasonCode::MissingTenant => AuditReason::MissingTenant,
        PolicyReasonCode::MissingGroup => AuditReason::MissingGroup,
        PolicyReasonCode::MissingRole => AuditReason::MissingRole,
        PolicyReasonCode::MissingScope => AuditReason::MissingScope,
        PolicyReasonCode::MissingDataLabel => AuditReason::MissingDataLabel,
        PolicyReasonCode::MissingPrincipalAssurance => AuditReason::MissingPrincipalAssurance,
        PolicyReasonCode::TokenAudienceMismatch => AuditReason::TokenAudienceMismatch,
        PolicyReasonCode::TokenExpired => AuditReason::ExpiredCredential,
        PolicyReasonCode::TokenNotYetValid => AuditReason::TokenNotYetValid,
        PolicyReasonCode::ReplayDetected => AuditReason::Replay,
    }
}

impl GatewayState {
    pub async fn record_policy_admission(
        &self,
        subject: &AuthenticatedSubject,
        target: &PolicyTarget,
        detail: AuditDetail,
        decision: &PolicyDecision,
    ) -> Result<()> {
        let allowed = decision.effect == PolicyEffect::Allow;
        let draft = subject.audit_draft(
            mcp_audit_target(target)?,
            detail,
            if allowed {
                AuditOutcome::Allowed
            } else {
                AuditOutcome::Denied
            },
            policy_reason(decision.reason),
        )?;
        self.record_audit(draft).await
    }
}
