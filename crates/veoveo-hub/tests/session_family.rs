#[path = "support/database.rs"]
mod database;
use database as fixture;

use chrono::{SubsecRound, TimeDelta, Utc};
use std::{collections::BTreeSet, num::NonZeroU32};
use veoveo_hub::audit::{Authentication, AuthenticationActivity, hub_audit};
use veoveo_hub::contract::{
    AuthMethod, AuthReasonCode, AuthorizationServerId, PrincipalDisplayName,
};
use veoveo_hub::{
    HubRefreshDeliveryWindow, HubRefreshExchange, HubRefreshIssueRequest,
    HubRefreshRotationRequest, HubState, RefreshTokenDeliveryCipher,
};
use veoveo_hub_sdk::ProtectedResourceId;
use veoveo_hub_sdk::*;
use veoveo_types::{InvocationMode, PrincipalId, ScopeName, TenantId, WorkContextId};

#[tokio::test]
async fn session_binding_survives_rotation_and_rejects_cross_replica_revocation() {
    let db = fixture::TestDb::new().await;
    let first = HubState::new(db.a.clone());
    let second = HubState::new(db.b.clone());
    let profile = HubProfileId::parse("operator").unwrap();
    let resource = ProtectedResourceId::parse("https://veoveo.example/mcp/operator").unwrap();
    let authorization_server = AuthorizationServerId::parse("veoveo").unwrap();
    let client = OAuthClientId::parse("console").unwrap();
    let context = WorkContextId::parse("mission").unwrap();
    let principal = Principal {
        id: PrincipalId::parse("https://identity.example#one").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://identity.example").unwrap(),
        subject: TokenSubject::parse("one").unwrap(),
        tenant: Some(TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    };
    let display = PrincipalDisplayName::new("Same Display Name").unwrap();
    let cipher = RefreshTokenDeliveryCipher::new(b"0123456789abcdef0123456789abcdef").unwrap();
    // Each case has its own real family, using the same two independent clients.
    for ending in ["logout", "replay", "expiry", "missing"] {
        let now = Utc::now().trunc_subsecs(6);
        let issued = first
            .issue_refresh_token(HubRefreshIssueRequest {
                authorization_server: &authorization_server,
                protected_resource: &resource,
                oauth_client_id: &client,
                work_context: &context,
                principal: &principal,
                principal_display_name: &display,
                scopes: &principal.scopes,
                now,
            })
            .await
            .unwrap();
        let token = AccessTokenSubject {
            issuer: TokenIssuer::parse("https://veoveo.example").unwrap(),
            subject: principal.subject.clone(),
            oauth_client_id: client.clone(),
            session_family: Some(issued.grant.family_id.clone()),
            audience: ProtectedResourceId::parse("https://veoveo.example/mcp/operator").unwrap(),
            work_context: context.clone(),
            invocation_mode: InvocationMode::Direct,
            initiator: Some(principal.id.clone()),
            delegation_id: None,
            scopes: principal.scopes.clone(),
            jwt_id: Some(JwtId::parse("access-one").unwrap()),
            issued_at: now,
            not_before: None,
            expires_at: now + TimeDelta::minutes(15),
        };
        // Match JwtVerifier: the signed hub issuer becomes Principal.issuer,
        // while the refresh family still holds the upstream OIDC issuer.
        let mut authenticated = principal.clone();
        authenticated.issuer = token.issuer.clone();
        assert!(
            second
                .access_token_session_valid(
                    &resource,
                    &authorization_server,
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        let mut foreign = authenticated.clone();
        foreign.subject = TokenSubject::parse("two").unwrap();
        assert!(
            !second
                .access_token_session_valid(&resource, &authorization_server, &token, &foreign)
                .await
                .unwrap()
        );
        foreign = authenticated.clone();
        foreign.tenant = Some(TenantId::parse("tenant-b").unwrap());
        assert!(
            !second
                .access_token_session_valid(&resource, &authorization_server, &token, &foreign)
                .await
                .unwrap()
        );
        assert!(
            !second
                .access_token_session_valid(
                    &ProtectedResourceId::parse("https://other.example/resource").unwrap(),
                    &authorization_server,
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        assert!(
            !second
                .access_token_session_valid(
                    &resource,
                    &AuthorizationServerId::parse("other").unwrap(),
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        for mismatch in ["client", "context", "scope"] {
            let mut bad = token.clone();
            match mismatch {
                "client" => bad.oauth_client_id = OAuthClientId::parse("other").unwrap(),
                "context" => bad.work_context = WorkContextId::parse("other").unwrap(),
                _ => {
                    bad.scopes.insert(ScopeName::parse("admin:use").unwrap());
                }
            }
            assert!(
                !second
                    .access_token_session_valid(
                        &resource,
                        &authorization_server,
                        &bad,
                        &authenticated
                    )
                    .await
                    .unwrap()
            );
        }
        let success = audit(&profile, &principal, AuthReasonCode::AuthAllow);
        let duplicate = audit(
            &profile,
            &principal,
            AuthReasonCode::RefreshTokenDuplicateDelivery,
        );
        let request = |at| HubRefreshRotationRequest {
            authorization_server: &authorization_server,
            protected_resource: &resource,
            oauth_client_id: &client,
            now: at,
            delivery_window: HubRefreshDeliveryWindow::from_seconds(NonZeroU32::new(1).unwrap())
                .unwrap(),
            delivery_cipher: &cipher,
            success_audit: &success,
            duplicate_delivery_audit: &duplicate,
        };
        let rotated = match first
            .rotate_refresh_token(&issued.token, request(now))
            .await
            .unwrap()
        {
            HubRefreshExchange::Rotated(rotated) => rotated,
            _ => panic!("rotation must succeed"),
        };
        assert_eq!(rotated.grant.family_id, issued.grant.family_id);
        assert!(
            second
                .access_token_session_valid(
                    &resource,
                    &authorization_server,
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        assert!(!second.session_compromised(&token).await.unwrap());
        match ending {
            "logout" => {
                first
                    .revoke_refresh_token_family(
                        &rotated.token,
                        &authorization_server,
                        &resource,
                        &client,
                        now,
                    )
                    .await
                    .unwrap()
                    .unwrap();
            }
            "replay" => assert!(matches!(
                first
                    .rotate_refresh_token(&issued.token, request(now + TimeDelta::seconds(2)))
                    .await
                    .unwrap(),
                HubRefreshExchange::ReplayDetected { .. }
            )),
            "expiry" => {
                sqlx::query_file!(
                    "tests/session_family/expire_family.sql",
                    issued.grant.family_id.as_uuid(),
                    Utc::now() - TimeDelta::seconds(1),
                )
                .execute(db.a.pool())
                .await
                .unwrap();
            }
            _ => {
                sqlx::query_file!(
                    "tests/session_family/delete_family.sql",
                    issued.grant.family_id.as_uuid(),
                )
                .execute(db.a.pool())
                .await
                .unwrap();
            }
        }
        // A still-unexpired signed access token cannot outlive committed family revocation.
        assert!(
            !second
                .access_token_session_valid(
                    &resource,
                    &authorization_server,
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        // A running Task stops acting for the caller only after a replay (D37): a logout,
        // an expiry or a removed family leaves its delegation in place.
        assert_eq!(
            second.session_compromised(&token).await.unwrap(),
            ending == "replay",
            "{ending}"
        );
    }
    // A caller without a browser session has nothing to compromise.
    let service: AccessTokenSubject = serde_json::from_value(serde_json::json!({
        "issuer": "https://veoveo.example",
        "subject": "service",
        "oauth_client_id": "service",
        "audience": "https://veoveo.example/mcp/operator",
        "work_context": "mission",
        "invocation_mode": "automated",
        "issued_at": Utc::now(),
        "expires_at": Utc::now() + TimeDelta::minutes(15)
    }))
    .unwrap();
    assert!(!first.session_compromised(&service).await.unwrap());
}

fn audit(
    profile: &HubProfileId,
    principal: &Principal,
    reason: AuthReasonCode,
) -> veoveo_audit_contract::AuditDraft {
    use veoveo_audit_contract::*;
    let activity = if reason == AuthReasonCode::RefreshTokenDuplicateDelivery {
        AuthenticationActivity::DuplicateRefresh
    } else {
        AuthenticationActivity::Refresh
    };
    let mut draft = AuditDraft::builder(
        AuditRequest::background(),
        hub_audit()
            .detail(&Authentication {
                activity,
                method: AuthMethod::RefreshToken,
                duration_ms: 0,
                session_family: None,
                client: None,
                scopes: principal.scopes.clone(),
                data_labels: principal.data_labels.clone(),
            })
            .unwrap(),
        AuditOutcome::Success,
    )
    .target(veoveo_hub::audit::profile_target(profile).unwrap())
    .actor(AuditActor {
        principal: principal.id.clone(),
        kind: AuditPrincipalKind::User,
        tenant: principal.tenant.clone(),
        work_context: None,
    });
    if let Some(reason) = veoveo_hub::audit::authentication_reason(reason) {
        draft = draft.reason(reason);
    }
    draft.build().unwrap()
}
