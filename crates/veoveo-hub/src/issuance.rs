use std::collections::BTreeSet;
use veoveo_hub_sdk::ProtectedResourceId;

use crate::HubCatalog;
use crate::contract::PrincipalDisplayName;
use crate::control_plane::ResourceAuthorizationServer;
use anyhow::anyhow;
use chrono::{TimeDelta, Utc};
use jsonwebtoken::encode;
use serde::Serialize;
use veoveo_hub_sdk::{JwtId, OAuthClientId, Principal, PrincipalKind, TokenSubject};
use veoveo_types::{InvocationProvenance, PrincipalId, ScopeName, TenantId, WorkContextId};

pub const ACCESS_TOKEN_TTL_SECONDS: i64 = 15 * 60;

#[derive(Serialize)]
struct AccessTokenClaims {
    #[serde(flatten)]
    extensions: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(
        rename = "https://veoveo.ai/claims/token-purpose",
        skip_serializing_if = "Option::is_none"
    )]
    purpose: Option<crate::token_profile::TokenPurpose>,
    iss: String,
    sub: String,
    #[serde(rename = "https://veoveo.ai/claims/principal")]
    principal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://veoveo.ai/claims/principal-display-name")]
    principal_display_name: Option<String>,
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://veoveo.ai/claims/session-family")]
    session_family: Option<veoveo_hub_sdk::HubRefreshFamilyId>,
    #[serde(rename = "https://veoveo.ai/claims/work-context")]
    work_context: String,
    #[serde(rename = "https://veoveo.ai/claims/invocation-mode")]
    invocation_mode: veoveo_types::InvocationMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://veoveo.ai/claims/initiator")]
    initiator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://veoveo.ai/claims/delegation")]
    delegation_id: Option<String>,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    #[serde(rename = "https://veoveo.ai/claims/principal-kind")]
    principal_kind: PrincipalKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://veoveo.ai/claims/tenant")]
    tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde(rename = "https://veoveo.ai/claims/data-labels")]
    data_labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde(rename = "https://veoveo.ai/claims/principal-assurances")]
    principal_assurances: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AccessTokenInvocation {
    pub lifetime: Option<crate::token_profile::TokenLifetime>,
    pub not_after: Option<chrono::DateTime<Utc>>,
    pub purpose: Option<crate::token_profile::TokenPurpose>,
    pub extensions: veoveo_types::AdmittedExtensions,
    pub session_family: Option<veoveo_hub_sdk::HubRefreshFamilyId>,
    pub work_context: WorkContextId,
    pub provenance: InvocationProvenance,
}

pub struct IssuedAccessToken {
    pub access_token: String,
    pub jwt_id: JwtId,
    pub expires_at: chrono::DateTime<Utc>,
    pub scopes: BTreeSet<ScopeName>,
}

impl IssuedAccessToken {
    pub fn expires_in(&self) -> u64 {
        (self.expires_at.timestamp() - Utc::now().timestamp()).max(0) as u64
    }
}
impl std::fmt::Debug for IssuedAccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssuedAccessToken")
            .field("access_token", &"[REDACTED]")
            .field("jwt_id", &self.jwt_id)
            .finish()
    }
}

pub struct ServiceTokenAuthority {
    pub work_context: WorkContextId,
    pub extensions: veoveo_types::AdmittedExtensions,
}

