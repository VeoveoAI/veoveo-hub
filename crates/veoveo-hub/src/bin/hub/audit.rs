use std::{sync::Arc, time::Instant};
use veoveo_audit_contract::{AuditActor, AuditDraft, AuditOutcome, AuditPrincipalKind};
pub(super) use veoveo_hub::audit::{AdminOperationFailure, AdministrativeOperation};
use veoveo_hub::audit::{Authentication, AuthenticationActivity, hub_audit};
use veoveo_hub::audit::{authentication_reason, client_target, profile_target};
use veoveo_hub::contract::HubAction;
pub(super) use veoveo_hub::http::action_audit::{
    AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationStatus,
    authorize_hub_action, record_hub_operation_audit,
};
use veoveo_hub::http::auth_support::audit_request;
pub(super) use veoveo_hub::http::auth_support::{
    AuthAuditTarget, auth_audit_error_response, internal_error_response,
};

use axum::response::Response;
use sha2::{Digest, Sha256};
use veoveo_hub::contract::{AuthMethod, AuthOutcome, AuthReasonCode};
use veoveo_hub::control_plane::{
    HubControlPlane, HubProfile, PolicyTarget, ResourceAuthorizationServer,
};
use veoveo_hub::{AuthenticatedSubject, HubCatalog, HubState};
use veoveo_hub_sdk::{HubProfileId, OAuthClientId, Principal};
use veoveo_types::PrincipalId;

use crate::runtime::{AdminState, current_catalog};

pub(super) async fn authorize_admin_request(
    state: &AdminState,
    profile_id: &HubProfileId,
    subject: AuthenticatedSubject,
    action: HubAction,
    operation: AdministrativeOperation,
    started_at: Instant,
) -> std::result::Result<(Arc<HubCatalog>, HubProfile, AuthenticatedSubject), Box<Response>> {
    authorize_admin_target_request(
        state,
        profile_id,
        subject,
        AdminAuthorizationRequest {
            audit_target: None,
            action: action.into(),
            target: PolicyTarget::Hub,
            operation,
            started_at,
        },
    )
    .await
}

pub(super) async fn authorize_admin_target_request(
    state: &AdminState,
    profile_id: &HubProfileId,
    subject: AuthenticatedSubject,
    request: AdminAuthorizationRequest,
) -> std::result::Result<(Arc<HubCatalog>, HubProfile, AuthenticatedSubject), Box<Response>> {
    let catalog = current_catalog(&state.catalog);
    authorize_hub_action(&state.hub_state, catalog, profile_id, subject, request).await
}

pub(super) fn control_plane_sha256(control_plane: &HubControlPlane) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(control_plane)?;
    let digest = Sha256::digest(bytes);
    Ok(digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}

pub(super) async fn record_admin_operation_audit(
    state: &AdminState,
    profile: &HubProfile,
    subject: &AuthenticatedSubject,
    record: AdminOperationAuditRecord,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(state, profile, subject, PolicyTarget::Hub, record).await
}

pub(super) async fn record_admin_target_operation_audit(
    state: &AdminState,
    profile: &HubProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    record: AdminOperationAuditRecord,
) -> anyhow::Result<()> {
    let catalog = current_catalog(&state.catalog);
    record_hub_operation_audit(&state.hub_state, profile, subject, &catalog, target, record).await
}

pub(super) struct AuthAuditRecord<'a> {
    pub(super) authorization_server: Option<&'a ResourceAuthorizationServer>,
    pub(super) client_id: Option<&'a OAuthClientId>,
    pub(super) principal: Option<&'a Principal>,
    pub(super) outcome: AuthOutcome,
    pub(super) reason: AuthReasonCode,
    pub(super) started_at: Instant,
}

pub(super) async fn record_token_auth_audit<'a>(
    hub_state: &HubState,
    target: impl Into<AuthAuditTarget<'a>>,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    let target = target.into();
    let draft = authentication_draft(target, record, AuthMethod::ClientCredentialsPrivateKeyJwt)?;
    hub_state.record_audit(draft).await
}

