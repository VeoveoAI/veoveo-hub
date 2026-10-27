//! Native Lore wire contribution. All deployments and assignments come from the
//! admitted control-plane revision; only native protocol types live in code.
pub(crate) mod catalog;
pub(crate) mod client_token;
pub(crate) mod computer_client;
pub(crate) mod factory;
pub(crate) mod instance;
pub(crate) mod instance_client;
pub(crate) mod person;
pub(crate) mod publication;
pub(crate) mod repositories;
pub(crate) mod repository_client;
pub(crate) mod resources;
pub(crate) mod sharing;
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
/// A group's ID, random and in the canonical form of a repository ID. It stays inside
/// the Hub: people name a group by its name.
#[veoveo_types::id(text(RepositoryIds))]
pub struct GroupId(String);
/// A group's name, unique in its tenant: 1 to 64 of `a`–`z`, `0`–`9` and `-`,
/// starting with a letter.
#[veoveo_types::id(text(GroupNames))]
pub struct GroupName(String);
#[doc(hidden)]
pub struct GroupNames;
impl veoveo_types::IdProfile for GroupNames {
    type Error = veoveo_types::IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            let mut bytes = value.bytes();
            if value.len() <= 64
                && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
                && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                Ok(())
            } else {
                Err(veoveo_types::IdentifierError::new(
                    value,
                    "a group name is 1 to 64 of a-z, 0-9 and -, starting with a letter",
                ))
            }
        });
}
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
/// A member's role in a group. Both roles hold the group's assignments; an owner also
/// sets the group's memberships.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Member,
    Owner,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Member {
    pub principal: PrincipalId,
    pub role: GroupRole,
}
/// People of one tenant whom assignments name together.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Group {
    pub id: GroupId,
    pub tenant: TenantId,
    pub name: GroupName,
    pub members: Vec<Member>,
}
impl Group {
    pub(crate) fn role(&self, principal: &PrincipalId) -> Option<GroupRole> {
        self.members
            .iter()
            .find(|member| &member.principal == principal)
            .map(|member| member.role)
    }
}
/// Whom an assignment grants: one person, or every member of one group of the
/// repository's tenant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    Principal(PrincipalId),
    Group(GroupId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "AssignmentDocument", into = "AssignmentDocument")]
