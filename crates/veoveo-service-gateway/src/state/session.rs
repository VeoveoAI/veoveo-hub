//! Current browser-family binding for signed access tokens. Never cache positive results.
use std::collections::BTreeSet;

use crate::contract::AuthorizationServerId;
use crate::store::RefreshFamily;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_sdk_rust::{AccessTokenSubject, GatewayProfileId, OAuthClientId, Principal};
use veoveo_types::{PrincipalId, ScopeName, TenantId, WorkContextId};

use super::GatewayState;
use crate::policy::session::{SessionFamilyAuthority, SessionRequest};

impl GatewayState {
    /// Unbound service/exchange tokens use their existing token authority. They do
    /// not acquire a browser session or permission for session-bound renewal.
    pub async fn access_token_session_valid(
        &self,
        profile: &GatewayProfileId,
        authorization_server: &AuthorizationServerId,
        token: &AccessTokenSubject,
        principal: &Principal,
    ) -> Result<bool> {
        let Some(family_id) = &token.session_family else {
            return Ok(true);
        };
        let family = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.store.refresh_family(family_id),
        )
        .await
        .context("access-token session read deadline")?
        .context("failed to read access-token session family")?;
        let Some(family) = family else {
            return Ok(false);
        };
        Ok(
            SessionFamilyAuthority::from_stored(&SessionView::from(&family))?.allows(
                SessionRequest {
                    profile,
                    authorization_server,
                    token,
                    principal,
                    now: Utc::now(),
                },
            ),
        )
    }
}

impl GatewayState {
    /// Whether the login session a caller authenticated with was revoked because one of
    /// its refresh tokens was replayed, which suggests the session was stolen. A running
    /// Task stops acting for such a caller; a logout or a session that simply expired
    /// leaves its Tasks' delegation in place.
    pub async fn session_compromised(&self, token: &AccessTokenSubject) -> Result<bool> {
        let Some(family_id) = &token.session_family else {
            return Ok(false);
        };
        let family = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.store.refresh_family(family_id),
        )
        .await
        .context("session revocation read deadline")?
        .context("failed to read the caller's session family")?;
        Ok(family.is_some_and(|family| {
            family.revocation == Some(crate::store::RefreshRevocation::TokenReplay)
        }))
    }
}

/// The members of a family the session decision reads.
#[derive(Serialize)]
struct SessionView<'a> {
    authorization_server: &'a AuthorizationServerId,
    profile: &'a GatewayProfileId,
    oauth_client_id: &'a OAuthClientId,
    work_context: &'a WorkContextId,
    principal_id: &'a PrincipalId,
    tenant: &'a Option<TenantId>,
    scopes: &'a BTreeSet<ScopeName>,
    principal: SessionPrincipal<'a>,
    current_generation: u64,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct SessionPrincipal<'a> {
    principal: &'a Principal,
}

impl<'a> From<&'a RefreshFamily> for SessionView<'a> {
    fn from(family: &'a RefreshFamily) -> Self {
        let principal = &family.principal.principal;
        Self {
            authorization_server: &family.authorization_server,
            profile: &family.profile,
            oauth_client_id: &family.oauth_client_id,
            work_context: &family.work_context,
            principal_id: &principal.id,
            tenant: &principal.tenant,
            scopes: &family.scopes,
            principal: SessionPrincipal { principal },
            current_generation: family.current_generation,
            expires_at: family.expires_at,
            revoked_at: family.revoked_at,
        }
    }
}
