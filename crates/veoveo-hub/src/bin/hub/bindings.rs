//! Kernel catalog admission and Hub state for this binary. OAuth clients come
//! from the installation configuration; the executable binds typed contributions.
use std::sync::Arc;

use veoveo_hub::{HubCatalogAdmission, HubState, oauth_clients::CatalogOAuthClientResolver};
use veoveo_store::PlatformStore;

pub(super) fn catalog_admission() -> anyhow::Result<HubCatalogAdmission> {
    HubCatalogAdmission::unbound().bind(crate::contributions::bindings()?.catalog.clone())
}

pub(super) fn hub_state(platform: PlatformStore) -> anyhow::Result<HubState> {
    HubState::new(platform)
        .bind_token_extensions(crate::contributions::bindings()?.profiles.claim_registry())?
        .bind_oauth_client_resolver(Arc::new(CatalogOAuthClientResolver))
}
