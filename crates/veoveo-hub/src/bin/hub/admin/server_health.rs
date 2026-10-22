//! Per-server health for operators and tools. A registered server is healthy while
//! its registration's login holds a live kernel instance; an instance reports no state
//! here, because its health comes from its factory sandbox.
use veoveo_hub::contract::{HubAction, ServerHealthEntry, ServerHealthReport};

use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_hub::audit::AdministrativeOperation;
use veoveo_hub::{AuthenticatedSubject, HubServerHealth};
use veoveo_types::{DeploymentId, ServerSlug};

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
        HubAction::AdminRead,
        AdministrativeOperation::ServerHealth,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let probed = match state.registrations.health().await {
        Ok(probed) => probed,
        Err(error) => return crate::audit::internal_error_response(error),
    };
    let servers = catalog
        .registrations()
        .iter()
        .map(|server| (server.slug.clone(), server.deployment));
    Json(report(servers, |deployment| probed.get(deployment))).into_response()
}

fn report<'a>(
    servers: impl Iterator<Item = (ServerSlug, DeploymentId)>,
    probed: impl Fn(&DeploymentId) -> Option<&'a HubServerHealth>,
) -> ServerHealthReport {
    let mut servers = servers
        .map(|(server, deployment)| {
            let health = probed(&deployment);
            ServerHealthEntry {
                state: health.map(|health| health.state),
                checked_at: health.map(|health| health.checked_at),
                server,
                deployment,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by(|left, right| {
        (&left.server, left.deployment).cmp(&(&right.server, right.deployment))
    });
    ServerHealthReport { servers }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::collections::BTreeMap;
    use veoveo_hub::contract::HubServerHealthState;

    use super::*;

    #[test]
    fn servers_without_a_probe_report_no_state_instead_of_offline() {
        let checked_at = Utc::now();
        let healthy = DeploymentId::new();
        let unprobed = DeploymentId::new();
        let probed = BTreeMap::from([(
            healthy,
            HubServerHealth {
                state: HubServerHealthState::Healthy,
                checked_at,
            },
        )]);
        let report = report(
            [("upstream-b", unprobed), ("upstream-a", healthy)]
                .into_iter()
                .map(|(slug, deployment)| (ServerSlug::parse(slug).unwrap(), deployment)),
            |slug| probed.get(slug),
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["servers"][0]["server"], "upstream-a");
        assert_eq!(json["servers"][0]["state"], "healthy");
        assert_eq!(json["servers"][1]["server"], "upstream-b");
        assert!(json["servers"][1]["state"].is_null());
        assert!(json["servers"][1]["checkedAt"].is_null());
    }
    #[test]
    fn simultaneous_deployments_of_one_namespace_keep_independent_health() {
        let a = DeploymentId::new();
        let b = DeploymentId::new();
        let checked_at = Utc::now();
        let health = HubServerHealth {
            state: HubServerHealthState::Healthy,
            checked_at,
        };
        let report = report(
            [a, b]
                .into_iter()
                .map(|id| (ServerSlug::parse("fixture").unwrap(), id)),
            |id| (*id == a).then_some(&health),
        );
        assert_eq!(report.servers.len(), 2);
        assert_eq!(
            report
                .servers
                .iter()
                .find(|entry| entry.deployment == a)
                .unwrap()
                .state,
            Some(HubServerHealthState::Healthy)
        );
        assert!(
            report
                .servers
                .iter()
                .find(|entry| entry.deployment == b)
                .unwrap()
                .state
                .is_none()
        );
    }
}
