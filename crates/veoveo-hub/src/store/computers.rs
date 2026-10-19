//! Durable owner mapping, single-use PKCE state and fail-closed custody transitions.
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use veoveo_hub_sdk::HubRefreshFamilyId;
use veoveo_types::{PrincipalId, Sha256Digest, TenantId};

use super::{HubStore, HubStoreError, RefreshFamily, admitted};
use crate::{computers::ComputerId, contract::PkceCodeVerifier};

#[derive(Clone, Debug)]
pub struct Computer {
    pub id: ComputerId,
    pub tenant: TenantId,
    pub principal: PrincipalId,
    pub workspace_request: Uuid,
    pub template_request: Uuid,
    pub model_provider_request: Uuid,
    pub hub_provider_request: Uuid,
    pub sandbox_request: Uuid,
    pub handoff: Option<Uuid>,
    pub configure_request: Option<Uuid>,
    pub rotate_request: Option<Uuid>,
    pub family: Option<HubRefreshFamilyId>,
    pub ready: bool,
}
impl Computer {
    /// Native OpenShell DNS-routable workspace/sandbox names are limited to 19
    /// characters. Keep the full ID in Hub ownership and the cloned base path.
    pub fn native_name(&self) -> String {
        const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
        let digest = Sha256::digest(self.id.as_uuid().as_bytes());
        let mut name = String::with_capacity(19);
        name.push('c');
        // Encode the first 90 digest bits as 18 lowercase base32 characters.
        for bit in (0..90).step_by(5) {
            let byte = bit / 8;
            let window = (u16::from(digest[byte]) << 8) | u16::from(digest[byte + 1]);
            let index = usize::from((window >> (11 - bit % 8)) & 31);
            name.push(char::from(ALPHABET[index]));
        }
        name
    }
    pub fn base_path(&self) -> String {
        format!("/computer/c/{}/", self.id)
    }
    pub fn owned_by(&self, tenant: &TenantId, principal: &PrincipalId) -> bool {
        self.tenant == *tenant && self.principal == *principal
    }
}

struct ComputerRow {
    id: Uuid,
    tenant: String,
    principal_id: String,
    workspace_request: Uuid,
    template_request: Uuid,
    model_provider_request: Uuid,
    hub_provider_request: Uuid,
    sandbox_request: Uuid,
    handoff_id: Option<Uuid>,
    configure_request: Option<Uuid>,
    rotate_request: Option<Uuid>,
    family_id: Option<Uuid>,
    ready: bool,
}
impl TryFrom<ComputerRow> for Computer {
    type Error = HubStoreError;
    fn try_from(row: ComputerRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: admitted("computer.id", ComputerId::parse(row.id.to_string()))?,
            tenant: admitted("computer.tenant", TenantId::parse(row.tenant))?,
            principal: admitted(
                "computer.principal_id",
                PrincipalId::parse(row.principal_id),
            )?,
            workspace_request: row.workspace_request,
            template_request: row.template_request,
            model_provider_request: row.model_provider_request,
            hub_provider_request: row.hub_provider_request,
            sandbox_request: row.sandbox_request,
            handoff: row.handoff_id,
            configure_request: row.configure_request,
            rotate_request: row.rotate_request,
            family: row
                .family_id
                .map(|id| {
                    admitted(
                        "computer.family_id",
                        HubRefreshFamilyId::parse(id.to_string()),
                    )
                })
                .transpose()?,
            ready: row.ready,
        })
    }
}

pub struct ComputerHandoff {
    pub computer: Computer,
    transaction: Transaction<'static, Postgres>,
}
impl ComputerHandoff {
    /// Only the owned native worker commits ready state after Configure AND Rotate.
    pub async fn complete(
        mut self,
        family: &HubRefreshFamilyId,
        digest: &Sha256Digest,
    ) -> Result<(), HubStoreError> {
        sqlx::query_file!(
            "src/store/computers/complete.sql",
            self.computer.id.as_uuid(),
            family.as_uuid(),
            self.computer.tenant.as_str(),
            self.computer.principal.as_str()
        )
        .execute(&mut *self.transaction)
        .await?;
        sqlx::query_file!(
            "src/store/computers/session_insert.sql",
            digest.as_str(),
            self.computer.id.as_uuid(),
            family.as_uuid(),
            Utc::now()
        )
        .execute(&mut *self.transaction)
        .await?;
        self.transaction.commit().await?;
        Ok(())
    }
}

