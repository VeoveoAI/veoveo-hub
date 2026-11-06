//! What happened in a Lore repository, read as the caller under their current grant: `GET
//! /lore/{profile}/repositories/{id}/branches`, `.../history/{branch}` and
//! `.../changes/{revision}`. The token is the person's, so Lore's rules decide each read, and the
//! Hub keeps nothing. See the design's "What happened in a repository".
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use futures::future::BoxFuture;
use lore_proto::lore::{
    model::v1::Branch as NativeBranch,
    revision::v1::{BranchListRequest, revision_service_client::RevisionServiceClient},
    thin_client::v1::{
        Action, DiffChange, NodeType, Revision as NativeRevision, RevisionDiffRequest,
        RevisionInfoRequest, RevisionTreeRequest, TreeNode, revision_diff_request,
        revision_diff_response, revision_info_request,
        thin_client_service_client::ThinClientServiceClient,
    },
};
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use veoveo_hub::{
    AuthenticatedSubject,
    factory::Revision,
    http::ProfileAuthState,
    person::{Email, Person},
    problem::Problem,
    token_profile::TokenIssueError,
};
use veoveo_types::{PrincipalId, TenantId};

use super::{
    Lore, RepositoryId,
    factory::{Answered, Refused, native, read as tree_read},
    files::{content_address, marked, unavailable},
    person::issue,
    sharing::{Caller, Refusal, refused},
};
use crate::runtime::AdminState;

/// How long one read of Lore may take.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a whole diff may take, since Lore walks both revisions for it.
const DIFF_TIMEOUT: Duration = Duration::from_secs(120);
/// How many revisions a page holds unless asked.
const PAGE: u32 = 30;
/// The most revisions a page holds, each being one read of Lore.
const MOST_ON_A_PAGE: u32 = 50;
/// The most changes one answer lists.
const MOST_CHANGES: usize = 2000;

#[derive(Clone)]
struct Revisions {
    state: AdminState,
}

pub(crate) fn router(state: AdminState, auth: ProfileAuthState) -> Router {
    Router::new()
        .route("/lore/{profile}/repositories/{id}/branches", get(branches))
        .route(
            "/lore/{profile}/repositories/{id}/history/{branch}",
            get(history),
        )
        .route(
            "/lore/{profile}/repositories/{id}/changes/{revision}",
            get(changes),
        )
        .with_state(Revisions { state })
        .layer(axum::middleware::from_fn_with_state(
            auth,
            veoveo_hub::http::authenticate_profile,
        ))
}

/// What a route reads Lore through.
trait Log: Sync {
    /// Every branch Lore lists.
    fn branches(&self) -> BoxFuture<'_, Result<Vec<NativeBranch>>>;
    /// The branch's tip; none while it has no revision.
    fn tip<'a>(&'a self, branch: &'a str) -> BoxFuture<'a, Result<Option<Revision>>>;
    /// Lore's record of one revision.
    fn revision<'a>(&'a self, revision: &'a Revision) -> BoxFuture<'a, Result<NativeRevision>>;
    /// What changed from one revision to another, and whether the list stopped short.
    fn diff<'a>(
        &'a self,
        from: &'a Revision,
        to: &'a Revision,
    ) -> BoxFuture<'a, Result<(Vec<DiffChange>, bool)>>;
    /// Everything in one revision.
    fn everything<'a>(&'a self, revision: &'a Revision) -> BoxFuture<'a, Result<Vec<TreeNode>>>;
}

/// Lore, with the token of one person for one repository.
struct NativeLog {
    channel: Channel,
    repository: RepositoryId,
    token: String,
}

