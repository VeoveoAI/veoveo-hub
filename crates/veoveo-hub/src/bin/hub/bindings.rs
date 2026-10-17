//! Kernel catalog admission and Hub state for this binary. OAuth clients come
//! from the installation configuration; tokens carry no extension claims.
use std::sync::Arc;

use veoveo_hub::{HubCatalogAdmission, HubState, oauth_clients::CatalogOAuthClientResolver};
use veoveo_store::PlatformStore;

pub(super) fn catalog_admission() -> anyhow::Result<HubCatalogAdmission> {
    HubCatalogAdmission::unbound().bind(veoveo_hub::kernel_catalog_registry()?)
}

pub(super) fn hub_state(platform: PlatformStore) -> anyhow::Result<HubState> {
    HubState::new(platform)
        .bind_token_extensions(Default::default())?
        .bind_oauth_client_resolver(Arc::new(CatalogOAuthClientResolver))
}
