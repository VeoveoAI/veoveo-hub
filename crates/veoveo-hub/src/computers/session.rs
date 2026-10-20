//! A browser cookie selects a live native refresh family; it is never an access JWT.
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use veoveo_hub_sdk::{HubRefreshFamilyId, Principal, PrincipalKind};
use veoveo_types::Sha256Digest;

use super::ComputerId;
use crate::{
    HubCatalog, HubState,
    contract::{OAuthClientAuthMethod, OAuthGrantType},
    control_plane::Computers,
    store::{Computer, RefreshFamily},
};

/// Poll cadence for already-open streams. With the separate five-second read
/// deadline, revocation is observed within ten seconds of normally scheduled
/// checks. Family expiry is an independent hard deadline; reads fail closed.
pub const STREAM_ADMISSION_INTERVAL: Duration = Duration::from_secs(5);
pub const ADMISSION_READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct BrowserSession {
    computer: Computer,
    family: HubRefreshFamilyId,
    principal: Principal,
    expires_at: DateTime<Utc>,
}
impl BrowserSession {
    pub fn computer(&self) -> &Computer {
        &self.computer
    }
    pub fn family(&self) -> &HubRefreshFamilyId {
        &self.family
    }
    pub fn principal(&self) -> &Principal {
        &self.principal
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

impl HubState {
    /// Read fresh authority for every request and periodically for open streams.
    /// The configuration selects the installed browser client; owner equality is
    /// mandatory even for administrators. Refresh generation is deliberately irrelevant.
    pub async fn computer_browser_session(
        &self,
        catalog: &HubCatalog,
        config: &Computers,
        digest: &Sha256Digest,
        route: Option<&ComputerId>,
    ) -> Result<Option<BrowserSession>> {
        tokio::time::timeout(ADMISSION_READ_TIMEOUT, async {
            let Some((computer, family)) = self.hub_store().computer_session(digest).await? else {
                return Ok(None);
            };
            if !computer.ready
                || computer.handoff.is_some()
                || computer.family.as_ref() != Some(&family.id)
                || route.is_some_and(|id| id != &computer.id)
            {
                return Ok(None);
            }
            self.admit_computer_family(catalog, config, computer, family)
                .await
        })
        .await
        .context("Computer admission read deadline")?
    }

    pub(super) async fn admit_computer_family(
        &self,
        catalog: &HubCatalog,
        config: &Computers,
        computer: Computer,
        family: RefreshFamily,
    ) -> Result<Option<BrowserSession>> {
        let Some(profile) = catalog.profile(&config.profile) else {
            return Ok(None);
        };
        let Some(client) = self
            .effective_oauth_client(catalog, &config.oauth_client)
            .await?
        else {
            return Ok(None);
        };
        let principal = &family.principal.principal;
        if family.revoked_at.is_some() || family.expires_at <= Utc::now() {
            return Ok(None);
        }
        if !(family.protected_resource == profile.protected_resource
            && family.authorization_server == profile.authorization_server
            && family.oauth_client_id == config.oauth_client
            && family.work_context == config.work_context)
        {
            return Ok(None);
        }
        if !(principal.kind == PrincipalKind::User
            && principal.tenant.as_ref() == Some(&computer.tenant)
            && principal.id == computer.principal)
        {
            return Ok(None);
        }
        if !(client.registration.authorization_server == profile.authorization_server
            && client
                .registration
                .allowed_resources
                .contains(&profile.protected_resource)
            && family.scopes.is_subset(&client.registration.allowed_scopes)
            && profile
                .required_scopes
                .iter()
                .all(|scope| family.scopes.contains(scope))
            && client.registration.invocation_mode == veoveo_types::InvocationMode::Direct
            && client
                .registration
                .auth_methods
                .contains(&OAuthClientAuthMethod::None)
            && client
                .registration
                .grant_types
                .contains(&OAuthGrantType::AuthorizationCodePkce)
            && client
                .registration
                .grant_types
                .contains(&OAuthGrantType::RefreshToken))
        {
            return Ok(None);
        }
        if client
            .membership(catalog, &family.work_context, principal)
            .is_err()
        {
            return Ok(None);
        }
        Ok(Some(BrowserSession {
            computer,
            family: family.id,
            principal: family.principal.principal,
            expires_at: family.expires_at,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HubRefreshIssueRequest, contract::PrincipalDisplayName};
    use std::collections::BTreeSet;
    use veoveo_hub_sdk::{TokenIssuer, TokenSubject};
    use veoveo_types::{PrincipalId, TenantId};

    /// Exercise the real family/store path: stale authorization restarts login,
    /// while loss of the database remains an operational refusal, not a login.
    #[tokio::test]
    async fn revoked_browser_family_is_denied_but_store_failure_is_unavailable() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::new().await;
            let hub = HubState::new(db.a.clone())
                .bind_token_extensions(Default::default()).unwrap()
                .bind_oauth_client_resolver(std::sync::Arc::new(crate::oauth_clients::CatalogOAuthClientResolver)).unwrap();
            let catalog = HubCatalog::from_control_plane(
                serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap(),
                crate::catalog_fixture::binding(),
            ).unwrap();
            let config: Computers = serde_json::from_value(serde_json::json!({
                "gateway": {"endpoint": "https://gateway.example/", "certificateAuthority": "/unused/ca", "bearerEnvironment": "UNUSED"},
                "template": {"workspace": "installation", "name": "computer"},
                "policyFile": "/unused/policy", "command": ["/unused/app"],
                "oauthClient": "operator-local-public", "profile": "operator", "workContext": "operations", "port": 4096,
                "hubProvider": {"profile": "fixture-hub", "credentialKey": "HUB_REFERENCE"},
                "modelProvider": {"profile": "fixture-model", "credentialKey": "MODEL_REFERENCE", "secret": "unused"}
            })).unwrap();
            let profile = catalog.profile(&config.profile).unwrap();
            let principal = Principal {
                id: PrincipalId::parse("fixture-user").unwrap(), kind: PrincipalKind::User,
                issuer: TokenIssuer::parse("https://identity.example").unwrap(), subject: TokenSubject::parse("fixture-user").unwrap(),
                tenant: Some(TenantId::parse("tenant-a").unwrap()), groups: BTreeSet::new(), group_roles: BTreeSet::new(), roles: BTreeSet::new(),
                scopes: profile.required_scopes.iter().cloned().collect(), data_labels: BTreeSet::new(), assurances: BTreeSet::new(), authenticated_at: None,
            };
            let issued = hub.issue_refresh_token(HubRefreshIssueRequest {
                authorization_server: &profile.authorization_server, protected_resource: &profile.protected_resource, oauth_client_id: &config.oauth_client,
                work_context: &config.work_context, principal: &principal, principal_display_name: &PrincipalDisplayName::new("Fixture user").unwrap(),
                scopes: &principal.scopes, now: Utc::now(),
            }).await.unwrap();
            let computer = hub.hub_store().ensure_computer(principal.tenant.as_ref().unwrap(), &principal.id, &crate::computers::ComputerProvisionSpec::fixture(config.clone())).await.unwrap();
            let pending = hub.hub_store().begin_computer_handoff(&computer).await.unwrap();
            let digest = Sha256Digest::from_bytes([42; 32]);
            hub.hub_store().lock_computer_handoff(&computer, pending).await.unwrap().unwrap()
                .complete(&issued.grant.family_id, &digest).await.unwrap();
            assert!(hub.computer_browser_session(&catalog, &config, &digest, None).await.unwrap().is_some());
            hub.revoke_refresh_token_family(&issued.token, &profile.authorization_server, &profile.protected_resource, &config.oauth_client, Utc::now()).await.unwrap();
            assert!(hub.computer_browser_session(&catalog, &config, &digest, None).await.unwrap().is_none(), "revoked cookie must start a fresh login rather than stick at 503");
            db.a.pool().close().await;
            assert!(hub.computer_browser_session(&catalog, &config, &digest, None).await.is_err(), "store loss must not be disguised as an unauthenticated cookie");
        }).await.expect("session fixture deadline");
    }
}
