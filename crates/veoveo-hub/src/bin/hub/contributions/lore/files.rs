//! A file of a Lore repository as a URL. `GET
//! /lore/{profile}/repositories/{id}/files/{branch}/{path}` answers the file at that path
//! on the branch's tip, or a folder's listing, with nothing checked out, stored or buffered
//! on the way, so a push is live on the next request. [`read`] is that answer for any
//! person a route can name. It issues the person's Lore token under their current grant on
//! every request and reads with that token alone, so Lore's rules decide each read:
//! `RevisionTree` resolves the path to a content address, and Lore's HTTP content route
//! streams the bytes back. The address is the `ETag`, so a person holding an unchanged file
//! is answered 304, after the same checks.
use std::time::Duration;

use anyhow::Result;
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::State,
    http::{
        HeaderMap, HeaderName, HeaderValue, StatusCode, Uri,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
            IF_NONE_MATCH, IF_RANGE, RANGE, X_CONTENT_TYPE_OPTIONS,
        },
    },
    response::{IntoResponse, Response},
    routing::get,
};
use futures::future::BoxFuture;
use lore_proto::lore::thin_client::v1::{
    NodeType, RevisionTreeRequest, TreeNode, thin_client_service_client::ThinClientServiceClient,
};
use percent_encoding::percent_decode_str;
use serde::Serialize;
use tonic::transport::Channel;
use veoveo_hub::{
    AuthenticatedSubject, HubCatalog, factory::Revision, http::ProfileAuthState, problem::Problem,
    token_profile::TokenIssueError,
};
use veoveo_types::PrincipalId;

use super::{
    Deployment, Lore, RepositoryId,
    factory::{Answered, Refused, native, read as tree_read},
    person::issue,
    sharing::{Caller, Refusal, refused},
};
use crate::runtime::AdminState;

/// How long Lore may take to accept a connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// How long Lore may pause in an answer while the caller waits for it. The whole answer has
/// no limit, since a file takes as long to stream as it is large.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// A path in a Lore tree as a URL names it, percent-decoded one segment at a time, so that
/// `%2F` never separates. Every name is one a tree can hold; none at a branch's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilePath(Vec<String>);

impl FilePath {
    /// The path of `encoded`, a URL path without its leading `/` and with at most one
    /// trailing `/`, as a folder's URL may have. `None` when a segment cannot be a name in a
    /// tree: empty, `.` or `..`, holding a separator or NUL, or not UTF-8 once decoded.
    pub(crate) fn decode(encoded: &str) -> Option<Self> {
        let encoded = encoded.strip_suffix('/').unwrap_or(encoded);
        if encoded.is_empty() {
            return Some(Self(Vec::new()));
        }
        encoded
            .split('/')
            .map(name)
            .collect::<Option<Vec<_>>>()
            .map(Self)
    }

    /// The path as Lore names it.
    fn tree_path(&self) -> String {
        self.0.join("/")
    }

    /// The folder holding this path, `None` at the top level; none at the root.
    fn parent(&self) -> Option<String> {
        let (_, folders) = self.0.split_last()?;
        (!folders.is_empty()).then(|| folders.join("/"))
    }

    fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

fn name(encoded: &str) -> Option<String> {
    let name = percent_decode_str(encoded).decode_utf8().ok()?;
    let held = !matches!(name.as_ref(), "" | "." | "..") && !name.contains(['/', '\\', '\0']);
    held.then(|| name.into_owned())
}

/// The repository, branch and path a files route's URL path names. The branch is one
/// segment, which `%2F` may divide as a branch's name can be. axum decodes a wildcard
/// capture whole, which would turn `%2F` into a separator, so this reads the raw path.
fn named(url_path: &str) -> Option<(&str, String, FilePath)> {
    // "", "lore", profile, "repositories", id, "files", branch, path.
    let mut parts = url_path.splitn(8, '/');
    let id = parts.nth(4)?;
    let branch = parts.nth(1)?;
    let branch = percent_decode_str(branch).decode_utf8().ok()?.into_owned();
    let path = FilePath::decode(parts.next().unwrap_or(""))?;
    Some((id, branch, path))
}

#[derive(Clone)]
struct Files {
    state: AdminState,
    http: reqwest::Client,
}

pub(crate) fn router(state: AdminState, auth: ProfileAuthState) -> Result<Router> {
    let files = Files {
        state,
        http: client()?,
    };
    Ok(Router::new()
        .route("/lore/{profile}/repositories/{id}/tree/{branch}", get(tree))
        .route(
            "/lore/{profile}/repositories/{id}/files/{branch}",
            get(file),
        )
        .route(
            "/lore/{profile}/repositories/{id}/files/{branch}/",
            get(file),
        )
        .route(
            "/lore/{profile}/repositories/{id}/files/{branch}/{*path}",
            get(file),
        )
        .with_state(files)
        .layer(axum::middleware::from_fn_with_state(
            auth,
            veoveo_hub::http::authenticate_profile,
        )))
}

/// The client that reads Lore's HTTP endpoint: it follows no redirect and leaves the bytes
/// as they are.
pub(crate) fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

async fn file(
    State(files): State<Files>,
    Extension(subject): Extension<AuthenticatedSubject>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    marked(as_caller(&files, &subject, uri.path(), &headers).await)
}

async fn as_caller(
    files: &Files,
    subject: &AuthenticatedSubject,
    url_path: &str,
    headers: &HeaderMap,
) -> Result<Response, Problem> {
    let (id, branch, path) = named(url_path).ok_or_else(not_found)?;
    let repository = RepositoryId::parse(id).map_err(|_| refused(Refusal::UnknownRepository))?;
    let tenant = subject
        .principal
        .tenant
        .as_ref()
        .ok_or_else(|| refused(Refusal::NotGranted))?;
    let catalog = files.state.catalog.current();
    let section = Lore::of(&catalog).map_err(|error| {
        tracing::error!("the Lore section is unreadable: {error:#}");
        unavailable()
    })?;
    let deployment = section.sole_deployment().map_err(|reason| {
        tracing::error!("Lore files have no deployment: {reason}");
        unavailable()
    })?;
    let caller = Caller {
        tenant,
        principal: &subject.principal.id,
    };
    section
        .readable(&deployment.id, &repository, caller)
        .map_err(refused)?;
    read(
        &catalog,
        &files.http,
        &subject.principal.id,
        FileRead {
            deployment,
            repository: &repository,
            branch: &branch,
            path: &path,
        },
        headers,
    )
    .await
}

async fn tree(
    State(files): State<Files>,
    Extension(subject): Extension<AuthenticatedSubject>,
    axum::extract::Path((_, id, branch)): axum::extract::Path<(String, String, String)>,
) -> Response {
    marked(whole_tree(&files, &subject, &id, &branch).await)
}

/// Everything on a branch's tip, by one read of Lore, for the caller with a grant on the
/// repository. A link is listed and not entered, so what it answers does not depend on what
/// else the caller may read.
async fn whole_tree(
    files: &Files,
    subject: &AuthenticatedSubject,
    id: &str,
    branch: &str,
) -> Result<Response, Problem> {
    let repository = RepositoryId::parse(id).map_err(|_| refused(Refusal::UnknownRepository))?;
    let tenant = subject
        .principal
        .tenant
        .as_ref()
        .ok_or_else(|| refused(Refusal::NotGranted))?;
    let catalog = files.state.catalog.current();
    let section = Lore::of(&catalog).map_err(|error| {
        tracing::error!("the Lore section is unreadable: {error:#}");
        unavailable()
    })?;
    let deployment = section.sole_deployment().map_err(|reason| {
        tracing::error!("Lore files have no deployment: {reason}");
        unavailable()
    })?;
    let caller = Caller {
        tenant,
        principal: &subject.principal.id,
    };
    section
        .readable(&deployment.id, &repository, caller)
        .map_err(refused)?;
    let token = issue(&catalog, &subject.principal.id, &deployment.id, &repository)
        .await
        .map_err(|error| match error {
            TokenIssueError::Denied(cause) => {
                tracing::info!("no Lore token to read a tree: {cause:#}");
                refused(Refusal::NotGranted)
            }
            TokenIssueError::Unavailable(cause) => {
                tracing::error!("failed to issue a Lore token to read a tree: {cause:#}");
                unavailable()
            }
        })?
        .access_token;
    let tree = NativeTree::at_tip(deployment, &repository, branch, &token)
        .await
        .map_err(failed)?;
    let nodes = tree.everything().await.map_err(failed)?;
    let mut response = Json(WholeTree {
        repository: &repository,
        branch,
        entries: tree_entries(nodes),
    })
    .into_response();
    if let Some(revision) = tree
        .revision()
        .and_then(|revision| HeaderValue::from_str(&revision).ok())
    {
        response.headers_mut().insert(LORE_REVISION, revision);
    }
    Ok(response)
}

/// A tree's entries by their whole paths, leaving out what lies under a link.
fn tree_entries(nodes: Vec<TreeNode>) -> Vec<TreeEntry> {
    let links: std::collections::HashSet<&str> = nodes
        .iter()
        .filter(|node| NodeType::try_from(node.node_type) == Ok(NodeType::Link))
        .map(|node| node.path.as_str())
        .collect();
    let under_a_link = |path: &str| {
        let mut path = path;
        while let Some((parent, _)) = path.rsplit_once('/') {
            if links.contains(parent) {
                return true;
            }
            path = parent;
        }
        false
    };
    nodes
        .iter()
        .filter(|node| !node.path.is_empty() && !under_a_link(&node.path))
        .map(|node| {
            let entry = Entry::of(node);
            TreeEntry {
                path: node.path.clone(),
                kind: entry.kind,
                size: entry.size,
                address: entry.address,
            }
        })
        .collect()
}

/// What one read names.
#[derive(Clone, Copy)]
pub(crate) struct FileRead<'a> {
    pub deployment: &'a Deployment,
    pub repository: &'a RepositoryId,
    pub branch: &'a str,
    pub path: &'a FilePath,
}

