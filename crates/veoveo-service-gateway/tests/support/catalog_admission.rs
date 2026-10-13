//! The Gateway's kernel catalog registry and admission binding for integration tests.
#![allow(dead_code, reason = "Each integration test uses part of the fixture")]
use std::sync::OnceLock;
use veoveo_service_gateway::GatewayCatalogAdmission;
use veoveo_service_gateway::contract::CatalogRegistry;

pub fn fresh_registry() -> CatalogRegistry {
    veoveo_service_gateway::kernel_catalog_registry().unwrap()
}

pub fn registry() -> CatalogRegistry {
    static REGISTRY: OnceLock<CatalogRegistry> = OnceLock::new();
    REGISTRY.get_or_init(fresh_registry).clone()
}

pub fn binding() -> GatewayCatalogAdmission {
    static BINDING: OnceLock<GatewayCatalogAdmission> = OnceLock::new();
    BINDING
        .get_or_init(|| GatewayCatalogAdmission::unbound().bind(registry()).unwrap())
        .clone()
}
