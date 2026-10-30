use crate::contract::AuthorizationServerId;
use std::{collections::BTreeSet, num::NonZeroU32, sync::Arc};

use crate::contract::PrincipalDisplayName;
use crate::control_plane::{HubRefreshGrant, OAuthRefreshToken};
use crate::store::{
    NewRefreshFamily, NewRefreshToken, RefreshBinding, RefreshDeliveryEnvelope, RefreshFamily,
    RefreshPrincipal, RefreshRetention, RefreshRotation,
};
use anyhow::{Context, Result, anyhow};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD},
};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use chrono::{DateTime, TimeDelta, Utc};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use veoveo_hub_sdk::{HubRefreshFamilyId, OAuthClientId, Principal, ProtectedResourceId};
use veoveo_store::{MaintenanceBatch, Pruned};
use veoveo_types::{ScopeName, Sha256Digest, WorkContextId};

use super::HubState;

pub const REFRESH_TOKEN_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const REFRESH_DELIVERY_NONCE_BYTES: usize = 24;
const MAX_REFRESH_DELIVERY_WINDOW_SECONDS: u32 = 30;

#[derive(Clone)]
pub struct RefreshTokenDeliveryCipher(Arc<XChaCha20Poly1305>);

impl std::fmt::Debug for RefreshTokenDeliveryCipher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RefreshTokenDeliveryCipher")
            .finish_non_exhaustive()
    }
}

impl RefreshTokenDeliveryCipher {
    pub fn from_base64(encoded: &str) -> Result<Self> {
        let key = BASE64_STANDARD
            .decode(encoded.trim())
            .context("refresh delivery key must be standard base64")?;
        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| anyhow!("refresh delivery key must decode to exactly 32 bytes"))?;
        Self::new(&key)
    }

    pub fn new(key: &[u8; 32]) -> Result<Self> {
        Ok(Self(Arc::new(
            XChaCha20Poly1305::new_from_slice(key)
                .map_err(|_| anyhow!("refresh delivery key must be exactly 32 bytes"))?,
        )))
    }

    fn seal(&self, token: &OAuthRefreshToken, aad: &[u8]) -> Result<SecretString> {
        let mut nonce = [0_u8; REFRESH_DELIVERY_NONCE_BYTES];
        getrandom::fill(&mut nonce).context("failed to generate refresh delivery nonce")?;
        let ciphertext = self
            .0
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: token.as_str().as_bytes(),
                    aad,
                },
            )
            .map_err(|_| anyhow!("failed to encrypt refresh delivery envelope"))?;
        let mut envelope = Vec::with_capacity(nonce.len() + ciphertext.len());
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&ciphertext);
        Ok(SecretString::from(URL_SAFE_NO_PAD.encode(envelope)))
    }

    fn open(&self, envelope: &SecretString, aad: &[u8]) -> Result<OAuthRefreshToken> {
        let envelope = URL_SAFE_NO_PAD
            .decode(envelope.expose_secret())
            .context("refresh delivery envelope is not base64url")?;
        let (nonce, ciphertext) = envelope
            .split_at_checked(REFRESH_DELIVERY_NONCE_BYTES)
            .context("refresh delivery envelope is truncated")?;
        let nonce: [u8; REFRESH_DELIVERY_NONCE_BYTES] = nonce
            .try_into()
            .context("refresh delivery nonce is invalid")?;
        let plaintext = self
            .0
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| anyhow!("refresh delivery envelope authentication failed"))?;
        let token = String::from_utf8(plaintext).context("refresh delivery token is not UTF-8")?;
        OAuthRefreshToken::parse(token).map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HubRefreshDeliveryWindow(TimeDelta);

impl HubRefreshDeliveryWindow {
    pub fn from_seconds(seconds: NonZeroU32) -> Result<Self> {
        if seconds.get() > MAX_REFRESH_DELIVERY_WINDOW_SECONDS {
            anyhow::bail!(
                "refresh delivery window must be at most {MAX_REFRESH_DELIVERY_WINDOW_SECONDS} seconds"
            );
        }
        Ok(Self(TimeDelta::seconds(i64::from(seconds.get()))))
    }

    fn expires_at(self, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
        now.checked_add_signed(self.0)
            .context("refresh delivery window overflow")
    }
}

