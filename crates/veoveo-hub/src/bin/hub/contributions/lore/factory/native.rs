//! Lore's own protocol with tokens the Hub issues for one person: notifications,
//! branches and revisions over gRPC, and exact-revision reads through Lore's command
//! line. Every call issues a new token, so every call holds the person's current grant.
use std::path::Path;

use anyhow::{Context, Result};
use futures::StreamExt;
use lore_proto::lore::{
    model::v1::Branch as NativeBranch,
    notification::{SubscribeRequest, notification_service_client::NotificationServiceClient},
    revision::v1::{
        BranchGetRequest, BranchListRequest, branch_get_request,
        revision_service_client::RevisionServiceClient,
    },
    thin_client::v1::{
        NodeType, RevisionInfoRequest, revision_info_request,
        thin_client_service_client::ThinClientServiceClient,
    },
};
use tonic::{
    Request,
    transport::{Channel, ClientTlsConfig},
};
use veoveo_hub::{
    HubCatalog, HubCatalogHandle,
    factory::{LinkedPrepared, Prepared, Revision},
    token_profile::TokenIssueError,
};
use veoveo_types::{PrincipalId, TenantId};

use super::{Answered, BoxFuture, Branch, Events, Lore as LoreProtocol, Refused, Watched};
use crate::contributions::lore::{
    BranchId, Deployment, DeploymentId, Lore, RepositoryId, person::issue,
};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// How often an idle connection to Lore is pinged, well within the connection tracking
/// on the way (Cilium forgets an idle connection after 2 h 13 min), and how long a ping
/// may go unanswered before the connection, and a subscription on it, ends.
const KEEPALIVE: std::time::Duration = std::time::Duration::from_secs(30);
const KEEPALIVE_ANSWER: std::time::Duration = std::time::Duration::from_secs(20);

/// Lore as the installation's catalog registers it.
#[derive(Clone)]
pub(crate) struct NativeLore {
    catalog: HubCatalogHandle,
}

impl NativeLore {
    pub fn new(catalog: HubCatalogHandle) -> Self {
        Self { catalog }
    }

    /// The deployment of the repository and a token for `person` to use it.
    async fn token(&self, person: &PrincipalId, watched: &Watched) -> Result<(Deployment, String)> {
        let catalog = self.catalog.current();
        let deployment = deployment(&catalog, &watched.deployment)?;
        let token = issue(&catalog, person, &watched.deployment, &watched.repository)
            .await
            .map_err(|error| match error {
                TokenIssueError::Denied(cause) => anyhow::Error::new(Refused(format!(
                    "the Hub issues {person} no Lore token for repository {}: {cause:#}",
                    watched.repository
                ))),
                TokenIssueError::Unavailable(cause) => {
                    cause.context("the Hub could not issue a Lore token")
                }
            })?;
        Ok((deployment, token.access_token))
    }
}

fn deployment(catalog: &HubCatalog, id: &DeploymentId) -> Result<Deployment> {
    Lore::of(catalog)?
        .deployments
        .into_iter()
        .find(|deployment| &deployment.id == id)
        .with_context(|| format!("the Lore deployment {id} is not registered"))
}

/// The Lore URL of a repository of the deployment.
fn repository_url(deployment: &Deployment, repository: &RepositoryId) -> url::Url {
    let mut url = deployment.endpoint.clone();
    url.set_path(&format!("/{repository}"));
    url
}

pub(crate) async fn connect(deployment: &Deployment) -> Result<Channel> {
    deployment
        .tls_endpoint()?
        .connect_timeout(TIMEOUT)
        .http2_keep_alive_interval(KEEPALIVE)
        .keep_alive_timeout(KEEPALIVE_ANSWER)
        .keep_alive_while_idle(true)
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await
        .with_context(|| format!("failed to connect to Lore at {}", deployment.endpoint))
}

/// A request in the repository's partition, with the person's token.
pub(crate) fn request<T>(message: T, token: &str, repository: &RepositoryId) -> Result<Request<T>> {
    let mut request = Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    let bytes = hex::decode(repository.as_str())?;
    for key in ["lore-partition-bin", "urc-repository-id-bin"] {
        request
            .metadata_mut()
            .insert_bin(key, tonic::metadata::MetadataValue::from_bytes(&bytes));
    }
    Ok(request)
}