impl Log for NativeLog {
    fn branches(&self) -> BoxFuture<'_, Result<Vec<NativeBranch>>> {
        Box::pin(async move {
            let mut client = RevisionServiceClient::new(self.channel.clone());
            let message = BranchListRequest {
                creator: None,
                include_deleted: false,
            };
            tokio::time::timeout(READ_TIMEOUT, async {
                let mut records = client
                    .branch_list(native::request(message, &self.token, &self.repository)?)
                    .await
                    .map_err(|status| native::failed("list the branches", &status))?
                    .into_inner();
                let mut branches = Vec::new();
                while let Some(record) = records
                    .message()
                    .await
                    .map_err(|status| native::failed("list the branches", &status))?
                {
                    branches.push(record.branch.context("Lore listed no branch")?);
                }
                Ok(branches)
            })
            .await
            .context("Lore did not list the branches in time")?
        })
    }

    fn tip<'a>(&'a self, branch: &'a str) -> BoxFuture<'a, Result<Option<Revision>>> {
        Box::pin(native::tip(
            self.channel.clone(),
            branch,
            &self.token,
            &self.repository,
        ))
    }

    fn revision<'a>(&'a self, revision: &'a Revision) -> BoxFuture<'a, Result<NativeRevision>> {
        Box::pin(async move {
            let mut client = ThinClientServiceClient::new(self.channel.clone());
            let message = RevisionInfoRequest {
                query: Some(revision_info_request::Query::Signature(
                    hex::decode(revision.as_str())?.into(),
                )),
            };
            tokio::time::timeout(
                READ_TIMEOUT,
                client.revision_info(native::request(message, &self.token, &self.repository)?),
            )
            .await
            .context("Lore did not answer for the revision in time")?
            .map_err(|status| native::failed("read the revision", &status))?
            .into_inner()
            .revision
            .context("Lore answered without the revision")
        })
    }

    fn diff<'a>(
        &'a self,
        from: &'a Revision,
        to: &'a Revision,
    ) -> BoxFuture<'a, Result<(Vec<DiffChange>, bool)>> {
        Box::pin(async move {
            let mut client = ThinClientServiceClient::new(self.channel.clone());
            let message = RevisionDiffRequest {
                query_from: Some(revision_diff_request::QueryFrom::SignatureFrom(
                    hex::decode(from.as_str())?.into(),
                )),
                query_to: Some(revision_diff_request::QueryTo::SignatureTo(
                    hex::decode(to.as_str())?.into(),
                )),
                autoresolve: false,
            };
            tokio::time::timeout(DIFF_TIMEOUT, async {
                let mut stream = client
                    .revision_diff(native::request(message, &self.token, &self.repository)?)
                    .await
                    .map_err(|status| native::failed("compare the revisions", &status))?
                    .into_inner();
                let mut changes = Vec::new();
                while let Some(answer) = stream
                    .message()
                    .await
                    .map_err(|status| native::failed("compare the revisions", &status))?
                {
                    let change = match answer.payload {
                        Some(revision_diff_response::Payload::Change(change)) => Some(change),
                        Some(revision_diff_response::Payload::Conflict(conflict)) => {
                            conflict.change_to
                        }
                        _ => None,
                    };
                    let Some(change) = change else { continue };
                    if changes.len() == MOST_CHANGES {
                        return Ok((changes, true));
                    }
                    changes.push(change);
                }
                Ok((changes, false))
            })
            .await
            .context("Lore did not compare the revisions in time")?
        })
    }

    fn everything<'a>(&'a self, revision: &'a Revision) -> BoxFuture<'a, Result<Vec<TreeNode>>> {
        Box::pin(async move {
            let request = |message: RevisionTreeRequest| {
                native::request(message, &self.token, &self.repository)
            };
            tree_read::everything(
                ThinClientServiceClient::new(self.channel.clone()),
                request,
                revision,
            )
            .await
        })
    }
}

/// A repository the caller may read, and Lore with their token for it.
struct Opened {
    section: Lore,
    repository: RepositoryId,
    tenant: TenantId,
    log: NativeLog,
}

async fn open(
    revisions: &Revisions,
    subject: &AuthenticatedSubject,
    id: &str,
) -> Result<Opened, Problem> {
    let repository = RepositoryId::parse(id).map_err(|_| refused(Refusal::UnknownRepository))?;
    let tenant = subject
        .principal
        .tenant
        .clone()
        .ok_or_else(|| refused(Refusal::NotGranted))?;
    let catalog = revisions.state.catalog.current();
    let section = Lore::of(&catalog).map_err(|error| {
        tracing::error!("the Lore section is unreadable: {error:#}");
        unavailable()
    })?;
    let deployment = section.sole_deployment().map_err(|reason| {
        tracing::error!("Lore revisions have no deployment: {reason}");
        unavailable()
    })?;
    section
        .readable(
            &deployment.id,
            &repository,
            Caller {
                tenant: &tenant,
                principal: &subject.principal.id,
            },
        )
        .map_err(refused)?;
    let token = issue(&catalog, &subject.principal.id, &deployment.id, &repository)
        .await
        .map_err(|error| match error {
            TokenIssueError::Denied(cause) => {
                tracing::info!("no Lore token to read revisions: {cause:#}");
                refused(Refusal::NotGranted)
            }
            TokenIssueError::Unavailable(cause) => {
                tracing::error!("failed to issue a Lore token to read revisions: {cause:#}");
                unavailable()
            }
        })?
        .access_token;
    let channel = native::connect(deployment).await.map_err(failed)?;
    let log = NativeLog {
        channel,
        repository: repository.clone(),
        token,
    };
    Ok(Opened {
        section,
        repository,
        tenant,
        log,
    })
}