pub struct HubRefreshRotationRequest<'a> {
    pub authorization_server: &'a AuthorizationServerId,
    pub protected_resource: &'a ProtectedResourceId,
    pub oauth_client_id: &'a OAuthClientId,
    pub now: DateTime<Utc>,
    pub delivery_window: HubRefreshDeliveryWindow,
    pub delivery_cipher: &'a RefreshTokenDeliveryCipher,
    pub success_audit: &'a veoveo_audit_contract::AuditDraft,
    pub duplicate_delivery_audit: &'a veoveo_audit_contract::AuditDraft,
}

pub struct HubRefreshIssueRequest<'a> {
    pub authorization_server: &'a AuthorizationServerId,
    pub protected_resource: &'a ProtectedResourceId,
    pub oauth_client_id: &'a OAuthClientId,
    pub work_context: &'a WorkContextId,
    pub principal: &'a Principal,
    pub principal_display_name: &'a PrincipalDisplayName,
    pub scopes: &'a BTreeSet<ScopeName>,
    pub now: DateTime<Utc>,
}

#[derive(Clone)]
pub struct IssuedHubRefreshToken {
    pub token: OAuthRefreshToken,
    pub grant: HubRefreshGrant,
}

impl std::fmt::Debug for IssuedHubRefreshToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IssuedHubRefreshToken")
            .field("token", &"[REDACTED]")
            .field("grant", &self.grant)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum HubRefreshExchange {
    Rotated(IssuedHubRefreshToken),
    DuplicateDelivery(IssuedHubRefreshToken),
    ReplayDetected { grant: HubRefreshGrant },
    Invalid,
}

impl HubState {
    /// An omitted refresh resource means this retained grant's exact resource,
    /// not a default inferred from the client's current resource registrations.
    /// Consumed predecessors still reach the atomic delivery/replay checks.
    pub async fn refresh_token_resource(
        &self,
        token: &OAuthRefreshToken,
        authorization_server: &AuthorizationServerId,
        oauth_client_id: &OAuthClientId,
        now: DateTime<Utc>,
    ) -> Result<Option<ProtectedResourceId>> {
        let Some((current, family)) = self
            .store
            .refresh_grant(&refresh_token_digest(token))
            .await
            .context("failed to read hub refresh-token resource")?
        else {
            return Ok(None);
        };
        if &family.authorization_server != authorization_server
            || &family.oauth_client_id != oauth_client_id
            || current.expires_at <= now
            || family.expires_at <= now
            || family.revoked_at.is_some()
        {
            return Ok(None);
        }
        Ok(Some(family.protected_resource))
    }

    pub async fn refresh_token_grant(
        &self,
        token: &OAuthRefreshToken,
        authorization_server: &AuthorizationServerId,
        protected_resource: &ProtectedResourceId,
        oauth_client_id: &OAuthClientId,
        now: DateTime<Utc>,
    ) -> Result<Option<HubRefreshGrant>> {
        let Some((current, family)) = self
            .store
            .refresh_grant(&refresh_token_digest(token))
            .await
            .context("failed to read hub refresh-token grant")?
        else {
            return Ok(None);
        };
        let binding = RefreshBinding {
            authorization_server,
            protected_resource,
            oauth_client_id,
        };
        if !family.bound_to(binding)
            || current.expires_at <= now
            || family.expires_at <= now
            || family.revoked_at.is_some()
        {
            return Ok(None);
        }
        Ok(Some(grant_from_family(family)))
    }

    pub async fn revoke_refresh_token_family(
        &self,
        token: &OAuthRefreshToken,
        authorization_server: &AuthorizationServerId,
        protected_resource: &ProtectedResourceId,
        oauth_client_id: &OAuthClientId,
        now: DateTime<Utc>,
    ) -> Result<Option<HubRefreshGrant>> {
        Ok(self
            .store
            .revoke_refresh_family(
                &refresh_token_digest(token),
                RefreshBinding {
                    authorization_server,
                    protected_resource,
                    oauth_client_id,
                },
                now,
            )
            .await
            .context("failed to revoke hub refresh-token family")?
            .map(grant_from_family))
    }