/// Answer `file` as `person` under their current grant, to a request of `headers`. A route
/// passes the answer through [`marked`]. It uses only the validators and `Range` of
/// `headers`, and nothing of the request's URL, so no caller chooses a content type.
pub(crate) async fn read(
    catalog: &HubCatalog,
    http: &reqwest::Client,
    person: &PrincipalId,
    file: FileRead<'_>,
    headers: &HeaderMap,
) -> Result<Response, Problem> {
    let endpoint = file.deployment.http_endpoint.as_ref().ok_or_else(|| {
        tracing::error!(
            deployment = %file.deployment.id,
            "the Lore deployment names no HTTP endpoint, so its files cannot be read"
        );
        unavailable()
    })?;
    let token = issue(catalog, person, &file.deployment.id, file.repository)
        .await
        .map_err(|error| match error {
            TokenIssueError::Denied(cause) => {
                tracing::info!(%person, "no Lore token to read a file: {cause:#}");
                refused(Refusal::NotGranted)
            }
            TokenIssueError::Unavailable(cause) => {
                tracing::error!("failed to issue a Lore token to read a file: {cause:#}");
                unavailable()
            }
        })?
        .access_token;
    let tree = NativeTree::at_tip(file.deployment, file.repository, file.branch, &token)
        .await
        .map_err(failed)?;
    let content = Content {
        http,
        endpoint,
        repository: file.repository,
        token: &token,
    };
    answer(&tree, &content, file.branch, file.path, headers).await
}

/// `result` as an answer, with what every answer of a file route says.
pub(crate) fn marked(result: Result<Response, Problem>) -> Response {
    let mut response = result.unwrap_or_else(IntoResponse::into_response);
    let headers = response.headers_mut();
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, no-transform"),
    );
    response
}

/// What a read asks of a branch's tip, as the person whose token it holds.
/// The answer's header that names the branch's tip the file was read at.
const LORE_REVISION: HeaderName = HeaderName::from_static("lore-revision");

trait Tree: Sync {
    /// The revision the tree is read at, when the branch has one.
    fn revision(&self) -> Option<String>;
    /// The entries directly in `folder`, or at the top level, links listed and not entered;
    /// none when the tree has no such folder.
    fn folder<'a>(&'a self, folder: Option<&'a str>) -> BoxFuture<'a, Result<Vec<TreeNode>>>;
}

/// A branch's tip in Lore. A branch without a revision yet holds nothing.
struct NativeTree<'a> {
    client: ThinClientServiceClient<Channel>,
    repository: &'a RepositoryId,
    token: &'a str,
    tip: Option<Revision>,
}

