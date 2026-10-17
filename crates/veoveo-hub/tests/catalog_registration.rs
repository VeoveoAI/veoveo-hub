#[path = "support/catalog_admission.rs"]
mod catalog_admission;
use veoveo_hub::HubCatalog;
use veoveo_hub::control_plane::HubControlPlane;

#[test]
fn raw_duplicates_and_typed_core_collision_fail_before_canonicalization() {
    let text = include_str!("../testdata/hub.json");
    let duplicate = format!("{{\"profiles\":[],{}", &text[text.find('{').unwrap() + 1..]);
    assert!(serde_json::from_str::<HubControlPlane>(&duplicate).is_err());
    let mut plane: HubControlPlane = serde_json::from_str(text).unwrap();
    plane
        .extensions
        .insert("servers".into(), serde_json::json!([]));
    assert!(HubCatalog::from_control_plane(plane, catalog_admission::binding()).is_err());
}
