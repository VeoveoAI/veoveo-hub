//! RFC 8628 request state. The row lock serializes polling and one-time redemption;
//! codes are random capabilities stored only as SHA-256 digests.
use super::HubStore;
use crate::contract::PrincipalDisplayName;
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::types::Json;
use std::collections::BTreeSet;
use uuid::Uuid;
use veoveo_hub_sdk::{OAuthClientId, Principal, ProtectedResourceId};
use veoveo_types::{ScopeName, Sha256Digest, WorkContextId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub enum DeviceStatus {
    Pending,
    Authorizing,
    Approved,
    Denied,
    Consumed,
}
struct DeviceRow {
    id: Uuid,
    protected_resource: String,
    oauth_client_id: String,
    work_context: String,
    scopes: Vec<String>,
    state: DeviceStatus,
    browser_digest: Option<String>,
    principal: Option<Json<Principal>>,
    principal_display_name: Option<String>,
    poll_interval: i32,
    next_poll: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}
#[derive(Clone)]
pub struct DeviceAuthorization {
    pub id: Uuid,
    pub resource: ProtectedResourceId,
    pub client: OAuthClientId,
    pub work_context: WorkContextId,
    pub scopes: BTreeSet<ScopeName>,
    pub expires_at: DateTime<Utc>,
    pub browser_digest: Option<Sha256Digest>,
}
impl TryFrom<DeviceRow> for DeviceAuthorization {
    type Error = anyhow::Error;
    fn try_from(row: DeviceRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            resource: ProtectedResourceId::parse(row.protected_resource)?,
            client: OAuthClientId::parse(row.oauth_client_id)?,
            work_context: WorkContextId::parse(row.work_context)?,
            scopes: row
                .scopes
                .into_iter()
                .map(ScopeName::parse)
                .collect::<std::result::Result<_, _>>()?,
            expires_at: row.expires_at,
            browser_digest: row.browser_digest.map(Sha256Digest::parse).transpose()?,
        })
    }
}
pub struct NewDeviceAuthorization<'a> {
    pub id: Uuid,
    pub device: &'a Sha256Digest,
    pub user: &'a Sha256Digest,
    pub resource: &'a ProtectedResourceId,
    pub client: &'a OAuthClientId,
    pub work_context: &'a WorkContextId,
    pub scopes: &'a BTreeSet<ScopeName>,
    pub now: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
