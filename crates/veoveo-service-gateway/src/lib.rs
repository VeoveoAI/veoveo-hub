pub mod auth;
mod catalog;
mod catalog_admission;
mod catalog_registry;
pub mod http;
pub use catalog_admission::GatewayCatalogAdmission;
pub use catalog_registry::{kernel_audit_target_registry, kernel_catalog_registry};
mod control_store;
pub mod local;
pub mod mcp;
mod mcp_support;
mod metadata;
pub mod oauth_clients;
mod policy;
mod principal_audit;
pub mod problem;
pub mod registration;
pub mod request_observation;
pub mod secrets;
pub mod state;
#[cfg(test)]
use veoveo_test_fixtures::store as test_store;
mod tool_name;

pub use auth::{
    AuthError, AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    IdJagConfig, IdJagVerifier, JwtAuthConfig, JwtVerifier, OidcIdTokenConfig, OidcIdTokenVerifier,
    VerifiedAccessToken, VerifiedClientAssertion, VerifiedIdJag, VerifiedOidcIdentity,
};
pub use catalog::{
    GatewayAuthorityError, GatewayCatalog, GatewayCatalogHandle, GatewayCatalogSnapshot,
};
pub use control_store::{
    GatewayControlPlaneRevisionHead, GatewayControlStore, is_revision_conflict,
    new_gateway_control_plane_revision_id,
};
pub use mcp::{
    GatewayMcp, GatewayServerHealth, GatewayServerHealthState, GatewayUpstreamHttpClientPool,
    upstream_client_builder,
};
pub use metadata::{
    AuthorizationExtensionMetadata, AuthorizationServerMetadata, GatewayMetadataError,
    ProtectedResourceMetadata, www_authenticate_challenge,
};
pub use policy::{PolicyRequest, mcp_method_name, resource_scheme_from_uri};
pub use principal_audit::{merge_principal_audit_metadata, principal_audit_metadata};
pub use secrets::{GatewaySecretResolver, ResolvedSecretString, SecretResolverError};
pub use state::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, GatewayState, IssuedGatewayRefreshToken,
    REFRESH_TOKEN_TTL_SECONDS, RefreshTokenDeliveryCipher,
};
pub use tool_name::{GatewayNameError, GatewayToolProjection};

pub mod audit;

#[cfg(test)]
mod test_catalog;

#[cfg(test)]
use test_catalog as catalog_fixture;
