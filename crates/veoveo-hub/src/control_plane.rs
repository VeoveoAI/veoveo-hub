use crate::contract::{
    AuthorizationServerId, CertificateAuthoritySource, OAuthAuthorizationCode, OAuthStateValue,
    OidcClientRegistrationId, OidcNonce, PkceCodeChallenge, PkceCodeVerifier, PrincipalDisplayName,
    ProtectedResourceName, SecretOwner, SecretPurpose, SecretReference, SecretReferenceId,
};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_hub_sdk::{
    CanonicalTaskId, HubProfileId, HubRefreshFamilyId, JwtId, LocalToolName, OAuthClientId,
    Principal, PromptName, ProtectedResourceId, ServerSlug, TokenIssuer,
};
use veoveo_types::{
    DataLabelId, GroupId, IdentifierError, PolicyVersion, PrincipalId, ResourceUri, RoleId,
    ScopeName, TenantId, WorkContextId,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use validation::{
    validate_oauth_client_registration, validate_oidc_client_registration, validate_policy_set,
    validate_profile_auth_modes,
};
use wire::{validate_https_url, validate_local_file_path, validate_oauth_endpoint_url};

pub const MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION: &str =
    "io.modelcontextprotocol/enterprise-managed-authorization";
pub const MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION: &str =
    "io.modelcontextprotocol/oauth-client-credentials";
mod computers;
pub use computers::*;
mod native_upstreams;
pub use native_upstreams::*;
mod authorization_config;
pub use authorization_config::*;
mod policy;
use crate::contract::HubAction;
mod validation;
mod wire;
pub use policy::*;
mod runtime_state;
pub use runtime_state::*;
mod server_config;
pub use server_config::*;
mod auth_config;
pub use auth_config::*;
mod ids;
pub use ids::*;
mod data_label;
pub use data_label::*;
mod tenant;
pub use tenant::*;
mod branding;
pub use branding::*;
mod catalog_schema;
mod catalog_wire;
pub mod work_context;
pub use catalog_schema::composed_hub_schema;
pub use work_context::{WorkContextDefinition, WorkContextMembershipRule};
pub const HUB_CORE_FIELDS: &[&str] = &[
    "branding",
    "identityProviders",
    "authorizationServers",
    "profiles",
    "authorization",
    "registration",
    "tenants",
    "workContexts",
    "policies",
    "dataLabels",
    "oauthClients",
    "oidcClients",
    "secrets",
    "nativeUpstreams",
    "computers",
    "execution",
    "metadata",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HubControlPlane {
    /// Trusted admitted routing outputs; source manifests remain owned by source adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<crate::execution::ExecutionConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branding: Option<InstallationBranding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_upstreams: Option<NativeUpstreams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computers: Option<Computers>,
    pub identity_providers: Vec<IdentityProvider>,
    pub authorization_servers: Vec<ResourceAuthorizationServer>,
    pub profiles: Vec<HubProfile>,
    /// Roles, grants and overrides over registered servers' declared permissions.
    #[serde(default)]
    pub authorization: InstallationAuthorization,
    /// Required for a Hub that admits server registrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<RegistrationSettings>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
    pub tenants: Vec<TenantDefinition>,
    pub work_contexts: Vec<crate::control_plane::WorkContextDefinition>,
    pub policies: Vec<PolicySet>,
    pub data_labels: Vec<DataLabelDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_clients: Vec<OAuthClientRegistration>,
    pub oidc_clients: Vec<IdentityProviderOidcClientRegistration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretReference>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HubControlPlaneRevision {
    pub revision_id: HubControlPlaneRevisionId,
    pub sha256: String,
    pub source: HubControlPlaneRevisionSource,
    pub applied_at: DateTime<Utc>,
    pub applied_by: PrincipalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    pub control_plane: HubControlPlane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HubControlPlaneRevisionSource {
    AdminApi,
    SeedFile,
    /// Authenticated executable contribution, with its actual caller audit actor.
    Contribution,
}

impl HubControlPlane {
    /// Public files that must be mounted beside this control-plane document.
    #[must_use]
    pub fn jwks_file_paths(&self) -> BTreeSet<&str> {
        self.identity_providers
            .iter()
            .map(|provider| &provider.jwks)
            .chain(self.authorization_servers.iter().map(|server| &server.jwks))
            .chain(
                self.oauth_clients
                    .iter()
                    .filter_map(|client| client.jwks.as_ref()),
            )
            .filter_map(|source| match source {
                JwksSource::File { path } => Some(path.as_str()),
                JwksSource::Remote { .. } => None,
            })
            .collect()
    }

    /// Public CA bundles that must be mounted beside this control-plane document.
    #[must_use]
    pub fn certificate_authority_file_paths(&self) -> BTreeSet<&str> {
        self.identity_providers
            .iter()
            .flat_map(|provider| provider.trusted_certificate_authorities.iter())
            .map(|source| match source {
                CertificateAuthoritySource::File { path } => path.as_str(),
            })
            .collect()
    }

    pub fn validate(
        &self,
        registry: &crate::contract::CatalogRegistry,
    ) -> Result<crate::contract::AdmittedCatalogSections, HubControlPlaneError> {
        if let Some(execution) = &self.execution {
            execution
                .validate()
                .map_err(HubControlPlaneError::InvalidExecution)?;
        }
        registry
            .check_reserved(
                HUB_CORE_FIELDS,
                &[
                    "hub",
                    "server",
                    "tool",
                    "resource",
                    "resource_template",
                    "prompt",
                    "task",
                    "platform_task",
                ],
            )
            .map_err(HubControlPlaneError::CatalogAdmission)?;
        if self
            .extensions
            .keys()
            .any(|key| HUB_CORE_FIELDS.contains(&key.as_str()))
        {
            return Err(HubControlPlaneError::CatalogAdmission(
                veoveo_types::ExtensionError::new("catalog section collides with a core field"),
            ));
        }
        registry
            .action_key::<HubAction>()
            .map_err(HubControlPlaneError::CatalogAdmission)?;
        let facts = self.catalog_facts();
        let sections = registry
            .admit_sections(&self.extensions, &facts)
            .map_err(HubControlPlaneError::CatalogAdmission)?;
        if let Some(branding) = &self.branding {
            branding.validate()?;
        }

        let mut identity_providers = BTreeMap::new();
        for identity_provider in &self.identity_providers {
            if identity_providers
                .insert(identity_provider.id.clone(), identity_provider)
                .is_some()
            {
                return Err(HubControlPlaneError::DuplicateIdentityProvider(
                    identity_provider.id.clone(),
                ));
            }
        }

        let mut authorization_servers = BTreeMap::new();
        for authorization_server in &self.authorization_servers {
            if authorization_servers
                .insert(authorization_server.id.clone(), authorization_server)
                .is_some()
            {
                return Err(HubControlPlaneError::DuplicateAuthorizationServer(
                    authorization_server.id.clone(),
                ));
            }
            if let Some(identity_provider) = &authorization_server.identity_provider
                && !identity_providers.contains_key(identity_provider)
            {
                return Err(
                    HubControlPlaneError::UnknownAuthorizationServerIdentityProvider {
                        authorization_server: authorization_server.id.clone(),
                        identity_provider: identity_provider.clone(),
                    },
                );
            }
        }

        let mut policies = BTreeSet::new();
        let mut policy_by_id = BTreeMap::new();
        for policy in &self.policies {
            if !policies.insert(policy.version.clone()) {
                return Err(HubControlPlaneError::DuplicatePolicy(
                    policy.version.clone(),
                ));
            }
            policy_by_id.insert(policy.version.clone(), policy);
        }

        let mut data_labels = BTreeSet::new();
        for data_label in &self.data_labels {
            if !data_labels.insert(data_label.id.clone()) {
                return Err(HubControlPlaneError::DuplicateDataLabel(
                    data_label.id.clone(),
                ));
            }
        }
        let mut tenants = BTreeSet::new();
        for tenant in &self.tenants {
            if !tenants.insert(tenant.id.clone()) {
                return Err(HubControlPlaneError::DuplicateTenant(tenant.id.clone()));
            }
        }
        if let Some(execution) = &self.execution {
            for (environment, record) in &execution.environments {
                if !tenants.contains(&record.owner.tenant) {
                    return Err(HubControlPlaneError::UnknownEnvironmentTenant {
                        environment: *environment,
                        tenant: record.owner.tenant.clone(),
                    });
                }
            }
        }
        for identity_provider in &self.identity_providers {
            if let Some(mapping) = &identity_provider.claim_mapping.tenant {
                for tenant in mapping.values.values() {
                    if !tenants.contains(tenant) {
                        return Err(HubControlPlaneError::UnknownIdentityProviderMappedTenant {
                            identity_provider: identity_provider.id.clone(),
                            tenant: tenant.clone(),
                        });
                    }
                }
            }
        }

        let mut work_contexts = BTreeMap::new();
        for context in &self.work_contexts {
            if work_contexts.insert(context.id.clone(), context).is_some() {
                return Err(HubControlPlaneError::DuplicateWorkContext(
                    context.id.clone(),
                ));
            }
            if !tenants.contains(&context.tenant) {
                return Err(HubControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!("references unknown tenant `{}`", context.tenant),
                });
            }
            if !policies.contains(&context.policy_revision) {
                return Err(HubControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!(
                        "references unknown policy revision `{}`",
                        context.policy_revision
                    ),
                });
            }
            if context.title.trim().is_empty() {
                return Err(HubControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: "title must not be empty".to_owned(),
                });
            }
            if context.memberships.is_empty()
                || context
                    .memberships
                    .iter()
                    .any(|membership| !membership.has_selector())
            {
                return Err(HubControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: "each context requires at least one membership rule with a selector"
                        .to_owned(),
                });
            }
            if let Some(label) = context
                .output_policy
                .data_labels
                .iter()
                .chain(context.output_policy.classification.iter())
                .find(|label| !data_labels.contains(*label))
            {
                return Err(HubControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!("output policy references unknown data label `{label}`"),
                });
            }
        }

        let mut profiles = BTreeSet::new();
        let mut profile_by_id = BTreeMap::new();
        let mut protected_resources = BTreeSet::new();
        for profile in &self.profiles {
            if !profiles.insert(profile.id.clone()) {
                return Err(HubControlPlaneError::DuplicateProfile(profile.id.clone()));
            }
            profile_by_id.insert(profile.id.clone(), profile);
            if !protected_resources.insert(profile.protected_resource.clone()) {
                return Err(HubControlPlaneError::DuplicateProtectedResource(
                    profile.protected_resource.clone(),
                ));
            }
            let Some(identity_provider) = identity_providers.get(&profile.identity_provider) else {
                return Err(HubControlPlaneError::UnknownIdentityProvider {
                    profile: profile.id.clone(),
                    identity_provider: profile.identity_provider.clone(),
                });
            };
            let Some(authorization_server) =
                authorization_servers.get(&profile.authorization_server)
            else {
                return Err(HubControlPlaneError::UnknownAuthorizationServer {
                    profile: profile.id.clone(),
                    authorization_server: profile.authorization_server.clone(),
                });
            };
            if !policies.contains(&profile.policy_version) {
                return Err(HubControlPlaneError::UnknownPolicy {
                    profile: profile.id.clone(),
                    policy_version: profile.policy_version.clone(),
                });
            }
            validate_profile_auth_modes(profile, identity_provider, authorization_server)?;
        }

        let mut contributed_resources = BTreeMap::new();
        for resource in sections.protected_resources() {
            if contributed_resources
                .insert(resource.name.clone(), resource)
                .is_some()
                || !protected_resources.insert(resource.resource.clone())
            {
                return Err(HubControlPlaneError::DuplicateProtectedResource(
                    resource.resource.clone(),
                ));
            }
            if !authorization_servers.contains_key(&resource.authorization_server)
                || !policies.contains(&resource.policy_version)
            {
                return Err(HubControlPlaneError::CatalogAdmission(
                    veoveo_types::ExtensionError::new(
                        "contributed protected resource references unknown authorization server or policy",
                    ),
                ));
            }
        }

        for policy in &self.policies {
            validate_policy_set(
                policy,
                validation::PolicyValidation {
                    profiles: &profiles,
                    protected_resources: &protected_resources,
                    data_labels: &data_labels,
                    tenants: &tenants,
                    registry,
                },
            )?;
        }

        let mut secrets = BTreeSet::new();
        let mut secret_refs = BTreeMap::new();
        for secret in &self.secrets {
            if !secrets.insert(secret.id.clone()) {
                return Err(HubControlPlaneError::DuplicateSecret(secret.id.clone()));
            }
            secret_refs.insert(secret.id.clone(), secret);
            match &secret.owner {
                SecretOwner::Hub => {}
                SecretOwner::Tenant { tenant } => {
                    if !tenants.contains(tenant) {
                        return Err(HubControlPlaneError::UnknownSecretOwnerTenant {
                            secret: secret.id.clone(),
                            tenant: tenant.clone(),
                        });
                    }
                }
                SecretOwner::Profile { profile } => {
                    if !profiles.contains(profile) {
                        return Err(HubControlPlaneError::UnknownSecretOwnerProfile {
                            secret: secret.id.clone(),
                            profile: profile.clone(),
                        });
                    }
                }
            }
        }

        for authorization_server in &self.authorization_servers {
            if authorization_server.access_token_key_id.is_some()
                != authorization_server.access_token_signing_key.is_some()
            {
                return Err(
                    HubControlPlaneError::InvalidAuthorizationServerSigningConfiguration(
                        authorization_server.id.clone(),
                    ),
                );
            }
            let Some(signing_key) = &authorization_server.access_token_signing_key else {
                continue;
            };
            let Some(secret) = secret_refs.get(signing_key) else {
                return Err(HubControlPlaneError::UnknownAuthorizationServerSigningKey {
                    authorization_server: authorization_server.id.clone(),
                    secret: signing_key.clone(),
                });
            };
            if secret.purpose != SecretPurpose::JwksPrivateKey {
                return Err(
                    HubControlPlaneError::AuthorizationServerSigningKeyPurposeMismatch {
                        authorization_server: authorization_server.id.clone(),
                        secret: signing_key.clone(),
                        purpose: secret.purpose,
                    },
                );
            }
        }

        let mut oidc_clients = BTreeSet::new();
        for client in &self.oidc_clients {
            if !oidc_clients.insert(client.id.clone()) {
                return Err(HubControlPlaneError::DuplicateOidcClient(client.id.clone()));
            }
            validate_oidc_client_registration(
                client,
                &identity_providers,
                &authorization_servers,
                &profile_by_id,
                &contributed_resources,
                &secret_refs,
            )?;
        }

        let mut oauth_clients = BTreeSet::new();
        for client in &self.oauth_clients {
            if !oauth_clients.insert(client.id.clone()) {
                return Err(HubControlPlaneError::DuplicateOAuthClient(
                    client.id.clone(),
                ));
            }
            validate_oauth_client_registration(
                client,
                &authorization_servers,
                &profile_by_id,
                &contributed_resources,
                &policy_by_id,
                &secret_refs,
            )?;
            let Some(context) = work_contexts.get(&client.default_work_context) else {
                return Err(HubControlPlaneError::UnknownOAuthClientWorkContext {
                    client: client.id.clone(),
                    context: client.default_work_context.clone(),
                });
            };
            if client
                .tenant
                .as_ref()
                .is_some_and(|tenant| tenant != &context.tenant)
            {
                return Err(HubControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!("OAuth client `{}` belongs to a different tenant", client.id),
                });
            }
            let mode_matches = match client.invocation_mode {
                veoveo_types::InvocationMode::Direct => client.grant_types.iter().any(|grant| {
                    matches!(
                        grant,
                        OAuthGrantType::AuthorizationCodePkce | OAuthGrantType::DeviceAuthorization
                    )
                }),
                veoveo_types::InvocationMode::Delegated => client
                    .grant_types
                    .contains(&OAuthGrantType::EnterpriseManagedAuthorization),
                veoveo_types::InvocationMode::Automated => client
                    .grant_types
                    .contains(&OAuthGrantType::ClientCredentials),
            };
            if !mode_matches {
                return Err(HubControlPlaneError::OAuthClientInvocationModeMismatch {
                    client: client.id.clone(),
                    mode: client.invocation_mode,
                });
            }
        }
        for profile in &self.profiles {
            for auth_mode in &profile.auth_modes {
                // Managed service clients are admitted durably after installation.
                // A profile may start empty; each request still requires an effective
                // registration bound to its issuer, resource, scopes and authority.
                if *auth_mode == AuthMode::OAuthClientCredentials {
                    continue;
                }
                let required_grant = OAuthGrantType::from(*auth_mode);
                let has_client = self.oauth_clients.iter().any(|client| {
                    client.authorization_server == profile.authorization_server
                        && client
                            .allowed_resources
                            .contains(&profile.protected_resource)
                        && client.grant_types.contains(&required_grant)
                });
                if !has_client {
                    return Err(HubControlPlaneError::MissingOAuthClientForAuthMode {
                        profile: profile.id.clone(),
                        auth_mode: *auth_mode,
                    });
                }
            }
            if profile
                .auth_modes
                .contains(&AuthMode::OidcAuthorizationCodePkce)
            {
                let has_oidc_client = self.oidc_clients.iter().any(|client| {
                    client.identity_provider == profile.identity_provider
                        && client.authorization_server == profile.authorization_server
                        && client
                            .allowed_resources
                            .contains(&profile.protected_resource)
                });
                if !has_oidc_client {
                    return Err(HubControlPlaneError::MissingOidcClientForProfile {
                        profile: profile.id.clone(),
                        identity_provider: profile.identity_provider.clone(),
                        authorization_server: profile.authorization_server.clone(),
                    });
                }
            }
        }

        self.validate_authorization(&tenants)?;
        if let Some(registration) = &self.registration {
            let Some(secret) = secret_refs.get(&registration.secret) else {
                return Err(HubControlPlaneError::UnknownRegistrationSecret(
                    registration.secret.clone(),
                ));
            };
            if secret.purpose != SecretPurpose::RegistrationSecret
                || secret.owner != SecretOwner::Hub
            {
                return Err(HubControlPlaneError::RegistrationSecretMismatch(
                    registration.secret.clone(),
                ));
            }
        }

        if let Some(computers) = &self.computers {
            computers.validate(self)?;
        }
        Ok(sections)
    }

    fn validate_authorization(
        &self,
        tenants: &BTreeSet<TenantId>,
    ) -> Result<(), HubControlPlaneError> {
        let mut roles = BTreeSet::new();
        for role in &self.authorization.roles {
            if !roles.insert(&role.name) {
                return Err(HubControlPlaneError::DuplicateRole(role.name.clone()));
            }
        }
        for grant in &self.authorization.grants {
            if !roles.contains(&grant.role) {
                return Err(HubControlPlaneError::UnknownGrantRole(grant.role.clone()));
            }
            if let Some(tenant) = &grant.tenant
                && !tenants.contains(tenant)
            {
                return Err(HubControlPlaneError::UnknownGrantTenant {
                    role: grant.role.clone(),
                    tenant: tenant.clone(),
                });
            }
        }
        let mut overridden = BTreeMap::new();
        for permission_override in &self.authorization.overrides {
            let deny = matches!(permission_override, PermissionOverride::Deny { .. });
            if let Some(previous) =
                overridden.insert(permission_override.permission().clone(), deny)
                && (previous || deny)
            {
                return Err(HubControlPlaneError::ConflictingPermissionOverride(
                    permission_override.permission().clone(),
                ));
            }
        }
        Ok(())
    }
}

mod error;
pub use error::*;

#[cfg(test)]
mod tests;
