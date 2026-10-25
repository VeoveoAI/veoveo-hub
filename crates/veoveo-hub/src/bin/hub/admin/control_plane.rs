use std::{sync::Arc, time::Instant};
use veoveo_http::RequestJson;
use veoveo_hub::audit::AdministrativeOperation;
use veoveo_hub::contract::HubAction;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
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

    let entity_tag = revision_id
        .as_ref()
        .and_then(|revision_id| HeaderValue::from_str(&entity_tag(revision_id)).ok());
    let mut response = Json(ControlPlaneReadResult {
        status: "ok",
        revision_id,
        sha256,
        servers: catalog.server_count(),
        profiles: catalog.profile_count(),
        control_plane: catalog.control_plane().clone(),
    })
    .into_response();
    if let Some(entity_tag) = entity_tag {
        response.headers_mut().insert(header::ETAG, entity_tag);
    }
    response
}

/// A revision as a strong entity tag, RFC 9110 §8.8.3.
fn entity_tag(revision_id: &HubControlPlaneRevisionId) -> String {
    format!("\"{revision_id}\"")
}

/// The revision `If-Match` names, RFC 9110 §13.1.1: one strong entity tag. `None` when
/// the header is absent, or names a weak tag, a list or `*`, none of which names the one
/// revision a publication succeeds.
pub(crate) fn expected_revision(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::IF_MATCH)?.to_str().ok()?.trim();
    let tag = value.strip_prefix('"')?.strip_suffix('"')?;
    (!tag.is_empty() && !tag.contains(['"', ','])).then(|| tag.to_owned())
}

pub(crate) async fn update_control_plane(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    RequestJson(control_plane): RequestJson<HubControlPlane>,
) -> Response {
    // The durable publication and this process's swap are one ordered operation.
    // No await separates successful commit from the local replacement.
    let _publication = state.publication.lock().await;
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
    let precondition_failure = |status, failure| AdminOperationAuditRecord {
        audit_target: None,
        action: HubAction::AdminWrite.into(),
        operation: AdministrativeOperation::ControlPlane,
        started_at,
        status,
        failure: Some(failure),
    };
    let Some(expected) = expected_revision(&headers) else {
        if let Err(audit_err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            precondition_failure(
                AdminOperationStatus::Rejected,
                AdminOperationFailure::RevisionPrecondition,
            ),
        )
        .await
        {
            return internal_error_response(audit_err);
        }
        return veoveo_hub::problem::Problem::new(
            StatusCode::PRECONDITION_REQUIRED,
            "control-plane-precondition-required",
            "Control plane precondition required",
        )
        .detail("name the revision this document is based on as If-Match: \"<revisionId>\"")
        .into_response();
    };
    let active = match state.control_store.load_active_revision().await {
        Ok(active) => active,
        Err(err) => {
            if let Err(audit_err) = record_admin_operation_audit(
                &state,
                &profile,
                &subject,
                precondition_failure(
                    AdminOperationStatus::Failed,
                    AdminOperationFailure::LatestRevisionRead,
                ),
            )
            .await
            {
                return internal_error_response(audit_err);
            }
            return internal_error_response(err);
        }
    };
    let Some(based_on) = active.filter(|active| active.revision_id.to_string() == expected) else {
        if let Err(audit_err) = record_admin_operation_audit(
            &state,
            &profile,
            &subject,
            precondition_failure(
                AdminOperationStatus::Rejected,
                AdminOperationFailure::RevisionPrecondition,
            ),
        )
        .await
        {
            return internal_error_response(audit_err);
        }
        return revision_changed(&state).await;
    };

    let new_catalog =
        match HubCatalog::from_control_plane(control_plane, state.catalog.current().admission())
            .and_then(|catalog| {
                // Reject changes to stable gateway/auth bootstrap before preparation.
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
    let new_catalog = match new_catalog
        .as_ref()
        .clone()
        .prepare_runtime(
            Some(&state.catalog.current()),
            &state.public_base_url,
            new_http.clone(),
        )
        .await
    {
        Ok(catalog) => Arc::new(catalog),
        Err(error) => {
            tracing::warn!(%error, "rejected unprepared runtime configuration");
            if let Err(error) = record_admin_operation_audit(
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
                return internal_error_response(error);
            }
            return (StatusCode::BAD_REQUEST, "invalid runtime configuration").into_response();
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
        .record_revision_on(Some(&based_on), &revision, &audit_context)
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
            return revision_changed(&state).await;
        }
        return internal_error_response(err);
    }

    let servers = new_catalog.server_count();
    let profiles = new_catalog.profile_count();
    // Success is recorded atomically by HubControlStore. A second post-commit
    // audit must not prevent this process from adopting the committed revision.
    replace_http_client(&state.http, new_http);
    if let Err(err) = replace_catalog(&state.catalog, new_catalog) {
        return internal_error_response(err);
    }
    crate::contributions::after_activation(&state.control_store, state.hub_state.hub_store());
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

/// 412: the revision a publication is based on is no longer active. The detail names
/// the active one, which the client reads, changes and publishes again.
async fn revision_changed(state: &AdminState) -> Response {
    let active = match state.control_store.load_active_revision_head().await {
        Ok(Some(head)) => head.revision_id.to_string(),
        Ok(None) => "none".to_owned(),
        Err(err) => return internal_error_response(err),
    };
    veoveo_hub::problem::Problem::new(
        StatusCode::PRECONDITION_FAILED,
        "control-plane-revision-changed",
        "Control plane changed",
    )
    .detail(format!(
        "the active revision is {active}; read the control plane, apply the change to it and publish with its revision in If-Match"
    ))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_if_match(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn if_match_names_one_revision_as_a_strong_entity_tag() {
        let id = "gcp-0199e3a0-0000-7000-8000-000000000000";
        assert_eq!(
            expected_revision(&with_if_match(&format!("\"{id}\""))).as_deref(),
            Some(id)
        );
        assert_eq!(
            expected_revision(&with_if_match(&format!(" \"{id}\" "))).as_deref(),
            Some(id)
        );
        for refused in ["*", id, "W/\"gcp-1\"", "\"gcp-1\", \"gcp-2\"", "\"\""] {
            assert_eq!(
                expected_revision(&with_if_match(refused)),
                None,
                "{refused}"
            );
        }
        assert_eq!(expected_revision(&HeaderMap::new()), None);
    }
}
