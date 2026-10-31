//! An administrator ends every session of a person and stops their Computer, as when
//! their access changes in the identity provider: their next sign-in brings what it
//! names then.
use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Serialize;
use veoveo_hub::{
    AuthenticatedSubject,
    audit::AdministrativeOperation,
    computers::{Stop, stop_owned},
    contract::HubAction,
    control_plane::PolicyTarget,
    person::Email,
    problem::Problem,
};
use veoveo_types::{PrincipalId, ResourceUriBuilder, TenantId, UriSegment};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
        AdminOperationStatus, authorize_admin_target_request, internal_error_response,
        record_admin_operation_audit,
    },
    runtime::AdminState,
};

/// What ending a person's sessions did.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ended {
    person: String,
    sessions_ended: u64,
    computer: Stop,
}

/// `DELETE /admin/{profile}/people/{email}/sessions`: end every session of the person of
/// the caller's tenant who signed in with `email`, clear their Computer's Lore client and
/// stop their Computer, keeping its home.
pub(crate) async fn end_person_sessions(
    State(state): State<AdminState>,
    AxumPath((profile, email)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (catalog, profile, subject) = match authorize_admin_target_request(
        &state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            action: HubAction::AdminWrite.into(),
            target: PolicyTarget::Hub,
            audit_target: None,
            operation: AdministrativeOperation::SessionsEnd,
            started_at,
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let Ok(email) = Email::parse(&email) else {
        return Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid-request",
            "A person is named by an email address",
        )
        .into_response();
    };
    let store = state.hub_state.hub_store();
    let person = match subject.principal.tenant.as_ref() {
        Some(tenant) => match store.person_by_email(tenant, &email).await {
            Ok(person) => person,
            Err(error) => return internal_error_response(error),
        },
        None => None,
    };
    let Some(person) = person else {
        return Problem::new(
            StatusCode::NOT_FOUND,
            "person-not-found",
            "No one has signed in with this email",
        )
        .detail(
            "People are named by the email they sign in to the Hub with, and no one in your \
             tenant has signed in with this one, so they hold no session.",
        )
        .into_response();
    };
    let target = match person_target(&person.tenant, &person.principal) {
        Ok(target) => target,
        Err(error) => return internal_error_response(error),
    };
    let audit = |status, failure| AdminOperationAuditRecord {
        audit_target: Some(target.clone()),
        action: HubAction::AdminWrite.into(),
        operation: AdministrativeOperation::SessionsEnd,
        started_at,
        status,
        failure,
    };
    let sessions_ended = match store
        .end_person_sessions(&person.tenant, &person.principal, Utc::now())
        .await
    {
        Ok(ended) => ended,
        Err(error) => {
            let record = audit(
                AdminOperationStatus::Failed,
                Some(AdminOperationFailure::SessionsEnd),
            );
            if let Err(audit_error) =
                record_admin_operation_audit(&state, &profile, &subject, record).await
            {
                return internal_error_response(audit_error);
            }
            return internal_error_response(error);
        }
    };
    let computer = match stop_owned(
        &state.hub_state,
        &catalog,
        &person.tenant,
        &person.principal,
    )
    .await
    {
        Ok(stop) => stop,
        Err(error) => {
            tracing::warn!(principal = %person.principal, "a person's Computer was not stopped: {error:#}");
            let record = audit(
                AdminOperationStatus::Failed,
                Some(AdminOperationFailure::StopComputer),
            );
            if let Err(audit_error) =
                record_admin_operation_audit(&state, &profile, &subject, record).await
            {
                return internal_error_response(audit_error);
            }
            return Problem::new(
                StatusCode::BAD_GATEWAY,
                "computer-not-stopped",
                "Their sessions ended, but their Computer was not stopped",
            )
            .detail(format!(
                "{sessions_ended} sessions ended and their Computer can mint no Lore token, \
                 but its sandbox could not be removed. Try again: ending their sessions again \
                 is harmless."
            ))
            .into_response();
        }
    };
    if let Err(error) = record_admin_operation_audit(
        &state,
        &profile,
        &subject,
        audit(AdminOperationStatus::Succeeded, None),
    )
    .await
    {
        return internal_error_response(error);
    }
    tracing::info!(
        admin = %subject.principal.id,
        principal = %person.principal,
        sessions_ended,
        computer = ?computer,
        "a person's sessions ended"
    );
    Json(Ended {
        person: email.as_str().to_owned(),
        sessions_ended,
        computer,
    })
    .into_response()
}

/// `veoveo://people/{tenant}/{principal}`: a person of a tenant.
fn person_target(
    tenant: &TenantId,
    principal: &PrincipalId,
) -> anyhow::Result<veoveo_audit_contract::AuditTarget> {
    veoveo_hub::audit::resource_target(
        &ResourceUriBuilder::new("veoveo://people")?
            .segment(UriSegment::new(tenant.as_str())?)
            .segment(UriSegment::new(principal.as_str())?)
            .build()?,
    )
}
