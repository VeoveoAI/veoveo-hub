//! Executable composition. Registries retain typed code bindings only; deployment
//! records and assignments are resolved from each request's current catalog.
pub(crate) mod lore;
pub(crate) use lore::{
    computer_client::token as computer_token, instance_client::token as instance_token,
};
use std::sync::LazyLock;
use veoveo_hub::{contract::CatalogRegistry, token_profile::TokenProfiles};
use veoveo_types::ExtensionName;

pub struct Bindings {
    pub catalog: CatalogRegistry,
    pub profiles: TokenProfiles,
    /// The registered Lore profile, for the Hub's own typed requests of it.
    pub lore: lore::Profile,
}
static BINDINGS: LazyLock<anyhow::Result<Bindings>> = LazyLock::new(|| {
    let mut catalog = veoveo_hub::kernel_catalog_registry_builder()?;
    let key = catalog.register_section::<lore::Lore>(ExtensionName::parse("lore")?)?;
    let mut claims = veoveo_hub::auth::access_token_extension_registry_builder();
    for name in ["identity", "root_domains", "resources"] {
        claims.reserve(ExtensionName::parse(name)?)?;
    }
    claims.bind_serde::<veoveo_types::PrincipalId>(&ExtensionName::parse("identity")?)?;
    claims.bind_serde::<Vec<String>>(&ExtensionName::parse("root_domains")?)?;
    claims.bind_serde::<Vec<lore::NativeGrant>>(&ExtensionName::parse("resources")?)?;
    let mut profiles = TokenProfiles::new(claims.build())?;
    let lore = lore::Profile { key };
    profiles.register(lore.clone(), lore::Request::from_protocol);
    Ok(Bindings {
        catalog: catalog.build()?,
        profiles,
        lore,
    })
});
/// Apply what the active revision's activation changes outside the Hub, once per
/// installation, in the background: after a publication, at startup and after a
/// replica adopts a revision, none of which it may fail or delay.
pub(crate) fn after_activation(
    control_store: &veoveo_hub::HubControlStore,
    store: &veoveo_hub::store::HubStore,
) {
    let (control_store, store) = (control_store.clone(), store.clone());
    tokio::spawn(async move { lore::renewal::after_activation(&control_store, &store).await });
}

pub fn bindings() -> anyhow::Result<&'static Bindings> {
    BINDINGS
        .as_ref()
        .map_err(|error| anyhow::anyhow!("issuer composition failed: {error}"))
}
