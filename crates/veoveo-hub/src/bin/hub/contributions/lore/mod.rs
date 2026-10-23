//! Native Lore wire contribution. All deployments and assignments come from the
//! admitted control-plane revision; only native protocol types live in code.
pub(crate) mod client_token;
pub(crate) mod computer_client;
pub(crate) mod factory;
pub(crate) mod instance;
pub(crate) mod instance_client;
pub(crate) mod person;
pub(crate) mod publication;
pub(crate) mod renewal;
pub(crate) mod repositories;
pub(crate) mod repository_client;
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_hub::{
    contract::{
        AuthorizationServerId, CatalogFacts, CatalogObjectDescriptor, CatalogSection,
        OAuthGrantType, ProtectedResourceDescriptor, ProtectedResourceName, SectionKey,
    },
    token_profile::{ProfileRequest, TokenContext, TokenLifetime, TokenProfile, TokenPurpose},
};
use veoveo_hub_sdk::ProtectedResourceId;
use veoveo_types::{
    ExtensionError, ExtensionName, PolicyVersion, PrincipalId, ScopeName, TenantId,
};

#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct DeploymentId(String);
#[veoveo_types::id(text(RepositoryIds))]
pub struct RepositoryId(String);
/// A native branch ID, in the canonical form of a repository ID.
#[veoveo_types::id(text(RepositoryIds))]
pub struct BranchId(String);
#[doc(hidden)]
pub struct RepositoryIds;
impl veoveo_types::IdProfile for RepositoryIds {
    type Error = veoveo_types::IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            if value.len() == 32
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                Ok(())
            } else {
                Err(veoveo_types::IdentifierError::new(
                    value,
                    "expected a canonical native repository ID",
                ))
            }
        });
}

/// Native `{id}` substitution only, never a JSON/JWT template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct RepositoryTemplate(String);
impl<'de> Deserialize<'de> for RepositoryTemplate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let (prefix, suffix) = text
            .split_once("{id}")
            .ok_or_else(|| serde::de::Error::custom("repository template needs one {id}"))?;
        if suffix.contains("{id}")
            || prefix.contains(['{', '}'])
            || suffix.contains(['{', '}'])
            || text.chars().any(char::is_whitespace)
            || text.chars().any(char::is_control)
        {
            return Err(serde::de::Error::custom("invalid repository template"));
        }
        Ok(Self(text))
    }
}
impl RepositoryTemplate {
    fn render(&self, id: &RepositoryId) -> String {
        self.0.replace("{id}", id.as_str())
    }
    fn parse(&self, value: &str) -> Result<RepositoryId> {
        let (prefix, suffix) = self.0.split_once("{id}").expect("admitted template");
        let id = value
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
            .context("repository target does not match the deployment")?;
        let id = RepositoryId::parse(id)?;
        ensure!(self.render(&id) == value, "noncanonical repository target");
        Ok(id)
    }
}

/// Native named privileges. An empty set remains ordinary native access, including
/// unprotected writes; it is not a read-only role. No service-account inference.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NativeAction {
    Owner,
    Admin,
    Obliterate,
    Migrate,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Deployment {
    pub id: DeploymentId,
    pub resource_name: ProtectedResourceName,
    pub audience: ProtectedResourceId,
    pub endpoint: url::Url,
    pub authorization_server: AuthorizationServerId,
    pub policy_version: PolicyVersion,
    pub login_scopes: BTreeSet<ScopeName>,
    pub scope_template: RepositoryTemplate,
    pub resource_id_template: RepositoryTemplate,
    pub token_lifetime_seconds: TokenLifetime,
}
impl Deployment {
    /// Translate the admitted native authority into tonic's TLS transport URI.
    /// URL scheme mutation cannot cross from non-special `grpcs` to `https`.
    pub(crate) fn tls_endpoint(&self) -> Result<tonic::transport::Endpoint> {
        self.recipient()?;
        let authority = &self.endpoint[url::Position::BeforeHost..url::Position::AfterPort];
        Ok(tonic::transport::Endpoint::from_shared(format!(
            "https://{authority}/"
        ))?)
    }

