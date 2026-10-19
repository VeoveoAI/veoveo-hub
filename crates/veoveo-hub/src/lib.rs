pub mod auth;
mod catalog;
mod catalog_admission;
mod catalog_registry;
pub mod computers;
pub mod contract;
pub mod control_plane;
pub mod delegation;
pub mod http;
pub mod issuance;
pub use catalog_admission::HubCatalogAdmission;
pub use catalog_registry::{
    audit_registry, kernel_catalog_registry, kernel_catalog_registry_builder,
};
mod control_store;
pub mod local;
pub mod mcp;
mod mcp_support;
mod metadata;
pub mod oauth_clients;
mod oauth_resource;
pub mod token_profile;
pub use oauth_resource::RegisteredProtectedResource;
pub mod policy;
mod principal_audit;
pub mod problem;
pub mod registration;
pub mod request_observation;
pub mod secrets;
pub mod state;
pub mod store;
#[cfg(test)]
mod test_store;
mod tool_name;
pub mod usage;

pub use auth::{
    AuthError, AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    IdJagConfig, IdJagVerifier, JwtAuthConfig, JwtVerifier, OidcIdTokenConfig, OidcIdTokenVerifier,
    VerifiedAccessToken, VerifiedClientAssertion, VerifiedIdJag, VerifiedOidcIdentity,
};
pub use catalog::{HubAuthorityError, HubCatalog, HubCatalogHandle, HubCatalogSnapshot};
pub use control_store::{
    HubControlPlaneRevisionHead, HubControlStore, is_revision_conflict,
    new_hub_control_plane_revision_id,
};
pub use mcp::{
    HubMcp, HubServerHealth, HubServerHealthState, HubUpstreamHttpClientPool,
    upstream_client_builder,
};
pub use metadata::{
    AuthorizationExtensionMetadata, AuthorizationServerMetadata, BearerError, HubMetadataError,
    ProtectedResourceMetadata, www_authenticate_challenge,
};
pub use policy::{PolicyRequest, mcp_method_name, resource_scheme_from_uri};
pub use principal_audit::{merge_principal_audit_metadata, principal_audit_metadata};
pub use secrets::{HubSecretResolver, ResolvedSecretString, SecretResolverError};
pub use state::{
    HubRefreshDeliveryWindow, HubRefreshExchange, HubRefreshIssueRequest,
    HubRefreshRotationRequest, HubState, IssuedHubRefreshToken, REFRESH_TOKEN_TTL_SECONDS,
    RefreshTokenDeliveryCipher,
};
pub use tool_name::{HubNameError, HubToolProjection};

pub mod audit;

#[cfg(test)]
mod test_catalog;

#[cfg(test)]
use test_catalog as catalog_fixture;
