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

#[test]
fn retired_lore_issuer_configuration_is_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read("testdata/hub.json").unwrap()).unwrap();
    value["lore"] = serde_json::json!({
        "resource": "lores://retired-issuer.example:41337",
        "authorizationServer": "local",
        "grants": []
    });
    let error = serde_json::from_value::<HubControlPlane>(value).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("obsolete control-plane field `lore`")
    );
}
