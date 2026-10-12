//! The Gateway's own catalog declarations. Servers contribute their actions, targets and
//! configuration sections through registration, so the Gateway compiles in only its
//! kernel vocabulary.
use std::sync::Arc;
use veoveo_audit_contract::AuditTargetRegistry;
use veoveo_gateway_contract::{CatalogRegistry, CatalogRegistryBuilder, GatewayAction};
use veoveo_types::{ExtensionError, ExtensionName};

/// The catalog registry with the kernel sections, extension groups and actions only.
pub fn kernel_catalog_registry() -> Result<CatalogRegistry, ExtensionError> {
    let mut builder = CatalogRegistryBuilder::new(
        [
            "branding",
            "identity_providers",
            "authorization_servers",
            "servers",
            "profiles",
            "tenants",
            "work_contexts",
            "policies",
            "data_labels",
            "oauth_clients",
            "oidc_clients",
            "secrets",
            "metadata",
        ]
        .into_iter()
        .map(String::from),
        [
            "gateway",
            "server",
            "tool",
            "resource",
            "resource_template",
            "prompt",
            "task",
            "platform_task",
            "artifact",
            "usage",
        ]
        .into_iter()
        .map(ExtensionName::parse)
        .collect::<Result<Vec<_>, _>>()?,
    );
    builder.register_kernel::<GatewayAction>()?;
    builder.build()
}

/// Audit read codecs for the kernel targets; owners add theirs through registration.
pub fn kernel_audit_target_registry() -> Arc<AuditTargetRegistry> {
    Arc::new(AuditTargetRegistry::builder().build())
}
