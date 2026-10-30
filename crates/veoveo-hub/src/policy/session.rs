//! Pure current-session decision. Callers read the exact signed family ID from
//! their authoritative store and charge that read against their access deadline.
use crate::contract::AuthorizationServerId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_hub_sdk::{AccessTokenSubject, OAuthClientId, Principal, ProtectedResourceId};
use veoveo_types::{PrincipalId, ScopeName, TenantId, WorkContextId};

/// Read-only projection of the existing hub refresh-family record. Display
/// metadata, delivery envelopes and database IDs are outside this decision.
#[derive(Deserialize)]
pub struct SessionFamilyAuthority {
    authorization_server: AuthorizationServerId,
    protected_resource: ProtectedResourceId,
    oauth_client_id: OAuthClientId,
    work_context: WorkContextId,
    principal_id: PrincipalId,
    tenant: Option<TenantId>,
    scopes: BTreeSet<ScopeName>,
    principal: StoredPrincipal,
    current_generation: u64,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
struct StoredPrincipal {
    principal: Principal,
}

pub struct SessionRequest<'a> {
    pub protected_resource: &'a ProtectedResourceId,
    pub authorization_server: &'a AuthorizationServerId,
    pub token: &'a AccessTokenSubject,
    pub principal: &'a Principal,
    pub now: DateTime<Utc>,
}
impl SessionFamilyAuthority {
    /// Decode a trusted store record without introducing a store dependency into
    /// policy evaluation. Errors omit record contents, including principal data.
    pub fn from_stored(record: &impl Serialize) -> anyhow::Result<Self> {
        let value = serde_json::to_value(record)
            .map_err(|_| anyhow::anyhow!("session family encoding is invalid"))?;
        serde_json::from_value(value)
            .map_err(|_| anyhow::anyhow!("session family authority is invalid"))
    }
    pub fn allows(&self, request: SessionRequest<'_>) -> bool {
        let SessionRequest {
            protected_resource,
            authorization_server,
            token,
            principal,
            now,
        } = request;
        // Access is bound to the family, not a rotating refresh-token generation.
        // The decoder still rejects negative/corrupt stored generations.
        let _ = self.current_generation;
        token.session_family.is_some() && self.revoked_at.is_none() && self.expires_at > now
            && self.principal_id == principal.id && self.tenant == principal.tenant
            && self.authorization_server == *authorization_server && self.protected_resource == *protected_resource
            && self.oauth_client_id == token.oauth_client_id && self.work_context == token.work_context
            && self.principal.principal.id == principal.id && self.principal.principal.kind == principal.kind
            // The retained principal has its original IdP issuer; the verified
            // access-token principal has the hub issuer and the same identity.
            && token.issuer == principal.issuer && token.subject == principal.subject
            && self.principal.principal.subject == principal.subject
            && self.principal.principal.tenant == principal.tenant
            && token.scopes.is_subset(&self.scopes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, Timelike};
    use veoveo_hub_sdk::{HubRefreshFamilyId, JwtId, PrincipalKind};
    use veoveo_types::{GroupId, InvocationMode, TokenIssuer, TokenSubject};

    struct Fixture {
        family: SessionFamilyAuthority,
        token: AccessTokenSubject,
        principal: Principal,
        resource: ProtectedResourceId,
        server: AuthorizationServerId,
        now: DateTime<Utc>,
    }

    impl Fixture {
        fn allows(&self) -> bool {
            self.family.allows(SessionRequest {
                protected_resource: &self.resource,
                authorization_server: &self.server,
                token: &self.token,
                principal: &self.principal,
                now: self.now,
            })
        }
    }

    fn fixture() -> Fixture {
        let now = DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let resource =
            ProtectedResourceId::parse("https://hub.session-fixture.example/mcp/profile").unwrap();
        let server = AuthorizationServerId::parse("session-fixture").unwrap();
        let principal = Principal {
            id: PrincipalId::parse("https://idp.session-fixture.example#person").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://hub.session-fixture.example/oauth").unwrap(),
            subject: TokenSubject::parse("session-fixture-person").unwrap(),
            tenant: Some(TenantId::parse("session-fixture-tenant").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let token = AccessTokenSubject {
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::parse("session-fixture-client").unwrap(),
            session_family: Some(
                HubRefreshFamilyId::parse("019a33a6-1000-7000-8000-000000000001").unwrap(),
            ),
            audience: resource.clone(),
            work_context: WorkContextId::parse("session-fixture-context").unwrap(),
            invocation_mode: InvocationMode::Direct,
            initiator: Some(principal.id.clone()),
            delegation_id: None,
            scopes: BTreeSet::from([ScopeName::parse("session-fixture:read").unwrap()]),
            jwt_id: Some(JwtId::parse("session-fixture-access").unwrap()),
            issued_at: now,
            not_before: None,
            expires_at: now + TimeDelta::minutes(15),
        };
        let mut retained = principal.clone();
        retained.issuer = TokenIssuer::parse("https://idp.session-fixture.example").unwrap();
        let family = SessionFamilyAuthority {
            authorization_server: server.clone(),
            protected_resource: resource.clone(),
            oauth_client_id: token.oauth_client_id.clone(),
            work_context: token.work_context.clone(),
            principal_id: principal.id.clone(),
            tenant: principal.tenant.clone(),
            scopes: BTreeSet::from([
                ScopeName::parse("session-fixture:read").unwrap(),
                ScopeName::parse("session-fixture:write").unwrap(),
            ]),
            principal: StoredPrincipal {
                principal: retained,
            },
            current_generation: 7,
            expires_at: now + TimeDelta::hours(1),
            revoked_at: None,
        };
        Fixture {
            family,
            token,
            principal,
            resource,
            server,
            now,
        }
    }

    #[test]
    fn session_accepts_empty_equal_and_proper_subset_scopes() {
        let mut case = fixture();
        assert!(case.token.scopes.is_subset(&case.family.scopes));
        assert_ne!(case.token.scopes, case.family.scopes);
        assert!(case.allows(), "a proper subset is valid");
        case.token.scopes = case.family.scopes.clone();
        assert!(case.allows(), "equal scopes are valid");
        case.token.scopes.clear();
        assert!(case.allows(), "empty token scopes are valid");
        case.family.scopes.clear();
        assert!(case.allows(), "both sets may be empty");
    }

    #[test]
    fn session_rejects_every_compared_identity_or_scope_binding() {
        let cases: [(&str, fn(&mut Fixture)); 13] = [
            ("family principal", |f| {
                f.family.principal_id = PrincipalId::parse("session-fixture-other").unwrap()
            }),
            ("family tenant", |f| f.family.tenant = None),
            ("authorization server", |f| {
                f.server = AuthorizationServerId::parse("session-fixture-other").unwrap()
            }),
            ("protected resource", |f| {
                f.resource =
                    ProtectedResourceId::parse("https://hub.session-fixture.example/mcp/other")
                        .unwrap()
            }),
            ("OAuth client", |f| {
                f.token.oauth_client_id = OAuthClientId::parse("session-fixture-other").unwrap()
            }),
            ("Work Context", |f| {
                f.token.work_context = WorkContextId::parse("session-fixture-other").unwrap()
            }),
            ("retained principal", |f| {
                f.family.principal.principal.id =
                    PrincipalId::parse("session-fixture-other").unwrap()
            }),
            ("principal kind", |f| {
                f.family.principal.principal.kind = PrincipalKind::Service
            }),
            ("verified token issuer", |f| {
                f.token.issuer =
                    TokenIssuer::parse("https://other.session-fixture.example").unwrap()
            }),
            ("verified token subject", |f| {
                f.token.subject = TokenSubject::parse("session-fixture-other").unwrap()
            }),
            ("retained subject", |f| {
                f.family.principal.principal.subject =
                    TokenSubject::parse("session-fixture-other").unwrap()
            }),
            ("retained tenant", |f| {
                f.family.principal.principal.tenant = None
            }),
            ("token scope outside family", |f| {
                f.token.scopes = f.family.scopes.clone();
                f.token
                    .scopes
                    .insert(ScopeName::parse("session-fixture:outside").unwrap());
            }),
        ];
        for (label, mutate) in cases {
            let mut case = fixture();
            case.token.scopes = case.family.scopes.clone();
            assert!(case.allows(), "valid fixture before {label}");
            mutate(&mut case);
            assert!(!case.allows(), "must bind {label}");
        }
    }

    #[test]
    fn session_requires_present_unrevoked_and_strictly_live_family() {
        let mut case = fixture();
        case.token.session_family = None;
        assert!(!case.allows(), "pure predicate requires family presence");
        for revoked in [
            case.now - TimeDelta::seconds(1),
            case.now + TimeDelta::seconds(1),
        ] {
            let mut case = fixture();
            case.family.revoked_at = Some(revoked);
            assert!(!case.allows(), "any recorded revocation refuses");
        }
        let mut case = fixture();
        case.family.expires_at = case.now - TimeDelta::nanoseconds(1);
        assert!(!case.allows(), "past expiry refuses");
        case.family.expires_at = case.now;
        assert!(!case.allows(), "equal expiry refuses");
        case.family.expires_at = case.now.with_nanosecond(1).unwrap();
        assert!(
            case.allows(),
            "one nanosecond later in the same second is valid"
        );
    }

    #[test]
    fn session_preserves_rotation_upstream_issuer_and_unread_metadata() {
        let mut case = fixture();
        assert_ne!(
            case.family.principal.principal.issuer,
            case.principal.issuer
        );
        assert_eq!(case.token.issuer, case.principal.issuer);
        for generation in [0, 1, 7, u64::MAX] {
            case.family.current_generation = generation;
            assert!(
                case.allows(),
                "refresh generation {generation} does not bind access"
            );
        }
        case.family.principal.principal.issuer =
            TokenIssuer::parse("https://another-idp.session-fixture.example").unwrap();
        assert!(
            case.allows(),
            "retained original issuer is not the verified Hub issuer"
        );
        case.principal
            .groups
            .insert(GroupId::parse("session-fixture-group").unwrap());
        case.principal
            .scopes
            .insert(ScopeName::parse("session-fixture:metadata").unwrap());
        case.principal.authenticated_at = Some(case.now - TimeDelta::minutes(5));
        case.token.jwt_id = Some(JwtId::parse("session-fixture-other-access").unwrap());
        assert!(
            case.allows(),
            "unread metadata does not alter the pure predicate"
        );
        // Exact signed-ID lookup belongs to the state caller. This predicate
        // observes presence only; it cannot compare an absent family-ID field.
        case.token.session_family =
            Some(HubRefreshFamilyId::parse("019a33a6-1000-7000-8000-000000000002").unwrap());
        assert!(case.allows(), "pure family-ID presence is unchanged");
    }
}