    fn recipient(&self) -> Result<String> {
        ensure!(
            self.endpoint.scheme() == "grpcs"
                && self.endpoint.username().is_empty()
                && self.endpoint.password().is_none()
                && self.endpoint.query().is_none()
                && self.endpoint.fragment().is_none()
                && matches!(self.endpoint.path(), "" | "/"),
            "invalid native endpoint"
        );
        Ok(self
            .endpoint
            .host_str()
            .context("native endpoint has no host")?
            .to_owned())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Repository {
    pub deployment: DeploymentId,
    pub id: RepositoryId,
    pub tenant: TenantId,
    pub title: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assignment {
    pub deployment: DeploymentId,
    pub repository: RepositoryId,
    pub tenant: TenantId,
    pub principal: PrincipalId,
    pub actions: BTreeSet<NativeAction>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lore {
    pub deployments: Vec<Deployment>,
    pub repositories: Vec<Repository>,
    pub assignments: Vec<Assignment>,
}
impl CatalogSection for Lore {
    fn validate(&self, facts: &CatalogFacts) -> std::result::Result<(), ExtensionError> {
        self.check(facts)
            .map_err(|_| ExtensionError::new("native repository contribution is invalid"))
    }
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor> {
        self.deployments
            .iter()
            .map(|d| ProtectedResourceDescriptor {
                name: d.resource_name.clone(),
                resource: d.audience.clone(),
                authorization_server: d.authorization_server.clone(),
                policy_version: d.policy_version.clone(),
                required_scopes: d.login_scopes.clone(),
                grant_types: BTreeSet::from([
                    OAuthGrantType::DeviceAuthorization,
                    OAuthGrantType::RefreshToken,
                    OAuthGrantType::TokenExchange,
                    OAuthGrantType::ClientCredentials,
                ]),
            })
            .collect()
    }
    fn objects(&self) -> std::result::Result<Vec<CatalogObjectDescriptor>, ExtensionError> {
        let mut objects = Vec::new();
        for d in &self.deployments {
            objects.push(CatalogObjectDescriptor {
                tenant: None,
                kind: ExtensionName::parse("lore.deployment")?,
                id: d.id.to_string(),
                value: serde_json::to_value(d)
                    .map_err(|_| ExtensionError::new("native deployment encoding failed"))?,
            });
        }
        for r in &self.repositories {
            objects.push(CatalogObjectDescriptor {
                tenant: Some(r.tenant.clone()),
                kind: ExtensionName::parse("lore.repository")?,
                id: format!("{}/{}", r.deployment, r.id),
                value: serde_json::to_value(r)
                    .map_err(|_| ExtensionError::new("native repository encoding failed"))?,
            });
        }
        for a in &self.assignments {
            objects.push(CatalogObjectDescriptor {
                tenant: Some(a.tenant.clone()),
                kind: ExtensionName::parse("lore.assignment")?,
                id: format!("{}/{}/{}", a.deployment, a.repository, a.principal),
                value: serde_json::to_value(a)
                    .map_err(|_| ExtensionError::new("native assignment encoding failed"))?,
            });
        }
        Ok(objects)
    }
}
impl Lore {
    /// The section of the installation's current configuration.
    pub(crate) fn of(catalog: &veoveo_hub::HubCatalog) -> Result<Self> {
        Self::of_plane(catalog.control_plane())
    }

    /// The section of a configuration.
    pub(crate) fn of_plane(plane: &veoveo_hub::control_plane::HubControlPlane) -> Result<Self> {
        match plane.extensions.get("lore") {
            Some(value) => Ok(serde_json::from_value(value.clone())?),
            None => Ok(Self::default()),
        }
    }

    /// The installation's one Lore deployment, which Computers and the repositories
    /// people create use. A token has one audience, so several deployments leave it
    /// undetermined.
    pub(crate) fn sole_deployment(&self) -> std::result::Result<&Deployment, &'static str> {
        match self.deployments.as_slice() {
            [deployment] => Ok(deployment),
            _ => Err("the installation registers no single Lore deployment"),
        }
    }

    /// Register a repository its creator owns, with the creator's owner assignment.
    /// An already registered repository changes nothing and is admitted only while its
    /// creator still holds that assignment in its tenant: a creator never restores an
    /// assignment an administrator removed. Answers whether the section changed.
    pub(crate) fn register_owned(
        &mut self,
        owned: &Owned<'_>,
    ) -> std::result::Result<bool, OwnershipRefused> {
        if let Some(existing) = self
            .repositories
            .iter()
            .find(|r| &r.deployment == owned.deployment && &r.id == owned.repository)
        {
            let owns = &existing.tenant == owned.tenant
                && self.assignments.iter().any(|a| {
                    &a.deployment == owned.deployment
                        && &a.repository == owned.repository
                        && &a.tenant == owned.tenant
                        && &a.principal == owned.principal
                        && a.actions.contains(&NativeAction::Owner)
                });
            return if owns {
                Ok(false)
            } else {
                Err(OwnershipRefused)
            };
        }
        self.repositories.push(Repository {
            deployment: owned.deployment.clone(),
            id: owned.repository.clone(),
            tenant: owned.tenant.clone(),
            title: owned.title.to_owned(),
        });
        self.assignments.push(Assignment {
            deployment: owned.deployment.clone(),
            repository: owned.repository.clone(),
            tenant: owned.tenant.clone(),
            principal: owned.principal.clone(),
            actions: BTreeSet::from([NativeAction::Owner]),
        });
        Ok(true)
    }

    /// The principal's own assignments on `repository` in its tenant, one per native
    /// deployment: every Lore grant the Hub issues or honors rests on one.
    fn assignments<'a>(
        &'a self,
        repository: &'a RepositoryId,
        tenant: &'a TenantId,
        principal: &'a PrincipalId,
    ) -> impl Iterator<Item = &'a Assignment> {
        self.assignments.iter().filter(move |a| {
            &a.repository == repository && &a.tenant == tenant && &a.principal == principal
        })
    }

    /// Whether the principal holds a current grant on `repository`.
    pub(crate) fn grants(
        &self,
        principal: &veoveo_hub_sdk::Principal,
        repository: &RepositoryId,
    ) -> bool {
        principal.tenant.as_ref().is_some_and(|tenant| {
            self.assignments(repository, tenant, &principal.id)
                .next()
                .is_some()
        })
    }

    fn check(&self, facts: &CatalogFacts) -> Result<()> {
        let mut deployments = BTreeSet::new();
        let mut resources = BTreeSet::new();
        for d in &self.deployments {
            ensure!(
                deployments.insert(d.id.clone()) && resources.insert(d.audience.clone()),
                "duplicate native deployment"
            );
            ensure!(
                facts
                    .authorization_servers
                    .contains(&d.authorization_server)
                    && facts.policies.contains(&d.policy_version),
                "unknown deployment authority"
            );
            d.recipient()?;
            ensure!(
                !d.login_scopes.is_empty()
                    && d.login_scopes
                        .contains(&ScopeName::parse("offline_access")?),
                "native login scopes must explicitly include offline_access"
            );
            let probe = RepositoryId::parse("11111111111111111111111111111111")?;
            ScopeName::parse(d.scope_template.render(&probe))?;
            ensure!(
                d.scope_template.parse(&d.scope_template.render(&probe))? == probe,
                "ambiguous native scope template"
            );
        }
        let mut repositories = BTreeSet::new();
        for r in &self.repositories {
            ensure!(
                deployments.contains(&r.deployment)
                    && facts.tenants.contains(&r.tenant)
                    && !r.title.trim().is_empty()
                    && repositories.insert((r.deployment.clone(), r.id.clone())),
                "invalid native repository registration"
            );
        }
        let mut assignments = BTreeSet::new();
        for a in &self.assignments {
            ensure!(
                self.repositories
                    .iter()
                    .any(|r| r.deployment == a.deployment
                        && r.id == a.repository
                        && r.tenant == a.tenant),
                "assignment references another repository or tenant"
            );
            ensure!(
                assignments.insert((
                    a.deployment.clone(),
                    a.repository.clone(),
                    a.principal.clone()
                )),
                "duplicate native assignment"
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Profile {
    pub key: SectionKey<Lore>,
}

/// A repository its creator registers as its owner.
pub(crate) struct Owned<'a> {
    pub deployment: &'a DeploymentId,
    pub repository: &'a RepositoryId,
    pub tenant: &'a TenantId,
    pub principal: &'a PrincipalId,
    pub title: &'a str,
}

/// The repository is registered already and its creator holds no owner assignment on
/// it, or it belongs to another tenant.
#[derive(Debug, thiserror::Error)]
#[error("the repository is registered without its creator's owner assignment")]
pub(crate) struct OwnershipRefused;
pub enum Request {
    Login,
    Exchange {
        resource: Option<String>,
        scope: Option<String>,
    },
    /// One token for every repository the principal is assigned on the deployment,
    /// none when they are assigned none. No protocol request asks for it: the Hub
    /// issues it to a Computer's own client, for the Computer's owner.
    Assigned,
}
impl Request {
    pub fn from_protocol(request: ProfileRequest<'_>) -> Result<Self> {
        Ok(match request {
            ProfileRequest::Login => Self::Login,
            ProfileRequest::Exchange { resource, scope } => Self::Exchange {
                resource: resource.map(str::to_owned),
                scope: scope.map(str::to_owned),
            },
        })
    }
}
pub struct AuthorizedGrant {
    identity: PrincipalId,
    recipient: String,
    purpose: TokenPurpose,
    resources: Vec<NativeGrant>,
    lifetime: TokenLifetime,
    scopes: BTreeSet<ScopeName>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeGrant {
    resource_id: String,
    permission: BTreeSet<NativeAction>,
}
#[derive(Serialize)]
pub struct Claims {
    identity: PrincipalId,
    root_domains: Vec<String>,
    resources: Vec<NativeGrant>,
}
impl TokenProfile for Profile {
    type Section = Lore;
    type Request = Request;
    type AuthorizedGrant = AuthorizedGrant;
    type Claims = Claims;
    fn section(&self) -> &SectionKey<Lore> {
        &self.key
    }
    fn authorize(
        &self,
        section: &Lore,
        context: &TokenContext<'_>,
        request: &Request,
    ) -> Result<AuthorizedGrant> {
        let deployment = section
            .deployments
            .iter()
            .find(|d| &d.audience == context.resource().protected_resource())
            .context("native deployment is not registered")?;
        ensure!(
            context.scopes() == &deployment.login_scopes,
            "native login scope set changed"
        );
        let mut issued_scopes = context.scopes().clone();
        let (purpose, resources) = match request {
            Request::Login => (TokenPurpose::Login, Vec::new()),
            Request::Exchange { resource, scope } => {
                ensure!(resource.is_none(), "deployment expects one scope target");
                let id = deployment
                    .scope_template
                    .parse(scope.as_deref().context("repository scope is required")?)?;
                issued_scopes =
                    BTreeSet::from([ScopeName::parse(deployment.scope_template.render(&id))?]);
                let tenant = context
                    .principal()
                    .tenant
                    .as_ref()
                    .context("native principal has no tenant")?;
                ensure!(
                    section
                        .repositories
                        .iter()
                        .any(|r| r.deployment == deployment.id
                            && r.id == id
                            && &r.tenant == tenant),
                    "native repository is not registered for this tenant"
                );
                let assignment = section
                    .assignments(&id, tenant, &context.principal().id)
                    .find(|a| a.deployment == deployment.id)
                    .context("native repository assignment denied")?;
                (
                    TokenPurpose::Resource,
                    vec![NativeGrant {
                        resource_id: deployment.resource_id_template.render(&id),
                        permission: assignment.actions.clone(),
                    }],
                )
            }
            Request::Assigned => {
                let tenant = context
                    .principal()
                    .tenant
                    .as_ref()
                    .context("native principal has no tenant")?;
                // Admission keeps every assignment on a repository registered for its
                // deployment and tenant.
                let assigned = section
                    .assignments
                    .iter()
                    .filter(|a| {
                        a.deployment == deployment.id
                            && &a.tenant == tenant
                            && a.principal == context.principal().id
                    })
                    .collect::<Vec<_>>();
                issued_scopes = assigned
                    .iter()
                    .map(|a| ScopeName::parse(deployment.scope_template.render(&a.repository)))
                    .collect::<Result<_, _>>()?;
                (
                    TokenPurpose::Resource,
                    assigned
                        .into_iter()
                        .map(|a| NativeGrant {
                            resource_id: deployment.resource_id_template.render(&a.repository),
                            permission: a.actions.clone(),
                        })
                        .collect(),
                )
            }
        };
        Ok(AuthorizedGrant {
            identity: context.principal().id.clone(),
            recipient: deployment.recipient()?,
            purpose,
            resources,
            lifetime: deployment.token_lifetime_seconds,
            scopes: issued_scopes,
        })
    }
    fn purpose(&self, grant: &AuthorizedGrant) -> TokenPurpose {
        grant.purpose
    }
    fn lifetime(&self, grant: &AuthorizedGrant) -> TokenLifetime {
        grant.lifetime
    }
    fn issued_scopes(&self, grant: &AuthorizedGrant, _: &TokenContext<'_>) -> BTreeSet<ScopeName> {
        grant.scopes.clone()
    }
    fn encode(&self, grant: &AuthorizedGrant) -> Claims {
        Claims {
            identity: grant.identity.clone(),
            root_domains: vec![grant.recipient.clone()],
            resources: grant.resources.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