impl HubStore {
    /// Only an already-admitted browser owner calls this. Recheck the durable
    /// owner/current-family binding under the same row lock custody uses, then
    /// revoke and clear successor deliveries with the audit in one transaction.
    pub(crate) async fn revoke_computer_session(
        &self,
        computer: &Computer,
        family: &HubRefreshFamilyId,
        audit: &impl super::TransactionAppend,
    ) -> Result<bool, HubStoreError> {
        let mut transaction = self.pool().begin().await?;
        let row = sqlx::query_file!(
            "src/store/computers/revoke.sql",
            computer.id.as_uuid(),
            computer.tenant.as_str(),
            computer.principal.as_str(),
            family.as_uuid(),
            Utc::now(),
            super::RefreshRevocation::ClientRevocation as super::RefreshRevocation,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        if row.is_none() {
            return Ok(false);
        }
        // The family row remains locked through revocation, every bounded token
        // batch and the audit. Do not commit with older successor deliveries left.
        loop {
            let cleared =
                sqlx::query_file!("src/store/computers/clear_deliveries.sql", family.as_uuid())
                    .execute(&mut *transaction)
                    .await?
                    .rows_affected();
            if cleared == 0 {
                break;
            }
        }
        audit.append(&mut transaction).await?;
        transaction.commit().await?;
        Ok(true)
    }
    pub async fn ensure_computer(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<Computer, HubStoreError> {
        sqlx::query_file!(
            "src/store/computers/ensure.sql",
            Uuid::now_v7(),
            tenant.as_str(),
            principal.as_str(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Utc::now()
        )
        .execute(self.pool())
        .await?;
        sqlx::query_file_as!(
            ComputerRow,
            "src/store/computers/owner.sql",
            tenant.as_str(),
            principal.as_str()
        )
        .fetch_one(self.pool())
        .await?
        .try_into()
    }
    pub async fn computer(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        id: &ComputerId,
    ) -> Result<Option<Computer>, HubStoreError> {
        sqlx::query_file_as!(
            ComputerRow,
            "src/store/computers/read.sql",
            id.as_uuid(),
            tenant.as_str(),
            principal.as_str()
        )
        .fetch_optional(self.pool())
        .await?
        .map(Computer::try_from)
        .transpose()
    }
    /// Autocommitted before any native call. Concurrent logins wait behind an active
    /// worker row lock; a crashed worker leaves this durable pending state intact.
    pub async fn begin_computer_handoff(&self, computer: &Computer) -> Result<Uuid, HubStoreError> {
        let handoff = Uuid::now_v7();
        sqlx::query_file!(
            "src/store/computers/begin.sql",
            computer.id.as_uuid(),
            computer.tenant.as_str(),
            computer.principal.as_str(),
            handoff,
            Uuid::now_v7(),
            Uuid::now_v7()
        )
        .fetch_one(self.pool())
        .await?;
        Ok(handoff)
    }
    pub async fn lock_computer_handoff(
        &self,
        owner: &Computer,
        handoff: Uuid,
    ) -> Result<Option<ComputerHandoff>, HubStoreError> {
        let mut transaction = self.pool().begin().await?;
        let Some(row) = sqlx::query_file_as!(
            ComputerRow,
            "src/store/computers/lock.sql",
            owner.id.as_uuid(),
            owner.tenant.as_str(),
            owner.principal.as_str()
        )
        .fetch_optional(&mut *transaction)
        .await?
        else {
            return Ok(None);
        };
        let computer: Computer = row.try_into()?;
        if computer.handoff != Some(handoff) || computer.ready {
            return Ok(None);
        }
        Ok(Some(ComputerHandoff {
            computer,
            transaction,
        }))
    }
    /// Resolve the cookie's native family before selecting its tenant/principal's
    /// Computer. Current client, membership, expiry and readiness admission still
    /// belongs to BrowserSession construction.
    pub async fn computer_session(
        &self,
        digest: &Sha256Digest,
    ) -> Result<Option<(Computer, RefreshFamily)>, HubStoreError> {
        let row = sqlx::query_file!("src/store/computers/session_read.sql", digest.as_str())
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id = admitted(
            "computer_session.computer_id",
            ComputerId::parse(row.computer_id.to_string()),
        )?;
        let family = admitted(
            "computer_session.family_id",
            HubRefreshFamilyId::parse(row.family_id.to_string()),
        )?;
        let Some(family) = self.refresh_family(&family).await? else {
            return Ok(None);
        };
        let principal = &family.principal.principal;
        let Some(tenant) = principal.tenant.as_ref() else {
            return Ok(None);
        };
        Ok(self
            .computer(tenant, &principal.id, &id)
            .await?
            .map(|computer| (computer, family)))
    }
    pub async fn insert_computer_login(
        &self,
        state: &Sha256Digest,
        browser: &Sha256Digest,
        verifier: &PkceCodeVerifier,
        expires: DateTime<Utc>,
    ) -> Result<(), HubStoreError> {
        sqlx::query_file!(
            "src/store/computers/login_insert.sql",
            state.as_str(),
            browser.as_str(),
            verifier.as_str(),
            expires
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
    /// Consumes only the state bound to this initiating browser, once and before expiry.
    pub async fn consume_computer_login(
        &self,
        state: &Sha256Digest,
        browser: &Sha256Digest,
    ) -> Result<Option<PkceCodeVerifier>, HubStoreError> {
        let row = sqlx::query_file!(
            "src/store/computers/login_consume.sql",
            state.as_str(),
            browser.as_str(),
            Utc::now()
        )
        .fetch_optional(self.pool())
        .await?;
        row.map(|r| {
            admitted(
                "computer_login.verifier",
                PkceCodeVerifier::parse(r.verifier),
            )
        })
        .transpose()
    }
    pub async fn prune_computer_logins(&self) -> Result<(), HubStoreError> {
        sqlx::query_file!("src/store/computers/login_prune.sql", Utc::now())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HubRefreshIssueRequest, HubState,
        contract::{AuthorizationServerId, PrincipalDisplayName},
    };
    use chrono::TimeDelta;
    use std::{collections::BTreeSet, time::Duration};
    use veoveo_hub_sdk::{OAuthClientId, Principal, PrincipalKind, TokenIssuer, TokenSubject};
    use veoveo_types::WorkContextId;

    #[tokio::test]
    async fn native_handoff_rows_serialize_owners_and_fail_closed_after_rollback() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::new().await;
            let first = HubStore::new(db.a.clone());
            let second = HubStore::new(db.b.clone());
            let principal = Principal {
                id: PrincipalId::parse("fixture-user").unwrap(),
                kind: PrincipalKind::User,
                issuer: TokenIssuer::parse("https://identity.test").unwrap(),
                subject: TokenSubject::parse("fixture-user").unwrap(),
                tenant: Some(TenantId::parse("tenant-a").unwrap()),
                groups: BTreeSet::new(),
                group_roles: BTreeSet::new(),
                roles: BTreeSet::new(),
                scopes: BTreeSet::new(),
                data_labels: BTreeSet::new(),
                assurances: BTreeSet::new(),
                authenticated_at: None,
            };
            let computer = first
                .ensure_computer(principal.tenant.as_ref().unwrap(), &principal.id)
                .await
                .unwrap();
            let same = second
                .ensure_computer(&computer.tenant, &computer.principal)
                .await
                .unwrap();
            assert_eq!(same.id, computer.id);
            assert_eq!(same.native_name(), computer.native_name());
            assert_eq!(computer.native_name().len(), 19);
            assert!(
                computer
                    .native_name()
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            );
            let other = first
                .ensure_computer(&computer.tenant, &PrincipalId::parse("other-user").unwrap())
                .await
                .unwrap();
            assert_ne!(other.id, computer.id);
            assert_ne!(other.native_name(), computer.native_name());
            let issued = HubState::new(db.a.clone())
                .issue_refresh_token(HubRefreshIssueRequest {
                    authorization_server: &AuthorizationServerId::parse("fixture").unwrap(),
                    protected_resource: &veoveo_hub_sdk::ProtectedResourceId::parse(
                        "https://fixture.example/resource",
                    )
                    .unwrap(),
                    oauth_client_id: &OAuthClientId::parse("browser").unwrap(),
                    work_context: &WorkContextId::parse("fixture").unwrap(),
                    principal: &principal,
                    principal_display_name: &PrincipalDisplayName::new("Fixture user").unwrap(),
                    scopes: &BTreeSet::new(),
                    now: Utc::now(),
                })
                .await
                .unwrap();
            let digest = Sha256Digest::from_bytes([7; 32]);
            let first_id = first.begin_computer_handoff(&computer).await.unwrap();
            for (tenant, principal) in [
                (
                    TenantId::parse("tenant-b").unwrap(),
                    computer.principal.clone(),
                ),
                (computer.tenant.clone(), other.principal.clone()),
            ] {
                assert!(
                    first
                        .computer(&tenant, &principal, &computer.id)
                        .await
                        .unwrap()
                        .is_none(),
                    "a Computer identifier cannot bypass its tenant and principal"
                );
                let mut wrong_owner = computer.clone();
                wrong_owner.tenant = tenant;
                wrong_owner.principal = principal;
                assert!(
                    first
                        .lock_computer_handoff(&wrong_owner, first_id)
                        .await
                        .unwrap()
                        .is_none(),
                    "custody cannot lock another owner even with the current handoff ID"
                );
            }
            let worker = first
                .lock_computer_handoff(&computer, first_id)
                .await
                .unwrap()
                .unwrap();
            let next = second.begin_computer_handoff(&computer);
            tokio::pin!(next);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), &mut next)
                    .await
                    .is_err(),
                "a second login must wait for the active native custody worker"
            );
            worker
                .complete(&issued.grant.family_id, &digest)
                .await
                .unwrap();
            let next_id = next.await.unwrap();
            assert_ne!(next_id, first_id);
            let (pending, _) = first.computer_session(&digest).await.unwrap().unwrap();
            assert!(!pending.ready);
            assert!(
                pending.family.is_none(),
                "the later explicit handoff invalidates the completed browser binding"
            );
            assert!(
                first
                    .lock_computer_handoff(&computer, first_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            drop(
                first
                    .lock_computer_handoff(&computer, next_id)
                    .await
                    .unwrap()
                    .unwrap(),
            );
            let pending = second
                .computer(&computer.tenant, &computer.principal, &computer.id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                pending.handoff,
                Some(next_id),
                "worker rollback/crash must retain the precommitted pending identity"
            );
            assert!(!pending.ready);
            let replacement = HubState::new(db.a.clone())
                .issue_refresh_token(HubRefreshIssueRequest {
                    authorization_server: &AuthorizationServerId::parse("fixture").unwrap(),
                    protected_resource: &veoveo_hub_sdk::ProtectedResourceId::parse(
                        "https://fixture.example/resource",
                    )
                    .unwrap(),
                    oauth_client_id: &OAuthClientId::parse("browser").unwrap(),
                    work_context: &WorkContextId::parse("fixture").unwrap(),
                    principal: &principal,
                    principal_display_name: &PrincipalDisplayName::new("Fixture user").unwrap(),
                    scopes: &BTreeSet::new(),
                    now: Utc::now(),
                })
                .await
                .unwrap();
            // Production callbacks generate a new random cookie per login.
            // Keep the old digest present to exercise stale-family refusal.
            let replacement_digest = Sha256Digest::from_bytes([8; 32]);
            first
                .lock_computer_handoff(&computer, next_id)
                .await
                .unwrap()
                .unwrap()
                .complete(&replacement.grant.family_id, &replacement_digest)
                .await
                .unwrap();
            struct NoAudit;
            impl super::super::TransactionAppend for NoAudit {
                async fn append(&self, _: &mut sqlx::PgConnection) -> Result<(), HubStoreError> {
                    Ok(())
                }
            }
            assert!(
                !first
                    .revoke_computer_session(&computer, &issued.grant.family_id, &NoAudit)
                    .await
                    .unwrap(),
                "stale logout cannot revoke the newer family"
            );
            let mut wrong_owner = computer.clone();
            wrong_owner.principal = other.principal;
            assert!(
                !first
                    .revoke_computer_session(&wrong_owner, &replacement.grant.family_id, &NoAudit)
                    .await
                    .unwrap()
            );
            // Synthetic stored envelopes cover two clearing batches. Audit
            // rollback must retain all of them, and successful logout clears all.
            let token_ids: Vec<_> = (0..1001).map(|_| Uuid::now_v7()).collect();
            let now = Utc::now();
            sqlx::query_file!(
                "src/store/computers/tests/seed_deliveries.sql",
                &token_ids,
                replacement.grant.family_id.as_uuid(),
                now,
                now + TimeDelta::minutes(5)
            )
            .execute(first.pool())
            .await
            .unwrap();
            struct FailedAudit;
            impl super::super::TransactionAppend for FailedAudit {
                async fn append(&self, _: &mut sqlx::PgConnection) -> Result<(), HubStoreError> {
                    Err(HubStoreError::InvalidStoredValue {
                        column: "test.audit",
                    })
                }
            }
            assert!(
                first
                    .revoke_computer_session(&computer, &replacement.grant.family_id, &FailedAudit)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_file!(
                    "src/store/computers/tests/count_deliveries.sql",
                    replacement.grant.family_id.as_uuid()
                )
                .fetch_one(second.pool())
                .await
                .unwrap()
                .remaining,
                1001,
                "audit failure must roll back every envelope batch"
            );
            assert!(
                second
                    .refresh_family(&replacement.grant.family_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .revoked_at
                    .is_none(),
                "audit failure must roll back logout revocation"
            );
            assert!(
                first
                    .revoke_computer_session(&computer, &replacement.grant.family_id, &NoAudit)
                    .await
                    .unwrap()
            );
            assert_eq!(
                sqlx::query_file!(
                    "src/store/computers/tests/count_deliveries.sql",
                    replacement.grant.family_id.as_uuid()
                )
                .fetch_one(second.pool())
                .await
                .unwrap()
                .remaining,
                0,
                "logout must clear every envelope, including later batches"
            );
            assert_eq!(
                second
                    .refresh_family(&replacement.grant.family_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .revocation,
                Some(super::super::RefreshRevocation::ClientRevocation)
            );
            assert!(
                second
                    .refresh_family(&issued.grant.family_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .revoked_at
                    .is_none()
            );
        })
        .await
        .expect("Computer handoff fixture exceeded 90 seconds");
    }

    #[tokio::test]
    async fn login_state_is_browser_bound_single_use_and_expiring() {
        let db = crate::test_store::TestDb::new().await;
        let store = HubStore::new(db.a.clone());
        let state = Sha256Digest::from_bytes([1; 32]);
        let browser = Sha256Digest::from_bytes([2; 32]);
        let wrong = Sha256Digest::from_bytes([3; 32]);
        let verifier = PkceCodeVerifier::parse("x".repeat(43)).unwrap();
        store
            .insert_computer_login(
                &state,
                &browser,
                &verifier,
                Utc::now() + TimeDelta::minutes(5),
            )
            .await
            .unwrap();
        assert!(
            store
                .consume_computer_login(&state, &wrong)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .consume_computer_login(&state, &browser)
                .await
                .unwrap(),
            Some(verifier.clone())
        );
        assert!(
            store
                .consume_computer_login(&state, &browser)
                .await
                .unwrap()
                .is_none()
        );
        store
            .insert_computer_login(
                &state,
                &browser,
                &verifier,
                Utc::now() - TimeDelta::seconds(1),
            )
            .await
            .unwrap();
        assert!(
            store
                .consume_computer_login(&state, &browser)
                .await
                .unwrap()
                .is_none()
        );
        store.prune_computer_logins().await.unwrap();
    }
}
