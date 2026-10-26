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
    /// Resolved at the authenticated execution boundary, never inferred from WorkContext.
    pub execution: Option<veoveo_hub_sdk::ExecutionContext>,
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
    pub fn request_context(
        &self,
    ) -> Result<veoveo_hub_sdk::HubRequestContext, veoveo_hub_sdk::InternalTokenError> {
        if let Some(delegation) = &self.delegation {
            if self
                .execution
                .as_ref()
                .is_some_and(|execution| execution != &delegation.request_context.execution)
            {
                return Err(veoveo_hub_sdk::InternalTokenError::InvalidRequestContext);
            }
            return Ok(delegation.request_context.clone());
        }
        Ok(veoveo_hub_sdk::HubRequestContext {
            execution: self
                .execution
                .clone()
                .ok_or(veoveo_hub_sdk::InternalTokenError::InvalidRequestContext)?,
            audit: self.audit.clone(),
            access_token: self.access_token.clone(),
            principal: self.principal.clone(),
        })
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
    /// The `email` claim, or `preferred_username` when it is an address, in lowercase.
    pub email: Option<crate::person::Email>,
    pub expires_at: DateTime<Utc>,
}
