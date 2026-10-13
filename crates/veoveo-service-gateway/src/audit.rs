//! Gateway-owned conversion from admitted identity and policy into audit records.
mod reasons;
mod targets;
mod vocabulary;

pub use reasons::*;
pub use targets::*;
pub use vocabulary::*;

use crate::{
    AuthenticatedSubject, GatewayState,
    request_observation::{RequestStage, StageTimer},
};
use anyhow::{Context, Result};
use veoveo_audit_contract::*;
use veoveo_sdk_rust::PrincipalKind;
use veoveo_sdk_rust::audit::AuditInvocation;

use crate::control_plane::{PolicyDecision, PolicyEffect, PolicyTarget};

impl AuthenticatedSubject {
    /// The registered server that made this call for the caller, if another server did.
    pub fn acting_server(&self) -> Option<&veoveo_types::ServerSlug> {
        self.delegation
            .as_ref()
            .map(|delegation| &delegation.acting_server.server)
    }

    /// The acting server's Task the call came from, when that server called from one.
    pub fn acting_task(&self) -> Option<&veoveo_sdk_rust::OpaqueTaskId> {
        self.delegation
            .as_ref()
            .and_then(|delegation| delegation.task.as_ref())
    }

    /// The actor a record names: the principal the request acts as, in its tenant and
    /// Work Context.
    pub fn audit_actor(&self) -> AuditActor {
        AuditActor {
            principal: self.actor.id.clone(),
            kind: match self.actor.kind {
                PrincipalKind::User => AuditPrincipalKind::User,
                PrincipalKind::Service => AuditPrincipalKind::Service,
            },
            tenant: self.actor.tenant.clone(),
            work_context: Some(self.authority.work_context.clone()),
        }
    }

    /// The server that acted for the caller, and its Task when the Task's identifier is a
    /// core Task identifier.
    pub fn audit_via(&self) -> Option<AuditVia> {
        self.acting_server().map(|server| AuditVia {
            server: server.clone(),
            task: self
                .acting_task()
                .and_then(|task| veoveo_types::TaskId::parse(task.as_str()).ok()),
        })
    }

    /// The invocation facts a record's detail carries beside the envelope.
    pub fn audit_invocation(&self) -> AuditInvocation {
        AuditInvocation {
            oauth_client: self.access_token.oauth_client_id.clone(),
            delegating_principal: (self.principal.id != self.actor.id)
                .then(|| self.principal.id.clone()),
            policy_revision: self.authority.policy_revision.clone(),
            scopes: self.actor.scopes.clone(),
            data_labels: self.actor.data_labels.clone(),
        }
    }

    pub fn audit_context(&self) -> AuditContext {
        AuditContext {
            actor: self.audit_actor(),
            via: self.audit_via(),
            request: self.audit.clone(),
        }
    }

    /// A record of this caller's request, its actor and acting server, with `detail`.
    pub fn audit_draft(&self, detail: AuditDetail, outcome: AuditOutcome) -> AuditDraftBuilder {
        self.audit_context().draft(detail, outcome)
    }
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
pub fn mcp_action_detail(
    action: crate::contract::GatewayAction,
    subject: &AuthenticatedSubject,
) -> Result<AuditDetail> {
    use crate::contract::GatewayAction;
    use veoveo_audit_contract::platform::{TaskCancel, TaskUpdate};
    use veoveo_sdk_rust::audit::{McpReadMethod, Read, ToolAdmission};
    let audit = gateway_audit();
    let invocation = Some(subject.audit_invocation());
    let read = |method| {
        audit.detail(&Read {
            method,
            invocation: invocation.clone(),
        })
    };
    match action {
        GatewayAction::ToolsCall => audit.detail(&ToolAdmission {
            invocation: invocation.clone(),
        }),
        GatewayAction::ResourcesRead => read(McpReadMethod::ResourceRead),
        GatewayAction::PromptsGet => read(McpReadMethod::PromptGet),
        GatewayAction::CompletionComplete => read(McpReadMethod::Completion),
        GatewayAction::SubscriptionsListen => read(McpReadMethod::Subscription),
        GatewayAction::TasksGet => audit.detail(&TaskStatusRead {
            invocation: invocation.clone(),
        }),
        GatewayAction::TasksUpdate => audit.detail(&TaskUpdate {}),
        GatewayAction::TasksCancel => audit.detail(&TaskCancel {}),
        _ => anyhow::bail!("action `{action:?}` requires its own audit producer"),
    }
}

/// The outcome of a policy decision.
pub fn policy_outcome(decision: &PolicyDecision) -> AuditOutcome {
    if decision.effect == PolicyEffect::Allow {
        AuditOutcome::Success
    } else {
        AuditOutcome::Denied
    }
}

/// A record of a policy decision about `target`, carrying the decision's reason code.
pub fn policy_draft(
    subject: &AuthenticatedSubject,
    target: AuditTarget,
    detail: AuditDetail,
    decision: &PolicyDecision,
) -> AuditDraftBuilder {
    let draft = subject
        .audit_draft(detail, policy_outcome(decision))
        .target(target);
    match policy_reason(decision.reason) {
        Some(reason) => draft.reason(reason),
        None => draft,
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
        let draft = policy_draft(subject, policy_target(target)?, detail, decision).build()?;
        self.record_audit(draft).await
    }
}
