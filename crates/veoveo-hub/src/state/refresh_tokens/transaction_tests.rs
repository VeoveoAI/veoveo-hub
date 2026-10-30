//! Native transaction witnesses using the actual store, cipher and audit append.
use super::*;
use crate::audit::{AuditAppend, Authentication, AuthenticationActivity, hub_audit};
use crate::contract::AuthMethod;
use crate::store::{HubStoreError, RefreshRevocation, TransactionAppend};
use chrono::SubsecRound;
use serde_json::Value;
use tokio::sync::{Notify, oneshot};
use veoveo_audit_contract::{
    AuditActor, AuditDraft, AuditOutcome, AuditPrincipalKind, AuditRequest,
};
use veoveo_hub_sdk::{HubProfileId, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{PrincipalId, TenantId};

struct Fixture {
    db: crate::test_store::TestDb,
    first: HubState,
    second: HubState,
    server: AuthorizationServerId,
    resource: ProtectedResourceId,
    client: OAuthClientId,
    principal: Principal,
    now: DateTime<Utc>,
    cipher: RefreshTokenDeliveryCipher,
    issued: IssuedHubRefreshToken,
}

impl Fixture {
    async fn new() -> Self {
        let db = crate::test_store::TestDb::new().await;
        let first = HubState::new(db.a.clone());
        let second = HubState::new(db.b.clone());
        let server = AuthorizationServerId::parse("refresh-fixture").unwrap();
        let resource =
            ProtectedResourceId::parse("https://refresh-fixture.example/resource").unwrap();
        let client = OAuthClientId::parse("refresh-fixture-client").unwrap();
        // Stored PostgreSQL timestamps have microsecond precision. Boundary tests
        // use exactly represented fixtures, not an assumed nanosecond SQL roundtrip.
        let now = Utc::now().trunc_subsecs(6);
        let principal = Principal {
            id: PrincipalId::parse("https://refresh-fixture.example/idp#caller").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://refresh-fixture.example/idp").unwrap(),
            subject: TokenSubject::parse("caller").unwrap(),
            tenant: Some(TenantId::parse("refresh-fixture").unwrap()),
            groups: Default::default(),
            group_roles: Default::default(),
            roles: Default::default(),
            scopes: BTreeSet::from([ScopeName::parse("refresh-fixture:read").unwrap()]),
            data_labels: Default::default(),
            assurances: Default::default(),
            authenticated_at: Some(now),
        };
        let cipher = RefreshTokenDeliveryCipher::new(b"0123456789abcdef0123456789abcdef").unwrap();
        let issued = first
            .issue_refresh_token(HubRefreshIssueRequest {
                authorization_server: &server,
                protected_resource: &resource,
                oauth_client_id: &client,
                work_context: &WorkContextId::parse("refresh-fixture").unwrap(),
                principal: &principal,
                principal_display_name: &PrincipalDisplayName::new("Refresh fixture").unwrap(),
                scopes: &principal.scopes,
                now,
            })
            .await
            .unwrap();
        Self {
            db,
            first,
            second,
            server,
            resource,
            client,
            principal,
            now,
            cipher,
            issued,
        }
    }

    fn request<'a>(
        &'a self,
        now: DateTime<Utc>,
        rotated: &'a AuditDraft,
        duplicate: &'a AuditDraft,
    ) -> HubRefreshRotationRequest<'a> {
        HubRefreshRotationRequest {
            authorization_server: &self.server,
            protected_resource: &self.resource,
            oauth_client_id: &self.client,
            now,
            delivery_window: delivery_window(),
            delivery_cipher: &self.cipher,
            success_audit: rotated,
            duplicate_delivery_audit: duplicate,
        }
    }

    fn draft(&self, activity: AuthenticationActivity, now: DateTime<Utc>) -> AuditDraft {
        AuditDraft::builder(
            AuditRequest::background(),
            hub_audit()
                .detail(&Authentication {
                    activity,
                    method: AuthMethod::RefreshToken,
                    duration_ms: 1,
                    session_family: Some(self.issued.grant.family_id.clone()),
                    client: Some(self.client.clone()),
                    scopes: self.principal.scopes.clone(),
                    data_labels: Default::default(),
                })
                .unwrap(),
            AuditOutcome::Success,
        )
        .actor(AuditActor {
            principal: self.principal.id.clone(),
            kind: AuditPrincipalKind::User,
            tenant: self.principal.tenant.clone(),
            work_context: Some(self.issued.grant.work_context.clone()),
        })
        .target(
            crate::audit::profile_target(&HubProfileId::parse("refresh-fixture").unwrap()).unwrap(),
        )
        .occurred_at(now)
        .build()
        .unwrap()
    }

    async fn snapshot(&self) -> Value {
        sqlx::query_file!("src/state/refresh_tokens/tests/snapshot.sql")
            .fetch_one(self.db.b.pool())
            .await
            .unwrap()
            .snapshot
    }

    async fn replacement(&self, now: DateTime<Utc>) -> (OAuthRefreshToken, NewRefreshToken) {
        let (current, family) = self
            .first
            .store
            .refresh_grant(&refresh_token_digest(&self.issued.token))
            .await
            .unwrap()
            .unwrap();
        let generation = current.generation.checked_add(1).unwrap();
        let token = random_refresh_token().unwrap();
        let replacement = NewRefreshToken {
            id: Uuid::now_v7(),
            digest: refresh_token_digest(&token),
            generation,
            issued_at: now,
            expires_at: family.expires_at,
            delivery: Some(RefreshDeliveryEnvelope {
                envelope: self
                    .cipher
                    .seal(&token, &refresh_delivery_aad(&family, generation))
                    .unwrap(),
                expires_at: std::cmp::min(
                    delivery_window().expires_at(now).unwrap(),
                    family.expires_at,
                ),
            }),
        };
        (token, replacement)
    }
}

