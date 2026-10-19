use std::fs;

use veoveo_hub::control_plane::HubControlPlane;

const CORE_CONTROL_PLANES: [&str; 1] = ["testdata/hub.json"];

#[test]
fn core_control_planes_satisfy_the_hub_contract() {
    for path in CORE_CONTROL_PLANES {
        let bytes = fs::read(path).expect("read core control plane");
        serde_json::from_slice::<HubControlPlane>(&bytes)
            .expect("decode core control plane")
            .validate(&veoveo_hub::kernel_catalog_registry().unwrap())
            .unwrap_or_else(|error| panic!("{path}: {error}"));
    }
}
