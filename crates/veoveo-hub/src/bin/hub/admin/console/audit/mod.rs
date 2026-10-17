//! Audit-specific reads never enter the installation snapshot or its global feed.
use veoveo_http::RequestJson;
mod export;
mod model;
mod stream;
pub(crate) use export::export_audit;
pub(crate) use stream::stream_audit;

use crate::runtime::AdminState;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use model::{AuditParameters, AuditViewParameters, ViewParameters};
use std::time::Duration;
use veoveo_audit_contract::*;
use veoveo_hub::AuthenticatedSubject;
use veoveo_hub::audit::{HubReason, audit_partition_target};
use veoveo_hub_sdk::HubProfileId;
use veoveo_types::ScopeDefinition;

const READ_DEADLINE: Duration = Duration::from_secs(10);

pub(crate) async fn open_view(
    State(state): State<AdminState>,
    Path(profile): Path<HubProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
    RequestJson(partition): RequestJson<AuditPartition>,
) -> Response {
    if let Err(response) = admit(&state, &profile, &subject, &partition).await {
        return *response;
    }
    let draft = match partition_draft(
        &subject,
        &platform::AuditView {
            partition: partition.clone(),
        },
        &partition,
        AuditOutcome::Success,
        None,
    ) {
        Ok(draft) => draft,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let session = AuditViewSession {
        id: draft.id(),
        partition,
        expires_at: draft.occurred_at() + chrono::TimeDelta::minutes(15),
    };
    if state.hub_state.record_audit(draft).await.is_err() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ([(header::CACHE_CONTROL, "no-store")], Json(session)).into_response()
}

pub(super) async fn admit_view(
    state: &AdminState,
    profile: &HubProfileId,
    subject: &AuthenticatedSubject,
    partition: &AuditPartition,
    view: AuditRecordId,
) -> Result<AuditReadScope, Box<Response>> {
    let scope = admit(state, profile, subject, partition).await?;
    let actor_partition = subject
        .actor
        .tenant
        .clone()
        .map_or(AuditPartition::Installation, AuditPartition::Tenant);
    match tokio::time::timeout(
        READ_DEADLINE,
        state.hub_state.audit_store().view_admitted(
            &scope,
            partition,
            &actor_partition,
            &subject.actor.id,
            view,
        ),
    )
    .await
    {
        Ok(Ok(true)) => Ok(scope),
        Ok(Ok(false)) => Err(Box::new(
            (
                StatusCode::GONE,
                "Open a new audit view before requesting records.",
            )
                .into_response(),
        )),
        _ => Err(Box::new(StatusCode::SERVICE_UNAVAILABLE.into_response())),
    }
}

pub(super) fn scope(subject: &AuthenticatedSubject) -> AuditReadScope {
    AuditReadScope::new(
        subject.actor.tenant.clone(),
        [
            InstallationAuditRole::Administrator,
            InstallationAuditRole::Auditor,
        ]
        .into_iter()
        .any(|role| subject.actor.roles.contains(role.id())),
    )
}

/// Current authentication is checked again before stream output and export pages.
/// Catalog changes terminate streams; page requests always arrive through middleware.
pub(super) async fn current(
    state: &AdminState,
    profile: &HubProfileId,
    subject: &AuthenticatedSubject,
) -> bool {
    let catalog = state.catalog.current();
    let Some(profile) = catalog.profile(profile) else {
        return false;
    };
    if subject.access_token.expires_at <= Utc::now()
        || !subject.actor.scopes.contains(AuditScope::Read.name())
    {
        return false;
    }
    let check = async {
        if !state
            .hub_state
            .access_token_session_valid(
                &profile.id,
                &profile.authorization_server,
                &subject.access_token,
                &subject.principal,
            )
            .await?
        {
            return Ok::<_, anyhow::Error>(false);
        }
        if let Some(jwt) = &subject.access_token.jwt_id
            && state
                .hub_state
                .jwt_revocation(&profile.id, &subject.access_token.issuer, jwt, Utc::now())
                .await?
                .is_some()
        {
            return Ok(false);
        }
        Ok(true)
    };
    matches!(
        tokio::time::timeout(READ_DEADLINE, check).await,
        Ok(Ok(true))
    )
}

pub(super) async fn admit(
    state: &AdminState,
    profile: &HubProfileId,
    subject: &AuthenticatedSubject,
    partition: &AuditPartition,
) -> Result<AuditReadScope, Box<Response>> {
    let admitted = scope(subject);
    let reason = if !subject.actor.scopes.contains(AuditScope::Read.name()) {
        Some(HubReason::MissingScope)
    } else if !admitted.permits(partition) {
        Some(HubReason::PartitionDenied)
    } else if !current(state, profile, subject).await {
        Some(HubReason::Revoked)
    } else {
        None
    };
    if let Some(reason) = reason {
        let record = partition_draft(
            subject,
            &platform::AuditView {
                partition: partition.clone(),
            },
            partition,
            AuditOutcome::Denied,
            Some(reason),
        );
        let record =
            record.map_err(|_| Box::new(StatusCode::SERVICE_UNAVAILABLE.into_response()))?;
        if state.hub_state.record_audit(record).await.is_err() {
            return Err(Box::new(StatusCode::SERVICE_UNAVAILABLE.into_response()));
        }
        return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
    }
    Ok(admitted)
}

/// A record of `subject` opening a view of, or exporting, one audit partition.
pub(super) fn partition_draft<T: AuditDetailType>(
    subject: &AuthenticatedSubject,
    detail: &T,
    partition: &AuditPartition,
    outcome: AuditOutcome,
    reason: Option<HubReason>,
) -> anyhow::Result<AuditDraft> {
    let detail = veoveo_hub::audit::hub_audit().detail(detail)?;
    let draft = subject
        .audit_draft(detail, outcome)
        .target(audit_partition_target(partition)?);
    Ok(match reason {
        Some(reason) => draft.reason(reason.into()),
        None => draft,
    }
    .build()?)
}

pub(crate) async fn partitions(
    State(state): State<AdminState>,
    Path(profile): Path<HubProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let partition = subject
        .actor
        .tenant
        .clone()
        .map_or(AuditPartition::Installation, AuditPartition::Tenant);
    match admit(&state, &profile, &subject, &partition).await {
        Ok(scope) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(scope.partitions()),
        )
            .into_response(),
        Err(response) => *response,
    }
}

