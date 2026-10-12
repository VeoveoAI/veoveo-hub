#[path = "support/catalog_admission.rs"]
mod catalog_admission;
use veoveo_mcp_contract::GatewayControlPlane;
use veoveo_service_gateway::GatewayCatalog;

#[test]
fn raw_duplicates_and_typed_core_collision_fail_before_canonicalization() {
    let text = include_str!("../testdata/gateway.json");
    let duplicate = format!("{{\"profiles\":[],{}", &text[text.find('{').unwrap() + 1..]);
    assert!(serde_json::from_str::<GatewayControlPlane>(&duplicate).is_err());
    let mut plane: GatewayControlPlane = serde_json::from_str(text).unwrap();
    plane
        .extensions
        .insert("servers".into(), serde_json::json!([]));
    assert!(GatewayCatalog::from_control_plane(plane, catalog_admission::binding()).is_err());
}
