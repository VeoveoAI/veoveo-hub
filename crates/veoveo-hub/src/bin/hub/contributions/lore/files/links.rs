//! Exact native mount metadata, with independent current parent and target grants.
use super::*;
use anyhow::{Context, ensure};
use serde::Deserialize;
use std::{path::Path, process::Stdio};
use tokio::io::{AsyncRead, AsyncReadExt};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(90);
const OUTPUT_LIMIT: usize = 256 * 1024;
const DIAGNOSTIC_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Target {
    pub(super) repository: RepositoryId,
    pub(super) revision: Revision,
    pub(super) path: String,
}

#[derive(Serialize)]
struct Mount {
    repository: RepositoryId,
    revision: Revision,
    path: String,
    target: Target,
}

/// Decode the raw URL, preserving the files boundary's segment-by-segment rules.
fn named_mount(url: &str) -> Option<(RepositoryId, Revision, FilePath)> {
    let (repository, revision, path) = named(url)?;
    let repository = RepositoryId::parse(repository).ok()?;
    let revision = pinned(&revision)?;
    (!path.is_root()).then_some((repository, revision, path))
}

pub(super) async fn link(
    State(files): State<Files>,
    Extension(subject): Extension<AuthenticatedSubject>,
    uri: Uri,
) -> Response {
    let mut response = marked(as_caller(&files, &subject, uri.path()).await);
    // Resolved metadata must be reauthorized, including after a target grant is revoked.
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

async fn as_caller(
    files: &Files,
    subject: &AuthenticatedSubject,
    url: &str,
) -> Result<Response, Problem> {
    let (repository, revision, path) = named_mount(url).ok_or_else(not_found)?;
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
        tracing::error!("Lore links have no deployment: {reason}");
        unavailable()
    })?;
    let caller = Caller {
        tenant,
        principal: &subject.principal.id,
    };
    section
        .readable(&deployment.id, &repository, caller)
        .map_err(refused)?;
    let parent_token = token(&catalog, &subject.principal.id, deployment, &repository).await?;
    let branch = format!("@{revision}");
    let tree = NativeTree::at_tip(deployment, &repository, &branch, &parent_token)
        .await
        .map_err(failed)?;
    let node = mount_node(&tree, &path)
        .await
        .map_err(failed)?
        .ok_or_else(not_found)?;
    let target = resolve_target(
        &catalog,
        &section,
        caller,
        deployment,
        AuthorizedRevision {
            repository: &repository,
            revision: &revision,
            token: &parent_token,
        },
        &path,
        &node,
    )
    .await?;
    Ok(Json(Mount {
        repository,
        revision,
        path: path.tree_path(),
        target,
    })
    .into_response())
}

/// Shared by logical file traversal and the explicit mount metadata route.
/// Current target authority is established before any native target metadata read.
pub(super) async fn resolve_target(
    catalog: &HubCatalog,
    section: &Lore,
    caller: Caller<'_>,
    deployment: &Deployment,
    parent: AuthorizedRevision<'_>,
    path: &FilePath,
    node: &TreeNode,
) -> Result<Target, Problem> {
    let (repository, revision) = authorized_target(section, deployment, caller, node)?;
    let target_token = token(catalog, caller.principal, deployment, &repository).await?;
    let source_path = native_source_path(
        deployment,
        parent,
        AuthorizedRevision {
            repository: &repository,
            revision: &revision,
            token: &target_token,
        },
        path,
        node,
    )
    .await
    .map_err(failed)?;
    Ok(Target {
        repository,
        revision,
        path: source_path,
    })
}

async fn token(
    catalog: &HubCatalog,
    person: &PrincipalId,
    deployment: &Deployment,
    repository: &RepositoryId,
) -> Result<String, Problem> {
    issue(catalog, person, &deployment.id, repository)
        .await
        .map(|issued| issued.access_token)
        .map_err(|error| match error {
            TokenIssueError::Denied(cause) => {
                tracing::info!("no Lore token to resolve a link: {cause:#}");
                refused(Refusal::NotGranted)
            }
            TokenIssueError::Unavailable(cause) => {
                tracing::error!("failed to issue a Lore token to resolve a link: {cause:#}");
                unavailable()
            }
        })
}

async fn mount_node(tree: &dyn Tree, path: &FilePath) -> Result<Option<TreeNode>> {
    let mut folder = None;
    for (index, segment) in path.0.iter().enumerate() {
        let name = folder
            .as_ref()
            .map_or_else(|| segment.clone(), |parent| format!("{parent}/{segment}"));
        let Some(node) = tree
            .folder(folder.as_deref())
            .await?
            .into_iter()
            .find(|node| node.path == name)
        else {
            return Ok(None);
        };
        if index + 1 == path.0.len() {
            return Ok((node.node_type == NodeType::Link as i32).then_some(node));
        }
        // This route resolves one mount owned by the named parent. Traversing
        // another link would require that intermediate repository's own lookup.
        if node.node_type != NodeType::Directory as i32 {
            return Ok(None);
        }
        folder = Some(name);
    }
    Ok(None)
}

fn node_target(node: &TreeNode) -> Result<(RepositoryId, Revision)> {
    let address = node
        .address
        .as_ref()
        .context("Lore listed a link without an address")?;
    Ok((
        RepositoryId::parse(hex::encode(&address.context))?,
        Revision::parse(hex::encode(&address.hash))?,
    ))
}

fn authorized_target(
    section: &Lore,
    deployment: &Deployment,
    caller: Caller<'_>,
    node: &TreeNode,
) -> Result<(RepositoryId, Revision), Problem> {
    let (repository, revision) = node_target(node).map_err(failed)?;
    section
        .readable(&deployment.id, &repository, caller)
        .map_err(refused)?;
    Ok((repository, revision))
}