    pub async fn issue_refresh_token(
        &self,
        request: HubRefreshIssueRequest<'_>,
    ) -> Result<IssuedHubRefreshToken> {
        let HubRefreshIssueRequest {
            authorization_server,
            protected_resource,
            oauth_client_id,
            work_context,
            principal,
            principal_display_name,
            scopes,
            now,
        } = request;
        let expires_at = now
            .checked_add_signed(TimeDelta::seconds(REFRESH_TOKEN_TTL_SECONDS))
            .context("refresh-token lifetime overflow")?;
        let family_id = HubRefreshFamilyId::parse(Uuid::now_v7().to_string())?;
        let token = random_refresh_token()?;
        let family = NewRefreshFamily {
            id: family_id.clone(),
            authorization_server: authorization_server.clone(),
            protected_resource: protected_resource.clone(),
            oauth_client_id: oauth_client_id.clone(),
            work_context: work_context.clone(),
            principal: RefreshPrincipal {
                principal: principal.clone(),
                display_name: principal_display_name.clone(),
            },
            scopes: scopes.clone(),
            issued_at: now,
            expires_at,
        };
        let first = NewRefreshToken {
            id: Uuid::now_v7(),
            digest: refresh_token_digest(&token),
            generation: 0,
            issued_at: now,
            expires_at,
            delivery: None,
        };
        self.store
            .create_refresh_family(&family, &first)
            .await
            .context("failed to create hub refresh-token family")?;
        Ok(IssuedHubRefreshToken {
            token,
            grant: HubRefreshGrant {
                family_id,
                authorization_server: authorization_server.clone(),
                protected_resource: protected_resource.clone(),
                oauth_client_id: oauth_client_id.clone(),
                work_context: work_context.clone(),
                principal: principal.clone(),
                principal_display_name: principal_display_name.clone(),
                scopes: scopes.clone(),
                generation: 0,
                issued_at: now,
                expires_at,
            },
        })
    }

    pub async fn rotate_refresh_token(
        &self,
        token: &OAuthRefreshToken,
        request: HubRefreshRotationRequest<'_>,
    ) -> Result<HubRefreshExchange> {
        let digest = refresh_token_digest(token);
        let Some((current, family)) = self
            .store
            .refresh_grant(&digest)
            .await
            .context("failed to read hub refresh-token grant")?
        else {
            return Ok(HubRefreshExchange::Invalid);
        };
        let binding = RefreshBinding {
            authorization_server: request.authorization_server,
            protected_resource: request.protected_resource,
            oauth_client_id: request.oauth_client_id,
        };
        if !family.bound_to(binding)
            || current.expires_at <= request.now
            || family.expires_at <= request.now
        {
            return Ok(HubRefreshExchange::Invalid);
        }
        let next_generation = current
            .generation
            .checked_add(1)
            .context("refresh-token generation overflow")?;
        let replacement_token = random_refresh_token()?;
        let delivery = RefreshDeliveryEnvelope {
            envelope: request.delivery_cipher.seal(
                &replacement_token,
                &refresh_delivery_aad(&family, next_generation),
            )?,
            expires_at: std::cmp::min(
                request.delivery_window.expires_at(request.now)?,
                family.expires_at,
            ),
        };
        let replacement = NewRefreshToken {
            id: Uuid::now_v7(),
            digest: refresh_token_digest(&replacement_token),
            generation: next_generation,
            issued_at: request.now,
            expires_at: family.expires_at,
            delivery: Some(delivery),
        };
        let rotation = self
            .store
            .rotate_refresh_token(
                &digest,
                &replacement,
                request.now,
                &self.audit_write(request.success_audit)?,
                &self.audit_write(request.duplicate_delivery_audit)?,
            )
            .await
            .context("failed to rotate hub refresh token")?;
        match rotation {
            RefreshRotation::Rotated { family, .. } => {
                Ok(HubRefreshExchange::Rotated(IssuedHubRefreshToken {
                    token: replacement_token,
                    grant: grant_from_family(family),
                }))
            }
            RefreshRotation::Redelivered {
                family,
                replacement,
            } => {
                let envelope = replacement
                    .delivery
                    .as_ref()
                    .context("duplicate refresh delivery is missing its encrypted envelope")?;
                let token = request.delivery_cipher.open(
                    &envelope.envelope,
                    &refresh_delivery_aad(&family, replacement.generation),
                )?;
                if refresh_token_digest(&token) != replacement.digest {
                    anyhow::bail!("duplicate refresh delivery token does not match its record");
                }
                Ok(HubRefreshExchange::DuplicateDelivery(
                    IssuedHubRefreshToken {
                        token,
                        grant: grant_from_family(family),
                    },
                ))
            }
            RefreshRotation::ReplayDetected(family) => Ok(HubRefreshExchange::ReplayDetected {
                grant: grant_from_family(family),
            }),
            RefreshRotation::Invalid => Ok(HubRefreshExchange::Invalid),
        }
    }