fn delivery_window() -> HubRefreshDeliveryWindow {
    HubRefreshDeliveryWindow::from_seconds(NonZeroU32::new(4).unwrap()).unwrap()
}

struct HoldAppend {
    audit: AuditAppend,
    entered: std::sync::Mutex<Option<oneshot::Sender<i32>>>,
    release: Arc<Notify>,
}

impl TransactionAppend for HoldAppend {
    async fn append(&self, connection: &mut sqlx::PgConnection) -> Result<(), HubStoreError> {
        let observation = sqlx::query_file!("src/state/refresh_tokens/tests/connection.sql")
            .fetch_one(&mut *connection)
            .await?;
        assert_eq!(observation.isolation, "read committed");
        self.audit.append(connection).await?;
        self.entered
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(observation.pid)
            .unwrap();
        self.release.notified().await;
        Ok(())
    }
}

struct FailAppend(AuditAppend);

impl TransactionAppend for FailAppend {
    async fn append(&self, connection: &mut sqlx::PgConnection) -> Result<(), HubStoreError> {
        self.0.append(connection).await?;
        Err(HubStoreError::Append(Box::new(std::io::Error::other(
            "refresh fixture fails after real append",
        ))))
    }
}

#[tokio::test]
async fn refresh_lock_overlap_redelivers_the_committed_successor() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let now = fixture.now + TimeDelta::seconds(1);
        let before = fixture.snapshot().await;
        let (predecessor_before, family_before) = fixture
            .first
            .store
            .refresh_grant(&refresh_token_digest(&fixture.issued.token))
            .await
            .unwrap()
            .unwrap();
        let (plaintext, replacement) = fixture.replacement(now).await;
        let expected_replacement = replacement.clone();
        let rotation = fixture.draft(AuthenticationActivity::Refresh, now);
        let first_duplicate = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
        let first = fixture.first.clone();
        let digest = refresh_token_digest(&fixture.issued.token);
        let (entered, reached) = oneshot::channel();
        let release = Arc::new(Notify::new());
        let held = HoldAppend {
            audit: first.audit_write(&rotation).unwrap(),
            entered: std::sync::Mutex::new(Some(entered)),
            release: release.clone(),
        };
        let duplicate = first.audit_write(&first_duplicate).unwrap();
        let winner = tokio::spawn(async move {
            first
                .store
                .rotate_refresh_token(&digest, &replacement, now, &held, &duplicate)
                .await
        });
        let owner = reached.await.unwrap();
        let second = fixture.second.clone();
        let token = fixture.issued.token.clone();
        let server = fixture.server.clone();
        let resource = fixture.resource.clone();
        let client = fixture.client.clone();
        let cipher = fixture.cipher.clone();
        let second_rotation = fixture.draft(AuthenticationActivity::Refresh, now);
        let second_duplicate = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
        let expected_second_duplicate = second_duplicate.clone();
        let retry = tokio::spawn(async move {
            second
                .rotate_refresh_token(
                    &token,
                    HubRefreshRotationRequest {
                        authorization_server: &server,
                        protected_resource: &resource,
                        oauth_client_id: &client,
                        now,
                        delivery_window: delivery_window(),
                        delivery_cipher: &cipher,
                        success_audit: &second_rotation,
                        duplicate_delivery_audit: &second_duplicate,
                    },
                )
                .await
        });
        loop {
            if sqlx::query_file!("src/state/refresh_tokens/tests/blocked.sql", owner)
                .fetch_one(fixture.db.b.pool())
                .await
                .unwrap()
                .blocked
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        // Snapshot contains ciphertext, never plaintext. Compare without Debug
        // formatting so failure logs do not disclose even fixture envelopes.
        assert!(
            fixture.snapshot().await == before,
            "held rotation is invisible before commit"
        );
        release.notify_one();
        let rotated = winner.await.unwrap().unwrap();
        let RefreshRotation::Rotated {
            family,
            replacement,
        } = rotated
        else {
            panic!("first caller must rotate")
        };
        assert_eq!(replacement.id, expected_replacement.id);
        assert_eq!(replacement.digest, expected_replacement.digest);
        let returned = retry.await.unwrap().unwrap();
        let HubRefreshExchange::DuplicateDelivery(returned) = returned else {
            panic!("blocked retry must redeliver")
        };
        assert!(
            returned.token.as_str() == plaintext.as_str(),
            "duplicate returns the identical committed plaintext after checks"
        );
        assert_eq!(returned.grant.family_id, family.id);
        assert_eq!(returned.grant.generation, 1);
        let mut expected_family = family_before;
        expected_family.current_generation = 1;
        assert_eq!(family, expected_family);
        assert_eq!(
            fixture
                .first
                .store
                .refresh_family(&family.id)
                .await
                .unwrap()
                .unwrap(),
            expected_family
        );
        let (stored_predecessor, _) = fixture
            .first
            .store
            .refresh_grant(&predecessor_before.digest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_predecessor.id, predecessor_before.id);
        assert_eq!(stored_predecessor.family, predecessor_before.family);
        assert_eq!(stored_predecessor.digest, predecessor_before.digest);
        assert_eq!(stored_predecessor.generation, predecessor_before.generation);
        assert_eq!(stored_predecessor.issued_at, predecessor_before.issued_at);
        assert_eq!(stored_predecessor.expires_at, predecessor_before.expires_at);
        assert_eq!(stored_predecessor.consumed_at, Some(now));
        assert_eq!(
            stored_predecessor.replacement,
            Some(expected_replacement.id)
        );
        assert!(stored_predecessor.replay_detected_at.is_none());
        assert!(stored_predecessor.delivery.is_none());
        let (stored_successor, _) = fixture
            .first
            .store
            .refresh_grant(&expected_replacement.digest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_successor.id, expected_replacement.id);
        assert_eq!(stored_successor.family, family.id);
        assert_eq!(stored_successor.digest, expected_replacement.digest);
        assert_eq!(stored_successor.generation, expected_replacement.generation);
        assert_eq!(stored_successor.issued_at, expected_replacement.issued_at);
        assert_eq!(stored_successor.expires_at, expected_replacement.expires_at);
        assert!(stored_successor.consumed_at.is_none());
        assert!(stored_successor.replacement.is_none());
        assert!(stored_successor.replay_detected_at.is_none());
        let stored_delivery = stored_successor.delivery.unwrap();
        let expected_delivery = expected_replacement.delivery.unwrap();
        assert_eq!(stored_delivery.expires_at, expected_delivery.expires_at);
        assert!(
            stored_delivery.envelope.expose_secret() == expected_delivery.envelope.expose_secret(),
            "the complete committed envelope is retained without disclosure"
        );
        let after = fixture.snapshot().await;
        assert_eq!(
            after["tokens"].as_array().unwrap().len(),
            2,
            "no extra successor row from duplicate caller"
        );
        let audits: Vec<_> = after["audit"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["draft"].clone())
            .collect();
        assert_eq!(
            audits,
            [
                serde_json::to_value(&rotation).unwrap(),
                serde_json::to_value(&expected_second_duplicate).unwrap()
            ],
            "only the winning success and the blocked caller's exact duplicate draft commit"
        );
    })
    .await
    .expect("deterministic refresh overlap exceeded 90 seconds");
}

