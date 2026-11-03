//! Current browser-family binding for signed access tokens. Never cache positive results.
use std::collections::BTreeSet;

use crate::contract::AuthorizationServerId;
use crate::store::RefreshFamily;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_hub_sdk::{AccessTokenSubject, OAuthClientId, Principal, ProtectedResourceId};
use veoveo_types::{PrincipalId, ScopeName, TenantId};

use super::HubState;
use crate::policy::session::{SessionFamilyAuthority, SessionRequest};

impl HubState {
    /// Unbound service/exchange tokens use their existing token authority. They do
    /// not acquire a browser session or permission for session-bound renewal.
    pub async fn access_token_session_valid(
        &self,
        protected_resource: &ProtectedResourceId,
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
                    protected_resource,
                    authorization_server,
                    token,
                    principal,
                    now: Utc::now(),
                },
            ),
        )
    }
}

impl HubState {
    /// Whether the login session a caller authenticated with was revoked because one of
    /// its refresh tokens was replayed, which suggests the session was stolen. A running
    /// Task stops acting for such a caller; a logout or a session that simply expired
    /// leaves its Tasks' delegation in place.
    pub async fn session_compromised(&self, token: &AccessTokenSubject) -> Result<bool> {
        Ok(matches!(
            self.caller_session(token).await?,
            CallerSession::Stored { replayed: true, .. }
        ))
    }

    /// The login session a caller's token is bound to, as a running Task reads it at each
    /// exchange: the principal it stored at sign-in, with the identity provider's groups
    /// that a refresh does not read again, and whether it was revoked for a replay.
    pub async fn caller_session(&self, token: &AccessTokenSubject) -> Result<CallerSession> {
        let Some(family_id) = &token.session_family else {
            return Ok(CallerSession::Unbound);
        };
        let family = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.store.refresh_family(family_id),
        )
        .await
        .context("session read deadline")?
        .context("failed to read the caller's session family")?;
        Ok(match family {
            None => CallerSession::Deleted,
            Some(family) => CallerSession::Stored {
                replayed: family.revocation == Some(crate::store::RefreshRevocation::TokenReplay),
                principal: Box::new(family.principal.principal),
            },
        })
    }
}

/// The login session a caller's token is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallerSession {
    /// The token is bound to no session, as a service's is.
    Unbound,
    /// The Hub deleted the session, which it does once the session has expired.
    Deleted,
    /// The session as stored: the principal from sign-in, before the installation's
    /// group memberships are added, and whether it was revoked for a replay.
    Stored {
        principal: Box<Principal>,
        replayed: bool,
    },
}

/// The members of a family the session decision reads.
#[derive(Serialize)]
struct SessionView<'a> {
    authorization_server: &'a AuthorizationServerId,
    protected_resource: &'a ProtectedResourceId,
    oauth_client_id: &'a OAuthClientId,
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
            protected_resource: &family.protected_resource,
            oauth_client_id: &family.oauth_client_id,
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
