//! Kernel catalog admission and Gateway state for this binary. OAuth clients come
//! from the installation configuration; tokens carry no extension claims.
use std::sync::Arc;

use veoveo_service_gateway::{
    GatewayCatalogAdmission, GatewayState, oauth_clients::CatalogOAuthClientResolver,
};
use veoveo_store::PlatformStore;

pub(super) fn catalog_admission() -> anyhow::Result<GatewayCatalogAdmission> {
    GatewayCatalogAdmission::unbound().bind(veoveo_service_gateway::kernel_catalog_registry()?)
}

pub(super) fn gateway_state(platform: PlatformStore) -> anyhow::Result<GatewayState> {
    GatewayState::new(platform)
        .bind_token_extensions(Default::default())?
        .bind_oauth_client_resolver(Arc::new(CatalogOAuthClientResolver))
}
