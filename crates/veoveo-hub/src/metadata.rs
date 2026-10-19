use crate::contract::AuthorizationServerId;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::contract::{OAuthClientAuthMethod, OAuthGrantType};
use crate::control_plane::{AuthMode, HubProfile, JwksSource};
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::HubProfileId;
use veoveo_types::{ScopeDefinition, ScopeName};

use crate::HubCatalog;

const ID_JAG_GRANT_PROFILE: &str = "urn:ietf:params:oauth:grant-profile:id-jag";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub bearer_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, AuthorizationExtensionMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_authorization_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes_supported: Vec<String>,
    /// RFC 8414 §2 requires the member; an authorization server without an
    /// authorization endpoint lists no response type.
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    /// Always present: RFC 8414 §2 reads an absent member as `authorization_code`
    /// and `implicit`.
    #[serde(default)]
    pub grant_types_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_challenge_methods_supported: Vec<String>,
    /// Always present: RFC 8414 §2 reads an absent member as `client_secret_basic`.
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_signing_alg_values_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocation_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authorization_grant_profiles_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthorizationExtensionMetadata {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubMetadataError {
    UnknownProfile(HubProfileId),
    UnknownAuthorizationServerId(AuthorizationServerId),
    UnknownAuthorizationServer {
        profile: HubProfileId,
        authorization_server: AuthorizationServerId,
    },
}

impl fmt::Display for HubMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProfile(profile) => write!(f, "unknown hub profile `{profile}`"),
            Self::UnknownAuthorizationServerId(authorization_server) => {
                write!(
                    f,
                    "unknown resource authorization server `{authorization_server}`"
                )
            }
            Self::UnknownAuthorizationServer {
                profile,
                authorization_server,
            } => write!(
                f,
                "hub profile `{profile}` references unknown resource authorization server `{authorization_server}`"
            ),
        }
    }
}

impl std::error::Error for HubMetadataError {}

impl HubCatalog {
    pub fn protected_resource_metadata(
        &self,
        profile_id: &HubProfileId,
    ) -> Result<ProtectedResourceMetadata, HubMetadataError> {
        let profile = self
            .profile(profile_id)
            .ok_or_else(|| HubMetadataError::UnknownProfile(profile_id.clone()))?;
        let authorization_server = self
            .authorization_server(&profile.authorization_server)
            .ok_or_else(|| HubMetadataError::UnknownAuthorizationServer {
                profile: profile.id.clone(),
                authorization_server: profile.authorization_server.clone(),
            })?;

        Ok(ProtectedResourceMetadata {
            resource: profile.protected_resource.to_string(),
            authorization_servers: vec![authorization_server.issuer.to_string()],
            scopes_supported: self
                .profile_supported_scopes(profile)
                .into_iter()
                .map(|scope| scope.to_string())
                .collect(),
            bearer_methods_supported: vec!["header".to_string()],
            extensions: authorization_extensions(profile),
        })
    }

    pub fn authorization_server_metadata(
        &self,
        profile_id: &HubProfileId,
    ) -> Result<AuthorizationServerMetadata, HubMetadataError> {
        let profile = self
            .profile(profile_id)
            .ok_or_else(|| HubMetadataError::UnknownProfile(profile_id.clone()))?;
        let authorization_server = self
            .authorization_server(&profile.authorization_server)
            .ok_or_else(|| HubMetadataError::UnknownAuthorizationServer {
                profile: profile.id.clone(),
                authorization_server: profile.authorization_server.clone(),
            })?;
        let clients = self.profile_oauth_clients(profile);
        let token_auth_methods = clients
            .iter()
            .flat_map(|client| client.auth_methods.iter().copied())
            .map(oauth_client_auth_method_name)
            .collect::<BTreeSet<_>>();
        let grant_types = clients
            .iter()
            .flat_map(|client| client.grant_types.iter().copied())
            .map(oauth_grant_type_name)
            .collect::<BTreeSet<_>>();
        let supports_authorization_code = profile
            .auth_modes
            .contains(&AuthMode::OidcAuthorizationCodePkce);
        let supports_private_key_jwt = clients.iter().any(|client| {
            client
                .auth_methods
                .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
        });
        let supports_public_refresh_revocation = clients.iter().any(|client| {
            client.grant_types.contains(&OAuthGrantType::RefreshToken)
                && client.auth_methods.contains(&OAuthClientAuthMethod::None)
        });

        Ok(AuthorizationServerMetadata {
            issuer: authorization_server.issuer.to_string(),
            authorization_endpoint: authorization_server
                .authorization_endpoint
                .as_ref()
                .map(ToString::to_string),
            token_endpoint: authorization_server.token_endpoint.to_string(),
            device_authorization_endpoint: clients
                .iter()
                .any(|client| {
                    client
                        .grant_types
                        .contains(&OAuthGrantType::DeviceAuthorization)
                })
                .then(|| {
                    format!(
                        "{}/device_authorization",
                        authorization_server.issuer.as_str().trim_end_matches('/')
                    )
                }),
            revocation_endpoint: supports_public_refresh_revocation.then(|| {
                format!(
                    "{}/revoke",
                    authorization_server.issuer.as_str().trim_end_matches('/')
                )
            }),
            jwks_uri: match &authorization_server.jwks {
                JwksSource::Remote { jwks_uri } => Some(jwks_uri.to_string()),
                JwksSource::File { .. } => None,
            },
            scopes_supported: self
                .profile_supported_scopes(profile)
                .into_iter()
                .map(|scope| scope.to_string())
                .collect(),
            response_types_supported: if supports_authorization_code {
                vec!["code".to_string()]
            } else {
                Vec::new()
            },
            grant_types_supported: grant_types.into_iter().map(str::to_string).collect(),
            code_challenge_methods_supported: if supports_authorization_code {
                vec!["S256".to_string()]
            } else {
                Vec::new()
            },
            token_endpoint_auth_methods_supported: token_auth_methods
                .into_iter()
                .map(str::to_string)
                .collect(),
            token_endpoint_auth_signing_alg_values_supported: if supports_private_key_jwt {
                vec![
                    "RS256".to_string(),
                    "RS384".to_string(),
                    "RS512".to_string(),
                    "PS256".to_string(),
                    "PS384".to_string(),
                    "PS512".to_string(),
                    "ES256".to_string(),
                    "ES384".to_string(),
                    "EdDSA".to_string(),
                ]
            } else {
                Vec::new()
            },
            revocation_endpoint_auth_methods_supported: if supports_public_refresh_revocation {
                vec!["none".to_string()]
            } else {
                Vec::new()
            },
            authorization_grant_profiles_supported: if profile
                .auth_modes
                .contains(&AuthMode::EnterpriseManagedAuthorization)
            {
                vec![ID_JAG_GRANT_PROFILE.to_string()]
            } else {
                Vec::new()
            },
        })
    }

