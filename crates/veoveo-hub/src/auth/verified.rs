use std::collections::BTreeSet;

use crate::contract::PrincipalDisplayName;
use chrono::{DateTime, Utc};
use veoveo_hub_sdk::{AccessTokenSubject, JwtId, OAuthClientId, Principal};
use veoveo_types::InvocationAuthority;
use veoveo_types::ScopeName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAccessToken {
    pub purpose: Option<crate::token_profile::TokenPurpose>,
    pub extensions: veoveo_types::AdmittedExtensions,
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
    pub principal_display_name: Option<PrincipalDisplayName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSubject {
    pub extensions: veoveo_types::AdmittedExtensions,
    pub audit: veoveo_audit_contract::AuditRequest,
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
    pub actor: Principal,
    pub principal_display_name: Option<PrincipalDisplayName>,
    pub authority: InvocationAuthority,
    /// The registered server that made this call for the caller, with the permissions
    /// its exchanged token carries; absent for a caller's own call.
    pub delegation: Option<crate::delegation::SubjectDelegation>,
}

impl AuthenticatedSubject {
    pub fn request_context(&self) -> veoveo_hub_sdk::HubRequestContext {
        veoveo_hub_sdk::HubRequestContext {
            audit: self.audit.clone(),
            access_token: self.access_token.clone(),
            principal: self.principal.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClientAssertion {
    pub client_id: OAuthClientId,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdJag {
    pub client_id: OAuthClientId,
    pub principal: Principal,
    pub scopes: BTreeSet<ScopeName>,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOidcIdentity {
    pub principal: Principal,
    pub principal_display_name: PrincipalDisplayName,
    pub expires_at: DateTime<Utc>,
}