#[tokio::test]
async fn refresh_duplicate_append_failure_rolls_back_and_allows_retry() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let now = fixture.now + TimeDelta::seconds(1);
        let rotation = fixture.draft(AuthenticationActivity::Refresh, now);
        let duplicate = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
        let first = fixture
            .first
            .rotate_refresh_token(
                &fixture.issued.token,
                fixture.request(now, &rotation, &duplicate),
            )
            .await
            .unwrap();
        let HubRefreshExchange::Rotated(first) = first else {
            panic!("first call must rotate")
        };
        let before = fixture.snapshot().await;
        let (_, candidate) = fixture.replacement(now + TimeDelta::seconds(1)).await;
        let failed = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
        let result = fixture
            .first
            .store
            .rotate_refresh_token(
                &refresh_token_digest(&fixture.issued.token),
                &candidate,
                now + TimeDelta::seconds(1),
                &fixture.first.audit_write(&rotation).unwrap(),
                &FailAppend(fixture.first.audit_write(&failed).unwrap()),
            )
            .await;
        assert!(
            matches!(result, Err(HubStoreError::Append(_))),
            "duplicate callback must fail after real append"
        );
        sqlx::query_file!(
            "src/state/refresh_tokens/tests/lock.sql",
            fixture.issued.grant.family_id.as_uuid()
        )
        .fetch_one(fixture.db.b.pool())
        .await
        .unwrap();
        assert!(
            fixture.snapshot().await == before,
            "failed duplicate append leaves exact family/token/audit rows"
        );
        let retry_audit = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
        let retry = fixture
            .second
            .rotate_refresh_token(
                &fixture.issued.token,
                fixture.request(now + TimeDelta::seconds(1), &rotation, &retry_audit),
            )
            .await
            .unwrap();
        let HubRefreshExchange::DuplicateDelivery(retry) = retry else {
            panic!("retry must remain deliverable after append rollback")
        };
        assert!(
            retry.token.as_str() == first.token.as_str(),
            "retry must recover original committed successor"
        );
    })
    .await
    .expect("duplicate append rollback exceeded 90 seconds");
}

