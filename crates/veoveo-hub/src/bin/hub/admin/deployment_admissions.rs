//! Operator-only initial deployment authority; never reachable from environment edits.
use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use veoveo_http::RequestJson;
use veoveo_hub::{
    AuthenticatedSubject, audit::AdministrativeOperation, contract::HubAction,
    registration::DeploymentAdmissionRequest,
};

use crate::{
    audit::{
        AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
        authorize_admin_request, internal_error_response, record_admin_operation_audit,
    },
    runtime::AdminState,
};

pub(crate) async fn admit_deployment(
    State(state): State<AdminState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(request): RequestJson<DeploymentAdmissionRequest>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = super::admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (_catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        HubAction::AdminWrite,
        AdministrativeOperation::DeploymentAdmission,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let issued = state.registrations.admit_deployment(request).await;
    if let Err(error) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            audit_target: None,
            action: HubAction::AdminWrite.into(),
            operation: AdministrativeOperation::DeploymentAdmission,
            started_at,
            status: if issued.is_ok() {
                AdminOperationStatus::Succeeded
            } else {
                AdminOperationStatus::Failed
            },
            failure: issued
                .as_ref()
                .err()
                .map(|_| AdminOperationFailure::PersistDeploymentAdmission),
        },
    )
    .await
    {
        return internal_error_response(error);
    }
    match issued {
        Ok(grant) => ([(header::CACHE_CONTROL, "no-store")], Json(grant)).into_response(),
        Err(error) => internal_error_response(error),
    }
}
