//! Per-server health for operators and tools. A registered server is healthy while
//! it renews its registration lease on schedule.
use veoveo_gateway_contract::{GatewayAction, ServerHealthEntry, ServerHealthReport};

use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_mcp_contract::audit::AdministrativeOperation;
use veoveo_service_gateway::{AuthenticatedSubject, GatewayServerHealth};
use veoveo_types::ServerSlug;

use super::admin_profile_id;
use crate::{audit::authorize_admin_request, runtime::AdminState};

pub(crate) async fn read_server_health(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (catalog, _profile, _subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        AdministrativeOperation::ServerHealth,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let leases = state.registrations.health();
    let servers = catalog
        .registrations()
        .iter()
        .map(|server| server.slug.clone());
    Json(report(servers, |slug| leases.get(slug))).into_response()
}

fn report<'a>(
    servers: impl Iterator<Item = ServerSlug>,
    probed: impl Fn(&ServerSlug) -> Option<&'a GatewayServerHealth>,
) -> ServerHealthReport {
    let mut servers = servers
        .map(|server| {
            let health = probed(&server);
            ServerHealthEntry {
                state: health.map(|health| health.state),
                checked_at: health.map(|health| health.checked_at),
                server,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by(|left, right| left.server.cmp(&right.server));
    ServerHealthReport {
        servers,
        module_bindings: vec![],
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::collections::BTreeMap;
    use veoveo_gateway_contract::GatewayServerHealthState;

    use super::*;

    #[test]
    fn servers_without_a_lease_report_no_state_instead_of_offline() {
        let checked_at = Utc::now();
        let probed = BTreeMap::from([(
            ServerSlug::parse("upstream-a").unwrap(),
            GatewayServerHealth {
                state: GatewayServerHealthState::Healthy,
                checked_at,
            },
        )]);
        let report = report(
            ["upstream-b", "upstream-a"]
                .into_iter()
                .map(|slug| ServerSlug::parse(slug).unwrap()),
            |slug| probed.get(slug),
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["servers"][0]["server"], "upstream-a");
        assert_eq!(json["servers"][0]["state"], "healthy");
        assert_eq!(json["servers"][1]["server"], "upstream-b");
        assert!(json["servers"][1]["state"].is_null());
        assert!(json["servers"][1]["checkedAt"].is_null());
    }
}
