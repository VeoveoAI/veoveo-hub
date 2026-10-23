//! Unmodified native v1 protocol. No namespace, proxy, or replacement auth path.
use anyhow::{Context, Result, ensure};
use lore_proto::lore::{
    model::v1::Repository,
    repository::v1::{
        RepositoryCreateRequest, RepositoryGetRequest, repository_get_request::Query,
        repository_service_client::RepositoryServiceClient,
    },
};
use tonic::{
    Request,
    transport::{Channel, ClientTlsConfig},
};

use super::{Association, BranchId};
use crate::contributions::lore::RepositoryId;
#[derive(Debug, thiserror::Error)]
#[error("native workspace request refused: {0}")]
pub(crate) struct NativeRefusal(pub tonic::Code);
use crate::contributions::lore::Deployment;

pub(crate) struct Client(RepositoryServiceClient<Channel>);
impl Client {
    pub async fn connect(deployment: &Deployment) -> Result<Self> {
        let channel = deployment
            .tls_endpoint()?
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(30))
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .connect()
            .await
            .context("connect native workspace service")?;
        Ok(Self(RepositoryServiceClient::new(channel)))
    }
    pub async fn create(&mut self, association: &Association, token: &str) -> Result<Repository> {
        let branch = association
            .default_branch
            .as_ref()
            .context("reserved native branch missing")?;
        self.create_repository(
            &association.repository,
            branch,
            &association.name,
            "Personal workspace",
            token,
        )
        .await
    }
    /// Create the repository `id` with its default branch `main` as the token's person,
    /// whom Lore records as its creator. Lore answers a repository of that ID and name
    /// that exists already, whoever created it, with its record unchanged.
    pub async fn create_repository(
        &mut self,
        id: &RepositoryId,
        branch: &BranchId,
        name: &str,
        description: &str,
        token: &str,
    ) -> Result<Repository> {
        let request = RepositoryCreateRequest {
            id: hex::decode(id.as_str())?.into(),
            default_branch_id: hex::decode(branch.as_str())?.into(),
            name: name.into(),
            description: description.into(),
            default_branch_name: "main".into(),
            creator: None,
        };
        self.0
            .repository_create(authorized(request, token)?)
            .await
            .map_err(|status| NativeRefusal(status.code()))?
            .into_inner()
            .repository
            .context("native workspace Create returned no repository")
    }
    pub async fn get(&mut self, association: &Association, token: &str) -> Result<Repository> {
        self.0
            .repository_get(authorized(
                RepositoryGetRequest {
                    query: Some(Query::Id(
                        hex::decode(association.repository.as_str())?.into(),
                    )),
                },
                token,
            )?)
            .await
            .map_err(|status| NativeRefusal(status.code()))?
            .into_inner()
            .repository
            .context("native workspace Get returned no repository")
    }
}
fn authorized<T>(message: T, token: &str) -> Result<Request<T>> {
    let mut request = Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    Ok(request)
}

#[derive(Clone, Copy)]
pub(super) enum RepositoryProof {
    Creation,
    Association,
}
pub(super) fn verify(
    association: &Association,
    repository: &Repository,
    proof: RepositoryProof,
) -> Result<BranchId> {
    ensure!(
        hex::encode(&repository.id) == association.repository.as_str(),
        "native workspace response repository differs"
    );
    if matches!(proof, RepositoryProof::Association)
        && let Some(branch) = &association.default_branch
    {
        return Ok(branch.clone());
    }
    let branch = BranchId::parse(hex::encode(&repository.default_branch_id))?;
    if matches!(proof, RepositoryProof::Creation) {
        ensure!(
            repository.creator == association.principal.as_str(),
            "native workspace creator differs from authenticated caller"
        );
        ensure!(
            repository.name == association.name && repository.default_branch_name == "main",
            "native workspace response differs from reserved creation"
        );
        ensure!(
            association.default_branch.as_ref() == Some(&branch),
            "native workspace default branch differs from reservation"
        );
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::lore::{DeploymentId, RepositoryId};
    use veoveo_types::{PrincipalId, TenantId};
    #[test]
    fn native_create_retry_verifies_actual_identity_not_only_name() {
        let association = Association {
            tenant: TenantId::parse("tenant").unwrap(),
            principal: PrincipalId::parse("alice").unwrap(),
            deployment: DeploymentId::parse("primary").unwrap(),
            repository: RepositoryId::parse("11111111111111111111111111111111").unwrap(),
            default_branch: Some(BranchId::parse("22222222222222222222222222222222").unwrap()),
            name: "workspace-one".into(),
            publication_id: veoveo_hub::new_hub_control_plane_revision_id().unwrap(),
            last_status: super::super::Status::Preparing,
            computer: None,
            mode: super::super::Mode::Create,
            state: super::super::State::Reserved,
        };
        let mut repository = Repository {
            id: hex::decode(association.repository.as_str()).unwrap().into(),
            name: association.name.clone(),
            default_branch_id: hex::decode(association.default_branch.as_ref().unwrap().as_str())
                .unwrap()
                .into(),
            default_branch_name: "main".into(),
            creator: "alice".into(),
            ..Default::default()
        };
        assert!(verify(&association, &repository, RepositoryProof::Creation).is_ok());
        // A completed association follows native IDs, not mutable display labels.
        repository.name = "renamed-personal-workspace".into();
        repository.default_branch_name = "renamed-default".into();
        repository.default_branch_id = vec![9; 16].into();
        assert_eq!(
            verify(&association, &repository, RepositoryProof::Association).unwrap(),
            association.default_branch.clone().unwrap()
        );
        assert!(verify(&association, &repository, RepositoryProof::Creation).is_err());
        repository.name = association.name.clone();
        repository.default_branch_name = "main".into();
        repository.default_branch_id =
            hex::decode(association.default_branch.as_ref().unwrap().as_str())
                .unwrap()
                .into();
        repository.creator = "another-user".into();
        assert!(verify(&association, &repository, RepositoryProof::Creation).is_err());
        repository.creator = "alice".into();
        repository.default_branch_id = vec![3; 16].into();
        assert!(verify(&association, &repository, RepositoryProof::Creation).is_err());
        repository.default_branch_id =
            hex::decode(association.default_branch.as_ref().unwrap().as_str())
                .unwrap()
                .into();
        repository.id = vec![4; 16].into();
        assert!(verify(&association, &repository, RepositoryProof::Creation).is_err());
    }
}
