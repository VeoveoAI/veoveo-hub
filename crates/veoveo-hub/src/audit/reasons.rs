//! The reason codes the Hub's audit records carry. A policy decision records its
//! policy reason code and an authentication its authentication reason code, each in its
//! own wire spelling; the Hub's other outcomes use [`HubReason`]. A successful
//! action records no reason.
use veoveo_audit_contract::AuditReason;
use veoveo_types::Vocabulary;

use crate::contract::AuthReasonCode;
use crate::control_plane::PolicyReasonCode;

/// Why a Hub action that neither policy nor authentication decided ended as it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum HubReason {
    /// The request was malformed or named something the Hub refuses.
    InvalidRequest,
    /// The Hub failed after admitting the action.
    InternalFailure,
    /// The target does not exist.
    NotFound,
    /// The registered server answered with an error or did not answer.
    UpstreamFailure,
    /// The server the request reached refused it.
    UpstreamDenied,
    /// The caller lacks the scope the action requires.
    MissingScope,
    /// The caller may not read the partition it named.
    PartitionDenied,
    /// The caller's session or grant changed after it was admitted.
    Revoked,
    /// The Hub's state was unavailable.
    Unavailable,
}

impl From<HubReason> for AuditReason {
    fn from(reason: HubReason) -> Self {
        code_reason(reason)
    }
}

/// The reason a Hub vocabulary value records: its snake_case wire spelling, such as
/// a refused token exchange's OAuth error or a failed administrative operation's step.
pub fn code_reason<V: Vocabulary>(code: V) -> AuditReason {
    AuditReason::parse(code.as_str()).expect("Hub vocabularies spell audit reasons")
}

/// The reason of a policy decision; an allowed decision records none.
pub fn policy_reason(code: PolicyReasonCode) -> Option<AuditReason> {
    (code != PolicyReasonCode::PolicyAllow)
        .then(|| AuditReason::parse(code.as_str()).expect("policy codes are audit reasons"))
}

/// The reason of an authentication event; an allowed one records none.
pub fn authentication_reason(code: AuthReasonCode) -> Option<AuditReason> {
    (code != AuthReasonCode::AuthAllow)
        .then(|| AuditReason::parse(code.as_str()).expect("authentication codes are audit reasons"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hub_code_is_an_audit_reason() {
        for reason in HubReason::ALL {
            assert_eq!(AuditReason::from(*reason).as_str(), reason.as_str());
        }
        for refusal in super::super::ExchangeRefusal::ALL {
            assert_eq!(code_reason(*refusal).as_str(), refusal.as_str());
        }
        for failure in super::super::AdminOperationFailure::ALL {
            assert_eq!(code_reason(*failure).as_str(), failure.as_str());
        }
    }

    #[test]
    fn policy_codes_record_their_wire_spelling() {
        use PolicyReasonCode::*;
        for code in [
            PolicyDeny,
            UnknownProfile,
            UnknownServer,
            UnknownTool,
            UnknownResource,
            UnknownPrompt,
            UnknownTask,
            UnknownPrincipal,
            UnknownScope,
            UnknownDataLabel,
            UnknownTenant,
            UnknownTokenIssuer,
            MissingPrincipal,
            MissingTenant,
            MissingGroup,
            MissingRole,
            MissingScope,
            MissingDataLabel,
            MissingPrincipalAssurance,
            TokenAudienceMismatch,
            TokenExpired,
            TokenNotYetValid,
            ReplayDetected,
            UndeclaredTarget,
            MissingPermission,
        ] {
            let wire = serde_json::to_value(code).unwrap();
            assert_eq!(wire.as_str(), Some(code.as_str()));
            assert_eq!(policy_reason(code).unwrap().as_str(), code.as_str());
        }
        assert_eq!(policy_reason(PolicyAllow), None);
    }

    #[test]
    fn authentication_codes_record_their_wire_spelling() {
        use AuthReasonCode::*;
        for code in [
            MissingAuthorizationHeader,
            InvalidAuthorizationHeader,
            UnknownIdentityProvider,
            UnknownAuthorizationServer,
            IdentityProviderUnavailable,
            AuthorizationServerUnavailable,
            InvalidAuthConfig,
            InvalidBearerToken,
            InvalidAuthorizationRequest,
            InvalidAuthorizationCode,
            InvalidRefreshToken,
            RefreshTokenDuplicateDelivery,
            RefreshTokenRevoked,
            RefreshTokenReplay,
            InvalidPkce,
            InvalidOidcIdToken,
            InvalidClient,
            UnsupportedGrantType,
            InvalidClientAssertion,
            ClientAssertionReplay,
            InvalidIdentityAssertion,
            IdentityAssertionReplay,
            InvalidScope,
            TokenSigningKeyUnavailable,
            TokenRevoked,
            AuthStateUnavailable,
            PolicyDenied,
        ] {
            assert_eq!(authentication_reason(code).unwrap().as_str(), code.as_str());
        }
        assert_eq!(authentication_reason(AuthAllow), None);
    }
}
