//! Trusted installation inputs for private, owner-scoped native Computers.
//! No browser request selects a gateway, image, provider profile or secret.
use std::{
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::{HubProfileId, OAuthClientId};
use veoveo_types::{InvocationMode, WorkContextId};

use super::{HubControlPlane, HubControlPlaneError};
use crate::contract::{
    OAuthClientAuthMethod, OAuthGrantType, SecretPurpose, SecretReferenceId, SecretSource,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Computers {
    /// Public Computer guidance, served from current control as the MCP server
    /// instructions of the Computers' profile to Computer clients; the Hub writes
    /// nothing into a Computer. Plain Markdown only; connection values remain in
    /// their native inputs. None or blank text serves no handbook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handbook: Option<String>,
    pub gateway: ComputerGateway,
    pub template: ComputerTemplate,
    /// Trusted native authored policy, admitted before runtime publication.
    /// Use an immutable versioned file path for each policy revision.
    pub policy_file: PathBuf,
    /// Fixed service command. No browser-controlled governance overrides.
    pub command: Vec<String>,
    pub oauth_client: OAuthClientId,
    pub profile: HubProfileId,
    pub work_context: WorkContextId,
    /// Native loopback destination inside each admitted sandbox.
    pub port: u16,
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
    pub model_provider: ComputerStaticProvider,
    /// The owner's Lore access. The gateway refreshes it with the Computer's own Lore
    /// client, whose tokens are the owner's login token or the token of the one repository
    /// a scope names; the sandbox's Lore command line reads the credential's placeholder.
    pub lore_provider: ComputerRefreshProvider,
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

fn default_forwarding_connection_limit() -> NonZeroUsize {
    NonZeroUsize::new(512).unwrap()
}

impl Computers {
    pub(super) fn validate(&self, plane: &HubControlPlane) -> Result<(), HubControlPlaneError> {
        let invalid = |reason| HubControlPlaneError::InvalidComputers(reason);
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
            || client.default_work_context != self.work_context
            || !plane
                .work_contexts
                .iter()
                .any(|context| context.id == self.work_context)
        {
            return Err(invalid(
                "browser client must admit direct PKCE and refresh for the configured profile and Work Context",
            ));
        }
        if !plane.secrets.iter().any(|secret| {
            secret.id == self.model_provider.secret
                && secret.source == SecretSource::Env
                && secret.purpose == SecretPurpose::ProviderApiKey
        }) {
            return Err(invalid(
                "model bootstrap requires a declared Env ProviderApiKey secret",
            ));
        }
        Ok(())
    }
}
