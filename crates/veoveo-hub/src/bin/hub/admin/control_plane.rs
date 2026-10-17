use std::{sync::Arc, time::Instant};
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
use serde::Serialize;
use veoveo_hub::control_plane::{
    HubControlPlane, HubControlPlaneRevision, HubControlPlaneRevisionId,
    HubControlPlaneRevisionSource,
};
use veoveo_hub::{AuthenticatedSubject, HubCatalog, new_hub_control_plane_revision_id};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
        authorize_admin_request, control_plane_sha256, internal_error_response,
        record_admin_operation_audit,
    },
    runtime::{AdminState, build_http_client, replace_catalog, replace_http_client},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlPlaneReadResult {
    status: &'static str,
    revision_id: Option<HubControlPlaneRevisionId>,
    sha256: String,
    servers: usize,
    profiles: usize,
    control_plane: HubControlPlane,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlPlaneApplyResult {
    status: &'static str,
    revision_id: HubControlPlaneRevisionId,
    sha256: String,
    servers: usize,
    profiles: usize,
}

pub(crate) async fn read_control_plane(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        HubAction::AdminRead,
        AdministrativeOperation::ControlPlane,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };

    let sha256 = match control_plane_sha256(catalog.control_plane()) {
        Ok(sha256) => sha256,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    audit_target: None,
                    action: HubAction::AdminRead.into(),
                    operation: AdministrativeOperation::ControlPlane,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::ControlPlaneSha),
                },
            )
            .await
            {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let revision_id = match state.control_store.load_active_revision().await {
        Ok(Some(revision)) if revision.sha256 == sha256 => Some(revision.revision_id),
        Ok(_) => None,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    audit_target: None,
                    action: HubAction::AdminRead.into(),
                    operation: AdministrativeOperation::ControlPlane,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::LatestRevisionRead),
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
            action: HubAction::AdminRead.into(),
            operation: AdministrativeOperation::ControlPlane,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
        },
    )
    .await
    {
        return internal_error_response(err);
    }

    Json(ControlPlaneReadResult {
        status: "ok",
        revision_id,
        sha256,
        servers: catalog.server_count(),
        profiles: catalog.profile_count(),
        control_plane: catalog.control_plane().clone(),
    })
    .into_response()
}

pub(crate) async fn update_control_plane(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(control_plane): RequestJson<HubControlPlane>,
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
        AdministrativeOperation::ControlPlane,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };

    let new_catalog =
        match HubCatalog::from_control_plane(control_plane, state.catalog.current().admission())
            .and_then(|catalog| {
                // Reject startup-only placement changes before durable publication.
                // replace() repeats this check under its write lock.
                state.catalog.validate_replacement(&catalog)?;
                Ok(catalog)
            }) {
            Ok(catalog) => Arc::new(catalog),
            Err(err) => {
                tracing::warn!("rejected invalid hub control plane update: {err}");
                if let Err(audit_err) = record_admin_operation_audit(
                    &state,
                    &profile,
                    &subject,
                    AdminOperationAuditRecord {
                        audit_target: None,
                        action: HubAction::AdminWrite.into(),
                        operation: AdministrativeOperation::ControlPlane,
                        started_at,
                        status: AdminOperationStatus::Rejected,
                        failure: Some(AdminOperationFailure::InvalidControlPlane),
                    },
                )
                .await
                {
                    return internal_error_response(audit_err);
                }
                return (StatusCode::BAD_REQUEST, "invalid hub control plane").into_response();
            }
        };
    let new_http = match build_http_client(&new_catalog) {
        Ok(client) => client,
        Err(err) => {
            tracing::error!("failed to rebuild hub HTTP client: {err}");
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    audit_target: None,
                    action: HubAction::AdminWrite.into(),
                    operation: AdministrativeOperation::ControlPlane,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::BuildHttpClient),
                },
            )
            .await
            {
                return internal_error_response(audit_err);
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to rebuild hub HTTP client",
            )
                .into_response();
        }
    };
    let control_plane = new_catalog.control_plane().clone();
    let sha256 = match control_plane_sha256(&control_plane) {
        Ok(sha256) => sha256,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    audit_target: None,
                    action: HubAction::AdminWrite.into(),
                    operation: AdministrativeOperation::ControlPlane,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::ControlPlaneSha),
                },
            )
            .await
            {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let revision_id = match new_hub_control_plane_revision_id() {
        Ok(revision_id) => revision_id,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                AdminOperationAuditRecord {
                    audit_target: None,
                    action: HubAction::AdminWrite.into(),
                    operation: AdministrativeOperation::ControlPlane,
                    started_at,
                    status: AdminOperationStatus::Failed,
                    failure: Some(AdminOperationFailure::RevisionId),
                },
            )
            .await
            {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let revision = HubControlPlaneRevision {
        revision_id: revision_id.clone(),
        sha256: sha256.clone(),
        source: HubControlPlaneRevisionSource::AdminApi,
        applied_at: Utc::now(),
        applied_by: subject.actor.id.clone(),
        tenant: subject.actor.tenant.clone(),
        control_plane,
    };
    let audit_context = subject.audit_context();
    if let Err(err) = state
        .control_store
        .record_revision(&revision, &audit_context)
        .await
    {
        tracing::error!("failed to persist hub control-plane revision: {err}");
        if let Err(audit_err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            AdminOperationAuditRecord {
                audit_target: None,
                action: HubAction::AdminWrite.into(),
                operation: AdministrativeOperation::ControlPlane,
                started_at,
                status: AdminOperationStatus::Failed,
                failure: Some(AdminOperationFailure::PersistControlPlaneRevision),
            },
        )
        .await
        {
            return internal_error_response(audit_err);
        }
        if veoveo_hub::is_revision_conflict(&err) {
            return veoveo_hub::problem::Problem::new(
                StatusCode::CONFLICT,
                "control-plane-revision-changed",
                "Control plane changed",
            )
            .detail("another revision became active; read the control plane and retry")
            .into_response();
        }
        return internal_error_response(err);
    }

    let servers = new_catalog.server_count();
    let profiles = new_catalog.profile_count();
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            audit_target: None,
            action: HubAction::AdminWrite.into(),
            operation: AdministrativeOperation::ControlPlane,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
        },
    )
    .await
    {
        return internal_error_response(err);
    }
    replace_http_client(&state.http, new_http);
    if let Err(err) = replace_catalog(&state.catalog, new_catalog) {
        return internal_error_response(err);
    }
    tracing::info!(
        profile = %profile.id,
        principal = %subject.principal.id,
        revision_id = %revision_id,
        sha256 = %sha256,
        servers,
        profiles,
        "hub control plane updated"
    );
    Json(ControlPlaneApplyResult {
        status: "applied",
        revision_id,
        sha256,
        servers,
        profiles,
    })
    .into_response()
}
