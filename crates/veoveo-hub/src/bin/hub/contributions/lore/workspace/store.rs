use anyhow::{Context, Result, ensure};
use serde::Serialize;
use veoveo_hub::control_plane::HubControlPlaneRevisionId;
use veoveo_store::PlatformStore;
use veoveo_types::{PrincipalId, TenantId};

use super::{BranchId, Selection};
use crate::contributions::lore::{DeploymentId, RepositoryId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Create,
    Reuse,
}
impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Reuse => "reuse",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Reserved,
    Verified,
    Ready,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Association {
    pub tenant: TenantId,
    pub principal: PrincipalId,
    pub deployment: DeploymentId,
    pub repository: RepositoryId,
    pub default_branch: Option<BranchId>,
    pub name: String,
    pub publication_id: HubControlPlaneRevisionId,
    pub last_status: super::Status,
    pub computer: Option<veoveo_hub::computers::ComputerId>,
    pub mode: Mode,
    pub state: State,
}
struct Row {
    tenant: String,
    principal_id: String,
    deployment: String,
    repository: String,
    default_branch: Option<String>,
    name: String,
    mode: String,
    state: String,
    publication_id: String,
    last_status: String,
    computer_id: Option<uuid::Uuid>,
}
impl TryFrom<Row> for Association {
    type Error = anyhow::Error;
    fn try_from(row: Row) -> Result<Self> {
        Ok(Self {
            tenant: TenantId::parse(row.tenant)?,
            principal: PrincipalId::parse(row.principal_id)?,
            deployment: DeploymentId::parse(row.deployment)?,
            repository: RepositoryId::parse(row.repository)?,
            default_branch: row.default_branch.map(BranchId::parse).transpose()?,
            name: row.name,
            publication_id: HubControlPlaneRevisionId::parse(row.publication_id)?,
            last_status: serde_json::from_value(serde_json::Value::String(row.last_status))?,
            computer: row
                .computer_id
                .map(|id| veoveo_hub::computers::ComputerId::parse(id.to_string()))
                .transpose()?,
            mode: match row.mode.as_str() {
                "create" => Mode::Create,
                "reuse" => Mode::Reuse,
                _ => anyhow::bail!("invalid stored workspace mode"),
            },
            state: match row.state.as_str() {
                "reserved" => State::Reserved,
                "verified" => State::Verified,
                "ready" => State::Ready,
                _ => anyhow::bail!("invalid stored workspace state"),
            },
        })
    }
}

#[derive(Clone)]
pub(crate) struct WorkspaceStore(PlatformStore);
impl WorkspaceStore {
    pub fn new(store: PlatformStore) -> Self {
        Self(store)
    }

    pub async fn get(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<Option<Association>> {
        sqlx::query_file_as!(
            Row,
            "src/bin/hub/contributions/lore/workspace/get.sql",
            tenant.as_str(),
            principal.as_str()
        )
        .fetch_optional(self.0.pool())
        .await?
        .map(Association::try_from)
        .transpose()
    }

    /// Reserve before native I/O. An interrupted request reuses the first IDs;
    /// a concurrent request/default change cannot replace the owner association.
    pub async fn reserve(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        selection: &Selection,
    ) -> Result<Association> {
        let (repository, branch, mode) = match &selection.repository {
            Some(repository) => (repository.clone(), None, Mode::Reuse),
            None => (
                RepositoryId::parse(uuid::Uuid::now_v7().simple().to_string())?,
                Some(BranchId::parse(uuid::Uuid::now_v7().simple().to_string())?),
                Mode::Create,
            ),
        };
        let name = format!("workspace-{}", repository.as_str());
        let publication_id = veoveo_hub::new_hub_control_plane_revision_id()?;
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/workspace/reserve.sql",
            tenant.as_str(),
            principal.as_str(),
            selection.deployment.as_str(),
            repository.as_str(),
            branch.as_ref().map(BranchId::as_str),
            name,
            mode.as_str(),
            publication_id.as_str()
        )
        .execute(self.0.pool())
        .await?;
        self.get(tenant, principal)
            .await?
            .context("workspace reservation missing")
    }