#[tokio::test]
async fn refresh_delivery_deadline_is_strict_and_replay_is_atomic() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let now = fixture.now + TimeDelta::seconds(1);
        let rotation = fixture.draft(AuthenticationActivity::Refresh, now);
        let unused_duplicate = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
        let first = fixture
            .first
            .rotate_refresh_token(
                &fixture.issued.token,
                fixture.request(now, &rotation, &unused_duplicate),
            )
            .await
            .unwrap();
        let HubRefreshExchange::Rotated(first) = first else {
            panic!("first call must rotate")
        };
        let deadline = now + TimeDelta::seconds(4);
        let inside = deadline - TimeDelta::nanoseconds(1);
        let duplicate = fixture.draft(AuthenticationActivity::DuplicateRefresh, inside);
        let retry = fixture
            .second
            .rotate_refresh_token(
                &fixture.issued.token,
                fixture.request(inside, &rotation, &duplicate),
            )
            .await
            .unwrap();
        let HubRefreshExchange::DuplicateDelivery(retry) = retry else {
            panic!("one nanosecond before deadline must redeliver")
        };
        assert!(
            retry.token.as_str() == first.token.as_str(),
            "inside-window retry preserves successor plaintext"
        );
        let before = fixture.snapshot().await;
        let at_boundary = fixture
            .first
            .rotate_refresh_token(
                &fixture.issued.token,
                fixture.request(deadline, &rotation, &unused_duplicate),
            )
            .await
            .unwrap();
        assert!(
            matches!(at_boundary, HubRefreshExchange::ReplayDetected { .. }),
            "equal delivery deadline must detect replay"
        );
        let (predecessor, family) = fixture
            .first
            .store
            .refresh_grant(&refresh_token_digest(&fixture.issued.token))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(family.revoked_at, Some(deadline));
        assert_eq!(family.revocation, Some(RefreshRevocation::TokenReplay));
        assert_eq!(predecessor.replay_detected_at, Some(deadline));
        let after = fixture.snapshot().await;
        assert_eq!(
            after["audit"], before["audit"],
            "store replay commits no success/duplicate draft; endpoint denial audit is separate"
        );
        assert_eq!(after["tokens"].as_array().unwrap().len(), 2);
        let (successor, _) = fixture
            .first
            .store
            .refresh_grant(&refresh_token_digest(&first.token))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(predecessor.replacement, Some(successor.id));
        assert_eq!(successor.generation, 1);
        assert!(successor.replay_detected_at.is_none());
    })
    .await
    .expect("strict delivery deadline exceeded 90 seconds");
}

