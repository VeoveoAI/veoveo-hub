use crate::contract::{AuthorizationServerId, SecretReference, SecretReferenceId};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::Arc,
};

use crate::contract::OidcClientRegistrationId;
use crate::control_plane::{
    DataLabelDefinition, HubControlPlane, HubProfile, IdentityProvider, IdentityProviderId,
    OAuthClientRegistration, PolicySet, ResourceAuthorizationServer, TenantDefinition,
};
use anyhow::{Context, Result};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use veoveo_hub_sdk::{
    HubProfileId, OAuthClientId, Principal, PrincipalKind, ProtectedResourceId, ServerSlug,
    TokenSubject,
};
use veoveo_types::{
    DataLabelId, InvocationAuthority, InvocationMode, InvocationProvenance, PolicyVersion,
    PrincipalId, TenantId,
};

use crate::policy::resource_scheme;
use crate::registration::RegisteredServer;
use crate::{AuthenticatedSubject, HubCatalogAdmission, VerifiedAccessToken};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubAuthorityError {
    ContributedTokenAuthority,
    UnknownOAuthClient(OAuthClientId),
    UnknownProtectedResource(ProtectedResourceId),
    MissingTenant(PrincipalId),
    UnknownTenant {
        tenant: TenantId,
        principal: PrincipalId,
    },
    InvocationModeMismatch {
        client: OAuthClientId,
        expected: InvocationMode,
        received: InvocationMode,
    },
    InvalidProvenance(InvocationMode),
    InvalidActorIdentity,
}

impl std::fmt::Display for HubAuthorityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContributedTokenAuthority => formatter
                .write_str("static OAuth registration cannot admit contributed token authority"),
            Self::UnknownOAuthClient(client) => {
                write!(formatter, "unknown OAuth client `{client}`")
            }
            Self::UnknownProtectedResource(resource) => {
                write!(
                    formatter,
                    "no profile serves protected resource `{resource}`"
                )
            }
            Self::MissingTenant(principal) => {
                write!(formatter, "principal `{principal}` has no tenant")
            }
            Self::UnknownTenant { tenant, principal } => write!(
                formatter,
                "principal `{principal}` belongs to tenant `{tenant}`, which this installation does not define"
            ),
            Self::InvocationModeMismatch {
                client,
                expected,
                received,
            } => write!(
                formatter,
                "OAuth client `{client}` requires `{expected:?}` invocation and received `{received:?}`"
            ),
            Self::InvalidProvenance(mode) => {
                write!(formatter, "`{mode:?}` invocation provenance is invalid")
            }
            Self::InvalidActorIdentity => formatter.write_str("resolved actor identity is invalid"),
        }
    }
}

impl std::error::Error for HubAuthorityError {}

#[derive(Debug, Clone)]
pub struct HubCatalogHandle {
    state: Arc<RwLock<HubCatalogHandleState>>,
    changes: tokio::sync::watch::Sender<u64>,
}

#[derive(Debug)]
struct HubCatalogHandleState {
    catalog: Arc<HubCatalog>,
    generation: u64,
}

#[derive(Debug, Clone)]
pub struct HubCatalogSnapshot {
    catalog: Arc<HubCatalog>,
    generation: u64,
}

