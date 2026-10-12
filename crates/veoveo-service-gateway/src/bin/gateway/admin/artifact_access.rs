use std::time::Instant;
use veoveo_gateway_contract::GatewayAction;
use veoveo_http::RequestJson;
use veoveo_mcp_contract::audit::{AdministrativeOperation, AuditTarget};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_contract::{
    ArtifactAccessRequestId, CreateArtifactAccessRequest, DecideArtifactAccessRequest,
    ListArtifactAccessRequests,
};
use veoveo_artifact_contract::{ArtifactId, ArtifactLedgerAddress};
use veoveo_mcp_contract::{ArtifactPlane, ArtifactPlaneError, PlaneCaller, PolicyTarget};
use veoveo_service_gateway::AuthenticatedSubject;

use crate::{
    admin::admin_profile_id,
    audit::{AdminAuthorizationRequest, authorize_admin_target_request},
    runtime::{AdminState, current_http_client},
};

const INTERNAL_TOKEN_TTL_SECONDS: i64 = 60;

pub(crate) async fn create_artifact_access_request(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(request): RequestJson<CreateArtifactAccessRequest>,
) -> Response {
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminRead,
        AdministrativeOperation::ArtifactAccessRequestCreate,
        AuditTarget::Artifact {
            artifact: artifact_id,
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    match plane
        .create_access_request(&caller, &artifact_id, request)
        .await
    {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(error) => artifact_error(error),
    }
}

pub(crate) async fn list_artifact_access_requests(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(request): Query<ListArtifactAccessRequests>,
) -> Response {
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminRead,
        AdministrativeOperation::ArtifactAccessRequestList,
        AuditTarget::PlatformResource {
            uri: ArtifactLedgerAddress::AccessRequests.uri(),
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    match plane.list_access_requests(&caller, request).await {
        Ok(page) => Json(page).into_response(),
        Err(error) => artifact_error(error),
    }
}

pub(crate) async fn decide_artifact_access_request(
    State(state): State<AdminState>,
    AxumPath((profile, request_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(decision): RequestJson<DecideArtifactAccessRequest>,
) -> Response {
    let Ok(request_id) = ArtifactAccessRequestId::parse(request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminWrite,
        AdministrativeOperation::ArtifactAccessRequestDecide,
        AuditTarget::PlatformResource {
            uri: ArtifactLedgerAddress::AccessRequest(request_id).uri(),
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    match plane
        .decide_access_request(&caller, &request_id, decision)
        .await
    {
        Ok(decided) => Json(decided).into_response(),
        Err(error) => artifact_error(error),
    }
}

pub(crate) async fn cancel_artifact_access_request(
    State(state): State<AdminState>,
    AxumPath((profile, request_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Ok(request_id) = ArtifactAccessRequestId::parse(request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (plane, caller) = match authorized_plane(
        &state,
        profile,
        subject,
        GatewayAction::AdminRead,
        AdministrativeOperation::ArtifactAccessRequestCancel,
        AuditTarget::PlatformResource {
            uri: ArtifactLedgerAddress::AccessRequest(request_id).uri(),
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    match plane.cancel_access_request(&caller, &request_id).await {
        Ok(cancelled) => Json(cancelled).into_response(),
        Err(error) => artifact_error(error),
    }
}

async fn authorized_plane(
    state: &AdminState,
    profile: String,
    subject: AuthenticatedSubject,
    action: GatewayAction,
    operation: AdministrativeOperation,
    audit_target: AuditTarget,
) -> Result<(HttpArtifactPlane, PlaneCaller), Box<Response>> {
    let Some(profile_id) = admin_profile_id(profile) else {
        return Err(StatusCode::NOT_FOUND.into_response().into());
    };
    let (_, _, subject) = authorize_admin_target_request(
        state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            action: action.into(),
            target: PolicyTarget::Gateway,
            operation,
            audit_target: Some(audit_target),
            started_at: Instant::now(),
        },
    )
    .await?;
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_TOKEN_TTL_SECONDS),
    );
    let artifacts = crate::runtime::artifact_service(state)?;
    let token = artifacts
        .issue(
            &state.internal_token_issuer,
            profile_id,
            &subject,
            expires_at,
        )
        .map_err(|error| {
            tracing::error!("failed to issue artifact access identity: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;
    let memberships = token.identity.actor.group_memberships();
    Ok((
        HttpArtifactPlane::with_client(artifacts.url(), current_http_client(&state.http)),
        PlaneCaller {
            bearer_token: token.bearer_token,
            identity: token.identity,
            memberships,
        },
    ))
}

/// RFC 9457 problem details for an Artifact service failure.
fn artifact_error(error: ArtifactPlaneError) -> Response {
    let (status, code, title) = match &error {
        ArtifactPlaneError::NotFound => (StatusCode::NOT_FOUND, "not-found", "Not found"),
        ArtifactPlaneError::Denied(_) => (StatusCode::FORBIDDEN, "denied", "Access denied"),
        ArtifactPlaneError::Unauthenticated => (
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "Authentication required",
        ),
        ArtifactPlaneError::InvalidRequest(_) => (
            StatusCode::BAD_REQUEST,
            "invalid-request",
            "Invalid request",
        ),
        ArtifactPlaneError::Conflict(_) => (StatusCode::CONFLICT, "conflict", "Conflict"),
        ArtifactPlaneError::Transport(_) => (
            StatusCode::BAD_GATEWAY,
            "artifact-service-unavailable",
            "Artifact service unavailable",
        ),
    };
    let problem = veoveo_service_gateway::problem::Problem::new(status, code, title);
    match error {
        // Transport failures stay in the logs: they can name internal addresses.
        ArtifactPlaneError::Transport(_) => problem,
        error => problem.detail(error.to_string()),
    }
    .into_response()
}
