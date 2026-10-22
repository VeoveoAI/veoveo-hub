//! The registered servers one catalog generation routes to.

use std::{collections::BTreeMap, sync::Arc};
use veoveo_hub_sdk::registration::{AuthorizationDeclaration, ServerEndpoint, ServerVersion};
use veoveo_types::{DeploymentId, OAuthClientId, ResourceScheme, ServerSlug};

/// The MCP surfaces the Hub queries on a server, derived from its declaration: a
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
    pub deployment: DeploymentId,
    pub slug: ServerSlug,
    pub uri_scheme: ResourceScheme,
    pub endpoint: ServerEndpoint,
    pub version: ServerVersion,
    pub client_id: OAuthClientId,
    pub authorization: Arc<AuthorizationDeclaration>,
    pub capabilities: ServerCapabilities,
    /// The branch this deployment serves, when it is an instance.
    pub branch: Option<crate::execution::BranchName>,
}

impl RegisteredServer {
    pub fn endpoint_url(&self) -> &str {
        self.endpoint.url().as_str()
    }
}

/// Independent admitted deployments. Unscoped lookup refuses ambiguous logical slugs;
/// request routing must select an immutable composition before exposing a catalog.
#[derive(Debug, Clone, Default)]
pub struct RegistrationSnapshot {
    servers: BTreeMap<DeploymentId, Arc<RegisteredServer>>,
}

impl RegistrationSnapshot {
    pub fn new(servers: impl IntoIterator<Item = RegisteredServer>) -> Self {
        Self {
            servers: servers
                .into_iter()
                .map(|server| (server.deployment, Arc::new(server)))
                .collect(),
        }
    }

    pub fn get(&self, slug: &ServerSlug) -> Option<&RegisteredServer> {
        let mut matching = self.servers.values().filter(|server| &server.slug == slug);
        let server = matching.next()?;
        matching.next().is_none().then_some(server.as_ref())
    }

    pub fn deployment(&self, deployment: &DeploymentId) -> Option<&RegisteredServer> {
        self.servers.get(deployment).map(Arc::as_ref)
    }

    /// Explicit unavailable selections produce no route and never inherit a base here.
    /// A deployment cannot be projected under another logical namespace.
    pub fn selected(&self, composition: &veoveo_hub_sdk::ResolvedComposition) -> Self {
        Self {
            servers: composition
                .components()
                .iter()
                .filter_map(|(slug, selection)| {
                    let veoveo_hub_sdk::ResolvedComponent::Deployment { deployment } = selection
                    else {
                        return None;
                    };
                    let server = self.servers.get(deployment)?;
                    (&server.slug == slug).then(|| (*deployment, server.clone()))
                })
                .collect(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_hub_sdk::{EnvironmentId, ResolvedComponent, ResolvedComposition};

    #[test]
    fn independently_registered_deployments_cannot_take_a_logical_route() {
        let base =
            crate::test_catalog::registered_server("observatory", "https://base.example/mcp");
        let replacement =
            crate::test_catalog::registered_server("observatory", "https://owned.example/mcp");
        let slug = base.slug.clone();
        let base_id = base.deployment;
        let replacement_id = replacement.deployment;
        let registrations = RegistrationSnapshot::new([base, replacement]);
        assert_eq!(registrations.len(), 2);
        assert!(registrations.get(&slug).is_none());
        let base = ResolvedComposition::new(
            EnvironmentId::new(),
            BTreeMap::from([(
                slug.clone(),
                ResolvedComponent::Deployment {
                    deployment: base_id,
                },
            )]),
        );
        let owned = ResolvedComposition::new(
            EnvironmentId::new(),
            BTreeMap::from([(
                slug.clone(),
                ResolvedComponent::Deployment {
                    deployment: replacement_id,
                },
            )]),
        );
        assert_eq!(
            registrations.selected(&base).get(&slug).unwrap().deployment,
            base_id
        );
        assert_eq!(
            registrations
                .selected(&owned)
                .get(&slug)
                .unwrap()
                .deployment,
            replacement_id
        );
        let unavailable = ResolvedComposition::new(
            EnvironmentId::new(),
            BTreeMap::from([(slug.clone(), ResolvedComponent::Unavailable {})]),
        );
        assert!(registrations.selected(&unavailable).get(&slug).is_none());
    }

    #[test]
    fn missing_or_foreign_namespace_deployment_cannot_fall_back() {
        let server =
            crate::test_catalog::registered_server("observatory", "https://base.example/mcp");
        let deployment = server.deployment;
        let registrations = RegistrationSnapshot::new([server]);
        for (slug, deployment) in [
            ("observatory", DeploymentId::new()),
            ("foreign", deployment),
        ] {
            let composition = ResolvedComposition::new(
                EnvironmentId::new(),
                BTreeMap::from([(
                    ServerSlug::parse(slug).unwrap(),
                    ResolvedComponent::Deployment { deployment },
                )]),
            );
            assert!(registrations.selected(&composition).is_empty());
        }
    }
}
