//! An OAuth audience admitted by the current catalog. MCP profiles are one kind of
//! protected resource; contributed resources do not acquire an MCP identity.
use crate::{
    HubCatalog,
    contract::{AuthorizationServerId, OAuthGrantType, ProtectedResourceDescriptor},
    control_plane::{AuthMode, HubProfile, IdentityProviderOidcClientRegistration},
};
use std::collections::BTreeSet;
use veoveo_hub_sdk::ProtectedResourceId;
use veoveo_types::ScopeName;

#[derive(Clone, Copy, Debug)]
pub enum RegisteredProtectedResource<'a> {
    Profile(&'a HubProfile),
    Contributed(&'a ProtectedResourceDescriptor),
}

impl<'a> RegisteredProtectedResource<'a> {
    pub fn profile(self) -> Option<&'a HubProfile> {
        match self {
            Self::Profile(profile) => Some(profile),
            Self::Contributed(_) => None,
        }
    }

    pub fn protected_resource(self) -> &'a ProtectedResourceId {
        match self {
            Self::Profile(profile) => &profile.protected_resource,
            Self::Contributed(resource) => &resource.resource,
        }
    }

    pub fn authorization_server(self) -> &'a AuthorizationServerId {
        match self {
            Self::Profile(profile) => &profile.authorization_server,
            Self::Contributed(resource) => &resource.authorization_server,
        }
    }

    pub fn supported_scopes(self, catalog: &HubCatalog) -> BTreeSet<ScopeName> {
        match self {
            Self::Profile(profile) => catalog.profile_supported_scopes(profile),
            Self::Contributed(resource) => resource.required_scopes.clone(),
        }
    }

    pub fn audit_target(self) -> crate::http::auth_support::AuthAuditTarget<'a> {
        self.into()
    }

    pub fn supports(self, grant: OAuthGrantType) -> bool {
        match self {
            Self::Profile(profile) => match grant {
                OAuthGrantType::AuthorizationCodePkce | OAuthGrantType::RefreshToken => profile
                    .auth_modes
                    .contains(&AuthMode::OidcAuthorizationCodePkce),
                OAuthGrantType::EnterpriseManagedAuthorization => profile
                    .auth_modes
                    .contains(&AuthMode::EnterpriseManagedAuthorization),
                OAuthGrantType::DeviceAuthorization | OAuthGrantType::TokenExchange => false,
                OAuthGrantType::ClientCredentials => profile
                    .auth_modes
                    .contains(&AuthMode::OAuthClientCredentials),
            },
            Self::Contributed(resource) => resource.grant_types.contains(&grant),
        }
    }

    /// The broker registration belongs to this audience and its configured identity
    /// provider, independently of any MCP profile.
    pub fn oidc_client(
        self,
        catalog: &'a HubCatalog,
    ) -> Option<&'a IdentityProviderOidcClientRegistration> {
        let server = catalog.authorization_server(self.authorization_server())?;
        let provider = match self {
            Self::Profile(profile) => &profile.identity_provider,
            Self::Contributed(_) => server.identity_provider.as_ref()?,
        };
        catalog.control_plane().oidc_clients.iter().find(|client| {
            &client.identity_provider == provider
                && &client.authorization_server == self.authorization_server()
                && client.allowed_resources.contains(self.protected_resource())
        })
    }
}

impl HubCatalog {
    pub fn registered_protected_resource(
        &self,
        resource: &ProtectedResourceId,
    ) -> Option<RegisteredProtectedResource<'_>> {
        self.profile_by_protected_resource(resource.as_str())
            .map(RegisteredProtectedResource::Profile)
            .or_else(|| {
                self.contributed_protected_resource(resource)
                    .map(RegisteredProtectedResource::Contributed)
            })
    }
}

impl<'a> From<&'a HubProfile> for RegisteredProtectedResource<'a> {
    fn from(profile: &'a HubProfile) -> Self {
        Self::Profile(profile)
    }
}
