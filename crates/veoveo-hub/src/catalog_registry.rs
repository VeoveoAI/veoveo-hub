//! The Hub's own catalog declarations. Servers contribute their actions, targets and
//! configuration sections through registration, so the Hub compiles in only its
//! kernel vocabulary.
use crate::contract::{CatalogRegistry, CatalogRegistryBuilder, HubAction};
use std::sync::Arc;
use veoveo_audit_contract::AuditRegistry;
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
            "hub",
            "server",
            "tool",
            "resource",
            "resource_template",
            "prompt",
            "task",
            "platform_task",
        ]
        .into_iter()
        .map(ExtensionName::parse)
        .collect::<Result<Vec<_>, _>>()?,
    );
    builder.register_kernel::<HubAction>()?;
    builder.build()
}

/// The installation's audit registry: core's platform classes, the MCP vocabulary and
/// the Hub's own classes and targets.
pub fn audit_registry() -> Arc<AuditRegistry> {
    crate::audit::hub_audit().registry().clone()
}