impl<'a> NativeTree<'a> {
    async fn at_tip(
        deployment: &Deployment,
        repository: &'a RepositoryId,
        branch: &str,
        token: &'a str,
    ) -> Result<Self> {
        let channel = native::connect(deployment).await?;
        let tip = native::tip(channel.clone(), branch, token, repository).await?;
        Ok(Self {
            client: ThinClientServiceClient::new(channel),
            repository,
            token,
            tip,
        })
    }
}

impl NativeTree<'_> {
    /// Everything on the tip; nothing when the branch has no revision.
    async fn everything(&self) -> Result<Vec<TreeNode>> {
        let Some(tip) = &self.tip else {
            return Ok(Vec::new());
        };
        let request =
            |message: RevisionTreeRequest| native::request(message, self.token, self.repository);
        tree_read::everything(self.client.clone(), request, tip).await
    }
}

impl Tree for NativeTree<'_> {
    fn revision(&self) -> Option<String> {
        self.tip.as_ref().map(ToString::to_string)
    }

    fn folder<'a>(&'a self, folder: Option<&'a str>) -> BoxFuture<'a, Result<Vec<TreeNode>>> {
        Box::pin(async move {
            let Some(tip) = &self.tip else {
                return Ok(Vec::new());
            };
            let request = |message: RevisionTreeRequest| {
                native::request(message, self.token, self.repository)
            };
            tree_read::folder(self.client.clone(), request, tip, folder).await
        })
    }
}

/// What a path names on a branch's tip.
enum Resolved {
    File { address: String },
    Folder(Vec<TreeNode>),
}

/// Lore's `RevisionTree` walks only folders, so a path resolves through the listing of the
/// folder that holds it. Only an exact name matches, so `..` never does. A link lists as the
/// folder it links to, when the person may read that repository.
async fn resolve(tree: &dyn Tree, path: &FilePath) -> Result<Option<Resolved>> {
    if path.is_root() {
        return Ok(Some(Resolved::Folder(tree.folder(None).await?)));
    }
    let parent = path.parent();
    let tree_path = path.tree_path();
    let Some(node) = tree
        .folder(parent.as_deref())
        .await?
        .into_iter()
        .find(|node| node.path == tree_path)
    else {
        return Ok(None);
    };
    if node.node_type == NodeType::File as i32 {
        let address = address(&node)
            .ok_or_else(|| anyhow::anyhow!("Lore listed the file {tree_path} with no address"))?;
        return Ok(Some(Resolved::File { address }));
    }
    Ok(Some(Resolved::Folder(tree.folder(Some(&tree_path)).await?)))
}

/// A tree entry's content address, as Lore's content route names it.
fn address(node: &TreeNode) -> Option<String> {
    let address = node.address.as_ref()?;
    Some(format!(
        "{}-{}",
        hex::encode(&address.hash),
        hex::encode(&address.context)
    ))
}

#[derive(Serialize)]
struct Listing<'a> {
    repository: &'a RepositoryId,
    branch: &'a str,
    path: String,
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct WholeTree<'a> {
    repository: &'a RepositoryId,
    branch: &'a str,
    entries: Vec<TreeEntry>,
}

#[derive(Serialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: &'static str,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
}

#[derive(Serialize)]
struct Entry {
    name: String,
    #[serde(rename = "type")]
    kind: &'static str,
    /// Bytes: of a file, or of all the files under a folder.
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
}

impl Entry {
    fn of(node: &TreeNode) -> Self {
        let (kind, address) = match NodeType::try_from(node.node_type) {
            Ok(NodeType::File) => ("file", address(node)),
            Ok(NodeType::Link) => ("link", None),
            _ => ("directory", None),
        };
        Self {
            name: node
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&node.path)
                .to_owned(),
            kind,
            size: node.size,
            address,
        }
    }
}

async fn answer(
    tree: &dyn Tree,
    content: &Content<'_>,
    branch: &str,
    path: &FilePath,
    headers: &HeaderMap,
) -> Result<Response, Problem> {
    let mut response = match resolve(tree, path).await.map_err(failed)? {
        None => return Err(not_found()),
        Some(Resolved::File { address }) => content.stream(&address, headers).await?,
        Some(Resolved::Folder(nodes)) => {
            let mut entries = nodes.iter().map(Entry::of).collect::<Vec<_>>();
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            Json(Listing {
                repository: content.repository,
                branch,
                path: path.tree_path(),
                entries,
            })
            .into_response()
        }
    };
    if let Some(revision) = tree
        .revision()
        .and_then(|revision| HeaderValue::from_str(&revision).ok())
    {
        response.headers_mut().insert(LORE_REVISION, revision);
    }
    Ok(response)
}

