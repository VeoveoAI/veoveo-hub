//! Exact-revision reads of `openshell/`. The revision's tree names the files and their
//! sizes; Lore's own command line, unchanged, reconstructs each file from a bare clone
//! of that revision. The Hub has no content codec of its own.
use std::{
    path::{Component, Path},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use lore_proto::lore::thin_client::v1::{
    FileMode, NodeType, RevisionTreeRequest, TreeNode, revision_tree_request,
    revision_tree_response, thin_client_service_client::ThinClientServiceClient,
};
use tokio::io::AsyncReadExt;
use tonic::{Request, transport::Channel};
use veoveo_hub::factory::{LORE, Prepared, Revision};

/// The directory a repository's factory files live in.
const ROOT: &str = "openshell";
/// How long one Lore command may run.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(90);
/// How much of a failed command's error output its failure keeps.
const DIAGNOSTICS: usize = 4096;

/// A file of `openshell/` at the revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct File {
    pub path: String,
    pub size: u64,
    pub executable: bool,
}

/// The tree under `openshell/` at the revision, which Lore proves by its signature.
pub(super) async fn tree(
    mut client: ThinClientServiceClient<Channel>,
    request: impl Fn(RevisionTreeRequest) -> Result<Request<RevisionTreeRequest>>,
    revision: &Revision,
) -> Result<Vec<TreeNode>> {
    let message = RevisionTreeRequest {
        query: Some(revision_tree_request::Query::Signature(
            hex::decode(revision.as_str())?.into(),
        )),
        path_prefix: Some(ROOT.into()),
        max_depth: None,
    };
    tokio::time::timeout(COMMAND_TIMEOUT, async {
        let mut stream = client
            .revision_tree(request(message)?)
            .await
            .map_err(|status| super::native::failed("read the revision's tree", &status))?
            .into_inner();
        let mut signed = false;
        let mut nodes = Vec::new();
        while let Some(response) = stream
            .message()
            .await
            .map_err(|status| super::native::failed("read the revision's tree", &status))?
        {
            match response.payload {
                Some(revision_tree_response::Payload::Header(header)) => {
                    ensure!(
                        hex::encode(&header.signature) == revision.as_str(),
                        "Lore answered with the tree of another revision"
                    );
                    signed = true;
                }
                Some(revision_tree_response::Payload::Node(node)) => nodes.push(node),
                None => {}
            }
        }
        ensure!(signed, "Lore answered without naming the revision");
        Ok(nodes)
    })
    .await
    .context("Lore did not answer with the revision's tree in time")?
}

/// The files to read for a revision whose `openshell/run` is an executable file: `run`,
/// `policy.yaml`, `providers.yaml` and the `image/` subtree. None when the revision has
/// no executable `openshell/run`.
pub(super) fn plan(nodes: &[TreeNode]) -> Result<Option<Vec<File>>> {
    let run = format!("{ROOT}/run");
    let file = |node: &TreeNode| node.node_type == NodeType::File as i32;
    let executable = |node: &TreeNode| node.mode & FileMode::Executable as u64 != 0;
    if !nodes
        .iter()
        .any(|node| node.path == run && file(node) && executable(node))
    {
        return Ok(None);
    }
    let policy = format!("{ROOT}/policy.yaml");
    let providers = format!("{ROOT}/providers.yaml");
    let image = format!("{ROOT}/image/");
    let mut files = Vec::new();
    for node in nodes.iter().filter(|node| file(node)) {
        if node.path != run
            && node.path != policy
            && node.path != providers
            && !node.path.starts_with(&image)
        {
            continue;
        }
        ensure!(
            Path::new(&node.path)
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
            "the revision's tree names the path {:?}, which leaves its directory",
            node.path
        );
        files.push(File {
            path: node.path.clone(),
            size: node.size,
            executable: executable(node),
        });
    }
    Ok(Some(files))
}

/// Write the files of the revision under `directory`, at their repository paths.
pub(super) async fn write(
    repository: &url::Url,
    token: &str,
    revision: &Revision,
    files: &[File],
    directory: &Path,
) -> Result<()> {
    let clone = directory.join("repository");
    let mut command = lore(directory, token);
    command
        .args(["repository", "clone"])
        .arg(repository.as_str())
        .arg(&clone)
        .args(["--revision", revision.as_str(), "--bare"]);
    run(command, "clone the revision").await?;
    for file in files {
        let output = directory.join(&file.path);
        std::fs::create_dir_all(output.parent().context("a file has a directory")?)?;
        let mut command = lore(directory, token);
        command
            .current_dir(&clone)
            .arg("--repository")
            .arg(&clone)
            .args([
                "file",
                "write",
                "--path",
                &file.path,
                "--revision",
                revision.as_str(),
                "--output",
            ])
            .arg(&output);
        run(command, &format!("write {}", file.path)).await?;
        let written = std::fs::metadata(&output)?;
        ensure!(
            written.is_file() && written.len() == file.size,
            "Lore wrote {} with {} bytes; the revision's tree names {}",
            file.path,
            written.len(),
            file.size
        );
        let mode = if file.executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&output, std::os::unix::fs::PermissionsExt::from_mode(mode))?;
    }
    Ok(())
}

/// What a revision written under `directory` applies.
pub(super) fn prepared(directory: &Path) -> Result<Prepared> {
    let root = directory.join(ROOT);
    let text = |name: &str| {
        let path = root.join(name);
        path.exists()
            .then(|| std::fs::read_to_string(&path))
            .transpose()
            .with_context(|| format!("the revision's openshell/{name} is not text"))
    };
    Ok(Prepared {
        image_context: root.join("image"),
        policy: text("policy.yaml")?,
        providers: text("providers.yaml")?,
    })
}