/// Lore's failure as a caller's error.
fn failed(error: anyhow::Error) -> Problem {
    if error.is::<Refused>() {
        return refused(Refusal::NotGranted);
    }
    if let Some(answered) = error.downcast_ref::<Answered>() {
        match answered.code {
            tonic::Code::NotFound | tonic::Code::InvalidArgument => return not_found(),
            tonic::Code::ResourceExhausted => {
                return Problem::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "lore-changes-too-large",
                    "Too many changes to compare",
                )
                .detail("Lore will not compare these revisions, because the change is too large.");
            }
            _ => {}
        }
    }
    tracing::error!("failed to read Lore's revisions: {error:#}");
    unavailable()
}

fn not_found() -> Problem {
    Problem::new(
        StatusCode::NOT_FOUND,
        "lore-revision-not-found",
        "No such revision",
    )
    .detail("This repository holds no such branch or revision.")
}

/// A signature a URL names; what cannot be one is not there.
fn revision_named(text: &str) -> Result<Revision, Problem> {
    Revision::parse(text).map_err(|_| not_found())
}

/// Lore's bytes as a revision; none for what is empty or all zero, which names none.
fn revision_of(bytes: &[u8]) -> Option<Revision> {
    if bytes.iter().all(|byte| *byte == 0) {
        return None;
    }
    Revision::parse(hex::encode(bytes)).ok()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchesAnswer {
    repository: RepositoryId,
    branches: Vec<BranchView>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchView {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<Revision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forked_at: Option<Revision>,
    created: u64,
}

fn branch_views(records: Vec<NativeBranch>) -> Vec<BranchView> {
    let mut branches = records
        .into_iter()
        .filter(|record| !record.deleted)
        .map(|record| BranchView {
            latest: revision_of(&record.latest),
            forked_at: record
                .stack
                .first()
                .and_then(|point| revision_of(&point.revision_signature)),
            created: record.created,
            id: hex::encode(&record.id),
            name: record.name,
        })
        .collect::<Vec<_>>();
    branches.sort_by(|a, b| a.name.cmp(&b.name));
    branches
}

async fn branches(
    State(revisions): State<Revisions>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path((_, id)): Path<(String, String)>,
) -> Response {
    marked(
        async {
            let opened = open(&revisions, &subject, &id).await?;
            let records = opened.log.branches().await.map_err(failed)?;
            Ok(json(BranchesAnswer {
                repository: opened.repository,
                branches: branch_views(records),
            }))
        }
        .await,
    )
}

fn json(answer: impl Serialize) -> Response {
    axum::response::IntoResponse::into_response(Json(answer))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    from: Option<String>,
    count: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryAnswer {
    repository: RepositoryId,
    branch: String,
    revisions: Vec<RevisionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<Revision>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionView {
    revision: Revision,
    /// The branch it was made on.
    #[serde(skip_serializing_if = "Option::is_none")]
    branch_id: Option<String>,
    number: u64,
    message: String,
    /// Milliseconds since the epoch.
    at: u64,
    by: By,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<Revision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    merged: Option<Merged>,
}

/// Who committed a revision. Lore holds an identity, which is a Hub principal for whoever a Hub
/// token named.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct By {
    #[serde(skip_serializing_if = "Option::is_none")]
    principal: Option<PrincipalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<Email>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Merged {
    revision: Revision,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
}

fn committer(record: &NativeRevision) -> &str {
    if record.committed_by.is_empty() {
        &record.created_by
    } else {
        &record.committed_by
    }
}

fn by(identity: &str, people: &[Person]) -> By {
    let principal = PrincipalId::parse(identity).ok();
    let known = principal
        .as_ref()
        .and_then(|principal| people.iter().find(|person| &person.principal == principal));
    By {
        name: known
            .map(|person| person.name.to_string())
            .or_else(|| (!identity.is_empty()).then(|| identity.to_owned())),
        email: known.and_then(|person| person.email.clone()),
        principal,
    }
}

/// Up to `count` revisions of a branch, newest first, from `from` or its tip, and where the next
/// page starts. Each record names the one before it. A branch's history ends where its parent is on
/// another branch, which that branch's history says.
async fn walk(
    log: &dyn Log,
    branch: &str,
    from: Option<Revision>,
    count: usize,
) -> Result<(Vec<NativeRevision>, Option<Revision>)> {
    let mut at = match from {
        Some(from) => Some(from),
        None => log.tip(branch).await?,
    };
    let mut records = Vec::new();
    while let Some(revision) = at.take() {
        if records.len() == count {
            return Ok((records, Some(revision)));
        }
        let record = log.revision(&revision).await?;
        at = record.parent_self.as_ref().and_then(|parent| {
            let own = record.identifier.as_ref().map(|id| &id.branch_id);
            let parents = parent.identifier.as_ref().map(|id| &id.branch_id);
            (own == parents)
                .then(|| revision_of(&parent.signature))
                .flatten()
        });
        records.push(record);
    }
    Ok((records, None))
}

/// The records as a page, with each person who signed in named by `people`.
fn revision_views(
    records: Vec<NativeRevision>,
    people: &[Person],
    branches: &BTreeMap<Vec<u8>, String>,
) -> Result<Vec<RevisionView>> {
    records
        .into_iter()
        .map(|record| {
            Ok(RevisionView {
                revision: Revision::parse(hex::encode(&record.signature))
                    .context("Lore named a revision without a signature")?,
                by: by(committer(&record), people),
                branch_id: record
                    .identifier
                    .as_ref()
                    .map(|id| hex::encode(&id.branch_id)),
                at: record.timestamp,
                parent: record
                    .parent_self
                    .as_ref()
                    .and_then(|parent| revision_of(&parent.signature)),
                merged: record.parent_other.as_ref().and_then(|other| {
                    Some(Merged {
                        revision: revision_of(&other.signature)?,
                        branch: other
                            .identifier
                            .as_ref()
                            .and_then(|id| branches.get(id.branch_id.as_ref()))
                            .cloned(),
                    })
                }),
                number: record.number,
                message: record.commit_message,
            })
        })
        .collect()
}

async fn history(
    State(revisions): State<Revisions>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path((_, id, branch)): Path<(String, String, String)>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    marked(
        async {
            let from = query.from.as_deref().map(revision_named).transpose()?;
            let count = query.count.unwrap_or(PAGE).clamp(1, MOST_ON_A_PAGE) as usize;
            let opened = open(&revisions, &subject, &id).await?;
            let (records, next) = walk(&opened.log, &branch, from, count)
                .await
                .map_err(failed)?;
            // Only a merge needs the branches' names.
            let names = if records.iter().any(|record| record.parent_other.is_some()) {
                opened
                    .log
                    .branches()
                    .await
                    .map_err(failed)?
                    .into_iter()
                    .map(|record| (record.id.to_vec(), record.name))
                    .collect()
            } else {
                BTreeMap::new()
            };
            let named = records
                .iter()
                .filter_map(|record| PrincipalId::parse(committer(record)).ok())
                .collect::<BTreeSet<_>>();
            let people = revisions
                .state
                .hub_state
                .hub_store()
                .people(&opened.tenant, &named.into_iter().collect::<Vec<_>>())
                .await
                .map_err(|error| {
                    tracing::error!("failed to read who committed revisions: {error:#}");
                    unavailable()
                })?;
            Ok(json(HistoryAnswer {
                repository: opened.repository,
                branch,
                revisions: revision_views(records, &people, &names).map_err(failed)?,
                next,
            }))
        }
        .await,
    )
}

#[derive(Debug, Deserialize)]
struct ChangesQuery {
    against: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangesAnswer {
    repository: RepositoryId,
    revision: Revision,
    #[serde(skip_serializing_if = "Option::is_none")]
    against: Option<Revision>,
    changes: Vec<ChangeView>,
    truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ChangeAction {
    Added,
    Removed,
    Changed,
    Moved,
    Copied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ChangeKind {
    File,
    Link,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangeView {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_path: Option<String>,
    action: ChangeAction,
    #[serde(rename = "type")]
    kind: ChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    address_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<LinkView>,
}

/// A link's move. A person holding no grant on the linked repository gets only `reached: false`.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkView {
    reached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<RepositoryId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<Revision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<Revision>,
    tracking: bool,
}

/// Names a linked repository the person holds a grant on.
type Reach<'a> = &'a (dyn Fn(&RepositoryId) -> Option<String> + Sync);

fn action_of(action: i32) -> ChangeAction {
    match Action::try_from(action) {
        Ok(Action::Add) => ChangeAction::Added,
        Ok(Action::Delete) => ChangeAction::Removed,
        Ok(Action::Move) => ChangeAction::Moved,
        Ok(Action::Copy) => ChangeAction::Copied,
        _ => ChangeAction::Changed,
    }
}

/// `changes` as a person sees them. Folders are left out, since their files say the same, and so
/// is what lies inside a link, since that repository's own history says it. `reach` names a linked
/// repository the person holds a grant on.
fn change_views(changes: Vec<DiffChange>, reach: Reach<'_>) -> Vec<ChangeView> {
    let mut seen = BTreeSet::new();
    let mut views = Vec::new();
    for change in changes {
        let link = change.node_type == NodeType::Link as i32;
        if !link && change.node_type != NodeType::File as i32 {
            continue;
        }
        if !link && change.link_repository_index != 0 {
            continue;
        }
        if change.action == Action::Keep as i32 && !link && change.content_from == change.content_to
        {
            continue;
        }
        // Lore announces a link's move, and its walk may name the same link again.
        if !seen.insert(change.path.clone()) {
            continue;
        }
        let view = if link {
            link_change(&change, reach)
        } else {
            ChangeView {
                from_path: (!change.path_from.is_empty()).then(|| change.path_from.clone()),
                action: action_of(change.action),
                kind: ChangeKind::File,
                address_from: change.content_from.as_ref().map(content_address),
                address_to: change.content_to.as_ref().map(content_address),
                link: None,
                path: change.path,
            }
        };
        views.push(view);
    }
    views
}

fn link_change(change: &DiffChange, reach: Reach<'_>) -> ChangeView {
    let repository = change
        .content_to
        .as_ref()
        .or(change.content_from.as_ref())
        .and_then(|address| RepositoryId::parse(hex::encode(&address.context)).ok());
    let title = repository.as_ref().and_then(reach);
    let link = match title {
        Some(title) => LinkView {
            reached: true,
            repository,
            title: Some(title),
            from: change
                .content_from
                .as_ref()
                .and_then(|address| revision_of(&address.hash)),
            to: change
                .content_to
                .as_ref()
                .and_then(|address| revision_of(&address.hash)),
            tracking: change.tracking,
        },
        None => LinkView::default(),
    };
    ChangeView {
        path: change.path.clone(),
        from_path: None,
        action: action_of(change.action),
        kind: ChangeKind::Link,
        address_from: None,
        address_to: None,
        link: Some(link),
    }
}

/// Everything in a revision that has no parent, as added.
fn all_added(nodes: Vec<TreeNode>, reach: Reach<'_>) -> (Vec<ChangeView>, bool) {
    let links = nodes
        .iter()
        .filter(|node| node.node_type == NodeType::Link as i32)
        .map(|node| format!("{}/", node.path))
        .collect::<Vec<_>>();
    let changes = nodes
        .into_iter()
        .filter(|node| {
            !links
                .iter()
                .any(|link| node.path.starts_with(link.as_str()))
        })
        .map(|node| DiffChange {
            path: node.path,
            path_from: String::new(),
            action: Action::Add as i32,
            node_type: node.node_type,
            automerged: false,
            link_repository_index: 0,
            tracking: node.tracking,
            content_from: None,
            content_to: node.address,
        })
        .collect::<Vec<_>>();
    let mut views = change_views(changes, reach);
    let truncated = views.len() > MOST_CHANGES;
    views.truncate(MOST_CHANGES);
    (views, truncated)
}

async fn changes_of(
    log: &dyn Log,
    revision: &Revision,
    against: Option<Revision>,
    reach: Reach<'_>,
) -> Result<(Option<Revision>, Vec<ChangeView>, bool)> {
    let against = match against {
        Some(against) => Some(against),
        None => log
            .revision(revision)
            .await?
            .parent_self
            .as_ref()
            .and_then(|parent| revision_of(&parent.signature)),
    };
    let Some(against) = against else {
        let (views, truncated) = all_added(log.everything(revision).await?, reach);
        return Ok((None, views, truncated));
    };
    let (changes, truncated) = log.diff(&against, revision).await?;
    Ok((Some(against), change_views(changes, reach), truncated))
}

async fn changes(
    State(revisions): State<Revisions>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path((_, id, revision)): Path<(String, String, String)>,
    Query(query): Query<ChangesQuery>,
) -> Response {
    marked(
        async {
            let revision = revision_named(&revision)?;
            let against = query.against.as_deref().map(revision_named).transpose()?;
            let opened = open(&revisions, &subject, &id).await?;
            let principal = &subject.principal;
            let section = &opened.section;
            let reach = |repository: &RepositoryId| {
                section
                    .grants(principal, repository)
                    .then(|| {
                        section
                            .repositories
                            .iter()
                            .find(|registered| &registered.id == repository)
                            .map(|registered| registered.title.clone())
                    })
                    .flatten()
            };
            let (against, changes, truncated) = changes_of(&opened.log, &revision, against, &reach)
                .await
                .map_err(failed)?;
            Ok(json(ChangesAnswer {
                repository: opened.repository.clone(),
                revision,
                against,
                changes,
                truncated,
            }))
        }
        .await,
    )
}

#[cfg(test)]
mod tests;