/// Lore's HTTP content route, as the person whose token it holds.
struct Content<'a> {
    http: &'a reqwest::Client,
    endpoint: &'a url::Url,
    repository: &'a RepositoryId,
    token: &'a str,
}

impl Content<'_> {
    /// The content of `address`, streamed from Lore as it arrives. Only `Range` goes to
    /// Lore; a query would let a caller choose the content type Lore answers.
    async fn stream(&self, address: &str, headers: &HeaderMap) -> Result<Response, Problem> {
        let etag = HeaderValue::from_str(&format!("\"{address}\""))
            .map_err(|_| Problem::internal().detail("a content address is no header value"))?;
        if none_match(headers, &etag) {
            let mut response = StatusCode::NOT_MODIFIED.into_response();
            response.headers_mut().insert(ETAG, etag);
            return Ok(response);
        }
        let mut url = self.endpoint.clone();
        url.set_path(&format!(
            "/v1/repository/{}/content/{address}",
            self.repository
        ));
        let mut request = self.http.get(url).bearer_auth(self.token);
        if let Some(range) = range(headers, &etag) {
            request = request.header(RANGE, range);
        }
        let lore = request.send().await.map_err(|error| {
            tracing::error!("failed to read Lore's content route: {error}");
            unavailable()
        })?;
        let status = lore.status();
        match status {
            StatusCode::OK | StatusCode::PARTIAL_CONTENT | StatusCode::RANGE_NOT_SATISFIABLE => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(refused(Refusal::NotGranted));
            }
            StatusCode::NOT_FOUND => return Err(not_found()),
            other => {
                tracing::error!(status = %other, "Lore's content route failed");
                return Err(unavailable());
            }
        }
        let mut response = Response::new(Body::empty());
        *response.status_mut() = status;
        let answered = response.headers_mut();
        for name in [CONTENT_LENGTH, CONTENT_RANGE] {
            if let Some(value) = lore.headers().get(&name) {
                answered.insert(name, value.clone());
            }
        }
        answered.insert(ETAG, etag);
        answered.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        answered.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        if status != StatusCode::RANGE_NOT_SATISFIABLE {
            *response.body_mut() = Body::from_stream(lore.bytes_stream());
        }
        Ok(response)
    }
}

/// Whether `If-None-Match` holds the `etag`, by weak comparison as RFC 9110 has it.
fn none_match(headers: &HeaderMap, etag: &HeaderValue) -> bool {
    headers
        .get_all(IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|tag| {
            tag == "*" || tag.strip_prefix("W/").unwrap_or(tag).as_bytes() == etag.as_bytes()
        })
}

/// The `Range` to ask Lore for. `If-Range` keeps it only for the file's own `ETag`; with a
/// date or another tag the whole file is answered, as RFC 9110 has it.
fn range<'a>(headers: &'a HeaderMap, etag: &HeaderValue) -> Option<&'a HeaderValue> {
    match headers.get(IF_RANGE) {
        Some(tag) if tag != etag => None,
        _ => headers.get(RANGE),
    }
}

/// Lore's failure as a caller's error: a refusal of the person, a path or branch that is not
/// there, or Lore failing.
fn failed(error: anyhow::Error) -> Problem {
    if error.is::<Refused>() {
        return refused(Refusal::NotGranted);
    }
    if let Some(answered) = error.downcast_ref::<Answered>()
        && matches!(
            answered.code,
            tonic::Code::NotFound | tonic::Code::InvalidArgument
        )
    {
        return not_found();
    }
    tracing::error!("failed to read a Lore file: {error:#}");
    unavailable()
}

fn not_found() -> Problem {
    Problem::new(StatusCode::NOT_FOUND, "lore-file-not-found", "No such file")
        .detail("This branch's latest revision holds nothing at this path.")
}

pub(crate) fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "lore-files-unavailable",
        "Lore files are unavailable",
    )
    .detail("The Hub could not read this file from Lore. Try again later.")
}

#[cfg(test)]
mod tests;