pub struct Assignment {
    pub deployment: DeploymentId,
    pub repository: RepositoryId,
    pub tenant: TenantId,
    pub subject: Subject,
    pub actions: BTreeSet<NativeAction>,
}
/// An assignment as the control plane writes it: its subject is exactly one of
/// `principal` and `group`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AssignmentDocument {
    deployment: DeploymentId,
    repository: RepositoryId,
    tenant: TenantId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<GroupId>,
    actions: BTreeSet<NativeAction>,
}
impl TryFrom<AssignmentDocument> for Assignment {
    type Error = &'static str;
    fn try_from(document: AssignmentDocument) -> std::result::Result<Self, Self::Error> {
        let subject = match (document.principal, document.group) {
            (Some(principal), None) => Subject::Principal(principal),
            (None, Some(group)) => Subject::Group(group),
            _ => return Err("an assignment names exactly one of a principal and a group"),
        };
        Ok(Self {
            deployment: document.deployment,
            repository: document.repository,
            tenant: document.tenant,
            subject,
            actions: document.actions,
        })
    }
}
impl From<Assignment> for AssignmentDocument {
    fn from(assignment: Assignment) -> Self {
        let (principal, group) = match assignment.subject {
            Subject::Principal(principal) => (Some(principal), None),
            Subject::Group(group) => (None, Some(group)),
        };
        Self {
            deployment: assignment.deployment,
            repository: assignment.repository,
            tenant: assignment.tenant,
            principal,
            group,
            actions: assignment.actions,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lore {
    pub deployments: Vec<Deployment>,
    pub repositories: Vec<Repository>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
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
        for g in &self.groups {
            objects.push(CatalogObjectDescriptor {
                tenant: Some(g.tenant.clone()),
                kind: ExtensionName::parse("lore.group")?,
                id: g.id.to_string(),
                value: serde_json::to_value(g)
                    .map_err(|_| ExtensionError::new("native group encoding failed"))?,
            });
        }
        for a in &self.assignments {
            let subject = match &a.subject {
                Subject::Principal(principal) => format!("principal/{principal}"),
                Subject::Group(group) => format!("group/{group}"),
            };
            objects.push(CatalogObjectDescriptor {
                tenant: Some(a.tenant.clone()),
                kind: ExtensionName::parse("lore.assignment")?,
                id: format!("{}/{}/{subject}", a.deployment, a.repository),
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
            // The creator's own assignment, not one through a group.
            let owns = &existing.tenant == owned.tenant
                && self.assignments.iter().any(|a| {
                    &a.deployment == owned.deployment
                        && &a.repository == owned.repository
                        && &a.tenant == owned.tenant
                        && a.subject == Subject::Principal(owned.principal.clone())
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
            subject: Subject::Principal(owned.principal.clone()),
            actions: BTreeSet::from([NativeAction::Owner]),
        });
        Ok(true)
    }

    /// The groups of `tenant` that `principal` belongs to, in either role.
    pub(crate) fn memberships<'a>(
        &'a self,
        tenant: &'a TenantId,
        principal: &'a PrincipalId,
    ) -> impl Iterator<Item = &'a Group> {
        self.groups
            .iter()
            .filter(move |g| &g.tenant == tenant && g.role(principal).is_some())
    }

    /// The assignments on `repository` in `tenant` that `principal` holds: their own and
    /// those of every group of the tenant they belong to, on any deployment. Every Lore
    /// grant the Hub issues or honors is one of these.
    fn held<'a>(
        &'a self,
        repository: &'a RepositoryId,
        tenant: &'a TenantId,
        principal: &'a PrincipalId,
    ) -> impl Iterator<Item = &'a Assignment> {
        let groups = self
            .memberships(tenant, principal)
            .map(|g| &g.id)
            .collect::<BTreeSet<_>>();
        self.assignments.iter().filter(move |a| {
            &a.repository == repository
                && &a.tenant == tenant
                && match &a.subject {
                    Subject::Principal(own) => own == principal,
                    Subject::Group(group) => groups.contains(group),
                }
        })
    }

    /// The person's native actions on the deployment's repository: the union of those of
    /// every assignment they hold on it, or none when they hold none, which is no grant.
    /// An assignment without actions is a grant with no named privilege.
    pub(crate) fn actions(
        &self,
        deployment: &DeploymentId,
        repository: &RepositoryId,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Option<BTreeSet<NativeAction>> {
        let mut held = self
            .held(repository, tenant, principal)
            .filter(|a| &a.deployment == deployment)
            .peekable();
        held.peek()?;
        Some(held.flat_map(|a| a.actions.iter().copied()).collect())
    }

    /// Whether the principal holds a current grant on `repository`, own or through a
    /// group, on any deployment.
    pub(crate) fn grants(
        &self,
        principal: &veoveo_hub_sdk::Principal,
        repository: &RepositoryId,
    ) -> bool {
        principal.tenant.as_ref().is_some_and(|tenant| {
            self.held(repository, tenant, &principal.id)
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
        let mut groups = BTreeSet::new();
        let mut names = BTreeSet::new();
        for g in &self.groups {
            ensure!(
                facts.tenants.contains(&g.tenant),
                "group of an unknown tenant"
            );
            ensure!(groups.insert(&g.id), "duplicate group ID");
            ensure!(
                names.insert((&g.tenant, &g.name)),
                "duplicate group name in a tenant"
            );
            let mut members = BTreeSet::new();
            ensure!(
                g.members.iter().all(|m| members.insert(&m.principal)),
                "duplicate group member"
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
            if let Subject::Group(id) = &a.subject {
                ensure!(
                    self.groups
                        .iter()
                        .any(|g| &g.id == id && g.tenant == a.tenant),
                    "assignment names no group of the repository's tenant"
                );
            }
            ensure!(
                assignments.insert((&a.deployment, &a.repository, &a.subject)),
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
/// What a Lore token is issued for: a login token, which names its person and carries no
/// repository, or the token of the one repository an exchange's scope names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Login,
    Exchange {
        resource: Option<String>,
        scope: Option<String>,
    },
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
                let permission = section
                    .actions(&deployment.id, &id, tenant, &context.principal().id)
                    .context("native repository assignment denied")?;
                (
                    TokenPurpose::Resource,
                    vec![NativeGrant {
                        resource_id: deployment.resource_id_template.render(&id),
                        permission,
                    }],
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