/// Only native metadata enters these clones: the exclusion view materializes zero files.
pub(super) struct AuthorizedRevision<'a> {
    pub(super) repository: &'a RepositoryId,
    pub(super) revision: &'a Revision,
    pub(super) token: &'a str,
}

async fn native_source_path(
    deployment: &Deployment,
    parent: AuthorizedRevision<'_>,
    target: AuthorizedRevision<'_>,
    path: &FilePath,
    node: &TreeNode,
) -> Result<String> {
    let root = tempfile::tempdir().context("creating private Lore metadata scratch")?;
    let view = root.path().join("exclude.view");
    tokio::fs::write(&view, "**\n").await?;
    let parent_url = native::repository_url(deployment, parent.repository);
    let target_url = native::repository_url(deployment, target.repository);
    let parent_command = clone_command(
        root.path(),
        parent.token,
        &parent_url,
        parent.revision,
        "parent",
    );
    let target_command = clone_command(
        root.path(),
        target.token,
        &target_url,
        target.revision,
        "target",
    );
    let mut info = tree_read::lore(&root.path().join("offline"), "");
    // No network or live token participates in the native source-path lookup.
    info.args(["--offline", "--json", "--repository"])
        .arg(root.path().join("parent"))
        .args(["link", "info"])
        .arg(path.tree_path())
        .current_dir(root.path().join("parent"));
    command_output(
        parent_command,
        "clone parent metadata",
        &[parent.token, target.token],
    )
    .await?;
    command_output(
        target_command,
        "clone target metadata",
        &[parent.token, target.token],
    )
    .await?;
    let output = command_output(
        info,
        "read native link metadata",
        &[parent.token, target.token],
    )
    .await?;
    decode_source_path(&output, path, node)
}

fn clone_command(
    root: &Path,
    token: &str,
    repository: &url::Url,
    revision: &Revision,
    name: &str,
) -> tokio::process::Command {
    let mut command = tree_read::lore(root, token);
    command
        .args([
            "--cache",
            "clone",
            "--use-shared-store",
            "--shared-store-path",
        ])
        .arg(root.join("cache"))
        .arg("--view")
        .arg(root.join("exclude.view"))
        .args(["--revision", revision.as_str()])
        .arg(repository.as_str())
        .arg(root.join(name));
    command
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    tag_name: String,
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct LinkInfo {
    entry: LinkEntry,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkEntry {
    link: RepositoryId,
    link_path: String,
    source_path: String,
    source_node: u64,
    revision: String,
}

fn decode_source_path(output: &[u8], path: &FilePath, node: &TreeNode) -> Result<String> {
    let (repository, revision) = node_target(node)?;
    let mut info = None;
    for line in output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let event: Event =
            serde_json::from_slice(line).context("Lore emitted invalid JSON metadata")?;
        if event.tag_name == "linkInfo" {
            ensure!(info.is_none(), "Lore emitted more than one link info event");
            info = Some(serde_json::from_value::<LinkInfo>(event.data)?.entry);
        }
    }
    let info = info.context("Lore emitted no link info event")?;
    let info_revision = Revision::parse(info.revision)
        .context("native link metadata names no canonical revision")?;
    ensure!(
        info.link == repository && info_revision == revision && info.link_path == path.tree_path(),
        "native link metadata disagrees with the exact parent tree"
    );
    if info.source_path == "/" {
        ensure!(
            info.source_node == 0,
            "native root link metadata names a nonroot source node"
        );
        return Ok(info.source_path);
    }
    // This is native text, not a URL. In particular, a literal '%' must not be decoded.
    ensure!(
        info.source_path
            .split('/')
            .all(|part| !matches!(part, "" | "." | "..") && !part.contains(['\\', '\0']))
            && info.source_node != 0,
        "native subtree link metadata names a noncanonical source path or root node"
    );
    Ok(info.source_path)
}

async fn bounded_output(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .await?;
    ensure!(
        output.len() <= limit,
        "Lore command output exceeded its bound"
    );
    Ok(output)
}

async fn command_output(
    mut command: tokio::process::Command,
    operation: &str,
    tokens: &[&str],
) -> Result<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("failed to start Lore's command line")?;
    let stdout = child.stdout.take().context("Lore's standard output")?;
    let stderr = child.stderr.take().context("Lore's diagnostic output")?;
    let result = tokio::time::timeout(COMMAND_TIMEOUT, async {
        tokio::try_join!(
            bounded_output(stdout, OUTPUT_LIMIT),
            bounded_output(stderr, DIAGNOSTIC_LIMIT),
            async { Ok::<_, anyhow::Error>(child.wait().await?) }
        )
    })
    .await;
    let (output, diagnostics, status) = match result {
        Ok(Ok(result)) => result,
        error => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return match error {
                Ok(Err(error)) => Err(error.context(format!("Lore could not {operation}"))),
                Err(_) => anyhow::bail!("Lore did not {operation} within {COMMAND_TIMEOUT:?}"),
                Ok(Ok(_)) => unreachable!(),
            };
        }
    };
    let mut diagnostics = String::from_utf8_lossy(&diagnostics).into_owned();
    for token in tokens.iter().filter(|token| !token.is_empty()) {
        diagnostics = diagnostics.replace(token, "[redacted]");
    }
    ensure!(
        status.success(),
        "Lore failed to {operation} ({status}): {}",
        diagnostics.trim()
    );
    Ok(output)
}

#[cfg(test)]
mod tests;
