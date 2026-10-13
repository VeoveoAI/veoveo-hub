use super::auth_support::internal_error_response;
use crate::audit::{
    AdminAdmission, AdminCompletion, AdminOperationFailure, AdministrativeAccess,
    AdministrativeOperation,
};
use crate::contract::{ActionAccess, GatewayAction, PolicyAction};
use crate::control_plane::{GatewayProfile, PolicyDecision, PolicyEffect, PolicyTarget, TraceId};
use crate::{AuthenticatedSubject, GatewayCatalog, GatewayState, PolicyRequest};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{sync::Arc, time::Instant};
use veoveo_audit_contract::{AuditOutcome, AuditTarget};
use veoveo_sdk_rust::GatewayProfileId;
pub struct AdminAuthorizationRequest {
    pub audit_target: Option<AuditTarget>,
    pub action: PolicyAction,
    pub target: PolicyTarget,
    pub operation: AdministrativeOperation,
    pub started_at: Instant,
}

pub async fn authorize_gateway_action(
    gateway: &GatewayState,
    catalog: Arc<GatewayCatalog>,
    profile_id: &GatewayProfileId,
    subject: AuthenticatedSubject,
    request: AdminAuthorizationRequest,
) -> std::result::Result<(Arc<GatewayCatalog>, GatewayProfile, AuthenticatedSubject), Box<Response>>
{
    let Some(profile) = catalog.profile(profile_id).cloned() else {
        return Err(Box::new(StatusCode::NOT_FOUND.into_response()));
    };
    let trace_id = match TraceId::parse(&subject.audit.trace_id) {
        Ok(trace_id) => trace_id,
        Err(err) => return Err(Box::new(internal_error_response(err))),
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: profile_id,
        action: request.action.clone(),
        target: &request.target,
        trace_id: &trace_id,
    });
    if let Err(err) = record_admin_audit(
        gateway,
        &profile,
        &subject,
        AdminAuditRecord {
            access: administrative_access(catalog.registry(), &request.action)
                .map_err(|error| Box::new(internal_error_response(error)))?,
            target: request.audit_target.unwrap_or(
                crate::audit::owner_policy_target(catalog.registry(), &request.target)
                    .map_err(|error| Box::new(internal_error_response(error)))?,
            ),
            decision: decision.clone(),
            operation: request.operation,
        },
    )
    .await
    {
        return Err(Box::new(internal_error_response(err)));
    }
    if decision.effect != PolicyEffect::Allow {
        tracing::warn!(
            profile = %profile_id,
            principal = %subject.principal.id,
            action = ?request.action,
            reason = ?decision.reason,
            "gateway admin request denied"
        );
        return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
    }

    Ok((catalog, profile, subject))
}

struct AdminAuditRecord {
    access: AdministrativeAccess,
    target: AuditTarget,
    decision: PolicyDecision,
    operation: AdministrativeOperation,
}

#[derive(Debug, Clone, Copy)]
pub enum AdminOperationStatus {
    Succeeded,
    Rejected,
    Failed,
}

pub struct AdminOperationAuditRecord {
    pub audit_target: Option<AuditTarget>,
    pub action: PolicyAction,
    pub operation: AdministrativeOperation,
    pub started_at: Instant,
    pub status: AdminOperationStatus,
    pub failure: Option<AdminOperationFailure>,
}

pub async fn record_gateway_operation_audit(
    gateway: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    catalog: &GatewayCatalog,
    target: PolicyTarget,
    record: AdminOperationAuditRecord,
) -> anyhow::Result<()> {
    use crate::audit::{GatewayReason, code_reason};
    let (outcome, reason) = match (record.status, record.failure) {
        (AdminOperationStatus::Succeeded, _) => (AuditOutcome::Success, None),
        (AdminOperationStatus::Rejected, Some(failure))
        | (AdminOperationStatus::Failed, Some(failure)) => {
            (AuditOutcome::Failure, Some(code_reason(failure)))
        }
        (AdminOperationStatus::Rejected, None) => (
            AuditOutcome::Failure,
            Some(GatewayReason::InvalidRequest.into()),
        ),
        (AdminOperationStatus::Failed, None) => (
            AuditOutcome::Failure,
            Some(GatewayReason::InternalFailure.into()),
        ),
    };
    let target = match record.audit_target {
        Some(target) => target,
        None => crate::audit::owner_policy_target(catalog.registry(), &target)?,
    };
    let detail = crate::audit::gateway_audit().detail(&AdminCompletion {
        profile: profile.id.clone(),
        operation: record.operation,
        access: administrative_access(catalog.registry(), &record.action)?,
        duration_ms: u64::try_from(record.started_at.elapsed().as_millis())?,
        invocation: subject.audit_invocation(),
    })?;
    let draft = subject.audit_draft(detail, outcome).target(target);
    let draft = match reason {
        Some(reason) => draft.reason(reason),
        None => draft,
    }
    .build()?;
    gateway.audit_writer().await.record_completion(draft).await;
    Ok(())
}

fn administrative_access(
    registry: &crate::contract::CatalogRegistry,
    action: &PolicyAction,
) -> anyhow::Result<AdministrativeAccess> {
    Ok(match action {
        PolicyAction::Kernel(GatewayAction::AdminRead) => AdministrativeAccess::Read,
        PolicyAction::Kernel(_) => AdministrativeAccess::Write,
        PolicyAction::Registered(handle) => {
            registry.check_action(handle)?;
            match registry
                .descriptor(handle.name())
                .ok_or_else(|| anyhow::anyhow!("owner action audit descriptor is unbound"))?
                .access
            {
                ActionAccess::Read => AdministrativeAccess::Read,
                ActionAccess::Write => AdministrativeAccess::Write,
            }
        }
    })
}

async fn record_admin_audit(
    gateway: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    record: AdminAuditRecord,
) -> anyhow::Result<()> {
    let detail = crate::audit::gateway_audit().detail(&AdminAdmission {
        profile: profile.id.clone(),
        operation: record.operation,
        access: record.access,
        invocation: subject.audit_invocation(),
    })?;
    let draft =
        crate::audit::policy_draft(subject, record.target, detail, &record.decision).build()?;
    gateway.record_audit(draft).await
}