    pub async fn verify(
        &self,
        association: &Association,
        branch: &BranchId,
    ) -> Result<Association> {
        ensure!(
            association
                .default_branch
                .as_ref()
                .is_none_or(|expected| expected == branch),
            "native workspace branch differs from reservation"
        );
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/workspace/verify.sql",
            association.tenant.as_str(),
            association.principal.as_str(),
            association.deployment.as_str(),
            association.repository.as_str(),
            branch.as_str()
        )
        .fetch_optional(self.0.pool())
        .await?
        .context("workspace verification superseded")?;
        self.get(&association.tenant, &association.principal)
            .await?
            .context("verified workspace missing")
    }

    /// Only after native response verification and authoritative CAS publication.
    pub async fn complete(&self, association: &Association) -> Result<()> {
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/workspace/complete.sql",
            association.tenant.as_str(),
            association.principal.as_str(),
            association.deployment.as_str(),
            association.repository.as_str()
        )
        .fetch_optional(self.0.pool())
        .await?
        .context("workspace completion superseded")?;
        Ok(())
    }
    pub async fn published(&self, association: &Association) -> Result<bool> {
        Ok(sqlx::query_file!(
            "src/bin/hub/contributions/lore/workspace/published.sql",
            association.publication_id.as_str()
        )
        .fetch_optional(self.0.pool())
        .await?
        .is_some())
    }
    pub async fn status(
        &self,
        association: &Association,
        computer: &veoveo_hub::computers::ComputerId,
        status: super::Status,
    ) -> Result<()> {
        sqlx::query_file!(
            "src/bin/hub/contributions/lore/workspace/status.sql",
            association.tenant.as_str(),
            association.principal.as_str(),
            status.as_str(),
            computer.as_uuid()
        )
        .execute(self.0.pool())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_hub::store::HubStore;

    #[tokio::test]
    async fn concurrent_first_use_keeps_reserved_native_and_publication_ids_across_retries() {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let db = veoveo_fixtures::store::TestDb::new().await;
            HubStore::new(db.a.clone()).migrate().await.unwrap();
            let store = WorkspaceStore::new(db.a.clone());
            let tenant = TenantId::parse("tenant-a").unwrap();
            let principal = PrincipalId::parse("alice").unwrap();
            let selection = Selection {
                deployment: DeploymentId::parse("source").unwrap(),
                repository: None,
            };
            let reservations = futures::future::join_all(
                (0..8).map(|_| store.reserve(&tenant, &principal, &selection)),
            )
            .await;
            let first = reservations[0].as_ref().unwrap();
            for result in &reservations {
                let result = result.as_ref().unwrap();
                assert_eq!(result.repository, first.repository);
                assert_eq!(result.default_branch, first.default_branch);
                assert_eq!(result.publication_id, first.publication_id);
                assert_eq!(result.name, first.name);
            }
            let changed_default = Selection {
                deployment: DeploymentId::parse("another-source").unwrap(),
                repository: None,
            };
            let retained = store
                .reserve(&tenant, &principal, &changed_default)
                .await
                .unwrap();
            assert_eq!(retained.deployment, first.deployment);
            assert_eq!(retained.repository, first.repository);
            let other = store
                .reserve(
                    &TenantId::parse("tenant-b").unwrap(),
                    &principal,
                    &selection,
                )
                .await
                .unwrap();
            assert_ne!(other.repository, first.repository);
            let wrong_branch = BranchId::parse("ffffffffffffffffffffffffffffffff").unwrap();
            assert!(store.verify(first, &wrong_branch).await.is_err());
            assert_eq!(
                store.get(&tenant, &principal).await.unwrap().unwrap().state,
                State::Reserved
            );
            let verified = store
                .verify(first, first.default_branch.as_ref().unwrap())
                .await
                .unwrap();
            assert_eq!(verified.state, State::Verified);
            assert!(!store.published(&verified).await.unwrap());
            let plane: veoveo_hub::control_plane::HubControlPlane =
                serde_json::from_str(include_str!("../../../../../../testdata/hub.json")).unwrap();
            sqlx::query_file!(
                "src/bin/hub/contributions/lore/workspace/tests/publication.sql",
                verified.publication_id.as_str(),
                principal.as_str(),
                tenant.as_str(),
                sqlx::types::Json(plane) as _
            )
            .execute(db.a.pool())
            .await
            .unwrap();
            assert!(store.published(&verified).await.unwrap());
            store.complete(&verified).await.unwrap();
            assert_eq!(
                store.get(&tenant, &principal).await.unwrap().unwrap().state,
                State::Ready
            );
            // Ready state and immutable publication history remain independent of
            // any Computer/session FK or a later installation-default selection.
            assert_eq!(
                store
                    .reserve(&tenant, &principal, &changed_default)
                    .await
                    .unwrap()
                    .publication_id,
                first.publication_id
            );
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn reuse_keeps_existing_native_id_and_requires_actual_branch_verification() {
        let db = veoveo_fixtures::store::TestDb::new().await;
        HubStore::new(db.a.clone()).migrate().await.unwrap();
        let store = WorkspaceStore::new(db.a.clone());
        let tenant = TenantId::parse("tenant-a").unwrap();
        let principal = PrincipalId::parse("alice").unwrap();
        let repository = RepositoryId::parse("1234567890abcdef1234567890abcdef").unwrap();
        let selection = Selection {
            deployment: DeploymentId::parse("source").unwrap(),
            repository: Some(repository.clone()),
        };
        let association = store
            .reserve(&tenant, &principal, &selection)
            .await
            .unwrap();
        assert_eq!(association.mode, Mode::Reuse);
        assert_eq!(association.repository, repository);
        assert!(association.default_branch.is_none());
        assert!(store.complete(&association).await.is_err());
        let actual = BranchId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let verified = store.verify(&association, &actual).await.unwrap();
        assert_eq!(verified.default_branch, Some(actual));
        // Sharing an existing authorized workspace is a per-user binding, not
        // exclusive ownership of the repository ID and not a new Owner grant.
        let member = store
            .reserve(&tenant, &PrincipalId::parse("bob").unwrap(), &selection)
            .await
            .unwrap();
        assert_eq!(member.repository, repository);
    }
}
