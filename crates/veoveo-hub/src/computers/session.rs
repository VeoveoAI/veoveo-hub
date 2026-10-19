//! A browser cookie selects a live native refresh family; it is never an access JWT.
use std::time::Duration;

use anyhow::{Context, Result, ensure};
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
            let Some((computer, family_id)) = self.hub_store().computer_session(digest).await?
            else {
                return Ok(None);
            };
            if !computer.ready
                || computer.handoff.is_some()
                || computer.family.as_ref() != Some(&family_id)
                || route.is_some_and(|id| id != &computer.id)
            {
                return Ok(None);
            }
            let Some(family) = self.hub_store().refresh_family(&family_id).await? else {
                return Ok(None);
            };
            self.admit_computer_family(catalog, config, computer, family)
                .await
                .map(Some)
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
    ) -> Result<BrowserSession> {
        let profile = catalog
            .profile(&config.profile)
            .context("Computer profile unavailable")?;
        let client = self
            .effective_oauth_client(catalog, &config.oauth_client)
            .await?
            .context("Computer browser client unavailable")?;
        let principal = &family.principal.principal;
        ensure!(
            family.revoked_at.is_none() && family.expires_at > Utc::now(),
            "Computer family expired or revoked"
        );
        ensure!(
            family.profile == config.profile
                && family.authorization_server == profile.authorization_server
                && family.oauth_client_id == config.oauth_client
                && family.work_context == config.work_context,
            "Computer family binding differs from installation"
        );
        ensure!(
            principal.kind == PrincipalKind::User
                && principal.tenant.as_ref() == Some(&computer.tenant)
                && principal.id == computer.principal,
            "Computer owner mismatch"
        );
        ensure!(
            client.registration.authorization_server == profile.authorization_server
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
                    .contains(&OAuthGrantType::RefreshToken),
            "current Computer client denies family authority"
        );
        client.membership(catalog, &family.work_context, principal)?;
        Ok(BrowserSession {
            computer,
            family: family.id,
            principal: family.principal.principal,
            expires_at: family.expires_at,
        })
    }
}