pub(crate) async fn records(
    State(state): State<AdminState>,
    Path(profile): Path<HubProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(parameters): Query<AuditViewParameters>,
) -> Response {
    let Ok(reader) = state.hub_state.audit_reader().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let query = match model::decode_query(&parameters.query, reader.registry()) {
        Ok(query) => query,
        Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
    };
    if query.validate().is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let scope = match admit_view(
        &state,
        &profile,
        &subject,
        &query.partition,
        parameters.view,
    )
    .await
    {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    match tokio::time::timeout(READ_DEADLINE, reader.page(&scope, &query)).await {
        Ok(Ok(page)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(AuditSummaryPage {
                records: page
                    .records
                    .into_iter()
                    .map(AuditRecordSummary::from)
                    .collect(),
                next: page.next,
            }),
        )
            .into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn summary(
    State(state): State<AdminState>,
    Path(profile): Path<HubProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(parameters): Query<ViewParameters<AuditDailyQuery>>,
) -> Response {
    let query = parameters.query.0;
    if query.validate().is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let scope = match admit_view(
        &state,
        &profile,
        &subject,
        &query.partition,
        parameters.view,
    )
    .await
    {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    match tokio::time::timeout(
        READ_DEADLINE,
        state.hub_state.audit_store().daily(&scope, &query),
    )
    .await
    {
        Ok(Ok(page)) => ([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
