//! Kernel catalog registry and admission binding shared by the crate's unit tests.
use crate::contract::CatalogRegistry;
use std::sync::OnceLock;

use crate::HubCatalogAdmission;

pub fn fresh_registry() -> CatalogRegistry {
    crate::kernel_catalog_registry().unwrap()
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

/// The Hub issuer every unit test signs forwarded tokens with.
pub fn token_issuer() -> veoveo_hub_sdk::HubInternalTokenIssuer {
    use base64::Engine as _;
    veoveo_hub_sdk::HubInternalTokenIssuer::new(
        veoveo_hub_sdk::TokenIssuer::parse("https://hub.test").unwrap(),
        veoveo_hub_sdk::HubInternalSigningKey::new(
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
        endpoint: veoveo_hub_sdk::registration::ServerEndpoint::new(
            url::Url::parse(endpoint).unwrap(),
        )
        .unwrap(),
        version: veoveo_hub_sdk::registration::ServerVersion::new("0.1.0").unwrap(),
        client_id: veoveo_types::OAuthClientId::parse(format!("{slug}-0001")).unwrap(),
        authorization: std::sync::Arc::new(
            veoveo_hub_sdk::registration::AuthorizationDeclaration {
                permissions: Vec::new(),
                requirements: Default::default(),
                uses: Vec::new(),
            },
        ),
        capabilities: Default::default(),
    }
}

/// Publish a configuration that defines exactly one Work Context, so rows that
/// reference it can be written.
pub async fn publish_work_context(
    store: &crate::store::HubStore,
    tenant: &veoveo_types::TenantId,
    work_context: &veoveo_types::WorkContextId,
) {
    let plane: crate::control_plane::HubControlPlane = serde_json::from_value(serde_json::json!({
        "identityProviders": [],
        "authorizationServers": [],
        "profiles": [],
        "tenants": [],
        "workContexts": [{
            "id": work_context,
            "tenant": tenant,
            "title": "Fixture Work Context",
            "policyRevision": "r1",
            "outputPolicy": { "owner": { "kind": "group", "id": "owners" } },
            "memberships": []
        }],
        "policies": [],
        "dataLabels": [],
        "oidcClients": []
    }))
    .unwrap();
    let revision = crate::control_plane::HubControlPlaneRevision {
        revision_id: crate::control_plane::HubControlPlaneRevisionId::parse(format!(
            "gcp-{}",
            uuid::Uuid::now_v7()
        ))
        .unwrap(),
        sha256: "0".repeat(64),
        source: crate::control_plane::HubControlPlaneRevisionSource::SeedFile,
        applied_at: chrono::Utc::now(),
        applied_by: veoveo_types::PrincipalId::parse("fixture").unwrap(),
        tenant: None,
        control_plane: plane,
    };
    let active = store.active_control_revision().await.unwrap();
    let audit = crate::audit::AuditAppend::new(Default::default());
    store
        .publish_control_revision(
            crate::store::ControlPublication {
                revision: &revision,
                replaces: active.as_ref().map(|active| &active.revision_id),
                objects: &[],
            },
            &audit,
        )
        .await
        .unwrap();
}
