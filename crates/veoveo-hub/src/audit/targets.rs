//! The targets the Hub's audit records name: absolute URIs in the platform's
//! reserved `veoveo` scheme for what the Hub owns, the resource's own URI for a
//! resource, and core's UUID URN for a Task. The SDK builds the MCP targets of tools,
//! resource templates, prompts, discovery and Task routes.
use anyhow::{Context, Result};
use veoveo_audit_contract::{AuditPartition, AuditTarget, task_target};
use veoveo_hub_sdk::{HubProfileId, ProtectedResourceId, audit::McpTarget};
use veoveo_types::{
    OAuthClientId, ResourceUri, ResourceUriBuilder, ServerSlug, TenantId, UriSegment, WorkContextId,
};

use crate::control_plane::PolicyTarget;

fn target(uri: ResourceUri) -> Result<AuditTarget> {
    AuditTarget::parse(uri.as_str()).context("an audit target must be an absolute URI")
}

/// A builder for a URI in the reserved `veoveo` scheme, from a fixed base.
fn platform(base: &'static str) -> Result<ResourceUriBuilder> {
    Ok(ResourceUriBuilder::new(base)?)
}

fn segment(value: &str) -> Result<UriSegment> {
    Ok(UriSegment::new(value)?)
}

/// `veoveo://hub`: the Hub itself, such as its administrative API.
pub fn hub_target() -> Result<AuditTarget> {
    target(platform("veoveo://hub")?.build()?)
}

/// `veoveo://servers/{server}`: a registered server.
pub fn server_target(server: &ServerSlug) -> Result<AuditTarget> {
    target(
        platform("veoveo://servers")?
            .segment(segment(server.as_str())?)
            .build()?,
    )
}

/// `veoveo://profiles/{profile}`: a Hub profile.
pub fn profile_target(profile: &HubProfileId) -> Result<AuditTarget> {
    target(
        platform("veoveo://profiles")?
            .segment(segment(profile.as_str())?)
            .build()?,
    )
}

/// `veoveo://clients/{client}`: an OAuth client of the hosted authorization server.
pub fn client_target(client: &OAuthClientId) -> Result<AuditTarget> {
    target(
        platform("veoveo://clients")?
            .segment(segment(client.as_str())?)
            .build()?,
    )
}

/// `veoveo://tenants/{tenant}/work-contexts/{context}`: a Work Context of a tenant.
pub fn work_context_target(tenant: &TenantId, context: &WorkContextId) -> Result<AuditTarget> {
    target(
        platform("veoveo://tenants")?
            .segment(segment(tenant.as_str())?)
            .segment(segment("work-contexts")?)
            .segment(segment(context.as_str())?)
            .build()?,
    )
}

/// `veoveo://audit/installation` or `veoveo://audit/tenants/{tenant}`: one partition of
/// the audit log.
pub fn audit_partition_target(partition: &AuditPartition) -> Result<AuditTarget> {
    let audit = platform("veoveo://audit")?;
    target(
        match partition {
            AuditPartition::Installation => audit.segment(segment("installation")?),
            AuditPartition::Tenant(tenant) => audit
                .segment(segment("tenants")?)
                .segment(segment(tenant.as_str())?),
        }
        .build()?,
    )
}

/// A resource by its own URI.
pub fn resource_target(uri: &ResourceUri) -> Result<AuditTarget> {
    target(uri.clone())
}

/// A protected resource by its URL.
pub fn protected_resource_target(resource: &ProtectedResourceId) -> Result<AuditTarget> {
    AuditTarget::parse(resource.as_str()).context("a protected resource is an absolute URL")
}

/// The target of a policy decision about an MCP request or a Hub action.
pub fn policy_target(target: &PolicyTarget) -> Result<AuditTarget> {
    let mcp = |target: McpTarget| Ok(target.audit_target()?);
    match target {
        PolicyTarget::Hub => hub_target(),
        PolicyTarget::Server { server } => server_target(server),
        PolicyTarget::Tool { server, tool } => mcp(McpTarget::Tool {
            server: server.clone(),
            tool: tool.clone(),
        }),
        PolicyTarget::Resource { uri, .. } => resource_target(uri),
        PolicyTarget::ResourceTemplate { server, uri } => mcp(McpTarget::ResourceTemplate {
            server: server.clone(),
            uri: uri.clone(),
        }),
        PolicyTarget::Prompt { server, prompt } => mcp(McpTarget::Prompt {
            server: server.clone(),
            prompt: prompt.clone(),
        }),
        PolicyTarget::Task { server, task_id } => mcp(McpTarget::TaskRoute {
            server: server.clone(),
            route: task_id.clone(),
        }),
        PolicyTarget::PlatformTask { task_id, .. } => Ok(task_target(*task_id)),
        PolicyTarget::Owner(_) => anyhow::bail!("owner target audit requires its catalog registry"),
        PolicyTarget::Unadmitted(_) => {
            anyhow::bail!("unadmitted owner policy target cannot produce an audit target")
        }
    }
}

/// The target of a policy decision whose target an owner admitted through the catalog:
/// the owner's own resource URI.
pub fn owner_policy_target(
    registry: &crate::contract::CatalogRegistry,
    target: &PolicyTarget,
) -> Result<AuditTarget> {
    if let PolicyTarget::Owner(target) = target {
        registry.check_target(target)?;
        return resource_target(&target.audit().uri);
    }
    policy_target(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_targets_are_veoveo_uris_with_encoded_segments() {
        let target = |result: Result<AuditTarget>| result.unwrap().as_str().to_owned();
        assert_eq!(target(hub_target()), "veoveo://hub");
        assert_eq!(
            target(server_target(&ServerSlug::parse("time").unwrap())),
            "veoveo://servers/time"
        );
        assert_eq!(
            target(profile_target(&HubProfileId::parse("local").unwrap())),
            "veoveo://profiles/local"
        );
        assert_eq!(
            target(client_target(&OAuthClientId::parse("console app").unwrap())),
            "veoveo://clients/console%20app"
        );
        assert_eq!(
            target(audit_partition_target(&AuditPartition::Installation)),
            "veoveo://audit/installation"
        );
    }
}
