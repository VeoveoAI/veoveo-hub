//! Kernel catalog registry and admission binding shared by the crate's unit tests.
use std::sync::OnceLock;
use veoveo_gateway_contract::CatalogRegistry;

use crate::GatewayCatalogAdmission;

pub fn fresh_registry() -> CatalogRegistry {
    crate::kernel_catalog_registry().unwrap()
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

/// The Gateway issuer every unit test signs forwarded tokens with.
pub fn token_issuer() -> veoveo_mcp_contract::GatewayInternalTokenIssuer {
    use base64::Engine as _;
    veoveo_mcp_contract::GatewayInternalTokenIssuer::new(
        veoveo_mcp_contract::TokenIssuer::parse("https://gateway.test").unwrap(),
        veoveo_mcp_contract::GatewayInternalSigningKey::new(
            base64::engine::general_purpose::STANDARD
                .decode("MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl")
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

/// A registered server with no declared targets, reached at `endpoint`.
pub fn registered_server(slug: &str, endpoint: &str) -> crate::registration::RegisteredServer {
    crate::registration::RegisteredServer {
        slug: veoveo_types::ServerSlug::parse(slug).unwrap(),
        uri_scheme: veoveo_types::ResourceScheme::parse(slug).unwrap(),
        endpoint: veoveo_mcp_ext_registration::ServerEndpoint::new(
            url::Url::parse(endpoint).unwrap(),
        )
        .unwrap(),
        version: veoveo_mcp_ext_registration::ServerVersion::new("0.1.0").unwrap(),
        client_id: veoveo_types::OAuthClientId::parse(format!("{slug}-0001")).unwrap(),
        authorization: std::sync::Arc::new(veoveo_mcp_ext_registration::AuthorizationDeclaration {
            permissions: Vec::new(),
            requirements: Default::default(),
        }),
        capabilities: Default::default(),
    }
}
