//! Unmodified native v1 protocol. No namespace, proxy, or replacement auth path.
use anyhow::{Context, Result};
use lore_proto::lore::{
    model::v1::Repository,
    repository::v1::{
        RepositoryCreateRequest, RepositoryListRequest, RepositoryListResponse,
        RepositoryTextMetadata, repository_service_client::RepositoryServiceClient,
    },
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
    /// List exact native text matches as the caller; an ignored predicate is refused.
    pub async fn list_by_text(
        &mut self,
        filter: RepositoryTextMetadata,
        token: &str,
    ) -> Result<Vec<Repository>> {
        let response = self
            .0
            .repository_list(authorized(
                RepositoryListRequest {
                    creator: None,
                    text_metadata: Some(filter.clone()),
                },
                token,
            )?)
            .await
            .map_err(|status| NativeRefusal(status.code()))?;
        ensure_filter_support(response.metadata())?;
        let mut stream = response.into_inner();
        let mut repositories = Vec::new();
        while let Some(record) = stream
            .message()
            .await
            .map_err(|status| NativeRefusal(status.code()))?
        {
            anyhow::ensure!(
                repositories.len() < 4096,
                "native repository listing exceeds bound"
            );
            repositories.push(matched_repository(record, &filter)?);
        }
        Ok(repositories)
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

fn ensure_filter_support(metadata: &tonic::metadata::MetadataMap) -> Result<()> {
    anyhow::ensure!(
        metadata
            .get("lore-repository-text-filter")
            .is_some_and(|value| value == "1"),
        "native repository text predicate is unavailable"
    );
    Ok(())
}

fn matched_repository(
    record: RepositoryListResponse,
    filter: &RepositoryTextMetadata,
) -> Result<Repository> {
    anyhow::ensure!(
        record.matched_text_metadata.as_ref() == Some(filter),
        "native repository text predicate was not matched"
    );
    let repository = record
        .repository
        .context("native listing returned no repository")?;
    anyhow::ensure!(
        repository.id.len() == 16
            && !repository.name.is_empty()
            && repository.name.len() <= 256
            && !repository.default_branch_name.is_empty()
            && repository.default_branch_name.len() <= 256,
        "native listing returned invalid repository fields"
    );
    Ok(repository)
}

#[cfg(test)]
mod text_listing_tests {
    use super::*;

    #[test]
    fn ignored_filters_and_wrong_echoes_are_refused_including_empty_stream_support() {
        let mut metadata = tonic::metadata::MetadataMap::new();
        assert!(ensure_filter_support(&metadata).is_err());
        metadata.insert("lore-repository-text-filter", "1".parse().unwrap());
        assert!(ensure_filter_support(&metadata).is_ok());
        let filter = RepositoryTextMetadata {
            key: "app.project".into(),
            text: "true".into(),
        };
        let repository = Repository {
            id: vec![7; 16].into(),
            name: "source-project".into(),
            default_branch_name: "workspace".into(),
            ..Default::default()
        };
        let matching = RepositoryListResponse {
            repository: Some(repository.clone()),
            matched_text_metadata: Some(filter.clone()),
        };
        assert_eq!(
            matched_repository(matching.clone(), &filter)
                .unwrap()
                .default_branch_name,
            "workspace"
        );
        assert!(
            matched_repository(
                RepositoryListResponse {
                    matched_text_metadata: None,
                    ..matching.clone()
                },
                &filter
            )
            .is_err()
        );
        assert!(
            matched_repository(
                RepositoryListResponse {
                    matched_text_metadata: Some(RepositoryTextMetadata {
                        key: filter.key.clone(),
                        text: "TRUE".into()
                    }),
                    ..matching.clone()
                },
                &filter
            )
            .is_err()
        );
        assert!(
            matched_repository(
                RepositoryListResponse {
                    repository: None,
                    ..matching.clone()
                },
                &filter
            )
            .is_err()
        );
        assert!(
            matched_repository(
                RepositoryListResponse {
                    repository: Some(Repository {
                        default_branch_name: String::new(),
                        ..repository
                    }),
                    ..matching
                },
                &filter
            )
            .is_err()
        );
    }
}