/// Lore's answer to a call that failed: a refusal of the person, or another failure.
pub(crate) fn failed(operation: &str, status: &tonic::Status) -> anyhow::Error {
    let message = format!(
        "Lore refused to {operation}: {:?}: {}",
        status.code(),
        status.message()
    );
    match status.code() {
        tonic::Code::PermissionDenied | tonic::Code::Unauthenticated => {
            anyhow::Error::new(Refused(message))
        }
        code => anyhow::Error::new(Answered { code, message }),
    }
}

/// The latest revision of the live branch of `name`, with the token of a person who may
/// read the repository; none while the branch has no revision.
pub(crate) async fn tip(
    channel: Channel,
    name: &str,
    token: &str,
    repository: &RepositoryId,
) -> Result<Option<Revision>> {
    let mut client = RevisionServiceClient::new(channel);
    let message = BranchGetRequest {
        query: Some(branch_get_request::Query::Name(name.to_owned())),
    };
    let record = tokio::time::timeout(
        TIMEOUT,
        client.branch_get(request(message, token, repository)?),
    )
    .await
    .context("Lore did not answer for the branch in time")?
    .map_err(|status| failed("read the branch", &status))?
    .into_inner()
    .branch
    .context("Lore answered without the branch")?;
    if record.latest.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    Ok(Some(Revision::parse(hex::encode(&record.latest))?))
}

fn branch(record: NativeBranch) -> Result<Branch> {
    Ok(Branch {
        id: BranchId::parse(hex::encode(&record.id))?,
        name: record.name,
        latest: Revision::parse(hex::encode(&record.latest))?,
        deleted: record.deleted,
    })
}

impl NativeLore {
    /// Lore's record of the revision, as `person` reads it.
    async fn revision(
        &self,
        person: &PrincipalId,
        watched: &Watched,
        revision: &Revision,
    ) -> Result<lore_proto::lore::thin_client::v1::Revision> {
        let (deployment, token) = self.token(person, watched).await?;
        let mut client = ThinClientServiceClient::new(connect(&deployment).await?);
        let message = RevisionInfoRequest {
            query: Some(revision_info_request::Query::Signature(
                hex::decode(revision.as_str())?.into(),
            )),
        };
        tokio::time::timeout(
            TIMEOUT,
            client.revision_info(request(message, &token, &watched.repository)?),
        )
        .await
        .context("Lore did not answer for the revision in time")?
        .map_err(|status| failed("read the revision", &status))?
        .into_inner()
        .revision
        .context("Lore answered without the revision")
    }
}

impl LoreProtocol for NativeLore {
    fn locate(&self, id: &RepositoryId, tenant: &TenantId) -> Option<Watched> {
        let section = Lore::of(&self.catalog.current()).ok()?;
        section
            .repositories
            .into_iter()
            .find(|registered| &registered.id == id && &registered.tenant == tenant)
            .map(|registered| Watched {
                deployment: registered.deployment,
                repository: registered.id,
            })
    }