pub enum DevicePoll {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Invalid,
    Approved(Box<DeviceApproval>),
}
pub struct DeviceApproval {
    pub request: DeviceAuthorization,
    pub principal: Principal,
    pub display_name: PrincipalDisplayName,
}
impl HubStore {
    pub async fn create_device_authorization(
        &self,
        request: NewDeviceAuthorization<'_>,
    ) -> Result<()> {
        let scopes = request
            .scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        sqlx::query_file!(
            "src/store/device/create.sql",
            request.id,
            request.device.as_str(),
            request.user.as_str(),
            request.resource.as_str(),
            request.client.as_str(),
            request.work_context.as_str(),
            &scopes,
            request.now + TimeDelta::seconds(5),
            request.now,
            request.expires_at
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
    pub async fn device_authorization(
        &self,
        user: &Sha256Digest,
        now: DateTime<Utc>,
    ) -> Result<Option<DeviceAuthorization>> {
        sqlx::query_file_as!(DeviceRow, "src/store/device/lookup.sql", user.as_str(), now)
            .fetch_optional(self.pool())
            .await?
            .map(TryInto::try_into)
            .transpose()
    }
    pub async fn begin_device_authorization(
        &self,
        id: Uuid,
        user: &Sha256Digest,
        browser: &Sha256Digest,
        now: DateTime<Utc>,
    ) -> Result<Option<DeviceAuthorization>> {
        sqlx::query_file_as!(
            DeviceRow,
            "src/store/device/begin.sql",
            id,
            user.as_str(),
            browser.as_str(),
            now
        )
        .fetch_optional(self.pool())
        .await?
        .map(TryInto::try_into)
        .transpose()
    }
    pub async fn approve_device_authorization(
        &self,
        id: Uuid,
        browser: &Sha256Digest,
        principal: &Principal,
        display_name: &PrincipalDisplayName,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        Ok(sqlx::query_file!(
            "src/store/device/approve.sql",
            id,
            browser.as_str(),
            Json(principal) as _,
            display_name.as_str(),
            now
        )
        .fetch_optional(self.pool())
        .await?
        .is_some())
    }
    pub async fn deny_device_authorization(&self, id: Uuid) -> Result<()> {
        sqlx::query_file!("src/store/device/deny.sql", id)
            .fetch_optional(self.pool())
            .await?;
        Ok(())
    }
    pub async fn poll_device_authorization(
        &self,
        device: &Sha256Digest,
        client: &OAuthClientId,
        now: DateTime<Utc>,
    ) -> Result<DevicePoll> {
        let mut tx = self.pool().begin().await?;
        let Some(row) = sqlx::query_file_as!(
            DeviceRow,
            "src/store/device/poll.sql",
            device.as_str(),
            client.as_str()
        )
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(DevicePoll::Invalid);
        };
        if row.expires_at <= now {
            return Ok(DevicePoll::Expired);
        }
        if row.state == DeviceStatus::Denied {
            return Ok(DevicePoll::Denied);
        }
        if row.state == DeviceStatus::Consumed {
            return Ok(DevicePoll::Invalid);
        }
        let too_soon = row.next_poll > now;
        let interval = row
            .poll_interval
            .checked_add(if too_soon { 5 } else { 0 })
            .context("device polling interval overflow")?;
        sqlx::query_file!(
            "src/store/device/advance.sql",
            row.id,
            interval,
            now + TimeDelta::seconds(i64::from(interval))
        )
        .execute(&mut *tx)
        .await?;
        if too_soon {
            tx.commit().await?;
            return Ok(DevicePoll::SlowDown);
        }
        if row.state != DeviceStatus::Approved {
            tx.commit().await?;
            return Ok(DevicePoll::Pending);
        }
        let principal = row
            .principal
            .as_ref()
            .context("approved device has no principal")?
            .0
            .clone();
        let display_name = PrincipalDisplayName::new(
            row.principal_display_name
                .as_ref()
                .context("approved device has no display name")?
                .clone(),
        )?;
        ensure!(
            sqlx::query_file!("src/store/device/consume.sql", row.id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some(),
            "device approval changed while locked"
        );
        let request = row.try_into()?;
        tx.commit().await?;
        Ok(DevicePoll::Approved(Box::new(DeviceApproval {
            request,
            principal,
            display_name,
        })))
    }
    pub async fn prune_device_authorizations(&self, now: DateTime<Utc>) -> Result<()> {
        loop {
            let result = sqlx::query_file!("src/store/device/prune.sql", now)
                .execute(self.pool())
                .await?;
            if result.rows_affected() < 1000 {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn digest(byte: u8) -> Sha256Digest {
        Sha256Digest::parse(format!("sha256:{}", hex::encode([byte; 32]))).unwrap()
    }
    #[tokio::test]
    async fn device_polling_browser_binding_and_one_time_redemption_are_persisted() {
        tokio::time::timeout(std::time::Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::new().await;
            let left = HubStore::new(db.a.clone()); let right = HubStore::new(db.b.clone());
            let now = Utc::now(); let id = Uuid::now_v7(); let device = digest(1); let user = digest(2); let browser = digest(3);
            let client = OAuthClientId::parse("fixture-cli").unwrap(); let resource = ProtectedResourceId::parse("https://fixture.example/resource").unwrap(); let context = WorkContextId::parse("fixture").unwrap(); let scopes = BTreeSet::from([ScopeName::parse("offline_access").unwrap()]);
            left.create_device_authorization(NewDeviceAuthorization { id, device:&device, user:&user, client:&client, resource:&resource, work_context:&context, scopes:&scopes, now, expires_at: now + TimeDelta::minutes(10) }).await.unwrap();
            assert!(matches!(right.poll_device_authorization(&device, &OAuthClientId::parse("wrong").unwrap(), now).await.unwrap(), DevicePoll::Invalid));
            assert!(matches!(right.poll_device_authorization(&device, &client, now + TimeDelta::seconds(5)).await.unwrap(), DevicePoll::Pending));
            assert!(matches!(left.poll_device_authorization(&device, &client, now + TimeDelta::seconds(6)).await.unwrap(), DevicePoll::SlowDown));
            assert!(left.begin_device_authorization(id, &digest(9), &browser, now).await.unwrap().is_none());
            right.begin_device_authorization(id, &user, &browser, now).await.unwrap().unwrap();
            let principal: Principal = serde_json::from_value(serde_json::json!({"id":"fixture-user","kind":"user","issuer":"https://identity.example","subject":"user","tenant":"fixture"})).unwrap();
            let display = PrincipalDisplayName::new("Fixture").unwrap();
            assert!(!left.approve_device_authorization(id, &digest(4), &principal, &display, now).await.unwrap());
            assert!(left.approve_device_authorization(id, &browser, &principal, &display, now).await.unwrap());
            let time = now + TimeDelta::seconds(17);
            let (a,b) = tokio::join!(left.poll_device_authorization(&device, &client, time), right.poll_device_authorization(&device, &client, time));
            let results = [a.unwrap(), b.unwrap()];
            assert_eq!(results.iter().filter(|r| matches!(r, DevicePoll::Approved(_))).count(), 1);
            assert_eq!(results.iter().filter(|r| matches!(r, DevicePoll::Invalid)).count(), 1);
            assert!(left.device_authorization(&user, time).await.unwrap().is_none());
            let expiring_device = digest(5); let expiring_user = digest(6); let expiring_id = Uuid::now_v7();
            left.create_device_authorization(NewDeviceAuthorization { id:expiring_id, device:&expiring_device, user:&expiring_user, client:&client, resource:&resource, work_context:&context, scopes:&scopes, now, expires_at:now+TimeDelta::seconds(30) }).await.unwrap();
            assert!(matches!(right.poll_device_authorization(&expiring_device, &client, now+TimeDelta::seconds(31)).await.unwrap(), DevicePoll::Expired));
            left.deny_device_authorization(expiring_id).await.unwrap();
            assert!(matches!(right.poll_device_authorization(&expiring_device, &client, now+TimeDelta::seconds(10)).await.unwrap(), DevicePoll::Denied));
            left.prune_device_authorizations(now+TimeDelta::minutes(11)).await.unwrap();
            assert!(matches!(right.poll_device_authorization(&device, &client, now+TimeDelta::minutes(11)).await.unwrap(), DevicePoll::Invalid));
        }).await.expect("device store fixture exceeded 90 seconds");
    }
}
