//! Daily usage totals across every registered server. A caller reads its own totals in
//! its tenant; reading the whole tenant's is an administrative read the action policy
//! decides and the audit log records.
use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::NaiveDate;
use serde::Deserialize;
use veoveo_hub::AuthenticatedSubject;
use veoveo_hub::audit::AdministrativeOperation;
use veoveo_hub::contract::{DailyUsagePage, HubAction, UsageScope};
use veoveo_hub::problem::Problem;
use veoveo_hub::usage::{DailyUsageCursor, DailyUsageQuery, MAX_USAGE_PAGE};
use veoveo_types::CursorCodec;

use super::admin_profile_id;
use crate::{audit::authorize_admin_request, runtime::AdminState};

/// The longest span of days one view reads.
const MAX_DAYS: i64 = 366;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UsageParameters {
    #[serde(default)]
    scope: UsageScope,
    /// The first day read, and the day after the last.
    from: NaiveDate,
    to: NaiveDate,
    cursor: Option<String>,
    limit: Option<u16>,
}

fn invalid(detail: &'static str) -> Response {
    Problem::new(
        StatusCode::BAD_REQUEST,
        "invalid-request",
        "Invalid request",
    )
    .detail(detail)
    .into_response()
}

pub(crate) async fn daily_usage(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(parameters): Query<UsageParameters>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if parameters.from >= parameters.to || (parameters.to - parameters.from).num_days() > MAX_DAYS {
        return invalid("`from` must precede `to` by at most 366 days");
    }
    let limit = parameters.limit.unwrap_or(100);
    if limit == 0 || limit > MAX_USAGE_PAGE {
        return invalid("`limit` must lie between 1 and 1000");
    }
    let Some(tenant) = subject.actor.tenant.clone() else {
        return Problem::new(StatusCode::FORBIDDEN, "no-tenant", "No tenant")
            .detail("usage belongs to a tenant, and the caller acts in none")
            .into_response();
    };
    let principal = match parameters.scope {
        UsageScope::Own => Some(subject.actor.id.clone()),
        UsageScope::Tenant => {
            if let Err(response) = authorize_admin_request(
                &state,
                &profile_id,
                subject,
                HubAction::AdminRead,
                AdministrativeOperation::UsageRead,
                started_at,
            )
            .await
            {
                return *response;
            }
            None
        }
    };
    let codec = DailyUsageCursor {
        scope: parameters.scope,
        principal: principal.clone(),
        from: parameters.from,
        to: parameters.to,
    };
    let after = match parameters
        .cursor
        .as_deref()
        .map(|cursor| codec.decode(cursor))
    {
        Some(Ok(position)) => Some(position),
        Some(Err(detail)) => return invalid(detail),
        None => None,
    };
    let query = DailyUsageQuery {
        tenant,
        principal,
        from: parameters.from,
        to: parameters.to,
        after,
        limit,
    };
    let (rows, next) = match state.hub_state.daily_usage(&query).await {
        Ok(page) => page,
        Err(error) => {
            tracing::error!(error = %format!("{error:#}"), "daily usage read failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let next_cursor = match next.map(|position| codec.encode(&position)).transpose() {
        Ok(cursor) => cursor,
        Err(detail) => {
            tracing::error!(detail, "daily usage cursor encoding failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(DailyUsagePage { rows, next_cursor }),
    )
        .into_response()
}
