use std::collections::BTreeSet;
use veoveo_hub_sdk::ProtectedResourceId;

use crate::contract::OidcNonce;
use crate::control_plane::{IdentityProviderClaimMapping, OidcClientId};
use jsonwebtoken::Algorithm;
use veoveo_hub_sdk::{OAuthClientId, TokenIssuer};
use veoveo_types::ScopeName;

use super::support::{AuthError, is_symmetric_algorithm};

#[derive(Debug, Clone)]
pub struct JwtAuthConfig {
    pub issuer: TokenIssuer,
    pub audience: ProtectedResourceId,
    pub required_scopes: BTreeSet<ScopeName>,
    pub algorithms: Vec<Algorithm>,
}

#[derive(Debug, Clone)]
pub struct ClientAssertionConfig {
    pub client_id: OAuthClientId,
    pub audience: String,
    pub algorithms: Vec<Algorithm>,
}

#[derive(Debug, Clone)]
pub struct IdJagConfig {
    pub issuer: TokenIssuer,
    pub audience: TokenIssuer,
    pub resource: ProtectedResourceId,
    pub algorithms: Vec<Algorithm>,
}

#[derive(Debug, Clone)]
pub struct OidcIdTokenConfig {
    pub issuer: TokenIssuer,
    pub client_id: OidcClientId,
    pub nonce: OidcNonce,
    pub algorithms: Vec<Algorithm>,
    pub claim_mapping: IdentityProviderClaimMapping,
}

impl ClientAssertionConfig {
    pub fn new(
        client_id: OAuthClientId,
        audience: impl Into<String>,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        let audience = audience.into();
        if audience.is_empty() {
            return Err(AuthError::InvalidClientAssertionAudience);
        }
        Ok(Self {
            client_id,
            audience,
            algorithms,
        })
    }
}

impl JwtAuthConfig {
    pub fn new(
        issuer: TokenIssuer,
        audience: ProtectedResourceId,
        required_scopes: BTreeSet<ScopeName>,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        Ok(Self {
            issuer,
            audience,
            required_scopes,
            algorithms,
        })
    }
}

impl IdJagConfig {
    pub fn new(
        issuer: TokenIssuer,
        audience: TokenIssuer,
        resource: ProtectedResourceId,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        Ok(Self {
            issuer,
            audience,
            resource,
            algorithms,
        })
    }
}

impl OidcIdTokenConfig {
    pub fn new(
        issuer: TokenIssuer,
        client_id: OidcClientId,
        nonce: OidcNonce,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        Self::new_with_claim_mapping(
            issuer,
            client_id,
            nonce,
            algorithms,
            IdentityProviderClaimMapping::default(),
        )
    }

    pub fn new_with_claim_mapping(
        issuer: TokenIssuer,
        client_id: OidcClientId,
        nonce: OidcNonce,
        algorithms: Vec<Algorithm>,
        claim_mapping: IdentityProviderClaimMapping,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        Ok(Self {
            issuer,
            client_id,
            nonce,
            algorithms,
            claim_mapping,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BearerToken(String);

impl std::fmt::Debug for BearerToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BearerToken([REDACTED])")
    }
}

impl BearerToken {
    /// Admit RFC 6750 §2.1 credentials, `Bearer 1*SP b64token`, with the scheme
    /// compared without regard to case. Another scheme is
    /// [`AuthError::InvalidAuthorizationScheme`]; a bearer credential that is not a
    /// `b64token` is [`AuthError::InvalidBearerToken`].
    pub fn from_authorization_header(value: &str) -> Result<Self, AuthError> {
        let (scheme, credentials) = value.split_once(' ').unwrap_or((value, ""));
        if !scheme.eq_ignore_ascii_case("bearer") {
            return Err(AuthError::InvalidAuthorizationScheme);
        }
        let token = credentials.trim_start_matches(' ');
        if !is_b64token(token) {
            return Err(AuthError::InvalidBearerToken);
        }
        Ok(Self(token.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// RFC 6750 §2.1: `b64token = 1*( ALPHA / DIGIT / "-" / "." / "_" / "~" / "+" / "/" ) *"="`.
fn is_b64token(value: &str) -> bool {
    let body = value.trim_end_matches('=');
    !body.is_empty()
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
}
