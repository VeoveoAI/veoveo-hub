use std::fs;

use veoveo_service_gateway::control_plane::GatewayControlPlane;

const CORE_CONTROL_PLANES: [&str; 1] = ["testdata/gateway.json"];

#[test]
fn core_control_planes_satisfy_the_gateway_contract() {
    for path in CORE_CONTROL_PLANES {
        let bytes = fs::read(path).expect("read core control plane");
        serde_json::from_slice::<GatewayControlPlane>(&bytes)
            .expect("decode core control plane")
            .validate(&veoveo_service_gateway::kernel_catalog_registry().unwrap())
            .unwrap_or_else(|error| panic!("{path}: {error}"));
    }
}