    pub fn authorization_server_metadata_for_server(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Result<AuthorizationServerMetadata, HubMetadataError> {
        let authorization_server = self
            .authorization_server(authorization_server_id)
            .ok_or_else(|| {
                HubMetadataError::UnknownAuthorizationServerId(authorization_server_id.clone())
            })?;
        let clients = self.authorization_server_oauth_clients(authorization_server_id);
        let profiles = self.authorization_server_profiles(authorization_server_id);
        let token_auth_methods = clients
            .iter()
            .flat_map(|client| client.auth_methods.iter().copied())
            .map(oauth_client_auth_method_name)
            .collect::<BTreeSet<_>>();
        let auth_modes = profiles
            .iter()
            .flat_map(|profile| profile.auth_modes.iter().copied())
            .collect::<BTreeSet<_>>();
        let grant_types = clients
            .iter()
            .flat_map(|client| client.grant_types.iter().copied())
            .map(oauth_grant_type_name)
            .collect::<BTreeSet<_>>();
        let scopes = profiles
            .iter()
            .flat_map(|profile| self.profile_supported_scopes(profile).into_iter())
            .chain(
                self.sections()
                    .protected_resources()
                    .iter()
                    .filter(|resource| &resource.authorization_server == authorization_server_id)
                    .flat_map(|resource| resource.required_scopes.iter().cloned()),
            )
            .collect::<BTreeSet<_>>();
        let supports_authorization_code = auth_modes.contains(&AuthMode::OidcAuthorizationCodePkce);
        let supports_private_key_jwt = clients.iter().any(|client| {
            client
                .auth_methods
                .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
        });
        let supports_public_refresh_revocation = clients.iter().any(|client| {
            client.grant_types.contains(&OAuthGrantType::RefreshToken)
                && client.auth_methods.contains(&OAuthClientAuthMethod::None)
        });

        Ok(AuthorizationServerMetadata {
            issuer: authorization_server.issuer.to_string(),
            authorization_endpoint: authorization_server
                .authorization_endpoint
                .as_ref()
                .map(ToString::to_string),
            token_endpoint: authorization_server.token_endpoint.to_string(),
            device_authorization_endpoint: clients
                .iter()
                .any(|client| {
                    client
                        .grant_types
                        .contains(&OAuthGrantType::DeviceAuthorization)
                })
                .then(|| {
                    format!(
                        "{}/device_authorization",
                        authorization_server.issuer.as_str().trim_end_matches('/')
                    )
                }),
            revocation_endpoint: supports_public_refresh_revocation.then(|| {
                format!(
                    "{}/revoke",
                    authorization_server.issuer.as_str().trim_end_matches('/')
                )
            }),
            jwks_uri: match &authorization_server.jwks {
                JwksSource::Remote { jwks_uri } => Some(jwks_uri.to_string()),
                JwksSource::File { .. } => None,
            },
            scopes_supported: scopes.into_iter().map(|scope| scope.to_string()).collect(),
            response_types_supported: if supports_authorization_code {
                vec!["code".to_string()]
            } else {
                Vec::new()
            },
            grant_types_supported: grant_types.into_iter().map(str::to_string).collect(),
            code_challenge_methods_supported: if supports_authorization_code {
                vec!["S256".to_string()]
            } else {
                Vec::new()
            },
            token_endpoint_auth_methods_supported: token_auth_methods
                .into_iter()
                .map(str::to_string)
                .collect(),
            token_endpoint_auth_signing_alg_values_supported: if supports_private_key_jwt {
                vec![
                    "RS256".to_string(),
                    "RS384".to_string(),
                    "RS512".to_string(),
                    "PS256".to_string(),
                    "PS384".to_string(),
                    "PS512".to_string(),
                    "ES256".to_string(),
                    "ES384".to_string(),
                    "EdDSA".to_string(),
                ]
            } else {
                Vec::new()
            },
            revocation_endpoint_auth_methods_supported: if supports_public_refresh_revocation {
                vec!["none".to_string()]
            } else {
                Vec::new()
            },
            authorization_grant_profiles_supported: if auth_modes
                .contains(&AuthMode::EnterpriseManagedAuthorization)
            {
                vec![ID_JAG_GRANT_PROFILE.to_string()]
            } else {
                Vec::new()
            },
        })
    }

    pub fn profile_supported_scopes(&self, profile: &HubProfile) -> BTreeSet<ScopeName> {
        let mut scopes = profile
            .required_scopes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        // The hub hosts the audit API for each profile. Client registrations
        // still control who can obtain this domain-owned optional scope.
        scopes.insert(veoveo_audit_contract::AuditScope::Read.name().clone());
        if let Some(policy) = self.policy(&profile.policy_version) {
            for rule in &policy.rules {
                if (rule.protected_resources.is_empty()
                    && (rule.profiles.is_empty() || rule.profiles.contains(&profile.id)))
                    || rule
                        .protected_resources
                        .contains(&profile.protected_resource)
                {
                    scopes.extend(rule.required_scopes.iter().cloned());
                }
            }
        }
        scopes
    }
}

/// An error code of an RFC 6750 §3.1 bearer challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BearerError {
    /// The request's credentials are malformed.
    InvalidRequest,
    /// The access token is malformed, expired, revoked or otherwise invalid.
    InvalidToken,
    /// The access token lacks a scope the resource requires.
    InsufficientScope,
}

