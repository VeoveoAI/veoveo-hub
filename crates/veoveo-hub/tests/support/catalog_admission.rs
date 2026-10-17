//! The Hub's kernel catalog registry and admission binding for integration tests.
#![allow(dead_code, reason = "Each integration test uses part of the fixture")]
use std::sync::OnceLock;
use veoveo_hub::HubCatalogAdmission;
use veoveo_hub::contract::CatalogRegistry;

pub fn fresh_registry() -> CatalogRegistry {
    veoveo_hub::kernel_catalog_registry().unwrap()
}

pub fn registry() -> CatalogRegistry {
    static REGISTRY: OnceLock<CatalogRegistry> = OnceLock::new();
    REGISTRY.get_or_init(fresh_registry).clone()
}

pub fn binding() -> HubCatalogAdmission {
    static BINDING: OnceLock<HubCatalogAdmission> = OnceLock::new();
    BINDING
        .get_or_init(|| HubCatalogAdmission::unbound().bind(registry()).unwrap())
        .clone()
}
