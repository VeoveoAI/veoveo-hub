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
    WorkContextDefinition,
};
use anyhow::{Context, Result};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use veoveo_hub_sdk::{
    HubProfileId, OAuthClientId, Principal, PrincipalKind, ServerSlug, TokenSubject,
};
use veoveo_types::{
    DataLabelId, InvocationMode, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel};

use crate::policy::resource_scheme;
use crate::registration::RegisteredServer;
use crate::{AuthenticatedSubject, HubCatalogAdmission, VerifiedAccessToken};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubAuthorityError {
    ContributedTokenAuthority,
    UnknownOAuthClient(OAuthClientId),
    UnknownWorkContext(WorkContextId),
    MissingTenant(PrincipalId),
    TenantMismatch {
        context: WorkContextId,
        principal: PrincipalId,
    },
    InvocationModeMismatch {
        client: OAuthClientId,
        expected: InvocationMode,
        received: InvocationMode,
    },
    InvalidProvenance(InvocationMode),
    MembershipDenied {
        context: WorkContextId,
        principal: PrincipalId,
    },
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
            Self::UnknownWorkContext(context) => {
                write!(formatter, "unknown Work Context `{context}`")
            }
            Self::MissingTenant(principal) => {
                write!(formatter, "principal `{principal}` has no tenant")
            }
            Self::TenantMismatch { context, principal } => write!(
                formatter,
                "Work Context `{context}` and principal `{principal}` belong to different tenants"
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
            Self::MembershipDenied { context, principal } => write!(
                formatter,
                "principal `{principal}` has no membership in Work Context `{context}`"
            ),
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

    /// Check startup-fixed configuration before publishing a catalog revision.
    pub fn validate_replacement(&self, catalog: &HubCatalog) -> Result<()> {
        Self::check_replacement(&self.state.read().catalog, catalog)
    }

    fn check_replacement(current: &HubCatalog, replacement: &HubCatalog) -> Result<()> {
        anyhow::ensure!(
            current.admission.same_binding(&replacement.admission),
            "catalog reload changed its admission binding"
        );
        anyhow::ensure!(
            current.control_plane.native_upstreams == replacement.control_plane.native_upstreams
                && current.control_plane.computers == replacement.control_plane.computers,
            "native upstream and Computers configuration is fixed at startup; restart the Hub to change it"
        );
        Ok(())
    }

    /// Replace the installation configuration, keeping the registered servers.
    pub fn replace(&self, catalog: Arc<HubCatalog>) -> Result<()> {
        let mut state = self.state.write();
        Self::check_replacement(&state.catalog, &catalog)?;
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
}

#[derive(Debug, Clone)]
pub struct HubCatalog {
    admission: HubCatalogAdmission,
    control_plane: Arc<HubControlPlane>,
    configuration_sha256: [u8; 32],
    identity_providers: BTreeMap<IdentityProviderId, usize>,
    authorization_servers: BTreeMap<AuthorizationServerId, usize>,
    registrations: Arc<crate::registration::RegistrationSnapshot>,
    profiles: BTreeMap<HubProfileId, usize>,
    sections: crate::contract::AdmittedCatalogSections,
    policies: BTreeMap<PolicyVersion, usize>,
    data_labels: BTreeMap<DataLabelId, usize>,
    tenants: BTreeMap<TenantId, usize>,
    work_contexts: BTreeMap<WorkContextId, usize>,
    oauth_clients: BTreeMap<OAuthClientId, usize>,
    oidc_clients: BTreeMap<OidcClientRegistrationId, usize>,
    secrets: BTreeMap<SecretReferenceId, usize>,
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
        let work_contexts = control_plane
            .work_contexts
            .iter()
            .enumerate()
            .map(|(index, context)| (context.id.clone(), index))
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

        Ok(Self {
            admission,
            control_plane: Arc::new(control_plane),
            configuration_sha256,
            identity_providers,
            authorization_servers,
            registrations: Arc::default(),
            profiles,
            sections,
            policies,
            data_labels,
            tenants,
            work_contexts,
            oauth_clients,
            oidc_clients,
            secrets,
        })
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

    pub fn oauth_client(&self, client_id: &OAuthClientId) -> Option<&OAuthClientRegistration> {
        self.oauth_clients
            .get(client_id)
            .map(|index| &self.control_plane.oauth_clients[*index])
    }

    pub fn work_context(&self, context_id: &WorkContextId) -> Option<&WorkContextDefinition> {
        self.work_contexts
            .get(context_id)
            .map(|index| &self.control_plane.work_contexts[*index])
    }

    pub fn work_contexts(&self) -> impl Iterator<Item = &WorkContextDefinition> {
        self.control_plane.work_contexts.iter()
    }

    pub fn work_context_membership(
        &self,
        client_id: &OAuthClientId,
        context_id: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel, HubAuthorityError> {
        let client = self
            .oauth_client(client_id)
            .ok_or_else(|| HubAuthorityError::UnknownOAuthClient(client_id.clone()))?;
        let context = self
            .work_context(context_id)
            .ok_or_else(|| HubAuthorityError::UnknownWorkContext(context_id.clone()))?;
        let tenant = principal
            .tenant
            .as_ref()
            .ok_or_else(|| HubAuthorityError::MissingTenant(principal.id.clone()))?;
        if tenant != &context.tenant {
            return Err(HubAuthorityError::TenantMismatch {
                context: context.id.clone(),
                principal: principal.id.clone(),
            });
        }
        context
            .membership_for(principal, &client.id)
            .ok_or_else(|| HubAuthorityError::MembershipDenied {
                context: context.id.clone(),
                principal: principal.id.clone(),
            })
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
        let membership = self.work_context_membership(
            &verified.access_token.oauth_client_id,
            &verified.access_token.work_context,
            &verified.principal,
        )?;
        self.resolve_subject_with_client(verified, client, membership)
    }

    pub(crate) fn resolve_subject_with_client(
        &self,
        verified: VerifiedAccessToken,
        client: &OAuthClientRegistration,
        membership: WorkContextMembershipLevel,
    ) -> Result<AuthenticatedSubject, HubAuthorityError> {
        let VerifiedAccessToken {
            purpose: _,
            extensions,
            access_token,
            principal,
            principal_display_name,
        } = verified;
        let context = self
            .work_context(&access_token.work_context)
            .ok_or_else(|| {
                HubAuthorityError::UnknownWorkContext(access_token.work_context.clone())
            })?;
        if access_token.invocation_mode != client.invocation_mode {
            return Err(HubAuthorityError::InvocationModeMismatch {
                client: client.id.clone(),
                expected: client.invocation_mode,
                received: access_token.invocation_mode,
            });
        }
        if principal.tenant.as_ref() != Some(&context.tenant) {
            return Err(HubAuthorityError::TenantMismatch {
                context: context.id.clone(),
                principal: principal.id.clone(),
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
            extensions,
            audit: crate::request_observation::RequestObservation::current()
                .map(|request| request.audit)
                .unwrap_or_else(veoveo_audit_contract::AuditRequest::background),
            access_token,
            principal,
            actor,
            principal_display_name,
            authority: InvocationAuthority {
                work_context: context.id.clone(),
                tenant: context.tenant.clone(),
                membership,
                policy_revision: context.policy_revision.clone(),
                output_policy: context.output_policy.clone(),
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

    /// The server whose URI scheme, or whose `ui://<slug>/` App prefix, owns `uri`.
    pub fn server_for_resource_uri(
        &self,
        profile_id: &HubProfileId,
        uri: &str,
    ) -> Option<&RegisteredServer> {
        let scheme = resource_scheme(uri)?;
        self.profile_servers(profile_id)
            .into_iter()
            .find(|server| server_owns_hub_resource_uri(server, uri, &scheme))
    }

    pub fn prompt_servers(
        &self,
        profile_id: &HubProfileId,
        prompt: &veoveo_hub_sdk::PromptName,
    ) -> Vec<&RegisteredServer> {
        self.profile_servers(profile_id)
            .into_iter()
            .filter(|server| {
                server
                    .authorization
                    .requirements
                    .prompts
                    .contains_key(prompt)
            })
            .collect()
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

fn server_owns_hub_resource_uri(
    server: &RegisteredServer,
    uri: &str,
    scheme: &veoveo_types::ResourceScheme,
) -> bool {
    server.uri_scheme == *scheme
        || (scheme.as_str() == "ui" && uri.starts_with(&format!("ui://{}/", server.slug.as_str())))
}

#[cfg(test)]
mod tests;
