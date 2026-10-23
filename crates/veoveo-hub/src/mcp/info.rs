use rmcp::model::{
    ExtensionCapabilities, Implementation, JsonObject, ServerCapabilities, ServerConfig,
};
use veoveo_hub_sdk::ServerSlug;

use super::HubMcp;

impl HubMcp {
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

    /// Servers register and leave while a session lasts, so the Hub advertises
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
        info.server_info = Implementation::new("veoveo-hub", env!("CARGO_PKG_VERSION"));
        let mut instructions = HUB_INSTRUCTIONS.to_string();
        if let Some(handbook) = self.computer_handbook() {
            instructions.push_str("\n\n");
            instructions.push_str(&handbook);
        }
        info.instructions = Some(instructions);
        info
    }

    /// The installation's Computer handbook, for a Computer's own client on the
    /// Computers' profile only: other callers of the Hub never receive it.
    fn computer_handbook(&self) -> Option<String> {
        let client = self.client.as_ref()?;
        let catalog = self.catalog.current();
        let computers = catalog.control_plane().computers.as_ref()?;
        if computers.profile != self.profile_id || &computers.oauth_client != client {
            return None;
        }
        computers
            .handbook
            .as_deref()
            .map(str::trim)
            .filter(|handbook| !handbook.is_empty())
            .map(str::to_owned)
    }
}

const HUB_INSTRUCTIONS: &str = "Veoveo Hub for registered MCP servers. Tool names are namespaced at this MCP surface; resource URIs remain server-owned.";

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use rmcp::handler::server::ServerHandler;
    use veoveo_hub_sdk::{HubProfileId, OAuthClientId};

    use super::HUB_INSTRUCTIONS;
    use crate::{HubCatalog, HubCatalogHandle, control_plane::HubControlPlane, mcp::HubMcp};

    const HANDBOOK: &str =
        "## Home\n\n`~` is always kept; everything else comes with the Computer version.\n";

    fn plane(handbook: Option<&str>) -> HubControlPlane {
        let mut plane: HubControlPlane =
            serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap();
        plane.secrets.push(
            serde_json::from_value(serde_json::json!({
                "id": "computer-model-api-key", "source": "env", "purpose": "provider_api_key",
                "locator": "UNUSED_MODEL_API_KEY", "owner": {"kind": "hub"}, "metadata": {}
            }))
            .unwrap(),
        );
        let mut computers: crate::control_plane::Computers = serde_json::from_value(serde_json::json!({
            "gateway": {"endpoint": "https://gateway.example/", "certificateAuthority": "/unused/ca", "bearerEnvironment": "UNUSED"},
            "template": {"workspace": "installation", "name": "computer"},
            "policyFile": "/unused/policy", "command": ["/usr/local/bin/opencode", "serve"],
            "oauthClient": "operator-local-public", "profile": "operator", "workContext": "operations", "port": 4096,
            "hubProvider": {"profile": "vv-computer-hub", "credentialKey": "COMPUTER_HUB_REFERENCE"},
            "modelProvider": {"profile": "vv-computer-model", "credentialKey": "DEEPINFRA_API_KEY", "secret": "computer-model-api-key"},
            "loreProvider": {"profile": "vv-lore", "credentialKey": "LORE_ACCESS_TOKEN"}
        }))
        .unwrap();
        computers.handbook = handbook.map(str::to_owned);
        plane.computers = Some(computers);
        plane
    }

    /// The instructions `server/discover` (and a legacy initialize) answers a caller of
    /// `client` on `profile`.
    fn instructions(
        state: &crate::HubState,
        plane: HubControlPlane,
        profile: &str,
        client: Option<&str>,
    ) -> String {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let catalog = HubCatalogHandle::new(Arc::new(
            HubCatalog::from_control_plane(plane, crate::test_catalog::binding()).unwrap(),
        ));
        let hub = HubMcp {
            client: client.map(|client| OAuthClientId::parse(client).unwrap()),
            ..HubMcp::new(
                catalog,
                HubProfileId::parse(profile).unwrap(),
                state.clone(),
                crate::test_catalog::token_issuer(),
                Default::default(),
            )
        };
        hub.get_info().instructions.unwrap()
    }

    #[tokio::test]
    async fn computer_clients_receive_the_handbook_as_server_instructions() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::new().await;
            let state = crate::HubState::new(db.a.clone());
            let computer = instructions(
                &state,
                plane(Some(HANDBOOK)),
                "operator",
                Some("operator-local-public"),
            );
            assert_eq!(
                computer,
                format!("{HUB_INSTRUCTIONS}\n\n{}", HANDBOOK.trim())
            );
            for (profile, client) in [
                // Another client of the Computers' profile.
                ("operator", Some("operator-delegated")),
                // The Computers' client on another profile.
                ("admin", Some("operator-local-public")),
                ("operator", None),
            ] {
                assert_eq!(
                    instructions(&state, plane(Some(HANDBOOK)), profile, client),
                    HUB_INSTRUCTIONS,
                    "{profile} {client:?} must not receive the Computer handbook"
                );
            }
            for handbook in [None, Some(""), Some(" \n")] {
                assert_eq!(
                    instructions(
                        &state,
                        plane(handbook),
                        "operator",
                        Some("operator-local-public")
                    ),
                    HUB_INSTRUCTIONS
                );
            }
        })
        .await
        .expect("instructions fixture deadline");
    }
}
