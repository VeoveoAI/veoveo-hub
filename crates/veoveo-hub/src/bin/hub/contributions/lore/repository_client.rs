//! Unmodified native v1 protocol. No namespace, proxy, or replacement auth path.
use anyhow::{Context, Result};
use lore_proto::lore::{
    model::v1::Repository,
    repository::v1::{RepositoryCreateRequest, repository_service_client::RepositoryServiceClient},
};
use tonic::{
    Request,
    transport::{Channel, ClientTlsConfig},
};

use super::{BranchId, Deployment, RepositoryId};

#[derive(Debug, thiserror::Error)]
#[error("native repository request refused: {0}")]
pub(crate) struct NativeRefusal(pub tonic::Code);

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
            .context("connect native repository service")?;
        Ok(Self(RepositoryServiceClient::new(channel)))
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
            .context("native repository Create returned no repository")
    }
}
fn authorized<T>(message: T, token: &str) -> Result<Request<T>> {
    let mut request = Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    Ok(request)
}