impl HubCatalogHandle {
    pub fn new(catalog: Arc<HubCatalog>) -> Self {
        Self {
            state: Arc::new(RwLock::new(HubCatalogHandleState {
                catalog,
                generation: 0,
            })),
            changes: tokio::sync::watch::channel(0).0,
        }
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub fn current(&self) -> Arc<HubCatalog> {
        self.snapshot().catalog
    }

    pub fn snapshot(&self) -> HubCatalogSnapshot {
        let state = self.state.read();
        HubCatalogSnapshot {
            catalog: state.catalog.clone(),
            generation: state.generation,
        }
    }

    /// The same request view with `server`'s slug served by `server` alone, as a request
    /// that reaches one runner of a linked server, or a Task one created, sees it.
    pub fn narrowed(&self, server: &ServerSlug, deployment: veoveo_types::DeploymentId) -> Self {
        let snapshot = self.snapshot();
        let registrations = snapshot
            .catalog
            .registrations()
            .narrowed(server, deployment);
        Self {
            state: Arc::new(RwLock::new(HubCatalogHandleState {
                catalog: Arc::new(snapshot.catalog.with_registrations(registrations)),
                generation: snapshot.generation,
            })),
            changes: tokio::sync::watch::channel(snapshot.generation).0,
        }
    }

    /// Check stable native connection/auth bootstrap before preparing a revision.
    pub fn validate_replacement(&self, catalog: &HubCatalog) -> Result<()> {
        Self::check_replacement(&self.state.read().catalog, catalog)
    }

    fn check_replacement(current: &HubCatalog, replacement: &HubCatalog) -> Result<()> {
        anyhow::ensure!(
            current.admission.same_binding(&replacement.admission),
            "catalog reload changed its admission binding"
        );
        anyhow::ensure!(
            current.control_plane.native_upstreams.is_some()
                == replacement.control_plane.native_upstreams.is_some(),
            "native gateway connection/authentication is process bootstrap; restart the Hub to change it"
        );
        if let (Some(left), Some(right)) = (
            &current.control_plane.native_upstreams,
            &replacement.control_plane.native_upstreams,
        ) {
            anyhow::ensure!(
                left.gateway == right.gateway
                    && left.certificate_authority == right.certificate_authority
                    && left.bearer_environment == right.bearer_environment,
                "native gateway connection/authentication is process bootstrap; restart the Hub to change it"
            );
        }
        if let (Some(left), Some(right)) = (
            &current.control_plane.computers,
            &replacement.control_plane.computers,
        ) {
            anyhow::ensure!(
                left.gateway == right.gateway,
                "Computer gateway connection/authentication is process bootstrap; restart the Hub to change it"
            );
        }
        Ok(())
    }

    /// Replace the installation configuration, keeping the registered servers.
    pub fn replace(&self, catalog: Arc<HubCatalog>) -> Result<()> {
        let mut state = self.state.write();
        Self::check_replacement(&state.catalog, &catalog)?;
        anyhow::ensure!(
            state.catalog.runtime.is_none() || catalog.runtime.is_some(),
            "hosted catalog replacement requires prepared runtime state"
        );
        let registrations = state.catalog.registrations().clone();
        state.catalog = Arc::new(catalog.with_registrations(registrations));
        state.generation = state.generation.saturating_add(1);
        self.changes.send_replace(state.generation);
        Ok(())
    }

    /// Route to another set of registered servers, keeping the installation
    /// configuration. Requests in flight finish on the snapshot they started with.
    pub fn replace_registrations(&self, registrations: crate::registration::RegistrationSnapshot) {
        let mut state = self.state.write();
        // An instance without a registration was removed: its forwarding closes.
        if let Some(pool) = state.catalog.upstream_http() {
            pool.retain_instances(|deployment| registrations.deployment(&deployment).is_some());
        }
        state.catalog = Arc::new(state.catalog.with_registrations(registrations));
        state.generation = state.generation.saturating_add(1);
        self.changes.send_replace(state.generation);
    }
}

impl HubCatalogSnapshot {
    pub fn catalog(&self) -> &Arc<HubCatalog> {
        &self.catalog
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// An immutable request view retaining this exact authority/runtime generation.
    /// The original live handle remains the source of publication notifications.
    pub fn selected(&self, composition: &veoveo_hub_sdk::ResolvedComposition) -> HubCatalogHandle {
        let registrations = self.catalog.registrations().selected(composition);
        HubCatalogHandle {
            state: Arc::new(RwLock::new(HubCatalogHandleState {
                catalog: Arc::new(self.catalog.with_registrations(registrations)),
                generation: self.generation,
            })),
            changes: tokio::sync::watch::channel(self.generation).0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HubCatalog {
    admission: HubCatalogAdmission,
    control_plane: Arc<HubControlPlane>,
    configuration_sha256: [u8; 32],
    pub(crate) runtime: Option<Arc<crate::hosting::HostedRuntime>>,
    identity_providers: BTreeMap<IdentityProviderId, usize>,
    authorization_servers: BTreeMap<AuthorizationServerId, usize>,
    registrations: Arc<crate::registration::RegistrationSnapshot>,
    profiles: BTreeMap<HubProfileId, usize>,
    sections: crate::contract::AdmittedCatalogSections,
    policies: BTreeMap<PolicyVersion, usize>,
    data_labels: BTreeMap<DataLabelId, usize>,
    tenants: BTreeMap<TenantId, usize>,
    oauth_clients: BTreeMap<OAuthClientId, usize>,
    oidc_clients: BTreeMap<OidcClientRegistrationId, usize>,
    secrets: BTreeMap<SecretReferenceId, usize>,
    /// The installation's own skills, `computers.skills`, with their digests.
    hub_skills: Arc<crate::skills::HubSkills>,
}

impl HubCatalog {
    pub fn from_control_plane(
        control_plane: HubControlPlane,
        admission: HubCatalogAdmission,
    ) -> Result<Self> {
        // Opaque JSON objects must have the same order before publication and
        // after storage. Dependencies may enable serde_json's preserve_order;
        // that must not change the digest checked by independent workers.
        // Reconstruct the typed document to preserve its established field order.
        // Validate the typed model before flattened fields can overwrite core JSON keys.
        admission.validate(&control_plane)?;
        let mut document = serde_json::to_value(control_plane)?;
        document.sort_all_objects();
        let control_plane: HubControlPlane = serde_json::from_value(document)?;
        let sections = admission.validate(&control_plane)?;
        let configuration_sha256 = Sha256::digest(serde_json::to_vec(&control_plane)?).into();

        let identity_providers = control_plane
            .identity_providers
            .iter()
            .enumerate()
            .map(|(index, identity_provider)| (identity_provider.id.clone(), index))
            .collect();
        let authorization_servers = control_plane
            .authorization_servers
            .iter()
            .enumerate()
            .map(|(index, authorization_server)| (authorization_server.id.clone(), index))
            .collect();
        let profiles = control_plane
            .profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| (profile.id.clone(), index))
            .collect();
        let policies = control_plane
            .policies
            .iter()
            .enumerate()
            .map(|(index, policy)| (policy.version.clone(), index))
            .collect();
        let data_labels = control_plane
            .data_labels
            .iter()
            .enumerate()
            .map(|(index, data_label)| (data_label.id.clone(), index))
            .collect();
        let tenants = control_plane
            .tenants
            .iter()
            .enumerate()
            .map(|(index, tenant)| (tenant.id.clone(), index))
            .collect();
        let oauth_clients = control_plane
            .oauth_clients
            .iter()
            .enumerate()
            .map(|(index, client)| (client.id.clone(), index))
            .collect();
        let oidc_clients = control_plane
            .oidc_clients
            .iter()
            .enumerate()
            .map(|(index, client)| (client.id.clone(), index))
            .collect();
        let secrets = control_plane
            .secrets
            .iter()
            .enumerate()
            .map(|(index, secret)| (secret.id.clone(), index))
            .collect();
        let hub_skills = Arc::new(match &control_plane.computers {
            Some(computers) => crate::skills::HubSkills::load(&computers.skills)?,
            None => crate::skills::HubSkills::default(),
        });

        Ok(Self {
            admission,
            control_plane: Arc::new(control_plane),
            configuration_sha256,
            runtime: None,
            identity_providers,
            authorization_servers,
            registrations: Arc::default(),
            profiles,
            sections,
            policies,
            data_labels,
            tenants,
            oauth_clients,
            oidc_clients,
            secrets,
            hub_skills,
        })
    }

    /// Prepare all fallible hosted state before committing a control revision.
    /// The catalog and its routing/provisioning generation are then swapped together.
    pub async fn prepare_runtime(
        mut self,
        previous: Option<&Self>,
        public: &str,
        http: reqwest::Client,
    ) -> Result<Self> {
        if let Some(previous) = previous {
            HubCatalogHandle::check_replacement(previous, &self)?;
        }
        self.runtime = Some(Arc::new(
            crate::hosting::HostedRuntime::prepare(&self, previous, public, http).await?,
        ));
        Ok(self)
    }

    pub(crate) fn upstream_http(&self) -> Option<&crate::HubUpstreamHttpClientPool> {
        self.runtime.as_ref().map(|runtime| &runtime.upstream_http)
    }

    pub fn load_json(path: impl AsRef<Path>, admission: HubCatalogAdmission) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read hub control plane {}", path.display()))?;
        let control_plane: HubControlPlane = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse hub control plane {}", path.display()))?;
        Self::from_control_plane(control_plane, admission)
            .with_context(|| format!("invalid hub control plane {}", path.display()))
    }

    pub fn admission(&self) -> HubCatalogAdmission {
        self.admission.clone()
    }

    pub fn control_plane(&self) -> &HubControlPlane {
        &self.control_plane
    }

    pub fn configuration_sha256(&self) -> [u8; 32] {
        self.configuration_sha256
    }

    pub fn profiles(&self) -> impl Iterator<Item = &HubProfile> {
        self.control_plane.profiles.iter()
    }

    pub fn authorization_servers(&self) -> impl Iterator<Item = &ResourceAuthorizationServer> {
        self.control_plane.authorization_servers.iter()
    }

    pub fn identity_providers(&self) -> impl Iterator<Item = &IdentityProvider> {
        self.control_plane.identity_providers.iter()
    }

    pub fn server_count(&self) -> usize {
        self.registrations.len()
    }

    /// The same installation configuration routing to another set of registrations.
    pub fn with_registrations(
        &self,
        registrations: crate::registration::RegistrationSnapshot,
    ) -> Self {
        Self {
            registrations: Arc::new(registrations),
            ..self.clone()
        }
    }

    pub fn registrations(&self) -> &crate::registration::RegistrationSnapshot {
        &self.registrations
    }

    pub fn profile_count(&self) -> usize {
        self.control_plane.profiles.len()
    }

    pub fn profile(&self, profile_id: &HubProfileId) -> Option<&HubProfile> {
        self.profiles
            .get(profile_id)
            .map(|index| &self.control_plane.profiles[*index])
    }

    pub fn identity_provider(
        &self,
        identity_provider_id: &IdentityProviderId,
    ) -> Option<&IdentityProvider> {
        self.identity_providers
            .get(identity_provider_id)
            .map(|index| &self.control_plane.identity_providers[*index])
    }

    pub fn authorization_server(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Option<&ResourceAuthorizationServer> {
        self.authorization_servers
            .get(authorization_server_id)
            .map(|index| &self.control_plane.authorization_servers[*index])
    }

    pub fn authorization_server_by_issuer(
        &self,
        issuer: &str,
    ) -> Option<&ResourceAuthorizationServer> {
        self.control_plane
            .authorization_servers
            .iter()
            .find(|authorization_server| authorization_server.issuer.as_str() == issuer)
    }

    /// The installation's own skills, served to Computers' clients alone.
    pub fn hub_skills(&self) -> &crate::skills::HubSkills {
        &self.hub_skills
    }

    pub fn oauth_client(&self, client_id: &OAuthClientId) -> Option<&OAuthClientRegistration> {
        self.oauth_clients
            .get(client_id)
            .map(|index| &self.control_plane.oauth_clients[*index])
    }

    /// The tenant `principal` acts in: the one its identity names, which this revision
    /// must define. Sign-in and every request require it; it grants nothing by itself.
    pub fn principal_tenant(
        &self,
        principal: &Principal,
    ) -> Result<&TenantDefinition, HubAuthorityError> {
        let tenant = principal
            .tenant
            .as_ref()
            .ok_or_else(|| HubAuthorityError::MissingTenant(principal.id.clone()))?;
        self.tenant(tenant)
            .ok_or_else(|| HubAuthorityError::UnknownTenant {
                tenant: tenant.clone(),
                principal: principal.id.clone(),
            })
    }

    /// `principal` with the groups it belongs to in its tenant in this revision added
    /// beside its identity provider's, each with its role in it. The subject of every
    /// request carries them; a token issued to a client does not.
    pub fn with_group_memberships(&self, mut principal: Principal) -> Principal {
        if let Some(tenant) = &principal.tenant {
            for membership in self.sections.group_memberships(tenant, &principal.id) {
                principal.groups.insert(membership.group.clone());
                principal.group_roles.insert(membership.clone());
            }
        }
        principal
    }

    /// Whether `saved`, a caller as a Task saved it when it started, held a group, or a
    /// role in one, that the caller does not hold now. Its groups now are computed as a
    /// request's are: `signed_in`, the principal its session stored at sign-in with the
    /// identity provider's groups, with this revision's memberships added. Groups and
    /// roles gained since do not count.
    pub fn lost_group_or_role(&self, saved: &Principal, signed_in: Principal) -> bool {
        let current = self.with_group_memberships(signed_in);
        let role_now = |group: &veoveo_types::GroupId| {
            current
                .group_roles
                .iter()
                .filter(|membership| &membership.group == group)
                .map(|membership| membership.role)
                .max()
        };
        !saved.groups.is_subset(&current.groups)
            || saved
                .group_roles
                .iter()
                .any(|held| role_now(&held.group).is_none_or(|now| now < held.role))
    }

    pub fn resolve_authenticated_subject(
        &self,
        verified: VerifiedAccessToken,
    ) -> Result<AuthenticatedSubject, HubAuthorityError> {
        if !verified.extensions.is_empty() {
            return Err(HubAuthorityError::ContributedTokenAuthority);
        }
        let client = self
            .oauth_client(&verified.access_token.oauth_client_id)
            .ok_or_else(|| {
                HubAuthorityError::UnknownOAuthClient(verified.access_token.oauth_client_id.clone())
            })?;
        self.resolve_subject_with_client(verified, client)
    }

    pub(crate) fn resolve_subject_with_client(
        &self,
        verified: VerifiedAccessToken,
        client: &OAuthClientRegistration,
    ) -> Result<AuthenticatedSubject, HubAuthorityError> {
        let VerifiedAccessToken {
            purpose: _,
            extensions,
            access_token,
            principal,
            principal_display_name,
        } = verified;
        let principal = self.with_group_memberships(principal);
        let tenant = self.principal_tenant(&principal)?.id.clone();
        let profile = self
            .profile_by_protected_resource(access_token.audience.as_str())
            .ok_or_else(|| {
                HubAuthorityError::UnknownProtectedResource(access_token.audience.clone())
            })?;
        let policy_revision = profile.policy_version.clone();
        if access_token.invocation_mode != client.invocation_mode {
            return Err(HubAuthorityError::InvocationModeMismatch {
                client: client.id.clone(),
                expected: client.invocation_mode,
                received: access_token.invocation_mode,
            });
        }
        let (actor, provenance) = match access_token.invocation_mode {
            InvocationMode::Direct => {
                if access_token.initiator.as_ref() != Some(&principal.id)
                    || access_token.delegation_id.is_some()
                {
                    return Err(HubAuthorityError::InvalidProvenance(InvocationMode::Direct));
                }
                (
                    principal.clone(),
                    InvocationProvenance::Direct {
                        initiator: principal.id.clone(),
                    },
                )
            }
            InvocationMode::Delegated => {
                let Some(delegation_id) = access_token.delegation_id.clone() else {
                    return Err(HubAuthorityError::InvalidProvenance(
                        InvocationMode::Delegated,
                    ));
                };
                if principal.kind != PrincipalKind::User
                    || access_token.initiator.as_ref() != Some(&principal.id)
                {
                    return Err(HubAuthorityError::InvalidProvenance(
                        InvocationMode::Delegated,
                    ));
                }
                let subject = TokenSubject::parse(client.id.as_str())
                    .map_err(|_| HubAuthorityError::InvalidActorIdentity)?;
                let actor_id = PrincipalId::parse(format!("{}#{subject}", access_token.issuer))
                    .map_err(|_| HubAuthorityError::InvalidActorIdentity)?;
                (
                    Principal {
                        id: actor_id,
                        kind: PrincipalKind::Service,
                        issuer: access_token.issuer.clone(),
                        subject,
                        tenant: principal.tenant.clone(),
                        groups: BTreeSet::new(),
                        group_roles: BTreeSet::new(),
                        roles: BTreeSet::new(),
                        scopes: principal.scopes.clone(),
                        data_labels: principal.data_labels.clone(),
                        assurances: principal.assurances.clone(),
                        authenticated_at: principal.authenticated_at,
                    },
                    InvocationProvenance::Delegated {
                        initiator: principal.id.clone(),
                        delegation_id,
                    },
                )
            }
            InvocationMode::Automated => {
                if principal.kind != PrincipalKind::Service
                    || principal.subject.as_str() != client.id.as_str()
                    || access_token.initiator.is_some()
                    || access_token.delegation_id.is_some()
                {
                    return Err(HubAuthorityError::InvalidProvenance(
                        InvocationMode::Automated,
                    ));
                }
                (principal.clone(), InvocationProvenance::Automated)
            }
        };
        Ok(AuthenticatedSubject {
            execution: None,
            extensions,
            audit: crate::request_observation::RequestObservation::current()
                .map(|request| request.audit)
                .unwrap_or_else(veoveo_audit_contract::AuditRequest::background),
            access_token,
            principal,
            actor,
            principal_display_name,
            authority: InvocationAuthority {
                tenant,
                policy_revision,
                provenance,
            },
            delegation: None,
        })
    }

    pub fn profile_oauth_clients(&self, profile: &HubProfile) -> Vec<&OAuthClientRegistration> {
        self.control_plane
            .oauth_clients
            .iter()
            .filter(|client| {
                client.authorization_server == profile.authorization_server
                    && client
                        .allowed_resources
                        .contains(&profile.protected_resource)
            })
            .collect()
    }

    pub fn authorization_server_oauth_clients(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Vec<&OAuthClientRegistration> {
        self.control_plane
            .oauth_clients
            .iter()
            .filter(|client| &client.authorization_server == authorization_server_id)
            .collect()
    }

    pub fn oidc_client(
        &self,
        client_id: &OidcClientRegistrationId,
    ) -> Option<&crate::control_plane::IdentityProviderOidcClientRegistration> {
        self.oidc_clients
            .get(client_id)
            .map(|index| &self.control_plane.oidc_clients[*index])
    }

    pub fn profile_oidc_client(
        &self,
        profile: &HubProfile,
    ) -> Option<&crate::control_plane::IdentityProviderOidcClientRegistration> {
        self.control_plane.oidc_clients.iter().find(|client| {
            client.identity_provider == profile.identity_provider
                && client.authorization_server == profile.authorization_server
                && client
                    .allowed_resources
                    .contains(&profile.protected_resource)
        })
    }

    pub fn profile_by_protected_resource(&self, resource: &str) -> Option<&HubProfile> {
        self.control_plane
            .profiles
            .iter()
            .find(|profile| profile.protected_resource.as_str() == resource)
    }

    pub fn client_single_profile(&self, client: &OAuthClientRegistration) -> Option<&HubProfile> {
        if client.allowed_resources.len() != 1 {
            return None;
        }
        client.allowed_resources.iter().next().and_then(|resource| {
            self.control_plane
                .profiles
                .iter()
                .find(|profile| &profile.protected_resource == resource)
        })
    }

    pub fn registry(&self) -> &crate::contract::CatalogRegistry {
        self.admission
            .registry()
            .expect("admitted catalog has a bound registry")
    }
    pub fn sections(&self) -> &crate::contract::AdmittedCatalogSections {
        &self.sections
    }
    pub fn contributed_protected_resource(
        &self,
        resource: &veoveo_hub_sdk::ProtectedResourceId,
    ) -> Option<&crate::contract::ProtectedResourceDescriptor> {
        self.sections
            .protected_resources()
            .iter()
            .find(|candidate| &candidate.resource == resource)
    }

    pub fn authorization_server_profiles(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Vec<&HubProfile> {
        self.control_plane
            .profiles
            .iter()
            .filter(|profile| &profile.authorization_server == authorization_server_id)
            .collect()
    }

    pub fn secret_reference(&self, secret_id: &SecretReferenceId) -> Option<&SecretReference> {
        self.secrets
            .get(secret_id)
            .map(|index| &self.control_plane.secrets[*index])
    }

    pub fn server(&self, server_slug: &ServerSlug) -> Option<&RegisteredServer> {
        self.registrations.get(server_slug)
    }

    /// The grant `principal` holds on the source repository `repository`, own or through
    /// a group, as the installation's sections record it.
    pub fn repository_grant(
        &self,
        principal: &Principal,
        repository: &veoveo_hub_sdk::registration::RepositoryId,
    ) -> Option<veoveo_hub_sdk::registration::RepositoryGrant> {
        let tenant = principal.tenant.as_ref()?;
        self.sections
            .repository_grant(tenant, &principal.id, repository)
    }

    /// Whether `principal` reaches `server`: every server that is no runner, and a runner
    /// when the principal holds a grant on its linked repository.
    pub fn reaches(&self, principal: &Principal, server: &RegisteredServer) -> bool {
        server.runner.as_ref().is_none_or(|runner| {
            self.repository_grant(principal, &runner.repository)
                .is_some()
        })
    }

    /// The registered servers `principal` reaches, when the profile exists: the runners
    /// of a linked server by link.
    pub fn reachable_servers(
        &self,
        profile_id: &HubProfileId,
        principal: &Principal,
    ) -> Vec<&RegisteredServer> {
        self.profile_servers(profile_id)
            .into_iter()
            .filter(|server| self.reaches(principal, server))
            .collect()
    }

    /// The deployment that serves `server` for `link` to `principal`: the server for no
    /// link, the runner of `link` when the principal reaches it, and none otherwise. A
    /// link the principal holds no grant on is no target, as one the branch lacks.
    pub fn routed_server(
        &self,
        principal: &Principal,
        server: &ServerSlug,
        link: Option<&veoveo_hub_sdk::hub::LinkName>,
    ) -> Option<&RegisteredServer> {
        match link {
            None => self.server(server).filter(|server| server.runner.is_none()),
            Some(link) => self
                .registrations
                .runners(server)
                .find(|runner| runner.link() == Some(link))
                .filter(|runner| self.reaches(principal, runner)),
        }
    }

    /// The deployment a discovery of `server` for `link` lists, when the profile exists:
    /// the server for no link and the runner of `link` otherwise, whoever asks.
    pub fn keyed_server(
        &self,
        profile_id: &HubProfileId,
        server: &ServerSlug,
        link: Option<&veoveo_hub_sdk::hub::LinkName>,
    ) -> Option<&RegisteredServer> {
        self.profile(profile_id)?;
        match link {
            None => self.server(server),
            Some(link) => self
                .registrations
                .runners(server)
                .find(|runner| runner.link() == Some(link)),
        }
    }

    /// The runner of a linked server that serves what all its runners share, its Apps and
    /// prompts, to `principal`: the first by link it reaches.
    pub fn shared_runner(
        &self,
        principal: &Principal,
        server: &ServerSlug,
    ) -> Option<&RegisteredServer> {
        let mut runners = self
            .registrations
            .runners(server)
            .filter(|runner| self.reaches(principal, runner))
            .collect::<Vec<_>>();
        runners.sort_by(|left, right| left.link().cmp(&right.link()));
        runners.into_iter().next()
    }

    /// A registered server, when the profile exists. Every registered server is visible
    /// to every profile; permissions decide what a caller may use.
    pub fn profile_server(
        &self,
        profile_id: &HubProfileId,
        server_slug: &ServerSlug,
    ) -> Option<(&HubProfile, &RegisteredServer)> {
        let profile = self.profile(profile_id)?;
        let server = self.server(server_slug)?;
        Some((profile, server))
    }

    pub fn profile_servers(&self, profile_id: &HubProfileId) -> Vec<&RegisteredServer> {
        if self.profile(profile_id).is_none() {
            return Vec::new();
        }
        self.registrations.iter().collect()
    }

    /// The server whose URI scheme, or whose `ui://<slug>/` App prefix or
    /// `skill://<slug>/` skills, owns `uri`, as it serves `principal`. A linked server's URIs name the link after the scheme,
    /// `{slug}://{link}/{rest}`, and reach that link's runner when the principal reaches
    /// it; its Apps are one set, which the runner the principal reaches first serves.
    pub fn server_for_resource_uri(
        &self,
        profile_id: &HubProfileId,
        principal: &Principal,
        uri: &str,
    ) -> ResourceRoute<'_> {
        let Some(scheme) = resource_scheme(uri) else {
            return ResourceRoute::Unowned;
        };
        let owners = self
            .profile_servers(profile_id)
            .into_iter()
            .filter(|server| server_owns_hub_resource_uri(server, uri, &scheme))
            .collect::<Vec<_>>();
        let Some(&owner) = owners.first() else {
            return ResourceRoute::Unowned;
        };
        if owner.runner.is_none() {
            return ResourceRoute::Server(owner);
        }
        // A linked server's Apps and skills are one set, which the runner the principal
        // reaches first serves.
        let routed = if shares_runners(&scheme) {
            self.shared_runner(principal, &owner.slug)
        } else {
            uri.strip_prefix(&format!("{scheme}://"))
                .and_then(|rest| rest.split_once('/'))
                .and_then(|(link, _)| veoveo_hub_sdk::hub::LinkName::parse(link).ok())
                .and_then(|link| self.routed_server(principal, &owner.slug, Some(&link)))
        };
        match routed {
            Some(server) => ResourceRoute::Server(server),
            None => ResourceRoute::Unreached(&owner.slug),
        }
    }

    /// Whether a registered server's URI scheme, `ui://<slug>/` App prefix or
    /// `skill://<slug>/` skills own `uri`.
    pub fn owns_resource_uri(&self, profile_id: &HubProfileId, uri: &str) -> bool {
        resource_scheme(uri).is_some_and(|scheme| {
            self.profile_servers(profile_id)
                .into_iter()
                .any(|server| server_owns_hub_resource_uri(server, uri, &scheme))
        })
    }

    /// The servers that declare `prompt`, one for each slug: a linked server's prompts
    /// are one set, which the runner `principal` reaches first serves.
    pub fn prompt_servers(
        &self,
        profile_id: &HubProfileId,
        principal: &Principal,
        prompt: &veoveo_hub_sdk::PromptName,
    ) -> Vec<&RegisteredServer> {
        let mut slugs = BTreeMap::<&ServerSlug, &RegisteredServer>::new();
        for server in self
            .profile_servers(profile_id)
            .into_iter()
            .filter(|server| {
                server
                    .authorization
                    .requirements
                    .prompts
                    .contains_key(prompt)
            })
        {
            let serving = match server.runner {
                None => Some(server),
                Some(_) => self.shared_runner(principal, &server.slug),
            };
            if let Some(serving) = serving {
                slugs.entry(&server.slug).or_insert(serving);
            }
        }
        slugs.into_values().collect()
    }

    pub fn policy(&self, version: &PolicyVersion) -> Option<&PolicySet> {
        self.policies
            .get(version)
            .map(|index| &self.control_plane.policies[*index])
    }

    pub fn data_label(&self, label: &DataLabelId) -> Option<&DataLabelDefinition> {
        self.data_labels
            .get(label)
            .map(|index| &self.control_plane.data_labels[*index])
    }

    pub fn tenant(&self, tenant: &TenantId) -> Option<&TenantDefinition> {
        self.tenants
            .get(tenant)
            .map(|index| &self.control_plane.tenants[*index])
    }
}

/// Where a resource URI routes for one principal.
#[derive(Debug, Clone, Copy)]
pub enum ResourceRoute<'a> {
    /// The server, or the runner, that serves it.
    Server(&'a RegisteredServer),
    /// A linked server's URI that names a link the principal holds no grant on, or no
    /// link of the selected branch: a target the server did not declare.
    Unreached(&'a ServerSlug),
    /// No registered server owns it.
    Unowned,
}

fn server_owns_hub_resource_uri(
    server: &RegisteredServer,
    uri: &str,
    scheme: &veoveo_types::ResourceScheme,
) -> bool {
    // A skill's owner is the server its authority names, whatever server's scheme is
    // `skill`; the Hub's own `skill://veoveo/` is no server's.
    if scheme.as_str() == veoveo_hub_sdk::skills::SCHEME {
        return veoveo_hub_sdk::skills::owned_by(uri, &server.slug);
    }
    server.uri_scheme == *scheme
        || (scheme.as_str() == "ui" && uri.starts_with(&format!("ui://{}/", server.slug.as_str())))
}

/// Whether URIs of `scheme` are one set for a linked server, served by one of its runners
/// unchanged: its Apps' and its skills'.
pub(crate) fn shares_runners(scheme: &veoveo_types::ResourceScheme) -> bool {
    matches!(scheme.as_str(), "ui" | veoveo_hub_sdk::skills::SCHEME)
}

#[cfg(test)]
mod tests;
