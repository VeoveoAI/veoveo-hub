//! Typed owner grants behind the common hosted token issuer. This is not a JWT
//! template API: a contribution authorizes against its admitted current section,
//! then encodes only its own claim types. Standard claims stay with the issuer.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::{OAuthClientId, Principal, ProtectedResourceId};
use veoveo_types::{AdmittedExtensions, ExtensionRegistry, ScopeName, WorkContextId};

use crate::{
    HubCatalog, RegisteredProtectedResource,
    contract::{CatalogSection, SectionKey},
};

/// Positive installation-selected token lifetime; the common signer sets times.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct TokenLifetime(std::num::NonZeroU32);
impl TokenLifetime {
    pub fn seconds(self) -> u32 {
        self.0.get()
    }
    pub fn from_seconds(seconds: std::num::NonZeroU32) -> Self {
        Self(seconds)
    }
}

pub const TOKEN_PURPOSE_CLAIM: &str = "https://veoveo.ai/claims/token-purpose";

/// Session issuance authority and a resource credential are deliberately distinct.
/// A resource credential must never be accepted as an unrestricted login grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenPurpose {
    Login,
    Resource,
}

/// Common admission after the OAuth handler has authenticated its caller. The
/// handler still owns proof verification and current refresh-family admission.
pub struct TokenContext<'a> {
    catalog: &'a HubCatalog,
    resource: RegisteredProtectedResource<'a>,
    client: &'a OAuthClientId,
    principal: &'a Principal,
    work_context: &'a WorkContextId,
    scopes: &'a BTreeSet<ScopeName>,
    family: Option<&'a veoveo_hub_sdk::HubRefreshFamilyId>,
    not_after: Option<chrono::DateTime<chrono::Utc>>,
}

impl<'a> TokenContext<'a> {
    pub fn admit(
        catalog: &'a HubCatalog,
        resource: &'a ProtectedResourceId,
        client: &'a OAuthClientId,
        principal: &'a Principal,
        work_context: &'a WorkContextId,
        scopes: &'a BTreeSet<ScopeName>,
        family: Option<&'a veoveo_hub_sdk::HubRefreshFamilyId>,
    ) -> Result<Self> {
        let resource = catalog
            .registered_protected_resource(resource)
            .context("token resource is not registered")?;
        let registration = catalog
            .oauth_client(client)
            .context("token client is not registered")?;
        ensure!(
            &registration.authorization_server == resource.authorization_server()
                && registration
                    .allowed_resources
                    .contains(resource.protected_resource()),
            "token client cannot request this resource"
        );
        ensure!(
            scopes.is_subset(&registration.allowed_scopes)
                && scopes.is_subset(&resource.supported_scopes(catalog)),
            "token scopes exceed the registration"
        );
        if resource.profile().is_some() {
            catalog.work_context_membership(client, work_context, principal)?;
        }
        Ok(Self {
            catalog,
            resource,
            client,
            principal,
            work_context,
            scopes,
            family,
            not_after: None,
        })
    }
    pub fn expires_before(mut self, limit: chrono::DateTime<chrono::Utc>) -> Self {
        self.not_after = Some(limit);
        self
    }
    pub fn resource(&self) -> RegisteredProtectedResource<'a> {
        self.resource
    }
    pub fn client(&self) -> &'a OAuthClientId {
        self.client
    }
    pub fn principal(&self) -> &'a Principal {
        self.principal
    }
    pub fn work_context(&self) -> &'a WorkContextId {
        self.work_context
    }
    pub fn scopes(&self) -> &'a BTreeSet<ScopeName> {
        self.scopes
    }
}

/// A refused owner request is distinct from unavailable encoding/signing material.
#[derive(Debug, thiserror::Error)]
pub enum TokenIssueError {
    #[error("token request is not authorized")]
    Denied(#[source] anyhow::Error),
    #[error("token issuer is unavailable")]
    Unavailable(#[source] anyhow::Error),
}
impl TokenIssueError {
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied(_))
    }
}
impl From<anyhow::Error> for TokenIssueError {
    fn from(error: anyhow::Error) -> Self {
        Self::Denied(error)
    }
}

pub trait TokenProfile: Send + Sync + 'static {
    type Section: CatalogSection;
    type Request: Send + Sync;
    type AuthorizedGrant: Send + Sync;
    type Claims: Serialize;

    fn section(&self) -> &SectionKey<Self::Section>;
    fn authorize(
        &self,
        section: &Self::Section,
        context: &TokenContext<'_>,
        request: &Self::Request,
    ) -> Result<Self::AuthorizedGrant>;
    fn purpose(&self, grant: &Self::AuthorizedGrant) -> TokenPurpose;
    fn lifetime(&self, grant: &Self::AuthorizedGrant) -> TokenLifetime;
    fn issued_scopes(
        &self,
        _grant: &Self::AuthorizedGrant,
        context: &TokenContext<'_>,
    ) -> BTreeSet<ScopeName> {
        context.scopes().clone()
    }
    fn encode(&self, grant: &Self::AuthorizedGrant) -> Self::Claims;
}