/// Lore's command line, from the Lore image the Hub's sandbox mounts, with its state
/// confined to `root`.
fn lore(root: &Path, token: &str) -> tokio::process::Command {
    let state = root.join("lore");
    let mut command = tokio::process::Command::new(LORE);
    command
        .env_clear()
        .env("HOME", state.join("home"))
        .env("XDG_CONFIG_HOME", state.join("config"))
        .env("XDG_DATA_HOME", state.join("data"))
        .env("LORE_GLOBAL_PATH", state.join("global"))
        .env("LORE_AUTH_PATH", state.join("auth"))
        .args(["--access-token", token]);
    pass_trust_roots(&mut command, |name| std::env::var_os(name));
    command
}

/// Keep the trust roots the Hub itself uses for Lore's TLS, which the sandbox supplies,
/// in a command whose environment was cleared.
fn pass_trust_roots(
    command: &mut tokio::process::Command,
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) {
    for name in ["SSL_CERT_FILE", "SSL_CERT_DIR"] {
        if let Some(value) = lookup(name) {
            command.env(name, value);
        }
    }
}

/// Run a Lore command to completion. Its failure keeps the end of its error output, and
/// a command that outlasts its time is killed and reaped before this returns.
async fn run(mut command: tokio::process::Command, operation: &str) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("failed to start Lore's command line")?;
    let mut stderr = child.stderr.take().context("Lore's error output")?;
    let finished = tokio::time::timeout(COMMAND_TIMEOUT, async {
        let mut diagnostics = Vec::new();
        let (read, status) = tokio::join!(stderr.read_to_end(&mut diagnostics), child.wait());
        read?;
        anyhow::Ok((status?, diagnostics))
    })
    .await;
    let Ok(finished) = finished else {
        let _ = child.start_kill();
        let _ = child.wait().await;
        anyhow::bail!("Lore did not {operation} within {COMMAND_TIMEOUT:?}");
    };
    let (status, diagnostics) = finished?;
    let start = diagnostics.len().saturating_sub(DIAGNOSTICS);
    ensure!(
        status.success(),
        "Lore failed to {operation} ({status}): {}",
        String::from_utf8_lossy(&diagnostics[start..]).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lore_keeps_the_trust_roots_of_a_cleared_environment() {
        let mut command = tokio::process::Command::new(LORE);
        command.env_clear();
        pass_trust_roots(&mut command, |name| {
            (name == "SSL_CERT_FILE").then(|| "/etc/ssl/bundle.pem".into())
        });
        let envs = command
            .as_std()
            .get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(ToOwned::to_owned)))
            .collect::<Vec<_>>();
        assert_eq!(
            envs,
            [("SSL_CERT_FILE".into(), Some("/etc/ssl/bundle.pem".into()))]
        );
    }

    fn node(path: &str, node_type: NodeType, mode: FileMode, size: u64) -> TreeNode {
        TreeNode {
            path: path.into(),
            node_type: node_type as i32,
            mode: mode as u64,
            size,
            ..Default::default()
        }
    }

    #[test]
    fn a_revision_is_served_while_its_run_is_an_executable_file() {
        let image = node("openshell/image", NodeType::Directory, FileMode::None, 30);
        let dockerfile = node(
            "openshell/image/Dockerfile",
            NodeType::File,
            FileMode::None,
            20,
        );
        let script = node(
            "openshell/image/setup.sh",
            NodeType::File,
            FileMode::Executable,
            10,
        );
        let policy = node("openshell/policy.yaml", NodeType::File, FileMode::None, 5);
        let providers = node(
            "openshell/providers.yaml",
            NodeType::File,
            FileMode::None,
            4,
        );
        let notes = node("openshell/NOTES.md", NodeType::File, FileMode::None, 5);
        let run = node("openshell/run", NodeType::File, FileMode::Executable, 3);
        let nodes = vec![
            image,
            dockerfile,
            script,
            policy,
            providers,
            notes,
            run.clone(),
        ];
        let files = plan(&nodes).unwrap().unwrap();
        let paths = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                "openshell/image/Dockerfile",
                "openshell/image/setup.sh",
                "openshell/policy.yaml",
                "openshell/providers.yaml",
                "openshell/run"
            ]
        );
        assert!(files[1].executable && files[4].executable && !files[0].executable);

        let mut plain = run.clone();
        plain.mode = FileMode::None as u64;
        assert_eq!(plan(&[plain]).unwrap(), None);
        let mut directory = run.clone();
        directory.node_type = NodeType::Directory as i32;
        assert_eq!(plan(&[directory]).unwrap(), None);
        assert_eq!(plan(&[]).unwrap(), None);
        let escape = node(
            "openshell/image/../../etc",
            NodeType::File,
            FileMode::None,
            1,
        );
        assert!(plan(&[run, escape]).is_err());
    }

    #[test]
    fn prepared_names_the_image_context_and_the_policy_when_present() {
        let directory = tempfile::tempdir().unwrap();
        let prepared = super::prepared(directory.path()).unwrap();
        assert_eq!(
            prepared.image_context,
            directory.path().join("openshell/image")
        );
        assert_eq!(prepared.policy, None);
        std::fs::create_dir_all(directory.path().join("openshell")).unwrap();
        std::fs::write(
            directory.path().join("openshell/policy.yaml"),
            "version: 1\n",
        )
        .unwrap();
        assert_eq!(
            super::prepared(directory.path()).unwrap().policy.as_deref(),
            Some("version: 1\n")
        );
    }

    #[tokio::test]
    async fn a_failed_command_reports_its_error_output() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "echo 'access denied' >&2; exit 3"]);
        let error = run(command, "clone the revision").await.unwrap_err();
        assert!(error.to_string().contains("access denied"), "{error:#}");
    }
}
