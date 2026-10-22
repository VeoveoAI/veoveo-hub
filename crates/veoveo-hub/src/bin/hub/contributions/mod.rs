//! Executable composition. Registries retain typed code bindings only; deployment
//! records and assignments are resolved from each request's current catalog.
pub(crate) mod lore;
pub(crate) use lore::instance_client::token as instance_token;
use std::sync::LazyLock;
use veoveo_hub::{contract::CatalogRegistry, token_profile::TokenProfiles};
use veoveo_types::ExtensionName;

pub struct Bindings {
    pub catalog: CatalogRegistry,
    pub profiles: TokenProfiles,
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
    profiles.register(lore::Profile { key }, lore::Request::from_protocol);
    Ok(Bindings {
        catalog: catalog.build()?,
        profiles,
    })
});
pub fn bindings() -> anyhow::Result<&'static Bindings> {
    BINDINGS
        .as_ref()
        .map_err(|error| anyhow::anyhow!("issuer composition failed: {error}"))
}