/// Only `authorize_token` can construct this result. Its audience/client/identity
/// binding cannot be replaced after the owner's grant has been admitted.
pub struct AuthorizedToken<'a, P: TokenProfile> {
    context: TokenContext<'a>,
    grant: P::AuthorizedGrant,
    purpose: TokenPurpose,
    lifetime: TokenLifetime,
    extensions: AdmittedExtensions,
    scopes: BTreeSet<ScopeName>,
}

impl<'a, P: TokenProfile> AuthorizedToken<'a, P> {
    pub fn context(&self) -> &TokenContext<'a> {
        &self.context
    }
    pub fn purpose(&self) -> TokenPurpose {
        self.purpose
    }
    pub fn grant(&self) -> &P::AuthorizedGrant {
        &self.grant
    }
    pub fn into_extensions(self) -> AdmittedExtensions {
        self.extensions
    }
}

pub fn authorize_token<'a, P: TokenProfile>(
    profile: &P,
    context: TokenContext<'a>,
    request: &P::Request,
    registry: &ExtensionRegistry,
) -> std::result::Result<AuthorizedToken<'a, P>, TokenIssueError> {
    // Section keys are bound to the catalog registry. A key from another
    // installation cannot be used against this admitted revision.
    let section = (|| -> Result<_> {
        let section = context
            .catalog
            .sections()
            .get(profile.section())?
            .context("token profile section is not installed")?;
        ensure!(
            section
                .protected_resources()
                .iter()
                .any(
                    |resource| &resource.resource == context.resource.protected_resource()
                        && &resource.authorization_server
                            == context.resource.authorization_server()
                ),
            "token profile does not own this resource"
        );
        Ok(section)
    })()
    .map_err(TokenIssueError::Unavailable)?;
    let grant = profile
        .authorize(&section, &context, request)
        .map_err(TokenIssueError::Denied)?;
    let purpose = profile.purpose(&grant);
    let lifetime = profile.lifetime(&grant);
    let scopes = profile.issued_scopes(&grant, &context);
    let admitted = (|| -> Result<AdmittedExtensions> {
        let wire = serde_json::to_value(profile.encode(&grant))
            .context("token profile claims could not be encoded")?;
        let wire: BTreeMap<String, serde_json::Value> =
            serde_json::from_value(wire).context("token profile must encode a claim object")?;
        ensure!(
            !wire
                .keys()
                .any(|name| crate::auth::ACCESS_TOKEN_CORE_CLAIMS.contains(&name.as_str())),
            "token profile collides with a common claim"
        );
        let admitted = registry.admit(wire.clone())?;
        ensure!(
            admitted.wire().keys().eq(wire.keys()),
            "token profile contains an undeclared claim"
        );
        crate::auth::validate_access_token_extensions(&admitted)?;
        Ok(admitted)
    })()
    .map_err(TokenIssueError::Unavailable)?;
    Ok(AuthorizedToken {
        context,
        grant,
        purpose,
        lifetime,
        scopes,
        extensions: admitted,
    })
}

/// The OAuth protocol asks for a login or names one owner target. The owner
/// parser converts that selector into its associated, domain-typed request.
#[derive(Clone, Copy)]
pub enum ProfileRequest<'a> {
    Login,
    Exchange {
        resource: Option<&'a str>,
        scope: Option<&'a str>,
    },
}

