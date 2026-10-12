use veoveo_gateway_contract::GatewayAction;

use rmcp::model::{
    CallToolResult, ContentBlock, ErrorData as McpError, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, ServerResult, Tool,
};
use serde_json::Value;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::{GatewayResourceProjection, PolicyTarget, ServerResourceUri, ServerSlug};
use veoveo_types::{ResourceTemplateUri, ResourceUri};

use crate::registration::RegisteredServer;

use crate::GatewayCatalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceReadKind {
    General,
    Artifact,
    Usage,
}

fn resource_read_kind(uri: &str) -> ResourceReadKind {
    match ServerResourceUri::parse(uri) {
        Ok(ServerResourceUri::Artifact { .. }) => ResourceReadKind::Artifact,
        Ok(ServerResourceUri::UsageRoot { .. } | ServerResourceUri::UsageTask { .. }) => {
            ResourceReadKind::Usage
        }
        Ok(_) | Err(_) => ResourceReadKind::General,
    }
}

pub(crate) fn resource_read_action(uri: &str) -> GatewayAction {
    match resource_read_kind(uri) {
        ResourceReadKind::Artifact => GatewayAction::ArtifactRead,
        ResourceReadKind::Usage => GatewayAction::UsageRead,
        ResourceReadKind::General => GatewayAction::ResourcesRead,
    }
}

pub(crate) fn gateway_resource_uri(uri: &str) -> Result<ResourceUri, McpError> {
    ResourceUri::new(uri.to_string())
        .map_err(|err| mcp_invalid_params(format!("invalid resource URI: {err}")))
}

/// Registered servers keep their own URIs: each server's URI scheme equals its slug, and
/// its Apps live under `ui://<slug>/`, so the Gateway routes them unchanged.
pub(crate) fn project_upstream_resource_uri_for_gateway(
    _server: &RegisteredServer,
    upstream_uri: &str,
) -> Result<ResourceUri, McpError> {
    gateway_resource_uri(upstream_uri)
}

pub(crate) fn project_gateway_resource_uri_for_upstream(
    _server: &RegisteredServer,
    gateway_uri: &str,
    _upstream_resources: &[Resource],
) -> Result<Option<ResourceUri>, McpError> {
    Ok(Some(gateway_resource_uri(gateway_uri)?))
}

pub(crate) fn project_listed_resource_uri(
    server: &RegisteredServer,
    resource: &mut Resource,
) -> Result<(), McpError> {
    resource.uri = project_upstream_resource_uri_for_gateway(server, &resource.uri)?.to_string();
    if let Some(meta) = &mut resource.meta {
        project_app_resource_metadata(server, meta)?;
    }
    Ok(())
}

pub(crate) fn project_resource_template_uri(
    server: &RegisteredServer,
    template: &mut ResourceTemplate,
) -> Result<(), McpError> {
    if let Some(meta) = &mut template.meta {
        project_app_resource_metadata(server, meta)?;
    }
    Ok(())
}

pub(crate) fn project_tool_resource_metadata(
    server: &RegisteredServer,
    tool: &mut Tool,
) -> Result<(), McpError> {
    if let Some(meta) = &mut tool.meta {
        project_app_resource_metadata(server, meta)?;
    }
    Ok(())
}

