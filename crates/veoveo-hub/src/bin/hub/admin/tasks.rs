use std::time::Instant;
use veoveo_hub::audit::AdministrativeOperation;
use veoveo_hub::contract::HubAction;

use axum::{
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_hub::control_plane::{HubProfile, PolicyTarget};
use veoveo_hub::mcp::ServerTaskCancellation;
use veoveo_hub::{AuthenticatedSubject, HubMcp};
use veoveo_hub_sdk::ServerSlug;
use veoveo_types::TaskId;

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
        internal_error_response, record_admin_target_operation_audit,
    },
    runtime::AdminState,
};

pub(crate) async fn cancel_task(
    State(state): State<AdminState>,
    AxumPath((profile, server, task_id)): AxumPath<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(task_id) = task_id.parse::<TaskId>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if task_id.as_uuid().get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(server_slug) = ServerSlug::parse(server) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let target = PolicyTarget::PlatformTask {
        server: server_slug.clone(),
        task_id,
    };
    let catalog = state.catalog.current();
    let Some(profile) = catalog.profile(&profile_id).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // The deployment that holds the Task owns its state: the Hub forwards
    // `tasks/cancel` to it as the caller, after the checks and the policy decision of
    // the caller's own `tasks/cancel` on the Task's captured composition, which
    // resolves the Task's deployment even while several deployments share its slug.
    let hub = HubMcp::new(
        state.catalog.clone(),
        profile_id,
        state.hub_state.clone(),
        state.internal_token_issuer.clone(),
        state.upstream_http.clone(),
    );
    let refusal = match hub
        .cancel_server_task(&subject, &server_slug, &task_id.to_string())
        .await
    {
        Ok(ServerTaskCancellation::Cancelled) => None,
        Ok(ServerTaskCancellation::Unrouted) => Some((
            StatusCode::NOT_FOUND,
            Some(AdminOperationFailure::TaskRoute),
        )),
        Ok(ServerTaskCancellation::Unowned) => Some((
            StatusCode::NOT_FOUND,
            Some(AdminOperationFailure::TaskOwnership),
        )),
        Ok(ServerTaskCancellation::Denied) => Some((StatusCode::FORBIDDEN, None)),
        Err(error) => {
            return audited_task_failure(
                &state,
                &profile,
                &subject,
                target,
                started_at,
                AdminOperationFailure::CancelTask,
                error,
            )
            .await;
        }
    };
    if let Some((status, failure)) = refusal {
        if let Err(error) = record_task_result(
            &state,
            &profile,
            &subject,
            target,
            started_at,
            AdminOperationStatus::Rejected,
            failure,
        )
        .await
        {
            return internal_error_response(error);
        }
        return status.into_response();
    }
    if let Err(error) = record_task_result(
        &state,
        &profile,
        &subject,
        target,
        started_at,
        AdminOperationStatus::Succeeded,
        None,
    )
    .await
    {
        return internal_error_response(error);
    }
    StatusCode::NO_CONTENT.into_response()
}

#[allow(clippy::too_many_arguments)]
async fn record_task_result(
    state: &AdminState,
    profile: &HubProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            audit_target: None,
            action: HubAction::TasksCancel.into(),
            operation: AdministrativeOperation::TaskCancel,
            started_at,
            status,
            failure,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn audited_task_failure(
    state: &AdminState,
    profile: &HubProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    started_at: Instant,
    failure: AdminOperationFailure,

    error: impl std::fmt::Display,
) -> Response {
    tracing::error!(failure = ?failure, "hub task cancellation failed: {error}");
    if let Err(audit_error) = record_task_result(
        state,
        profile,
        subject,
        target,
        started_at,
        AdminOperationStatus::Failed,
        Some(failure),
    )
    .await
    {
        return internal_error_response(audit_error);
    }
    StatusCode::BAD_GATEWAY.into_response()
}
