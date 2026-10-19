use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubJwtRevocation {
    pub profile: HubProfileId,
    pub issuer: TokenIssuer,
    pub jwt_id: JwtId,
    pub revoked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HubJwtRevocationRequest {
    pub profile: HubProfileId,
    pub issuer: TokenIssuer,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HubJwtRevocationAdminStatus {
    Revoked,
    Pruned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubJwtRevocationApplyResult {
    pub status: HubJwtRevocationAdminStatus,
    pub revocation: HubJwtRevocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubJwtRevocationPruneResult {
    pub status: HubJwtRevocationAdminStatus,
    pub deleted: u64,
}

/// Final delivery after one upstream browser identity proof. Device approval has
/// no downstream redirect URI or synthetic authorization code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorizationDestination {
    Code {
        redirect_uri: OAuthRedirectUri,
        client_state: Option<OAuthStateValue>,
        code_challenge: PkceCodeChallenge,
        code_challenge_method: PkceCodeChallengeMethod,
    },
    Device {
        request: uuid::Uuid,
        browser: veoveo_types::Sha256Digest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubAuthorizationRequest {
    pub idp_state: OAuthStateValue,
    pub protected_resource: ProtectedResourceId,
    pub oauth_client_id: OAuthClientId,
    pub work_context: WorkContextId,
    pub oidc_client: OidcClientRegistrationId,
    pub destination: AuthorizationDestination,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub requested_scopes: BTreeSet<ScopeName>,
    pub idp_code_verifier: PkceCodeVerifier,
    pub idp_code_challenge: PkceCodeChallenge,
    pub idp_code_challenge_method: PkceCodeChallengeMethod,
    pub nonce: OidcNonce,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubAuthorizationCodeRecord {
    pub code: OAuthAuthorizationCode,
    pub protected_resource: ProtectedResourceId,
    pub oauth_client_id: OAuthClientId,
    pub work_context: WorkContextId,
    pub oidc_client: OidcClientRegistrationId,
    pub redirect_uri: OAuthRedirectUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_state: Option<OAuthStateValue>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub scopes: BTreeSet<ScopeName>,
    pub code_challenge: PkceCodeChallenge,
    pub code_challenge_method: PkceCodeChallengeMethod,
    pub principal: Principal,
    pub principal_display_name: PrincipalDisplayName,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubRefreshGrant {
    pub family_id: HubRefreshFamilyId,
    pub authorization_server: AuthorizationServerId,
    pub protected_resource: ProtectedResourceId,
    pub oauth_client_id: OAuthClientId,
    pub work_context: WorkContextId,
    pub principal: Principal,
    pub principal_display_name: PrincipalDisplayName,
    pub scopes: BTreeSet<ScopeName>,
    pub generation: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OAuthTokenTypeHint {
    RefreshToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubRefreshRevocationRequest {
    pub token: OAuthRefreshToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type_hint: Option<OAuthTokenTypeHint>,
    pub client_id: OAuthClientId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<ProtectedResourceId>,
}

pub use crate::contract::PkceCodeChallengeMethod;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HubResourceProjection {
    pub server: ServerSlug,
    pub hub_uri: ResourceUri,
    pub upstream_uri: ResourceUri,
}
