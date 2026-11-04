//! Trusted installation inputs for private, owner-scoped native Computers.
//! No browser request selects a gateway, image, provider profile or secret.
use std::{
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::{HubProfileId, OAuthClientId};
use veoveo_types::InvocationMode;

use super::{HubControlPlane, HubControlPlaneError};
use crate::contract::{
    OAuthClientAuthMethod, OAuthGrantType, SecretPurpose, SecretReferenceId, SecretSource,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Computers {
    /// The installation's own skills for Computers' agents: each skill name, which is
    /// its directory's, to its files, each path relative to the skill's directory and
    /// `/`-separated to the file's UTF-8 text, `SKILL.md` among them. The Hub serves them
    /// at `skill://veoveo/{name}/{path}` to Computer clients on the Computers' profile
    /// alone and writes nothing into a Computer; an empty map serves none.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub skills: crate::skills::SkillFiles,
    pub gateway: ComputerGateway,
    pub template: ComputerTemplate,
    /// Trusted native authored policy, admitted before runtime publication.
    /// Use an immutable versioned file path for each policy revision.
    pub policy_file: PathBuf,
    /// Fixed service command. No browser-controlled governance overrides.
    pub command: Vec<String>,
    pub oauth_client: OAuthClientId,
    pub profile: HubProfileId,
    /// Native loopback destination inside each admitted sandbox.
    pub port: u16,
    /// Further programs of the Computer version, each served under
    /// `/computer/c/{computer}/apps/{name}/` from its own loopback port with the
    /// Computer's admission, headers and limits. The Computer's own program keeps every
    /// other path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<ComputerApp>,
    /// Global bound on simultaneous native Computer forwarding connections.
    #[serde(default = "default_forwarding_connection_limit")]
    pub forwarding_connection_limit: NonZeroUsize,
    /// How long a Computer stays idle, with no browser request through the Hub and no
    /// network traffic from its sandbox, before the Hub deletes its sandbox, keeping
    /// its home; the next entry creates it at the current version. None never
    /// collects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collect_after_seconds: Option<NonZeroU64>,
    pub hub_provider: ComputerRefreshProvider,
    /// The default model vendor's key, the workspace-local provider `computer-model`.
    pub model_provider: ComputerStaticProvider,
    /// Further model vendors' keys, each a workspace-local provider of its own name,
    /// which the installation's policy binds to that vendor's API alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_model_providers: Vec<ComputerModelProvider>,
    /// The owner's Lore access. The gateway refreshes it with the Computer's own Lore
    /// client, whose tokens are the owner's login token or the token of the one repository
    /// a scope names; the sandbox's Lore command line reads the credential's placeholder.
    pub lore_provider: ComputerRefreshProvider,
}

/// A further program of the Computer version: its name, a lowercase DNS label, which
/// names it in its path, and the loopback port it serves on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerApp {
    pub name: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerGateway {
    pub endpoint: url::Url,
    pub certificate_authority: PathBuf,
    /// Trusted provisioning authority; never injected into the Computer.
    pub bearer_environment: String,
}

/// An OpenShell workload template by its workspace and name: the installation's
/// Computer template, or its factory template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerTemplate {
    pub workspace: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerRefreshProvider {
    /// Installation-owned global native profile; its refresh scopes must be empty.
    pub profile: String,
    /// Native runtime environment key declared in the selected profile.
    pub credential_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerStaticProvider {
    pub profile: String,
    /// Native runtime environment key declared in the selected profile.
    pub credential_key: String,
    /// Only a trusted Env ProviderApiKey reference is admitted for bootstrap.
    pub secret: SecretReferenceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerModelProvider {
    /// The workspace-local provider's name, a lowercase DNS label, which the
    /// installation's policy names in `credential_binding`.
    pub name: String,
    /// Installation-owned global native profile; it neither mints nor refreshes.
    pub profile: String,
    /// Native runtime environment key declared in the selected profile.
    pub credential_key: String,
    /// Only a trusted Env ProviderApiKey reference is admitted for bootstrap.
    pub secret: SecretReferenceId,
}

fn default_forwarding_connection_limit() -> NonZeroUsize {
    NonZeroUsize::new(512).unwrap()
}

impl Computers {
    /// The further program named `name`.
    pub fn app(&self, name: &str) -> Option<&ComputerApp> {
        self.apps.iter().find(|app| app.name == name)
    }

    pub(super) fn validate(&self, plane: &HubControlPlane) -> Result<(), HubControlPlaneError> {
        let invalid = |reason| HubControlPlaneError::InvalidComputers(reason);
        crate::skills::HubSkills::load(&self.skills)
            .map_err(|error| HubControlPlaneError::InvalidComputerSkills(error.to_string()))?;
        let mut names = std::collections::BTreeSet::new();
        let mut ports = std::collections::BTreeSet::from([self.port]);
        for app in &self.apps {
            if !veoveo_hub_sdk::hub::LinkName::parse(&app.name).is_ok()
                || !names.insert(app.name.as_str())
                || app.port == 0
                || !ports.insert(app.port)
            {
                return Err(invalid(
                    "each Computer app needs a distinct lowercase DNS label for its name and a nonzero loopback port of its own",
                ));
            }
        }
        if self.gateway.endpoint.scheme() != "https"
            || self.gateway.endpoint.host_str().is_none()
            || !self.gateway.endpoint.username().is_empty()
            || self.gateway.endpoint.password().is_some()
            || self.gateway.endpoint.query().is_some()
            || self.gateway.endpoint.fragment().is_some()
            || self.gateway.endpoint.path() != "/"
            || !self.gateway.certificate_authority.is_absolute()
            || self.gateway.bearer_environment.is_empty()
            || !self
                .gateway
                .bearer_environment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || self.port == 0
            || !self.policy_file.is_absolute()
            || self.command.is_empty()
            || self.command.iter().any(|argument| argument.contains('\0'))
        {
            return Err(invalid(
                "gateway requires an HTTPS origin, absolute CA/policy paths, fixed command, environment reference and nonzero loopback port",
            ));
        }
        if [
            &self.template.workspace,
            &self.template.name,
            &self.hub_provider.profile,
            &self.hub_provider.credential_key,
            &self.model_provider.profile,
            &self.model_provider.credential_key,
            &self.lore_provider.profile,
            &self.lore_provider.credential_key,
        ]
        .into_iter()
        .chain(
            self.additional_model_providers
                .iter()
                .flat_map(|model| [&model.profile, &model.credential_key]),
        )
        .any(|name| name.is_empty() || name.len() > 253 || name.chars().any(char::is_whitespace))
        {
            return Err(invalid(
                "native template and provider references must be nonempty bounded names",
            ));
        }
        let profile = plane
            .profiles
            .iter()
            .find(|p| p.id == self.profile)
            .ok_or_else(|| invalid("profile is not declared"))?;
        let client = plane
            .oauth_clients
            .iter()
            .find(|c| c.id == self.oauth_client)
            .ok_or_else(|| invalid("browser OAuth client is not declared"))?;
        if client.authorization_server != profile.authorization_server
            || !client
                .allowed_resources
                .contains(&profile.protected_resource)
            || client.invocation_mode != InvocationMode::Direct
            || client.auth_methods != [OAuthClientAuthMethod::None].into()
            || !client
                .grant_types
                .contains(&OAuthGrantType::AuthorizationCodePkce)
            || !client.grant_types.contains(&OAuthGrantType::RefreshToken)
        {
            return Err(invalid(
                "browser client must admit direct PKCE and refresh for the configured profile",
            ));
        }
        let mut providers = std::collections::BTreeSet::from([
            crate::computers::HUB_PROVIDER,
            crate::computers::MODEL_PROVIDER,
            crate::computers::LORE_PROVIDER,
        ]);
        if self.additional_model_providers.iter().any(|model| {
            !veoveo_hub_sdk::hub::LinkName::parse(&model.name).is_ok()
                || !providers.insert(model.name.as_str())
        }) {
            return Err(invalid(
                "each further model provider needs a lowercase DNS label for its name, distinct from the others and from the Hub's, the default model's and Lore's providers",
            ));
        }
        if !std::iter::once(&self.model_provider.secret)
            .chain(
                self.additional_model_providers
                    .iter()
                    .map(|model| &model.secret),
            )
            .all(|id| {
                plane.secrets.iter().any(|secret| {
                    secret.id == *id
                        && secret.source == SecretSource::Env
                        && secret.purpose == SecretPurpose::ProviderApiKey
                })
            })
        {
            return Err(invalid(
                "model bootstrap requires a declared Env ProviderApiKey secret",
            ));
        }
        Ok(())
    }
}