pub(crate) fn project_call_tool_resource_uris(
    server: &RegisteredServer,
    result: &mut CallToolResult,
) -> Result<(), McpError> {
    if let Some(meta) = &mut result.meta {
        project_app_resource_metadata(server, meta)?;
    }
    for content in &mut result.content {
        match content {
            ContentBlock::Resource(resource) => {
                project_resource_content_uri(server, &mut resource.resource)?;
                if let Some(meta) = &mut resource.meta {
                    project_app_resource_metadata(server, meta)?;
                }
            }
            ContentBlock::ResourceLink(resource) => {
                project_listed_resource_uri(server, resource)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn project_resource_content_uri(
    server: &RegisteredServer,
    content: &mut ResourceContents,
) -> Result<(), McpError> {
    match content {
        ResourceContents::TextResourceContents { uri, .. }
        | ResourceContents::BlobResourceContents { uri, .. } => {
            *uri = project_upstream_resource_uri_for_gateway(server, uri)?.to_string();
        }
        _ => {}
    }
    Ok(())
}

/// Only the MCP Apps resource link is a routable address inside extensible metadata.
/// Domain payloads and other extension fields keep their upstream values.
fn project_app_resource_metadata(
    server: &RegisteredServer,
    meta: &mut rmcp::model::MetaObject,
) -> Result<(), McpError> {
    let Some(uri) = meta
        .0
        .get_mut(veoveo_mcp_ext_apps::UI_META_KEY)
        .and_then(|ui| ui.get_mut("resourceUri"))
    else {
        return Ok(());
    };
    let original = uri
        .as_str()
        .ok_or_else(|| mcp_internal("MCP Apps resourceUri must be a resource URI string"))?;
    *uri = Value::String(project_upstream_resource_uri_for_gateway(server, original)?.to_string());
    Ok(())
}

pub(crate) fn project_upstream_resource(
    server: &RegisteredServer,
    uri: &str,
) -> Result<GatewayResourceProjection, McpError> {
    Ok(GatewayResourceProjection {
        server: server.slug.clone(),
        gateway_uri: project_upstream_resource_uri_for_gateway(server, uri)?,
        upstream_uri: gateway_resource_uri(uri)?,
    })
}

pub(crate) fn project_listed_resource(
    resource: &mut rmcp::model::Resource,
    projection: &GatewayResourceProjection,
) {
    resource.uri = projection.gateway_uri.to_string();
}

pub(crate) fn project_read_resource_result(
    result: &mut ReadResourceResult,
    projection: &GatewayResourceProjection,
) -> Result<(), McpError> {
    for content in &mut result.contents {
        match content {
            ResourceContents::TextResourceContents { uri, .. }
            | ResourceContents::BlobResourceContents { uri, .. }
                if uri == projection.upstream_uri.as_str() =>
            {
                *uri = projection.gateway_uri.to_string();
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn resource_template_policy_target(
    server: ServerSlug,
    uri: &str,
) -> Result<PolicyTarget, McpError> {
    let uri = ResourceTemplateUri::new(uri)
        .map_err(|err| mcp_invalid_params(format!("invalid resource template: {err}")))?;
    Ok(PolicyTarget::ResourceTemplate { server, uri })
}

pub(crate) fn resource_policy_target(
    server: ServerSlug,
    uri: &str,
) -> Result<PolicyTarget, McpError> {
    let uri = gateway_resource_uri(uri)?;
    Ok(match resource_read_kind(uri.as_str()) {
        ResourceReadKind::Artifact => PolicyTarget::Artifact {
            server,
            artifact_uri: uri,
        },
        ResourceReadKind::Usage => PolicyTarget::Usage {
            server,
            usage_uri: uri,
        },
        ResourceReadKind::General => PolicyTarget::Resource { server, uri },
    })
}

pub(crate) fn parse_gateway_tool(
    catalog: &GatewayCatalog,
    name: &str,
) -> Result<crate::GatewayToolProjection, McpError> {
    let gateway_name = GatewayToolName::parse(name)
        .map_err(|err| mcp_invalid_params(format!("invalid gateway tool name: {err}")))?;
    catalog
        .parse_tool_name(&gateway_name)
        .map_err(|err| mcp_invalid_params(err.to_string()))
}

pub(crate) fn ensure_unique_prompts(prompts: &[rmcp::model::Prompt]) -> Result<(), McpError> {
    let mut seen = std::collections::BTreeSet::<&str>::new();
    for prompt in prompts {
        if !seen.insert(prompt.name.as_str()) {
            return Err(mcp_internal(format!(
                "prompt `{}` is ambiguous across profile servers",
                prompt.name
            )));
        }
    }
    Ok(())
}

pub(crate) fn upstream_error(error: rmcp::ServiceError) -> McpError {
    match error {
        rmcp::ServiceError::McpError(error) => error,
        error => mcp_internal(format!("upstream MCP request failed: {error}")),
    }
}

pub(crate) fn unexpected_upstream_response(method: &str, response: ServerResult) -> McpError {
    mcp_internal(format!(
        "upstream returned unexpected response for {method}: {}",
        server_result_name(&response)
    ))
}

fn server_result_name(result: &ServerResult) -> &'static str {
    match result {
        ServerResult::CompleteResult(_) => "complete",
        ServerResult::GetPromptResult(_) => "get_prompt",
        ServerResult::ListPromptsResult(_) => "list_prompts",
        ServerResult::ListResourcesResult(_) => "list_resources",
        ServerResult::ListResourceTemplatesResult(_) => "list_resource_templates",
        ServerResult::ReadResourceResult(_) => "read_resource",
        ServerResult::ListToolsResult(_) => "list_tools",
        ServerResult::ElicitResult(_) => "elicit",
        ServerResult::CreateTaskResult(_) => "create_task",
        ServerResult::GetTaskResult(_) => "get_task",
        ServerResult::TaskAckResult(_) => "task_ack",
        ServerResult::CallToolResult(_) => "call_tool",
        ServerResult::InputRequiredResult(_) => "input_required",
        ServerResult::EmptyResult(_) => "empty",
        ServerResult::CustomResult(_) => "custom",
        _ => "unknown",
    }
}

pub(crate) fn mcp_invalid_request(message: impl Into<String>) -> McpError {
    McpError::invalid_request(message.into(), None)
}

pub(crate) fn mcp_invalid_params(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), None)
}

pub(crate) fn mcp_internal(message: impl Into<String>) -> McpError {
    McpError::internal_error(message.into(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_protocol_errors_preserve_code_message_and_domain_data() {
        for error in [
            McpError::invalid_request("access unavailable", None),
            McpError::invalid_params("resource unavailable", None),
            McpError::internal_error(
                "provider unavailable",
                Some(serde_json::json!({
                    "providerCode": "capacity-exhausted",
                    "retryAfterSeconds": 15
                })),
            ),
        ] {
            assert_eq!(
                upstream_error(rmcp::ServiceError::McpError(error.clone())),
                error
            );
        }
        assert_eq!(
            upstream_error(rmcp::ServiceError::TransportClosed).code,
            rmcp::model::ErrorCode::INTERNAL_ERROR
        );
    }
}
