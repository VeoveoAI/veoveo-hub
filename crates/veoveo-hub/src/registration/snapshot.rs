//! The registered servers one catalog generation routes to.

use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Utc};
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
    /// When its newest registration was issued.
    pub registered_at: DateTime<Utc>,
    /// Whether housekeeping found that registration's login without a live kernel
    /// instance: its process ended, and the grace period has not removed it yet.
    pub orphaned: bool,
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
    namespaces: BTreeMap<ServerSlug, Namespace>,
}

/// The deployments of one slug that are not instances: the one requests route to, the
/// newest registered whose process lives, and those it sets aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    pub routed: DeploymentId,
    pub set_aside: Vec<DeploymentId>,
}

impl RegistrationSnapshot {
    pub fn new(servers: impl IntoIterator<Item = RegisteredServer>) -> Self {
        Self::of(
            servers
                .into_iter()
                .map(|server| (server.deployment, Arc::new(server)))
                .collect(),
        )
    }

    fn of(servers: BTreeMap<DeploymentId, Arc<RegisteredServer>>) -> Self {
        let mut by_slug = BTreeMap::<&ServerSlug, Vec<&RegisteredServer>>::new();
        for server in servers.values().filter(|server| server.branch.is_none()) {
            by_slug.entry(&server.slug).or_default().push(server);
        }
        let namespaces = by_slug
            .into_iter()
            .map(|(slug, mut deployments)| {
                // Live before orphaned, then newest first; equal times order by
                // deployment so every replica agrees. When every deployment is
                // orphaned, the newest still routes until the grace period removes it.
                deployments.sort_by(|left, right| {
                    (!right.orphaned, right.registered_at, right.deployment).cmp(&(
                        !left.orphaned,
                        left.registered_at,
                        left.deployment,
                    ))
                });
                let namespace = Namespace {
                    routed: deployments[0].deployment,
                    set_aside: deployments[1..]
                        .iter()
                        .map(|server| server.deployment)
                        .collect(),
                };
                (slug.clone(), namespace)
            })
            .collect();
        Self {
            servers,
            namespaces,
        }
    }

    /// Each slug's deployments that are not instances, by the deployment requests route
    /// to: the newest registered whose process lives, or the newest when none does.
    pub fn namespaces(&self) -> &BTreeMap<ServerSlug, Namespace> {
        &self.namespaces
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
        Self::of(
            composition
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
        )
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
    use veoveo_hub_sdk::{ResolvedComponent, ResolvedComposition};

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
        let base = ResolvedComposition::new(BTreeMap::from([(
            slug.clone(),
            ResolvedComponent::Deployment {
                deployment: base_id,
            },
        )]));
        let owned = ResolvedComposition::new(BTreeMap::from([(
            slug.clone(),
            ResolvedComponent::Deployment {
                deployment: replacement_id,
            },
        )]));
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
        let unavailable = ResolvedComposition::new(BTreeMap::from([(
            slug.clone(),
            ResolvedComponent::Unavailable {},
        )]));
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
            let composition = ResolvedComposition::new(BTreeMap::from([(
                ServerSlug::parse(slug).unwrap(),
                ResolvedComponent::Deployment { deployment },
            )]));
            assert!(registrations.selected(&composition).is_empty());
        }
    }
}