    /// Clear ended delivery envelopes, then delete expired tokens and families, one
    /// short transaction per batch.
    pub async fn prune_expired_refresh_tokens(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<RefreshRetention> {
        self.store
            .prune_refresh_state(now, batch)
            .await
            .context("failed to prune expired hub refresh-token state")
    }

    /// Clear delivery envelopes whose window ended, one short transaction per batch.
    pub async fn clear_expired_refresh_delivery_envelopes(
        &self,
        now: DateTime<Utc>,
        batch: MaintenanceBatch,
    ) -> Result<Pruned> {
        self.store
            .clear_refresh_delivery_envelopes(now, batch)
            .await
            .context("failed to clear expired hub refresh delivery envelopes")
    }
}

pub(super) fn grant_from_family(family: RefreshFamily) -> HubRefreshGrant {
    HubRefreshGrant {
        family_id: family.id,
        authorization_server: family.authorization_server,
        protected_resource: family.protected_resource,
        oauth_client_id: family.oauth_client_id,
        work_context: family.work_context,
        principal: family.principal.principal,
        principal_display_name: family.principal.display_name,
        scopes: family.scopes,
        generation: family.current_generation,
        issued_at: family.issued_at,
        expires_at: family.expires_at,
    }
}

/// Binds a delivery envelope to its family's client, Work Context and generation.
fn refresh_delivery_aad(family: &RefreshFamily, generation: u64) -> Vec<u8> {
    format!(
        "veoveo-refresh-delivery-v3\0{}\0{}\0{}\0{}\0{}\0{generation}",
        family.authorization_server,
        family.protected_resource,
        family.oauth_client_id,
        family.work_context,
        family.id,
    )
    .into_bytes()
}

fn random_refresh_token() -> Result<OAuthRefreshToken> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("failed to generate refresh-token entropy")?;
    OAuthRefreshToken::parse(URL_SAFE_NO_PAD.encode(bytes)).map_err(Into::into)
}

fn refresh_token_digest(token: &OAuthRefreshToken) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(token.as_str().as_bytes()).into())
}

#[cfg(test)]
mod transaction_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &[u8; 32] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn delivery_envelope_round_trips_only_with_its_bound_identity() {
        let cipher = RefreshTokenDeliveryCipher::new(TEST_KEY).unwrap();
        let token = OAuthRefreshToken::parse("r".repeat(43)).unwrap();
        let envelope = cipher
            .seal(&token, b"protected_resource-a/family-a/1")
            .unwrap();

        assert_ne!(envelope.expose_secret(), token.as_str());
        assert_eq!(
            cipher
                .open(&envelope, b"protected_resource-a/family-a/1")
                .unwrap()
                .as_str(),
            token.as_str(),
        );
        assert!(
            cipher
                .open(&envelope, b"protected_resource-b/family-a/1")
                .is_err()
        );

        let debug = format!("{cipher:?} {envelope:?}");
        assert!(!debug.contains(token.as_str()));
        assert!(!debug.contains(envelope.expose_secret()));
        assert!(!debug.contains("0123456789abcdef"));
    }

    #[test]
    fn delivery_configuration_rejects_wrong_key_length_and_long_windows() {
        let short_key = BASE64_STANDARD.encode([0_u8; 31]);
        assert!(RefreshTokenDeliveryCipher::from_base64(&short_key).is_err());
        assert!(HubRefreshDeliveryWindow::from_seconds(NonZeroU32::new(31).unwrap()).is_err());
    }

    #[test]
    fn refresh_digests_name_their_algorithm_and_never_contain_the_token() {
        let token = OAuthRefreshToken::parse("t".repeat(43)).unwrap();
        let digest = refresh_token_digest(&token);
        assert!(digest.as_str().starts_with("sha256:"));
        assert!(!digest.as_str().contains(token.as_str()));
        assert_eq!(digest, refresh_token_digest(&token));
    }
}
