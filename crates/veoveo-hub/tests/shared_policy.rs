//! The lightweight worker catalog and hub index must make the same decisions.
use veoveo_hub::contract::HubAction;
#[path = "support/catalog_admission.rs"]
mod catalog_admission;
use std::collections::BTreeSet;
use veoveo_hub::control_plane::{HubControlPlane, PolicyTarget, TraceId};
use veoveo_hub::policy::PolicyCatalog;
use veoveo_hub::{HubCatalog, PolicyRequest};
use veoveo_hub_sdk::{LocalToolName, Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{PrincipalId, RoleId, ServerSlug, TenantId};

#[test]
fn hub_and_background_catalogs_preserve_policy_semantics() {
    let plane: HubControlPlane =
        serde_json::from_str(include_str!("../testdata/hub.json")).unwrap();
    let hub = HubCatalog::from_control_plane(plane.clone(), catalog_admission::binding()).unwrap();
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
                    (HubAction::AdminRead, &PolicyTarget::Hub),
                    (HubAction::AdminWrite, &PolicyTarget::Hub),
                    (HubAction::ToolsCall, &unregistered),
                ] {
                    let request = PolicyRequest {
                        principal: &principal,
                        profile: &profile.id,
                        action: action.into(),
                        target,
                        trace_id: &trace,
                    };
                    let expected = hub.decide(request.clone());
                    let mut actual = veoveo_hub::policy::decide(&background, request);
                    // The evaluator records the time of each separate call.
                    actual.evaluated_at = expected.evaluated_at;
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}
