//! The registered servers one catalog generation routes to.

use std::{collections::BTreeMap, sync::Arc};
use veoveo_mcp_ext_registration::{AuthorizationDeclaration, ServerEndpoint, ServerVersion};
use veoveo_types::{OAuthClientId, ResourceScheme, ServerSlug};

/// The MCP surfaces the Gateway queries on a server, derived from its declaration: a
/// surface the server declared nothing for is never listed or routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub completions: bool,
    pub tasks: bool,
}

impl ServerCapabilities {
    pub fn from_declaration(declaration: &AuthorizationDeclaration) -> Self {
        let requirements = &declaration.requirements;
        let tools = !requirements.tools.is_empty();
        let resources =
            !requirements.resources.is_empty() || !requirements.resource_templates.is_empty();
        let prompts = !requirements.prompts.is_empty();
        Self {
            tools,
            resources,
            prompts,
            completions: resources || prompts,
            tasks: tools,
        }
    }
}

/// One registered server, as its newest registration declared it when the snapshot
/// was built.
#[derive(Debug, Clone)]
pub struct RegisteredServer {
    pub slug: ServerSlug,
    pub uri_scheme: ResourceScheme,
    pub endpoint: ServerEndpoint,
    pub version: ServerVersion,
    pub client_id: OAuthClientId,
    pub authorization: Arc<AuthorizationDeclaration>,
    pub capabilities: ServerCapabilities,
}

impl RegisteredServer {
    pub fn endpoint_url(&self) -> &str {
        self.endpoint.url().as_str()
    }
}

/// The registered servers, keyed by slug. Each slug has at most one current server.
#[derive(Debug, Clone, Default)]
pub struct RegistrationSnapshot {
    servers: BTreeMap<ServerSlug, Arc<RegisteredServer>>,
}

impl RegistrationSnapshot {
    pub fn new(servers: impl IntoIterator<Item = RegisteredServer>) -> Self {
        Self {
            servers: servers
                .into_iter()
                .map(|server| (server.slug.clone(), Arc::new(server)))
                .collect(),
        }
    }

    pub fn get(&self, slug: &ServerSlug) -> Option<&RegisteredServer> {
        self.servers.get(slug).map(Arc::as_ref)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RegisteredServer> {
        self.servers.values().map(Arc::as_ref)
    }

    pub fn len(&self) -> usize {
        self.servers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}
