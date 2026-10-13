use std::fmt;
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{ClaimTextProfile, PathIdProfile, TokenTextProfile},
};

use crate::contract::validate_oauth_authorization_code;

/// Configured identity provider id used by gateway profiles.
#[veoveo_types::id(text(PathIdProfile))]
pub struct IdentityProviderId(String);

/// Policy rule identifier used for decision evidence.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct PolicyRuleId(String);

/// OIDC client id assigned to the gateway by an enterprise identity provider.
#[veoveo_types::id(text(ClaimTextProfile))]
pub struct OidcClientId(String);

/// Opaque, rotating OAuth refresh token. Only its SHA-256 digest is persisted.
#[veoveo_types::id(text(GatewayRefreshTokens), secret, ordered)]
pub struct OAuthRefreshToken(String);

impl OAuthRefreshToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OAuthRefreshToken {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for OAuthRefreshToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthRefreshToken([REDACTED])")
    }
}

impl fmt::Display for OAuthRefreshToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

/// Request trace/correlation id used in audit and runtime state.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct TraceId(String);

/// Durable gateway control-plane revision id.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct GatewayControlPlaneRevisionId(String);

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct GatewayRefreshTokens;
impl IdProfile for GatewayRefreshTokens {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_refresh_token(value));
}

fn validate_refresh_token(value: &str) -> Result<(), IdentifierError> {
    validate_oauth_authorization_code(value).map_err(|_| {
        IdentifierError::new(
            "[REDACTED]",
            "must be 43 to 128 bytes containing only ASCII letters, digits, hyphen, period, underscore, or tilde",
        )
    })
}
