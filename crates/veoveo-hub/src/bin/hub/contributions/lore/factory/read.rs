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

/// The tree under `openshell/` at the revision, which Lore proves by its signature; none
/// when the revision has no `openshell/` directory.
pub(super) async fn tree(
    client: ThinClientServiceClient<Channel>,
    request: impl Fn(RevisionTreeRequest) -> Result<Request<RevisionTreeRequest>>,
    revision: &Revision,
) -> Result<Vec<TreeNode>> {
    walk(client, request, revision, Some(ROOT), None).await
}

/// The entries at the top level of the revision, links included, which Lore lists without
/// entering them.
pub(super) async fn top_level(
    client: ThinClientServiceClient<Channel>,
    request: impl Fn(RevisionTreeRequest) -> Result<Request<RevisionTreeRequest>>,
    revision: &Revision,
) -> Result<Vec<TreeNode>> {
    walk(client, request, revision, None, Some(1)).await
}

/// Everything at or under the top-level `directory` of the revision. A link under it is
/// listed and, when its person may read the linked repository, entered.
pub(super) async fn under(
    client: ThinClientServiceClient<Channel>,
    request: impl Fn(RevisionTreeRequest) -> Result<Request<RevisionTreeRequest>>,
    revision: &Revision,
    directory: &str,
) -> Result<Vec<TreeNode>> {
    walk(client, request, revision, Some(directory), None).await
}

/// Whether Lore's answer to a walk under a prefix says the revision holds no directory
/// there: `NotFound` for a missing node, `InvalidArgument` for a path that is no
/// directory.
fn absent(status: &tonic::Status) -> bool {
    matches!(
        status.code(),
        tonic::Code::NotFound | tonic::Code::InvalidArgument
    )
}

