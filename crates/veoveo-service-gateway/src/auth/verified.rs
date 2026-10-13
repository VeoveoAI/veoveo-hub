use std::collections::BTreeSet;

use crate::contract::PrincipalDisplayName;
use chrono::{DateTime, Utc};
use veoveo_sdk_rust::{AccessTokenSubject, JwtId, OAuthClientId, Principal};
use veoveo_types::InvocationAuthority;
use veoveo_types::ScopeName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAccessToken {
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
}

impl AuthenticatedSubject {
    pub fn request_context(&self) -> veoveo_sdk_rust::GatewayRequestContext {
        veoveo_sdk_rust::GatewayRequestContext {
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