pub async fn issue_client_credentials_access_token(
    catalog: &HubCatalog,
    authorization_server: &ResourceAuthorizationServer,
    protected_resource: &ProtectedResourceId,
    client_id: &OAuthClientId,
    service_principal: &Principal,
    authority: ServiceTokenAuthority,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<IssuedAccessToken> {
    issue_access_token(
        catalog,
        authorization_server,
        protected_resource,
        &service_principal.subject,
        client_id,
        PrincipalKind::Service,
        Some(service_principal),
        None,
        None,
        AccessTokenInvocation {
            lifetime: None,
            not_after: None,
            purpose: None,
            extensions: authority.extensions,
            session_family: None,
            work_context: authority.work_context,
            provenance: InvocationProvenance::Automated,
        },
        service_principal.id.clone(),
        scopes,
    )
    .await
}

pub fn client_credentials_principal(
    authorization_server: &ResourceAuthorizationServer,
    client_id: &OAuthClientId,
    tenant: &TenantId,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<Principal> {
    let subject = TokenSubject::parse(client_id.as_str())?;
    Ok(Principal {
        id: PrincipalId::parse(format!("{}#{subject}", authorization_server.issuer))?,
        kind: PrincipalKind::Service,
        issuer: authorization_server.issuer.clone(),
        subject,
        tenant: Some(tenant.clone()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: scopes.clone(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn issue_access_token(
    catalog: &HubCatalog,
    authorization_server: &ResourceAuthorizationServer,
    protected_resource: &ProtectedResourceId,
    subject: &TokenSubject,
    client_id: &OAuthClientId,
    principal_kind: PrincipalKind,
    principal: Option<&Principal>,
    principal_display_name: Option<&PrincipalDisplayName>,
    service_tenant: Option<&TenantId>,
    invocation: AccessTokenInvocation,
    principal_id: PrincipalId,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<IssuedAccessToken> {
    let signing_key = access_token_signing_key(catalog, authorization_server).await?;
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::seconds(
            invocation
                .lifetime
                .map_or(ACCESS_TOKEN_TTL_SECONDS, |value| i64::from(value.seconds())),
        ))
        .ok_or_else(|| anyhow!("access token expiration overflow"))?;
    let expires_at = invocation
        .not_after
        .map_or(expires_at, |limit| std::cmp::min(expires_at, limit));
    anyhow::ensure!(expires_at > now, "access-token authority has expired");
    let jwt_id = JwtId::parse(uuid::Uuid::new_v4().to_string())?;
    let scope = (!scopes.is_empty()).then(|| {
        scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    });
    crate::auth::validate_access_token_extensions(&invocation.extensions)?;
    let claims = AccessTokenClaims {
        purpose: invocation.purpose,
        iss: authorization_server.issuer.to_string(),
        sub: subject.to_string(),
        principal_id: principal_id.to_string(),
        principal_display_name: principal_display_name.map(ToString::to_string),
        client_id: client_id.to_string(),
        session_family: invocation.session_family,
        extensions: invocation.extensions.wire(),
        work_context: invocation.work_context.to_string(),
        invocation_mode: invocation.provenance.mode(),
        initiator: invocation.provenance.initiator().map(ToString::to_string),
        delegation_id: match &invocation.provenance {
            InvocationProvenance::Delegated { delegation_id, .. } => {
                Some(delegation_id.to_string())
            }
            InvocationProvenance::Direct { .. } | InvocationProvenance::Automated => None,
        },
        aud: protected_resource.to_string(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id.to_string(),
        principal_kind,
        scope,
        groups: principal
            .map(|principal| {
                principal
                    .groups
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        roles: principal
            .map(|principal| {
                principal
                    .roles
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        tenant: principal
            .and_then(|principal| principal.tenant.as_ref())
            .or(service_tenant)
            .map(ToString::to_string),
        data_labels: principal
            .map(|principal| {
                principal
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        principal_assurances: principal
            .map(|principal| {
                principal
                    .assurances
                    .iter()
                    .map(|assurance| match assurance {
                        veoveo_hub_sdk::PrincipalAssurance::UsPerson => "us_person".to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    let access_token = encode(&signing_key.header(), &claims, signing_key.encoding_key())?;
    Ok(IssuedAccessToken {
        access_token,
        jwt_id,
        expires_at,
        scopes: scopes.clone(),
    })
}

use crate::http::auth_support::access_token_signing_key;
pub use crate::http::auth_support::authorization_server_jwks_from_signing_key;

fn unix_seconds(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("timestamp before Unix epoch"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_access_token_debug_redacts_bearer_value() {
        let issued = IssuedAccessToken {
            access_token: "sensitive-access-token".to_owned(),
            jwt_id: JwtId::parse("test-jwt-id").unwrap(),
            expires_at: Utc::now(),
            scopes: BTreeSet::new(),
        };
        let debug = format!("{issued:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("sensitive-access-token"));
    }
}