struct PreparedToken<'a> {
    context: TokenContext<'a>,
    purpose: TokenPurpose,
    lifetime: TokenLifetime,
    extensions: AdmittedExtensions,
    scopes: BTreeSet<ScopeName>,
}
impl<'a, P: TokenProfile> AuthorizedToken<'a, P> {
    fn prepare(self) -> PreparedToken<'a> {
        PreparedToken {
            context: self.context,
            purpose: self.purpose,
            lifetime: self.lifetime,
            scopes: self.scopes,
            extensions: self.extensions,
        }
    }
}
trait ErasedProfile: Send + Sync {
    fn owns(&self, catalog: &HubCatalog, resource: &ProtectedResourceId) -> Result<bool>;
    fn prepare<'a>(
        &self,
        context: TokenContext<'a>,
        request: ProfileRequest<'_>,
        registry: &ExtensionRegistry,
    ) -> std::result::Result<PreparedToken<'a>, TokenIssueError>;
}
struct ProfileAdapter<P: TokenProfile> {
    profile: P,
    parse: for<'a> fn(ProfileRequest<'a>) -> Result<P::Request>,
}
impl<P: TokenProfile> ErasedProfile for ProfileAdapter<P> {
    fn owns(&self, catalog: &HubCatalog, resource: &ProtectedResourceId) -> Result<bool> {
        Ok(catalog
            .sections()
            .get(self.profile.section())?
            .is_some_and(|section| {
                section
                    .protected_resources()
                    .iter()
                    .any(|descriptor| &descriptor.resource == resource)
            }))
    }
    fn prepare<'a>(
        &self,
        context: TokenContext<'a>,
        request: ProfileRequest<'_>,
        registry: &ExtensionRegistry,
    ) -> std::result::Result<PreparedToken<'a>, TokenIssueError> {
        let request = (self.parse)(request).map_err(TokenIssueError::Denied)?;
        Ok(authorize_token(&self.profile, context, &request, registry)?.prepare())
    }
}

/// Executable composition binds owner implementations and claim codecs once.
/// Only code is retained here; every request reads its admitted catalog section.
#[derive(Clone)]
pub struct TokenProfiles {
    claims: ExtensionRegistry,
    profiles: Vec<std::sync::Arc<dyn ErasedProfile>>,
}
impl TokenProfiles {
    pub fn new(claims: ExtensionRegistry) -> Result<Self> {
        crate::auth::validate_access_token_registry(&claims)?;
        Ok(Self {
            claims,
            profiles: Vec::new(),
        })
    }
    pub fn register<P: TokenProfile>(
        &mut self,
        profile: P,
        parse: for<'a> fn(ProfileRequest<'a>) -> Result<P::Request>,
    ) {
        self.profiles
            .push(std::sync::Arc::new(ProfileAdapter { profile, parse }));
    }
    pub fn claim_registry(&self) -> ExtensionRegistry {
        self.claims.clone()
    }
    pub async fn issue(
        &self,
        context: TokenContext<'_>,
        request: ProfileRequest<'_>,
        display_name: Option<&crate::contract::PrincipalDisplayName>,
    ) -> std::result::Result<crate::issuance::IssuedAccessToken, TokenIssueError> {
        let mut selected = None;
        for profile in &self.profiles {
            if profile
                .owns(context.catalog, context.resource.protected_resource())
                .map_err(TokenIssueError::Unavailable)?
            {
                if selected.is_some() {
                    return Err(TokenIssueError::Unavailable(anyhow::anyhow!(
                        "multiple token profiles own this resource"
                    )));
                }
                selected = Some(profile);
            }
        }
        let profile = selected
            .context("resource token profile is not installed")
            .map_err(TokenIssueError::Unavailable)?;
        sign_prepared(
            profile.prepare(context, request, &self.claims)?,
            display_name,
        )
        .await
        .map_err(TokenIssueError::Unavailable)
    }
}

/// Consume the admitted owner grant through the same signer used by MCP.
pub async fn issue_profile_token<P: TokenProfile>(
    token: AuthorizedToken<'_, P>,
    display_name: Option<&crate::contract::PrincipalDisplayName>,
) -> std::result::Result<crate::issuance::IssuedAccessToken, TokenIssueError> {
    sign_prepared(token.prepare(), display_name)
        .await
        .map_err(TokenIssueError::Unavailable)
}
async fn sign_prepared(
    token: PreparedToken<'_>,
    display_name: Option<&crate::contract::PrincipalDisplayName>,
) -> Result<crate::issuance::IssuedAccessToken> {
    let context = token.context;
    let server = context
        .catalog
        .authorization_server(context.resource.authorization_server())
        .context("token authorization server is unavailable")?;
    let principal = context.principal;
    crate::issuance::issue_access_token(
        context.catalog,
        server,
        context.resource.protected_resource(),
        &principal.subject,
        context.client,
        principal.kind,
        Some(principal),
        display_name,
        None,
        crate::issuance::AccessTokenInvocation {
            purpose: Some(token.purpose),
            lifetime: Some(token.lifetime),
            not_after: context.not_after,
            extensions: token.extensions,
            session_family: context.family.cloned(),
            work_context: context.work_context.clone(),
            provenance: match principal.kind {
                veoveo_hub_sdk::PrincipalKind::User => veoveo_types::InvocationProvenance::Direct {
                    initiator: principal.id.clone(),
                },
                veoveo_hub_sdk::PrincipalKind::Service => {
                    veoveo_types::InvocationProvenance::Automated
                }
            },
        },
        principal.id.clone(),
        &token.scopes,
    )
    .await
}

#[cfg(test)]
mod tests;