async fn walk(
    mut client: ThinClientServiceClient<Channel>,
    request: impl Fn(RevisionTreeRequest) -> Result<Request<RevisionTreeRequest>>,
    revision: &Revision,
    prefix: Option<&str>,
    max_depth: Option<u32>,
) -> Result<Vec<TreeNode>> {
    let message = RevisionTreeRequest {
        query: Some(revision_tree_request::Query::Signature(
            hex::decode(revision.as_str())?.into(),
        )),
        path_prefix: prefix.map(str::to_owned),
        max_depth,
    };
    tokio::time::timeout(COMMAND_TIMEOUT, async {
        let mut stream = client
            .revision_tree(request(message)?)
            .await
            .map_err(|status| super::native::failed("read the revision's tree", &status))?
            .into_inner();
        let mut signed = false;
        let mut nodes = Vec::new();
        loop {
            let response = match stream.message().await {
                Ok(Some(response)) => response,
                Ok(None) => break,
                // Lore names the revision, then reports the prefix missing, or no directory:
                // the revision has nothing there.
                Err(status)
                    if signed && prefix.is_some() && nodes.is_empty() && absent(&status) =>
                {
                    return Ok(Vec::new());
                }
                Err(status) => {
                    return Err(super::native::failed("read the revision's tree", &status));
                }
            };
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

/// The links of a revision whose own tree `nodes` lists: its top level and everything under
/// its top-level directories. Each link names its linked repository in its address's
/// context and the revision it pins in its address's hash, and says whether it follows the
/// parent's branch. A factory sandbox's checkout leaves out exactly the links at the top
/// level whose names are lowercase DNS labels, so any other link would end its clone: a
/// revision that holds one is refused.
pub(super) fn links(nodes: &[TreeNode]) -> Result<Vec<super::LinkNode>> {
    let mut links = Vec::new();
    for node in nodes {
        if node.node_type != NodeType::Link as i32 {
            continue;
        }
        anyhow::ensure!(
            !node.path.contains('/'),
            "the revision holds the link {} below its top level, which a factory sandbox's \
             checkout cannot leave out: a factory serves only links at the top level of a \
             branch's revision, so move the link there or remove it",
            node.path
        );
        let name = veoveo_hub_sdk::hub::LinkName::parse(&node.path).map_err(|_| {
            anyhow::anyhow!(
                "the revision's link {} is not named by a lowercase DNS label, which a \
                 factory serves a link by: rename the link",
                node.path
            )
        })?;
        let address = node
            .address
            .as_ref()
            .with_context(|| format!("the link {name} has no address"))?;
        links.push(super::LinkNode {
            name,
            repository: crate::contributions::lore::RepositoryId::parse(hex::encode(
                &address.context,
            ))
            .with_context(|| format!("the link {} names no repository", node.path))?,
            pinned: Revision::parse(hex::encode(&address.hash))
                .with_context(|| format!("the link {} pins no revision", node.path))?,
            follows: node.tracking,
        });
    }
    links.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(links)
}

/// The files to read of a linked repository's revision: `openshell/policy.yaml` and
/// `openshell/providers.yaml`, when present.
pub(super) fn linked_plan(nodes: &[TreeNode]) -> Vec<File> {
    let file = |node: &TreeNode| node.node_type == NodeType::File as i32;
    nodes
        .iter()
        .filter(|node| {
            file(node)
                && (node.path == format!("{ROOT}/policy.yaml")
                    || node.path == format!("{ROOT}/providers.yaml"))
        })
        .map(|node| File {
            path: node.path.clone(),
            size: node.size,
            executable: false,
        })
        .collect()
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

/// What a linked repository's revision written under `directory` adds to a runner.
pub(super) fn linked(directory: &Path) -> Result<veoveo_hub::factory::LinkedPrepared> {
    let prepared = prepared(directory)?;
    Ok(veoveo_hub::factory::LinkedPrepared {
        policy: prepared.policy,
        providers: prepared.providers,
    })
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

    #[test]
    fn a_revision_serves_its_top_level_links_and_refuses_one_its_checkout_keeps() {
        let linked = |path: &str, tracking: bool| TreeNode {
            path: path.into(),
            node_type: NodeType::Link as i32,
            address: Some(lore_proto::lore::model::v1::Address {
                hash: vec![0xab; 32].into(),
                context: vec![0x0c; 16].into(),
            }),
            tracking,
            ..Default::default()
        };
        let directory = node("assets", NodeType::Directory, FileMode::None, 0);
        let found = links(&[
            linked("team-b", false),
            directory.clone(),
            linked("team-a", true),
        ])
        .unwrap();
        let names = found
            .iter()
            .map(|link| (link.name.as_str(), link.follows))
            .collect::<Vec<_>>();
        assert_eq!(names, [("team-a", true), ("team-b", false)]);
        assert_eq!(found[0].repository.as_str(), "0c".repeat(16));
        assert_eq!(found[0].pinned.as_str(), "ab".repeat(32));

        let deep = links(&[directory, linked("assets/lib", true)]).unwrap_err();
        assert!(
            deep.to_string()
                .contains("the revision holds the link assets/lib below its top level"),
            "{deep:#}"
        );
        let named = links(&[linked("Team_A", true)]).unwrap_err();
        assert!(
            named
                .to_string()
                .contains("the revision's link Team_A is not named by a lowercase DNS label"),
            "{named:#}"
        );
    }

    /// A thin-client server that names the revision, then answers each walk of the
    /// prefix `missing`, or of the whole tree, with `status`, and every other walk with
    /// one node.
    struct Answers {
        status: Option<tonic::Status>,
        before_header: bool,
    }

    #[tonic::async_trait]
    impl lore_proto::lore::thin_client::v1::thin_client_service_server::ThinClientService for Answers {
        type ContentDiffStream = futures::stream::Empty<
            std::result::Result<
                lore_proto::lore::thin_client::v1::ContentDiffResponse,
                tonic::Status,
            >,
        >;
        type RevisionDiffStream = futures::stream::Empty<
            std::result::Result<
                lore_proto::lore::thin_client::v1::RevisionDiffResponse,
                tonic::Status,
            >,
        >;
        type RevisionTreeStream = futures::stream::Iter<
            std::vec::IntoIter<
                std::result::Result<revision_tree_response_type::Response, tonic::Status>,
            >,
        >;

        async fn content_diff(
            &self,
            _: tonic::Request<lore_proto::lore::thin_client::v1::ContentDiffRequest>,
        ) -> std::result::Result<tonic::Response<Self::ContentDiffStream>, tonic::Status> {
            Err(tonic::Status::unimplemented("content diff"))
        }

        async fn revision_info(
            &self,
            _: tonic::Request<lore_proto::lore::thin_client::v1::RevisionInfoRequest>,
        ) -> std::result::Result<
            tonic::Response<lore_proto::lore::thin_client::v1::RevisionInfoResponse>,
            tonic::Status,
        > {
            Err(tonic::Status::unimplemented("revision info"))
        }

        async fn revision_diff(
            &self,
            _: tonic::Request<lore_proto::lore::thin_client::v1::RevisionDiffRequest>,
        ) -> std::result::Result<tonic::Response<Self::RevisionDiffStream>, tonic::Status> {
            Err(tonic::Status::unimplemented("revision diff"))
        }

        async fn revision_tree(
            &self,
            request: tonic::Request<RevisionTreeRequest>,
        ) -> std::result::Result<tonic::Response<Self::RevisionTreeStream>, tonic::Status> {
            let request = request.into_inner();
            let Some(revision_tree_request::Query::Signature(signature)) = request.query else {
                return Err(tonic::Status::invalid_argument("a signature"));
            };
            let missing = matches!(request.path_prefix.as_deref(), None | Some("missing"));
            if missing
                && self.before_header
                && let Some(status) = &self.status
            {
                return Err(status.clone());
            }
            let header = Ok(revision_tree_response_type::Response {
                payload: Some(revision_tree_response::Payload::Header(
                    lore_proto::lore::thin_client::v1::RevisionTreeHeader {
                        identifier: None,
                        signature,
                    },
                )),
            });
            let rest = match (&self.status, missing) {
                (Some(status), true) => Err(status.clone()),
                _ => Ok(revision_tree_response_type::Response {
                    payload: Some(revision_tree_response::Payload::Node(node(
                        "openshell/run",
                        NodeType::File,
                        FileMode::Executable,
                        3,
                    ))),
                }),
            };
            Ok(tonic::Response::new(futures::stream::iter(vec![
                header, rest,
            ])))
        }
    }

    mod revision_tree_response_type {
        pub(super) type Response = lore_proto::lore::thin_client::v1::RevisionTreeResponse;
    }

    async fn walk_answered(
        status: Option<tonic::Status>,
        before_header: bool,
        prefix: Option<&str>,
    ) -> Result<Vec<TreeNode>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(
                    lore_proto::lore::thin_client::v1::thin_client_service_server::ThinClientServiceServer::new(
                        Answers { status, before_header },
                    ),
                )
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener)),
        );
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{address}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        walk(
            ThinClientServiceClient::new(channel),
            |message| Ok(Request::new(message)),
            &Revision::parse("ab".repeat(32)).unwrap(),
            prefix,
            None,
        )
        .await
    }

    #[tokio::test]
    async fn a_revision_without_the_prefix_has_nothing_under_it() {
        // Lore names the revision, then finds no node at the prefix, or no directory.
        for status in [
            tonic::Status::not_found("A node in the tree could not be found"),
            tonic::Status::invalid_argument(
                "Cannot calculate tree for path that is not a directory",
            ),
        ] {
            assert_eq!(
                walk_answered(Some(status), false, Some("missing"))
                    .await
                    .unwrap(),
                []
            );
        }
        // A revision Lore cannot find at all, before it names one, still fails, and so
        // does any other refusal.
        for (status, before_header) in [
            (tonic::Status::not_found("no such revision"), true),
            (tonic::Status::permission_denied("no grant"), false),
        ] {
            let error = walk_answered(Some(status), before_header, Some("missing"))
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Lore refused to read the revision's tree"),
                "{error:#}"
            );
        }
        // A walk of the whole tree is never taken for an empty one, and a present prefix
        // lists what it holds.
        let missing = || {
            Some(tonic::Status::not_found(
                "A node in the tree could not be found",
            ))
        };
        assert!(walk_answered(missing(), false, None).await.is_err());
        assert_eq!(
            walk_answered(missing(), false, Some("openshell"))
                .await
                .unwrap()
                .len(),
            1
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
