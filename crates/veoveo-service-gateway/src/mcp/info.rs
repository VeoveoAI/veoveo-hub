use rmcp::model::{
    ExtensionCapabilities, Implementation, JsonObject, ServerCapabilities, ServerConfig,
};
use veoveo_sdk_rust::ServerSlug;

use super::GatewayMcp;

impl GatewayMcp {
    pub(super) fn profile_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .map(|server| server.slug.clone())
            .collect()
    }

    fn auth_extension_capabilities(&self) -> Option<ExtensionCapabilities> {
        let catalog = self.catalog.current();
        let profile = catalog.profile(&self.profile_id)?;
        let extensions: ExtensionCapabilities = profile
            .auth_modes
            .iter()
            .filter_map(|mode| mode.mcp_extension_id())
            .map(|extension| (extension.to_string(), JsonObject::new()))
            .collect();
        if extensions.is_empty() {
            None
        } else {
            Some(extensions)
        }
    }

    /// Servers register and leave while a session lasts, so the Gateway advertises
    /// every surface it routes and announces list changes for each of them.
    pub(super) fn handle_get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tool_list_changed()
            .enable_resources()
            .enable_resources_list_changed()
            .enable_resources_subscribe()
            .enable_prompts()
            .enable_prompts_list_changed()
            .enable_completions()
            .build();
        let mut extensions = self.auth_extension_capabilities().unwrap_or_default();
        extensions.insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            JsonObject::new(),
        );
        capabilities.extensions = Some(extensions);

        let mut info = ServerConfig::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("veoveo-service-gateway", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Veoveo Gateway for registered MCP servers. Tool names are namespaced at this MCP surface; resource URIs remain server-owned."
                .to_string(),
        );
        info
    }
}
