//! Durable-route authorization against an isolated store and the current policy catalog.
use std::collections::BTreeSet;
use veoveo_hub_sdk::ProtectedResourceId;

use crate::control_plane::HubControlPlane;
use chrono::{TimeDelta, Utc};
use veoveo_hub_sdk::{AccessTokenSubject, OAuthClientId};
use veoveo_types::{
    DataLabelId, DelegationId, InvocationMode, InvocationProvenance, PrincipalId, ScopeName,
    TenantId,
};

use super::*;
use crate::{HubCatalog, state::HubTaskOwnership};

pub(crate) fn subject() -> AuthenticatedSubject {
    let mut actor = super::tests::principal();
    let mut authority = super::tests::authority();
    actor.tenant = Some(TenantId::parse("enterprise").unwrap());
    authority.tenant = actor.tenant.clone().unwrap();
    actor.scopes = BTreeSet::from([ScopeName::parse("operator:use").unwrap()]);
    actor.data_labels.insert(DataLabelId::parse("cui").unwrap());
    AuthenticatedSubject {
        execution: Some(veoveo_hub_sdk::hosting::testing::execution_context()),
        extensions: Default::default(),
        audit: veoveo_audit_contract::AuditRequest::background(),
        access_token: AccessTokenSubject {
            issuer: actor.issuer.clone(),
            subject: actor.subject.clone(),
            oauth_client_id: OAuthClientId::parse("workspace").unwrap(),
            session_family: None,
            audience: ProtectedResourceId::parse("https://veoveo.example/mcp/workspace").unwrap(),
            work_context: authority.work_context.clone(),
            invocation_mode: InvocationMode::Direct,
            initiator: Some(actor.id.clone()),
            delegation_id: None,
            scopes: actor.scopes.clone(),
            jwt_id: None,
            issued_at: Utc::now(),
            not_before: None,
            expires_at: Utc::now() + TimeDelta::minutes(15),
        },
        principal: actor.clone(),
        actor,
        principal_display_name: None,
        authority,
        delegation: None,
    }
}

pub(super) fn hub(state: HubState, plane: HubControlPlane) -> HubMcp {
    let _ = rustls::crypto::ring::default_provider().install_default();
    HubMcp::new(
        HubCatalogHandle::new(Arc::new(
            HubCatalog::from_control_plane(plane, crate::test_catalog::binding()).unwrap(),
        )),
        HubProfileId::parse("workspace").unwrap(),
        state,
        crate::test_catalog::token_issuer(),
        HubUpstreamHttpClientPool::default(),
    )
}

#[test]
fn delegated_ownership_requires_the_same_initiator_and_grant() {
    let mut caller = subject();
    caller.authority.provenance = InvocationProvenance::Delegated {
        initiator: caller.principal.id.clone(),
        delegation_id: DelegationId::parse("grant-1").unwrap(),
    };
    let owner = HubTaskOwnership::from_invocation(&caller.actor, &caller.authority);
    assert!(owner.allows(&caller.actor, &caller.authority));
    for provenance in [
        InvocationProvenance::Delegated {
            initiator: caller.principal.id.clone(),
            delegation_id: DelegationId::parse("grant-2").unwrap(),
        },
        InvocationProvenance::Delegated {
            initiator: PrincipalId::parse("other#initiator").unwrap(),
            delegation_id: DelegationId::parse("grant-1").unwrap(),
        },
        InvocationProvenance::Direct {
            initiator: caller.principal.id.clone(),
        },
        InvocationProvenance::Automated,
    ] {
        caller.authority.provenance = provenance;
        assert!(!owner.allows(&caller.actor, &caller.authority));
    }
}

#[test]
fn captured_task_adapter_preserves_the_entire_current_subject_except_execution() {
    let current = subject();
    let captured = veoveo_hub_sdk::ResolvedComposition::new(
        [(
            veoveo_hub_sdk::ServerSlug::parse("upstream-a").unwrap(),
            veoveo_hub_sdk::ResolvedComponent::Unavailable {},
        )]
        .into_iter()
        .collect(),
    )
    .context();
    assert_ne!(current.execution.as_ref(), Some(&captured));
    let mut expected = current.clone();
    expected.execution = Some(captured.clone());
    let actual = super::authorization::with_task_execution(current, captured);
    assert_eq!(actual, expected);
    let context = actual.request_context().unwrap();
    assert_eq!(context.execution, actual.execution.unwrap());
    assert_eq!(context.access_token, expected.access_token);
    assert_eq!(context.principal, expected.principal);
}
