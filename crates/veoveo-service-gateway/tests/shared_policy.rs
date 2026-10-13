//! The lightweight worker catalog and gateway index must make the same decisions.
use veoveo_service_gateway::contract::GatewayAction;
#[path = "support/catalog_admission.rs"]
mod catalog_admission;
use std::collections::BTreeSet;
use veoveo_sdk_rust::{LocalToolName, Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_service_gateway::control_plane::{GatewayControlPlane, PolicyTarget, TraceId};
use veoveo_service_gateway::policy::PolicyCatalog;
use veoveo_service_gateway::{GatewayCatalog, PolicyRequest};
use veoveo_types::{PrincipalId, RoleId, ServerSlug, TenantId};

#[test]
fn gateway_and_background_catalogs_preserve_policy_semantics() {
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../testdata/gateway.json")).unwrap();
    let gateway =
        GatewayCatalog::from_control_plane(plane.clone(), catalog_admission::binding()).unwrap();
    let background = PolicyCatalog::new(plane.clone(), catalog_admission::registry()).unwrap();
    for kind in [PrincipalKind::User, PrincipalKind::Service] {
        for profile in &plane.profiles {
            for with_scopes in [false, true] {
                let principal = Principal {
                    id: PrincipalId::parse("https://policy.test#actor").unwrap(),
                    kind,
                    issuer: TokenIssuer::parse("https://policy.test").unwrap(),
                    subject: TokenSubject::parse("actor").unwrap(),
                    tenant: Some(TenantId::parse("enterprise").unwrap()),
                    groups: BTreeSet::new(),
                    group_roles: BTreeSet::new(),
                    roles: BTreeSet::from([
                        RoleId::parse("operator").unwrap(),
                        RoleId::parse("administrator").unwrap(),
                    ]),
                    scopes: if with_scopes {
                        profile.required_scopes.iter().cloned().collect()
                    } else {
                        BTreeSet::new()
                    },
                    data_labels: BTreeSet::new(),
                    assurances: BTreeSet::new(),
                    authenticated_at: None,
                };
                let trace = TraceId::parse("shared-policy-probe").unwrap();
                let unregistered = PolicyTarget::Tool {
                    server: ServerSlug::parse("unregistered").unwrap(),
                    tool: LocalToolName::parse("policy_probe").unwrap(),
                };
                for (action, target) in [
                    (GatewayAction::AdminRead, &PolicyTarget::Gateway),
                    (GatewayAction::AdminWrite, &PolicyTarget::Gateway),
                    (GatewayAction::ToolsCall, &unregistered),
                ] {
                    let request = PolicyRequest {
                        principal: &principal,
                        profile: &profile.id,
                        action: action.into(),
                        target,
                        trace_id: &trace,
                    };
                    let expected = gateway.decide(request.clone());
                    let mut actual = veoveo_service_gateway::policy::decide(&background, request);
                    // The evaluator records the time of each separate call.
                    actual.evaluated_at = expected.evaluated_at;
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}