#[tokio::test]
async fn refresh_first_revocation_reason_and_time_survive_both_orders() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        for logout_first in [true, false] {
            let fixture = Fixture::new().await;
            let now = fixture.now + TimeDelta::seconds(1);
            let rotation = fixture.draft(AuthenticationActivity::Refresh, now);
            let duplicate = fixture.draft(AuthenticationActivity::DuplicateRefresh, now);
            assert!(matches!(
                fixture
                    .first
                    .rotate_refresh_token(
                        &fixture.issued.token,
                        fixture.request(now, &rotation, &duplicate)
                    )
                    .await
                    .unwrap(),
                HubRefreshExchange::Rotated(_)
            ));
            let first_time = now + TimeDelta::seconds(5);
            let last_time = first_time + TimeDelta::seconds(1);
            let logout_time = if logout_first { first_time } else { last_time };
            let replay_time = if logout_first { last_time } else { first_time };
            if logout_first {
                fixture
                    .first
                    .revoke_refresh_token_family(
                        &fixture.issued.token,
                        &fixture.server,
                        &fixture.resource,
                        &fixture.client,
                        logout_time,
                    )
                    .await
                    .unwrap()
                    .unwrap();
            }
            assert!(
                matches!(
                    fixture
                        .second
                        .rotate_refresh_token(
                            &fixture.issued.token,
                            fixture.request(replay_time, &rotation, &duplicate)
                        )
                        .await
                        .unwrap(),
                    HubRefreshExchange::ReplayDetected { .. }
                ),
                "consumed-token replay branch precedes already-revoked refusal"
            );
            if !logout_first {
                fixture
                    .first
                    .revoke_refresh_token_family(
                        &fixture.issued.token,
                        &fixture.server,
                        &fixture.resource,
                        &fixture.client,
                        logout_time,
                    )
                    .await
                    .unwrap()
                    .unwrap();
            }
            let (token, family) = fixture
                .first
                .store
                .refresh_grant(&refresh_token_digest(&fixture.issued.token))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                family.revoked_at,
                Some(first_time),
                "first revocation timestamp survives later action"
            );
            assert_eq!(
                family.revocation,
                Some(if logout_first {
                    RefreshRevocation::ClientRevocation
                } else {
                    RefreshRevocation::TokenReplay
                }),
                "first reason survives; logout is not escalated into Task compromise"
            );
            assert_eq!(token.replay_detected_at, Some(replay_time));
            let after = fixture.snapshot().await;
            let drafts: Vec<_> = after["audit"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["draft"].clone())
                .collect();
            assert_eq!(
                drafts,
                [serde_json::to_value(&rotation).unwrap()],
                "neither replay nor explicit revocation appends a successful rotation draft"
            );
        }
    })
    .await
    .expect("revocation-order fixtures exceeded 90 seconds");
}