pub(super) async fn record_id_jag_auth_audit(
    hub_state: &HubState,
    profile: &HubProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    hub_state
        .record_audit(authentication_draft(
            profile.into(),
            record,
            AuthMethod::EnterpriseManagedIdJag,
        )?)
        .await
}
pub(super) async fn record_oidc_auth_audit(
    hub_state: &HubState,
    profile: &HubProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    hub_state
        .record_audit(authentication_draft(
            profile.into(),
            record,
            AuthMethod::OidcAuthorizationCodePkce,
        )?)
        .await
}
pub(super) async fn record_refresh_auth_audit(
    hub_state: &HubState,
    profile: &HubProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    hub_state
        .record_audit(refresh_auth_audit_event(profile, record)?)
        .await
}
pub(super) fn refresh_auth_audit_event(
    profile: &HubProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<AuditDraft> {
    authentication_draft(profile.into(), record, AuthMethod::RefreshToken)
}
fn authentication_draft(
    target: AuthAuditTarget<'_>,
    record: AuthAuditRecord<'_>,
    method: AuthMethod,
) -> anyhow::Result<AuditDraft> {
    let allowed = record.outcome == AuthOutcome::Allow;
    let activity = match record.reason {
        AuthReasonCode::RefreshTokenDuplicateDelivery => AuthenticationActivity::DuplicateRefresh,
        AuthReasonCode::RefreshTokenReplay
        | AuthReasonCode::ClientAssertionReplay
        | AuthReasonCode::IdentityAssertionReplay => AuthenticationActivity::Replay,
        AuthReasonCode::RefreshTokenRevoked if allowed => AuthenticationActivity::Revoke,
        _ if !allowed => AuthenticationActivity::CredentialDenial,
        _ if method == AuthMethod::RefreshToken => AuthenticationActivity::Refresh,
        _ => AuthenticationActivity::Issue,
    };
    let audit_target = match (target.profile, record.client_id) {
        (Some(profile), _) => profile_target(profile)?,
        (None, Some(client)) => client_target(client)?,
        (None, None) => target.audit_target()?,
    };
    let detail = hub_audit().detail(&Authentication {
        activity,
        method,
        duration_ms: u64::try_from(record.started_at.elapsed().as_millis())?,
        session_family: None,
        client: record.client_id.cloned(),
        scopes: record
            .principal
            .map(|principal| principal.scopes.clone())
            .unwrap_or_default(),
        data_labels: record
            .principal
            .map(|principal| principal.data_labels.clone())
            .unwrap_or_default(),
    })?;
    let mut draft = AuditDraft::builder(
        audit_request(),
        detail,
        if allowed {
            AuditOutcome::Success
        } else {
            AuditOutcome::Denied
        },
    )
    .target(audit_target);
    if let Some(reason) = authentication_reason(record.reason) {
        draft = draft.reason(reason);
    }
    if let Some(principal) = record.principal {
        draft = draft.actor(AuditActor {
            principal: principal.id.clone(),
            kind: match principal.kind {
                veoveo_hub_sdk::PrincipalKind::User => AuditPrincipalKind::User,
                veoveo_hub_sdk::PrincipalKind::Service => AuditPrincipalKind::Service,
            },
            tenant: principal.tenant.clone(),
            work_context: None,
        });
    } else if allowed
        && let (Some(server), Some(client)) = (record.authorization_server, record.client_id)
    {
        draft = draft.actor(AuditActor {
            principal: PrincipalId::parse(format!("{}#{}", server.issuer, client))?,
            kind: AuditPrincipalKind::Service,
            tenant: None,
            work_context: None,
        });
    }
    Ok(draft.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_hub_sdk::ProtectedResourceId;

    #[test]
    fn authentication_records_distinguish_revocation_from_revoked_credential_denial() {
        let principal: Principal = serde_json::from_value(serde_json::json!({
            "id": "verified-user", "kind": "user", "issuer": "https://idp.example",
            "subject": "user", "tenant": "tenant-a", "scopes": ["operator:use"],
            "data_labels": ["cui"]
        }))
        .unwrap();
        let resource = ProtectedResourceId::parse("https://hub.example/mcp/operator").unwrap();
        let profile = HubProfileId::parse("operator").unwrap();
        for (outcome, expected_outcome, activity) in [
            (
                AuthOutcome::Allow,
                AuditOutcome::Success,
                AuthenticationActivity::Revoke,
            ),
            (
                AuthOutcome::Deny,
                AuditOutcome::Denied,
                AuthenticationActivity::CredentialDenial,
            ),
        ] {
            let draft = authentication_draft(
                AuthAuditTarget {
                    profile: Some(&profile),
                    protected_resource: &resource,
                },
                AuthAuditRecord {
                    authorization_server: None,
                    client_id: None,
                    principal: Some(&principal),
                    outcome,
                    reason: AuthReasonCode::RefreshTokenRevoked,
                    started_at: Instant::now(),
                },
                AuthMethod::RefreshToken,
            )
            .unwrap();
            assert_eq!(draft.outcome(), expected_outcome);
            assert_eq!(
                draft.reason().map(|reason| reason.as_str()),
                Some("refresh_token_revoked")
            );
            let detail = hub_audit()
                .registry()
                .read::<Authentication>(draft.class(), draft.detail())
                .unwrap();
            assert_eq!(detail.activity, activity);
            assert_eq!(detail.method, AuthMethod::RefreshToken);
            assert_eq!(detail.scopes, principal.scopes);
            assert_eq!(detail.data_labels, principal.data_labels);
        }
    }

    #[test]
    fn anonymous_token_denial_identifies_its_protected_resource() {
        let resource = ProtectedResourceId::parse("https://hub.example/fixture-ingest").unwrap();
        let draft = authentication_draft(
            AuthAuditTarget {
                profile: None,
                protected_resource: &resource,
            },
            AuthAuditRecord {
                authorization_server: None,
                client_id: None,
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::MissingAuthorizationHeader,
                started_at: Instant::now(),
            },
            AuthMethod::ClientCredentialsPrivateKeyJwt,
        )
        .unwrap();
        assert!(draft.actor().is_none());
        assert_eq!(draft.outcome(), AuditOutcome::Denied);
        assert_eq!(
            draft.target().map(|target| target.as_str()),
            Some("https://hub.example/fixture-ingest")
        );
        assert_eq!(
            draft.reason().map(|reason| reason.as_str()),
            Some("missing_authorization_header")
        );
    }
}