    fn subscribe<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
    ) -> BoxFuture<'a, Result<Events>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let mut client = NotificationServiceClient::new(connect(&deployment).await?);
            let message = SubscribeRequest {
                repository: hex::decode(watched.repository.as_str())?.into(),
            };
            // Lore registers the subscription before it answers, so every later event
            // reaches this stream.
            let events = tokio::time::timeout(
                TIMEOUT,
                client.subscribe(request(message, &token, &watched.repository)?),
            )
            .await
            .context("Lore did not answer the subscription in time")?
            .map_err(|status| failed("subscribe to the repository", &status))?
            .into_inner();
            Ok(events.boxed())
        })
    }

    fn branches<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
    ) -> BoxFuture<'a, Result<Vec<Branch>>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let mut client = RevisionServiceClient::new(connect(&deployment).await?);
            let message = BranchListRequest {
                creator: None,
                include_deleted: true,
            };
            tokio::time::timeout(TIMEOUT, async {
                let mut records = client
                    .branch_list(request(message, &token, &watched.repository)?)
                    .await
                    .map_err(|status| failed("list the branches", &status))?
                    .into_inner();
                let mut branches = Vec::new();
                while let Some(record) = records
                    .message()
                    .await
                    .map_err(|status| failed("list the branches", &status))?
                {
                    branches.push(branch(record.branch.context("Lore listed no branch")?)?);
                }
                Ok(branches)
            })
            .await
            .context("Lore did not list the branches in time")?
        })
    }

    fn branch<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        id: &'a BranchId,
    ) -> BoxFuture<'a, Result<Branch>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let mut client = RevisionServiceClient::new(connect(&deployment).await?);
            let message = BranchGetRequest {
                query: Some(branch_get_request::Query::Id(
                    hex::decode(id.as_str())?.into(),
                )),
            };
            let record = tokio::time::timeout(
                TIMEOUT,
                client.branch_get(request(message, &token, &watched.repository)?),
            )
            .await
            .context("Lore did not answer for the branch in time")?
            .map_err(|status| failed("read the branch", &status))?
            .into_inner()
            .branch
            .context("Lore answered without the branch")?;
            branch(record)
        })
    }

    fn committer<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<PrincipalId>> {
        Box::pin(async move {
            let record = self.revision(person, watched, revision).await?;
            PrincipalId::parse(record.committed_by)
                .context("Lore names no committer of the revision")
        })
    }

    fn revision_branch<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<BranchId>> {
        Box::pin(async move {
            let record = self.revision(person, watched, revision).await?;
            let identifier = record
                .identifier
                .context("Lore names no branch of the revision")?;
            BranchId::parse(hex::encode(&identifier.branch_id))
                .context("Lore names no branch of the revision")
        })
    }

    fn asks_to_serve<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let channel = connect(&deployment).await?;
            let nodes = super::read::tree(
                ThinClientServiceClient::new(channel),
                |message| request(message, &token, &watched.repository),
                revision,
            )
            .await?;
            Ok(super::read::asks_to_serve(&nodes))
        })
    }

    fn links<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<super::ParentTree>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let client = ThinClientServiceClient::new(connect(&deployment).await?);
            let mut nodes = super::read::top_level(
                client.clone(),
                |message| request(message, &token, &watched.repository),
                revision,
            )
            .await?;
            // The revision's own tree, without entering its top-level links.
            let directories = nodes
                .iter()
                .filter(|node| node.node_type == NodeType::Directory as i32)
                .map(|node| node.path.clone())
                .collect::<Vec<_>>();
            for directory in directories {
                nodes.extend(
                    super::read::under(
                        client.clone(),
                        |message| request(message, &token, &watched.repository),
                        revision,
                        &directory,
                    )
                    .await?,
                );
            }
            super::read::parent_tree(&nodes)
        })
    }

    fn read_linked<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
        directory: &'a Path,
    ) -> BoxFuture<'a, Result<LinkedPrepared>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let nodes = super::read::tree(
                ThinClientServiceClient::new(connect(&deployment).await?),
                |message| request(message, &token, &watched.repository),
                revision,
            )
            .await?;
            let files = super::read::linked_plan(&nodes);
            if !files.is_empty() {
                let url = repository_url(&deployment, &watched.repository);
                super::read::write(&url, &token, revision, &files, directory).await?;
            }
            super::read::linked(directory)
        })
    }

    fn read<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
        directory: &'a Path,
    ) -> BoxFuture<'a, Result<Option<Prepared>>> {
        Box::pin(async move {
            let (deployment, token) = self.token(person, watched).await?;
            let channel = connect(&deployment).await?;
            let nodes = super::read::tree(
                ThinClientServiceClient::new(channel),
                |message| request(message, &token, &watched.repository),
                revision,
            )
            .await?;
            let Some(files) = super::read::plan(&nodes)? else {
                return Ok(None);
            };
            let url = repository_url(&deployment, &watched.repository);
            super::read::write(&url, &token, revision, &files, directory).await?;
            Ok(Some(super::read::prepared(directory)?))
        })
    }
}
