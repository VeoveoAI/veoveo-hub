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
    HubControlPlaneRevisionSource, HubProfile,
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
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlPlaneValidationResult {
    status: &'static str,
    sha256: String,
    servers: usize,
    profiles: usize,
    control_plane: HubControlPlane,
}

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

    let Prepared {
        catalog: new_catalog,
        http: new_http,
        sha256,
    } = match prepare_publication(&state, control_plane).await {
        Ok(prepared) => prepared,
        Err(refusal) => {
            return refusal
                .answer(
                    &state,
                    &profile,
                    &subject,
                    AdministrativeOperation::ControlPlane,
                    started_at,
                )
                .await;
        }
    };
    let control_plane = new_catalog.control_plane().clone();
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

/// What publishing a document would make active, without publishing it: Hub validates
/// and prepares the document as a publication does and answers it as Hub would keep and
/// serve it, with its SHA-256, so a client learns whether publishing changes anything.
pub(crate) async fn validate_control_plane(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(control_plane): RequestJson<HubControlPlane>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let operation = AdministrativeOperation::ControlPlaneValidation;
    let (_catalog, profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        HubAction::AdminWrite,
        operation,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let prepared = match prepare_publication(&state, control_plane).await {
        Ok(prepared) => prepared,
        Err(refusal) => {
            return refusal
                .answer(&state, &profile, &subject, operation, started_at)
                .await;
        }
    };
    if let Err(err) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        AdminOperationAuditRecord {
            audit_target: None,
            action: HubAction::AdminWrite.into(),
            operation,
            started_at,
            status: AdminOperationStatus::Succeeded,
            failure: None,
        },
    )
    .await
    {
        return internal_error_response(err);
    }
    Json(ControlPlaneValidationResult {
        status: "valid",
        sha256: prepared.sha256,
        servers: prepared.catalog.server_count(),
        profiles: prepared.catalog.profile_count(),
        control_plane: prepared.catalog.control_plane().clone(),
    })
    .into_response()
}

/// A document prepared for publication: its catalog, that catalog's HTTP client and the
/// SHA-256 of the control plane as Hub keeps it.
struct Prepared {
    catalog: Arc<HubCatalog>,
    http: reqwest::Client,
    sha256: String,
}

/// Why a document cannot be published, as audited and as answered.
struct Refusal {
    status: AdminOperationStatus,
    failure: AdminOperationFailure,
    response: Response,
}

impl Refusal {
    /// 400 `control-plane-invalid`: Hub refuses the document, for `reason`.
    fn invalid(reason: &anyhow::Error) -> Box<Self> {
        Box::new(Self {
            status: AdminOperationStatus::Rejected,
            failure: AdminOperationFailure::InvalidControlPlane,
            response: veoveo_hub::problem::Problem::new(
                StatusCode::BAD_REQUEST,
                "control-plane-invalid",
                "Invalid control plane",
            )
            .detail(format!("{reason:#}"))
            .into_response(),
        })
    }

    fn failed(failure: AdminOperationFailure, response: Response) -> Box<Self> {
        Box::new(Self {
            status: AdminOperationStatus::Failed,
            failure,
            response,
        })
    }

    /// Audits the refusal as `operation` and answers it.
    async fn answer(
        self: Box<Self>,
        state: &AdminState,
        profile: &HubProfile,
        subject: &AuthenticatedSubject,
        operation: AdministrativeOperation,
        started_at: Instant,
    ) -> Response {
        if let Err(audit_err) = record_admin_operation_audit(
            state,
            profile,
            subject,
            AdminOperationAuditRecord {
                audit_target: None,
                action: HubAction::AdminWrite.into(),
                operation,
                started_at,
                status: self.status,
                failure: Some(self.failure),
            },
        )
        .await
        {
            return internal_error_response(audit_err);
        }
        self.response
    }
}

/// Validates and prepares `control_plane` as the successor of the control plane active
/// now, as a publication does, and changes nothing. Validation and publication share it,
/// so a document validates exactly when its publication would be admitted.
async fn prepare_publication(
    state: &AdminState,
    control_plane: HubControlPlane,
) -> Result<Prepared, Box<Refusal>> {
    let catalog =
        HubCatalog::from_control_plane(control_plane, state.catalog.current().admission())
            .and_then(|catalog| {
                // Reject changes to stable gateway/auth bootstrap before preparation.
                // replace() repeats this check under its write lock.
                state.catalog.validate_replacement(&catalog)?;
                Ok(catalog)
            })
            .map_err(|err| {
                tracing::warn!("rejected invalid hub control plane: {err}");
                Refusal::invalid(&err)
            })?;
    let http = build_http_client(&catalog).map_err(|err| {
        tracing::error!("failed to rebuild hub HTTP client: {err}");
        Refusal::failed(
            AdminOperationFailure::BuildHttpClient,
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to rebuild hub HTTP client",
            )
                .into_response(),
        )
    })?;
    let catalog = catalog
        .prepare_runtime(
            Some(&state.catalog.current()),
            &state.public_base_url,
            http.clone(),
        )
        .await
        .map(Arc::new)
        .map_err(|err| {
            tracing::warn!(error = %err, "rejected unprepared runtime configuration");
            Refusal::invalid(&err)
        })?;
    let sha256 = control_plane_sha256(catalog.control_plane()).map_err(|err| {
        Refusal::failed(
            AdminOperationFailure::ControlPlaneSha,
            internal_error_response(err),
        )
    })?;
    Ok(Prepared {
        catalog,
        http,
        sha256,
    })
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