impl BearerError {
    /// The `error` attribute value RFC 6750 §3.1 registers.
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidToken => "invalid_token",
            Self::InsufficientScope => "insufficient_scope",
        }
    }
}

/// An RFC 6750 §3 `Bearer` challenge naming the protected resource's RFC 9728
/// metadata and the scopes it requires. A request without credentials receives no
/// error code (RFC 6750 §3.1).
pub fn www_authenticate_challenge(
    metadata_url: &str,
    scopes: &[ScopeName],
    error: Option<BearerError>,
) -> String {
    let mut challenge = String::from("Bearer ");
    if let Some(error) = error {
        challenge.push_str(&format!(r#"error="{}", "#, error.code()));
    }
    challenge.push_str(&format!(r#"resource_metadata="{metadata_url}""#));
    let scope = scopes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    if !scope.is_empty() {
        challenge.push_str(&format!(r#", scope="{scope}""#));
    }
    challenge
}

fn authorization_extensions(
    profile: &HubProfile,
) -> BTreeMap<String, AuthorizationExtensionMetadata> {
    profile
        .auth_modes
        .iter()
        .filter_map(|mode| mode.mcp_extension_id())
        .map(|extension| {
            (
                extension.to_string(),
                AuthorizationExtensionMetadata::default(),
            )
        })
        .collect()
}

fn oauth_grant_type_name(grant_type: OAuthGrantType) -> &'static str {
    match grant_type {
        OAuthGrantType::AuthorizationCodePkce => "authorization_code",
        OAuthGrantType::RefreshToken => "refresh_token",
        OAuthGrantType::DeviceAuthorization => "urn:ietf:params:oauth:grant-type:device_code",
        OAuthGrantType::TokenExchange => "urn:ietf:params:oauth:grant-type:token-exchange",
        OAuthGrantType::ClientCredentials => "client_credentials",
        OAuthGrantType::EnterpriseManagedAuthorization => {
            "urn:ietf:params:oauth:grant-type:jwt-bearer"
        }
    }
}

fn oauth_client_auth_method_name(auth_method: OAuthClientAuthMethod) -> &'static str {
    match auth_method {
        OAuthClientAuthMethod::None => "none",
        OAuthClientAuthMethod::PrivateKeyJwt => "private_key_jwt",
        OAuthClientAuthMethod::ClientSecretBasic => "client_secret_basic",
        OAuthClientAuthMethod::ClientSecretPost => "client_secret_post",
        OAuthClientAuthMethod::TlsClientAuth => "tls_client_auth",
    }
}
