use std::time::Instant;
use veoveo_http::RequestJson;
use veoveo_hub::audit::AdministrativeOperation;
use veoveo_hub::contract::HubAction;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use veoveo_hub::AuthenticatedSubject;
use veoveo_hub::control_plane::{
    HubJwtRevocation, HubJwtRevocationAdminStatus, HubJwtRevocationApplyResult,
    HubJwtRevocationPruneResult, HubJwtRevocationRequest,
};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
        authorize_admin_request, internal_error_response, record_admin_operation_audit,
    },
    runtime::AdminState,
};

pub(crate) async fn revoke_jwt(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(request): RequestJson<HubJwtRevocationRequest>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let revocation_target = match revocation_audit_target(&request) {
        Ok(target) => target,
        Err(error) => return internal_error_response(error),
    };
    let (catalog, profile, subject) = match crate::audit::authorize_admin_target_request(
        &state,
        &profile_id,
        subject,
        crate::audit::AdminAuthorizationRequest {
            action: HubAction::AdminWrite.into(),
            target: veoveo_hub::control_plane::PolicyTarget::Hub,
            audit_target: Some(revocation_target.clone()),
            operation: AdministrativeOperation::JwtRevoke,
            started_at,
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    if catalog.profile(&request.profile).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let revoked_at = Utc::now();
    if request.expires_at <= revoked_at {
        if let Err(err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            AdminOperationAuditRecord {
                audit_target: Some(revocation_target.clone()),
                action: HubAction::AdminWrite.into(),
                operation: AdministrativeOperation::JwtRevoke,
                started_at,
                status: AdminOperationStatus::Rejected,
                failure: Some(AdminOperationFailure::ExpiredRevocation),
            },
        )
        .await
        {
            return internal_error_response(err);
        }
        return (
            StatusCode::BAD_REQUEST,
            "revocation expiration must be in the future",
        )
            .into_response();
    }
    let revocation = HubJwtRevocation {
        profile: request.profile,
        issuer: request.issuer,
        jwt_id: request.jwt_id,
        revoked_at,
        expires_at: request.expires_at,
        reason: request.reason,
    };
    if let Err(err) = state.hub_state.record_jwt_revocation(&revocation).await {
        tracing::error!("failed to persist hub JWT revocation: {err}");
        if let Err(audit_err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            AdminOperationAuditRecord {
                audit_target: Some(revocation_target.clone()),
                action: HubAction::AdminWrite.into(),
                operation: AdministrativeOperation::JwtRevoke,
                started_at,
                status: AdminOperationStatus::Failed,
                failure: Some(AdminOperationFailure::PersistJwtRevocation),
            },
        )
        .await
        {
            return internal_error_response(audit_err);
        }
        return internal_error_response(err);
    }
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            audit_target: Some(revocation_target.clone()),
            action: HubAction::AdminWrite.into(),
            operation: AdministrativeOperation::JwtRevoke,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
        },
    )
    .await
    {
        return internal_error_response(err);
    }
    tracing::info!(
        admin_profile = %profile.id,
        target_profile = %revocation.profile,
        principal = %subject.principal.id,
        issuer = %revocation.issuer,
        jwt_id = %revocation.jwt_id,
        "hub JWT revoked"
    );
    Json(HubJwtRevocationApplyResult {
        status: HubJwtRevocationAdminStatus::Revoked,
        revocation,
    })
    .into_response()
}

pub(crate) async fn prune_jwt_revocations(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (_catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        HubAction::AdminWrite,
        AdministrativeOperation::JwtPrune,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let deleted = match state
        .hub_state
        .prune_expired_jwt_revocations(Utc::now(), veoveo_store::MaintenanceBatch::DEFAULT)
        .await
    {
        Ok(pruned) => pruned.rows,
        Err(err) => {
            tracing::error!("failed to prune expired hub JWT revocations: {err}");
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    audit_target: None,
                    action: HubAction::AdminWrite.into(),
                    operation: AdministrativeOperation::JwtPrune,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::PruneJwtRevocations),
                },
            )
            .await
            {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            audit_target: None,
            action: HubAction::AdminWrite.into(),
            operation: AdministrativeOperation::JwtPrune,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
        },
    )
    .await
    {
        return internal_error_response(err);
    }
    tracing::info!(
        profile = %profile.id,
        principal = %subject.principal.id,
        deleted,
        "expired hub JWT revocations pruned"
    );
    Json(HubJwtRevocationPruneResult {
        status: HubJwtRevocationAdminStatus::Pruned,
        deleted,
    })
    .into_response()
}

fn revocation_audit_target(
    request: &HubJwtRevocationRequest,
) -> anyhow::Result<veoveo_audit_contract::AuditTarget> {
    use veoveo_types::{ResourceUriBuilder, UriSegment};
    veoveo_hub::audit::resource_target(
        &ResourceUriBuilder::new("veoveo://jwt-revocations")?
            .segment(UriSegment::new(request.profile.to_string())?)
            .query_pair("issuer", request.issuer.as_str())?
            .query_pair("jwt_id", request.jwt_id.as_str())?
            .build()?,
    )
}
