//! Ending a person's sessions revokes each of their live families with its own reason and
//! clears their Computer's Lore client, and leaves an expired family and another person's
//! as they were.
use std::{collections::BTreeSet, time::Duration};

use chrono::{TimeDelta, Utc};
use veoveo_hub_sdk::{OAuthClientId, Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{PrincipalId, Sha256Digest, TenantId, WorkContextId};

use super::super::{HubStore, RefreshRevocation};
use crate::{
    HubRefreshIssueRequest, HubState,
    computers::ComputerProvisionSpec,
    contract::{AuthorizationServerId, PrincipalDisplayName},
};

fn person(id: &str) -> Principal {
    Principal {
        id: PrincipalId::parse(id).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://identity.test").unwrap(),
        subject: TokenSubject::parse(id).unwrap(),
        tenant: Some(TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    }
}

#[tokio::test]
async fn ending_a_persons_sessions_revokes_their_live_families_and_their_lore_client() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let store = HubStore::new(db.a.clone());
        let state = HubState::new(db.a.clone());
        let (ended, other) = (person("ended-user"), person("other-user"));
        let tenant = ended.tenant.clone().unwrap();
        let issue = |principal: &Principal, now| {
            let state = state.clone();
            let principal = principal.clone();
            async move {
                state
                    .issue_refresh_token(HubRefreshIssueRequest {
                        authorization_server: &AuthorizationServerId::parse("fixture").unwrap(),
                        protected_resource: &veoveo_hub_sdk::ProtectedResourceId::parse(
                            "https://fixture.example/resource",
                        )
                        .unwrap(),
                        oauth_client_id: &OAuthClientId::parse("browser").unwrap(),
                        work_context: &WorkContextId::parse("fixture").unwrap(),
                        principal: &principal,
                        principal_display_name: &PrincipalDisplayName::new("Fixture").unwrap(),
                        scopes: &BTreeSet::new(),
                        now,
                    })
                    .await
                    .unwrap()
                    .grant
                    .family_id
            }
        };
        let now = Utc::now();
        let browser = issue(&ended, now).await;
        let opencode = issue(&ended, now).await;
        let expired = issue(&ended, now - TimeDelta::days(30)).await;
        let others = issue(&other, now).await;

        let config: crate::control_plane::Computers = serde_json::from_value(serde_json::json!({
            "gateway": {"endpoint":"https://gateway.example/", "certificateAuthority":"/unused/ca", "bearerEnvironment":"UNUSED"},
            "template":{"workspace":"installation", "name":"initial"}, "policyFile":"/unused/policy", "command":["/initial/app"],
            "oauthClient":"fixture-client", "profile":"fixture", "workContext":"fixture", "port":4096,
            "hubProvider":{"profile":"fixture-hub", "credentialKey":"HUB_REFERENCE"},
            "modelProvider":{"profile":"fixture-model", "credentialKey":"MODEL_REFERENCE", "secret":"fixture-secret"},
            "loreProvider":{"profile":"fixture-lore", "credentialKey":"LORE_ACCESS_TOKEN"}
        }))
        .unwrap();
        let computer = store
            .ensure_computer(&tenant, &ended.id, &ComputerProvisionSpec::fixture(config))
            .await
            .unwrap();
        let lore = Sha256Digest::from_bytes([11; 32]);
        sqlx::query_file!(
            "src/store/sessions/tests/lore_client.sql",
            computer.id.as_uuid(),
            tenant.as_str(),
            lore.as_str()
        )
        .execute(store.pool())
        .await
        .unwrap();
        assert!(store.computer_of_lore_client(&lore).await.unwrap().is_some());

        let at = Utc::now();
        assert_eq!(store.end_person_sessions(&tenant, &ended.id, at).await.unwrap(), 2);
        for family in [&browser, &opencode] {
            let family = store.refresh_family(family).await.unwrap().unwrap();
            assert_eq!(family.revocation, Some(RefreshRevocation::AdministratorRevocation));
            assert!(family.revoked_at.is_some());
        }
        for family in [&expired, &others] {
            let family = store.refresh_family(family).await.unwrap().unwrap();
            assert_eq!(family.revocation, None, "an expired family and another person's stay");
        }
        assert!(
            store.computer_of_lore_client(&lore).await.unwrap().is_none(),
            "the Computer's Lore client mints nothing more"
        );
        assert_eq!(
            store.end_person_sessions(&tenant, &ended.id, Utc::now()).await.unwrap(),
            0,
            "ending them again finds nothing to revoke"
        );
    })
    .await
    .expect("ending a person's sessions exceeded ninety seconds");
}
