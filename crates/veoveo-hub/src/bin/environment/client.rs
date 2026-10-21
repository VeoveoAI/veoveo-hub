//! Explicit local apply/watch using the Computer's existing native opaque Hub ref.
//! Source transfer uses the configured platform profile; application environment
//! selection is a separate MCP connection header and is never added here.
use super::definition::{
    ApplyEnvironment, CompleteLocalSnapshot, EnvironmentDefinition, FollowDefinition,
    LoreDefinitionSource, MAX_DEFINITION_BYTES, SourceSelector,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use secrecy::{ExposeSecret as _, SecretString};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    str::FromStr,
    time::Instant,
};
use veoveo_hub::activation::{
    AcceptedApply, ActivationStatus, ApplyIdempotencyKey, BuildState, RelativePath,
    SnapshotIdentity,
};
use veoveo_hub_sdk::{
    HubProfileId, ServerSlug,
    client::{HubClient, StaticBearer},
};
use veoveo_mcp_artifact::{
    client::{ArtifactClient, UploadBody},
    contract::{ArtifactId, ArtifactName, MediaType, ObjectSize},
};

#[derive(Args)]
pub struct ApplyArgs {
    #[arg(long)]
    pub definition: PathBuf,
    /// Explicit selected source directory. Repeat for independently owned components.
    #[arg(long = "local")]
    pub local: Vec<LocalDirectory>,
    /// Paths relative to each selected source root; no implicit home synchronization.
    #[arg(long,value_parser=parse_relative)]
    pub exclude: Vec<RelativePath>,
    #[arg(long, env = "COMPUTER_HUB_URL")]
    pub hub_url: url::Url,
    #[arg(long,env="COMPUTER_HUB_REFERENCE",hide_env_values=true,value_parser=parse_reference)]
    pub hub_reference: SecretString,
}
/// Explicitly restore native configuration-file Follow. No second manifest.
#[derive(Args)]
pub struct FollowArgs {
    #[arg(long)]
    pub environment: veoveo_hub_sdk::EnvironmentId,
    #[arg(long)]
    pub deployment: crate::native_source_ids::DeploymentId,
    #[arg(long)]
    pub repository: crate::native_source_ids::RepositoryId,
    #[arg(long)]
    pub branch: crate::native_source_ids::BranchId,
    #[arg(long,value_parser=parse_relative)]
    pub path: RelativePath,
    #[arg(long, env = "COMPUTER_HUB_URL")]
    pub hub_url: url::Url,
    #[arg(long,env="COMPUTER_HUB_REFERENCE",hide_env_values=true,value_parser=parse_reference)]
    pub hub_reference: SecretString,
}
#[derive(Clone)]
pub struct LocalDirectory {
    component: ServerSlug,
    path: PathBuf,
}
impl FromStr for LocalDirectory {
    type Err = String;
    fn from_str(value: &str) -> std::result::Result<Self, String> {
        let (component, path) = value
            .split_once('=')
            .ok_or("expected COMPONENT=DIRECTORY")?;
        if path.is_empty() {
            return Err("local source directory is empty".into());
        }
        Ok(Self {
            component: ServerSlug::parse(component).map_err(|e| e.to_string())?,
            path: path.into(),
        })
    }
}
fn parse_reference(value: &str) -> std::result::Result<SecretString, String> {
    if value.is_empty() {
        Err("Hub reference is empty".into())
    } else {
        Ok(SecretString::from(value))
    }
}
fn parse_relative(value: &str) -> std::result::Result<RelativePath, String> {
    RelativePath::parse(value).map_err(|e| e.to_string())
}
pub(super) struct Archive {
    pub(super) file: tempfile::NamedTempFile,
    pub(super) identity: SnapshotIdentity,
}
const COMPLETION_RETRY: std::time::Duration = std::time::Duration::from_secs(5);
struct UploadedSource {
    artifact: ArtifactId,
    completed_at: Option<Instant>,
}
struct Connection {
    http: reqwest::Client,
    hub: url::Url,
    control: url::Url,
    profile: HubProfileId,
    reference: SecretString,
}
impl Connection {
    fn new(args: &ApplyArgs) -> Result<Self> {
        Self::from_values(args.hub_url.clone(), args.hub_reference.clone())
    }
    fn from_values(hub_url: url::Url, hub_reference: SecretString) -> Result<Self> {
        ensure!(
            hub_url.username().is_empty()
                && hub_url.password().is_none()
                && hub_url.query().is_none()
                && hub_url.fragment().is_none(),
            "Hub URL must be the configured MCP resource"
        );
        let path: Vec<_> = hub_url
            .path_segments()
            .context("Hub URL has no path")?
            .filter(|s| !s.is_empty())
            .collect();
        ensure!(
            path.len() >= 2 && path[path.len() - 2] == "mcp",
            "Hub URL must end in /mcp/PROFILE"
        );
        let profile = HubProfileId::parse(path[path.len() - 1])?;
        let mut control = hub_url.clone();
        let prefix = path[..path.len() - 2].join("/");
        control.set_path(&if prefix.is_empty() {
            "/".into()
        } else {
            format!("/{prefix}/")
        });
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            hub: hub_url,
            control,
            profile,
            reference: hub_reference,
        })
    }
    fn url(&self, suffix: &str) -> Result<url::Url> {
        Ok(self
            .control
            .join(&format!("environments/{}/{suffix}", self.profile))?)
    }
    async fn complete_local(
        &self,
        build: veoveo_types::BuildId,
        artifact: ArtifactId,
    ) -> Result<()> {
        self.http
            .post(self.url(&format!("applies/{build}/complete-local"))?)
            .bearer_auth(self.reference.expose_secret())
            .json(&CompleteLocalSnapshot { artifact })
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
    async fn status(&self, build: veoveo_types::BuildId) -> Result<ActivationStatus> {
        Ok(self
            .http
            .get(self.url(&format!("applies/{build}"))?)
            .bearer_auth(self.reference.expose_secret())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
pub(super) fn archive(source: &Path, excluded: &[RelativePath]) -> Result<Archive> {
    ensure!(source.is_dir(), "selected local source is not a directory");
    let mut file = tempfile::NamedTempFile::new()?;
    let mut builder = tar::Builder::new(file.as_file_mut());
    builder.mode(tar::HeaderMode::Deterministic);
    builder.follow_symlinks(false);
    fn append(
        builder: &mut tar::Builder<&mut fs::File>,
        root: &Path,
        directory: &Path,
        excluded: &[RelativePath],
    ) -> Result<()> {
        let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root)?;
            if excluded
                .iter()
                .any(|exclude| relative.starts_with(exclude.as_str()))
            {
                continue;
            }
            builder.append_path_with_name(&path, relative)?;
            if entry.file_type()?.is_dir() {
                append(builder, root, &path, excluded)?;
            }
        }
        Ok(())
    }
    append(&mut builder, source, source, excluded)?;
    builder.finish()?;
    drop(builder);
    let size = file.as_file().metadata()?.len();
    ensure!(
        size <= 2 * 1024 * 1024 * 1024,
        "selected source archive exceeds 2 GiB; explicitly exclude caches/build output"
    );
    Ok(Archive {
        identity: SnapshotIdentity {
            digest: super::build::digest_file(file.path())?,
            size,
        },
        file,
    })
}
fn prepare(args: &ApplyArgs) -> Result<(EnvironmentDefinition, BTreeMap<ServerSlug, Archive>)> {
    // The CLI supplies the exact typed LocalSnapshot selector before deserializing
    // the ONE environment contract. No second file/schema is persisted.
    let mut bytes = Vec::new();
    fs::File::open(&args.definition)?
        .take(MAX_DEFINITION_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_DEFINITION_BYTES,
        "environment definition exceeds byte bound"
    );
    let mut document: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut archives = BTreeMap::new();
    for source in &args.local {
        ensure!(
            !archives.contains_key(&source.component),
            "duplicate local component selection"
        );
        let snapshot = archive(&source.path, &args.exclude)?;
        let component = document
            .get_mut("components")
            .and_then(|v| v.get_mut(source.component.as_str()))
            .and_then(serde_json::Value::as_object_mut)
            .context("local component is absent from definition")?;
        component.insert(
            "source".into(),
            serde_json::to_value(SourceSelector::LocalSnapshot {
                snapshot: snapshot.identity.clone(),
            })?,
        );
        archives.insert(source.component.clone(), snapshot);
    }
    Ok((serde_json::from_value(document)?, archives))
}
pub async fn follow(args: FollowArgs) -> Result<()> {
    ensure!(
        args.path.as_str() != ".",
        "definition path must name a file"
    );
    let connection = Connection::from_values(args.hub_url, args.hub_reference)?;
    let url = connection.url(&format!("{}/source", args.environment))?;
    // Ordinary owner endpoint; it does not need installation AdminWrite. Unknown
    // environments must first be created through the normal owner definition API.
    let current: Option<serde_json::Value> = connection
        .http
        .get(url.clone())
        .bearer_auth(connection.reference.expose_secret())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let replaces = current
        .map(|v| {
            Ok::<_, anyhow::Error>(
                serde_json::from_value::<veoveo_hub::activation::Generation>(
                    v.get("generation")
                        .cloned()
                        .context("definition source omitted generation")?,
                )?,
            )
        })
        .transpose()?;
    let response = connection
        .http
        .post(url)
        .bearer_auth(connection.reference.expose_secret())
        .json(&FollowDefinition {
            replaces,
            source: LoreDefinitionSource {
                deployment: args.deployment,
                repository: args.repository,
                branch: args.branch,
                path: args.path,
            },
        })
        .send()
        .await?
        .error_for_status()?;
    ensure!(response.status().is_success(), "definition follow refused");
    println!(
        "environment={} native definition follow accepted",
        args.environment
    );
    Ok(())
}
pub async fn run(args: ApplyArgs, watch: bool) -> Result<()> {
    let connection = Connection::new(&args)?;
    let mut last = None;
    let mut reservation: Option<ApplyEnvironment> = None;
    let mut completed_uploads: BTreeMap<veoveo_types::BuildId, UploadedSource> = BTreeMap::new();
    let mut pending = std::collections::BTreeSet::new();
    loop {
        let (definition, archives) = prepare(&args)?;
        let identity = Sha256::digest(serde_json::to_vec(&definition)?).to_vec();
        if last.as_ref() != Some(&identity) {
            if reservation
                .as_ref()
                .is_none_or(|request| request.definition != definition)
            {
                reservation = Some(ApplyEnvironment {
                    idempotency: ApplyIdempotencyKey::parse(format!(
                        "apply-{}",
                        uuid::Uuid::now_v7()
                    ))?,
                    definition,
                });
            }
            let request = reservation.as_ref().expect("prepared current intent");
            let submission = async {
                // This reservation precedes upload. Reconnection can replay the SAME
                // request key; it never obtains a newer generation for an older upload.
                let accepted: Vec<AcceptedApply> = connection
                    .http
                    .put(connection.url(&request.definition.id.to_string())?)
                    .bearer_auth(connection.reference.expose_secret())
                    .json(request)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                let artifact = ArtifactClient::new(
                    HubClient::new(
                        connection.hub.clone(),
                        connection.http.clone(),
                        StaticBearer::new(connection.reference.expose_secret()),
                    ),
                    connection.http.clone(),
                );
                for accepted in accepted {
                    pending.insert(accepted.build);
                    let Some(source) = archives.get(&accepted.key.component) else {
                        continue;
                    };
                    let status = connection.status(accepted.build).await?;
                    ensure!(
                        status.current,
                        veoveo_hub::activation::ActivationError::Superseded
                    );
                    if !matches!(
                        status.state,
                        BuildState::Preparing
                            | BuildState::Failed {
                                failure: veoveo_hub::activation::BuildFailure::Source { .. }
                            }
                    ) {
                        continue;
                    }
                    let upload_start = Instant::now();
                    let artifact_id = if let Some(artifact) = completed_uploads.get(&accepted.build)
                    {
                        artifact.artifact
                    } else {
                        let uploaded = artifact
                            .upload(
                                ArtifactName::parse(format!("source-{}.tar", accepted.build))?,
                                MediaType::parse("application/x-tar")?,
                                UploadBody::reader(
                                    tokio::fs::File::open(source.file.path()).await?,
                                ),
                            )
                            .size(ObjectSize::new(source.identity.size)?)
                            .await
                            .map_err(|_| {
                                anyhow::anyhow!("private source upload refused or unavailable")
                            })?;
                        completed_uploads.insert(
                            accepted.build,
                            UploadedSource {
                                artifact: uploaded.artifact_id,
                                completed_at: None,
                            },
                        );
                        uploaded.artifact_id
                    };
                    println!(
                        "build={} deployment={} upload_ms={} snapshot={} bytes={}",
                        accepted.build,
                        accepted.deployment,
                        upload_start.elapsed().as_millis(),
                        source.identity.digest,
                        source.identity.size
                    );
                    if watch && prepare(&args)?.0 != request.definition {
                        anyhow::bail!(veoveo_hub::activation::ActivationError::Superseded);
                    }
                    connection
                        .complete_local(accepted.build, artifact_id)
                        .await?;
                    completed_uploads
                        .get_mut(&accepted.build)
                        .expect("retained uploaded source")
                        .completed_at = Some(Instant::now());
                    pending.insert(accepted.build);
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            match submission {
                Ok(()) => {
                    last = Some(identity);
                    reservation = None;
                }
                Err(error)
                    if watch
                        && error
                            .downcast_ref::<veoveo_hub::activation::ActivationError>()
                            .is_some_and(|e| {
                                matches!(e, veoveo_hub::activation::ActivationError::Superseded)
                            }) =>
                {
                    // A captured server apply or changed local intent cannot be
                    // replayed as a new reservation merely because transport returns.
                    last = Some(identity);
                    reservation = None;
                }
                Err(error) if watch && retryable_submission(&error) => {
                    eprintln!(
                        "source transport unavailable; retaining current reservation for reconnect"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    // Next iteration rereads actual local definition/content. If it
                    // changed, a new intent replaces this request rather than replaying it.
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        loop {
            let mut finished = Vec::new();
            for build in &pending {
                let outcome =
                    match poll(&connection, *build, completed_uploads.get_mut(build)).await {
                        Ok(outcome) => outcome,
                        Err(error) if watch && retryable_submission(&error) => continue,
                        Err(error) => return Err(error),
                    };
                if !matches!(outcome, PollOutcome::Pending) {
                    finished.push(*build);
                }
                ensure!(
                    watch || !matches!(outcome, PollOutcome::Failed | PollOutcome::Superseded),
                    "apply did not activate; previous serving revision retained"
                );
            }
            for build in finished {
                pending.remove(&build);
                completed_uploads.remove(&build);
            }
            if watch || pending.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        if !watch {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

fn retryable_submission(error: &anyhow::Error) -> bool {
    error.downcast_ref::<reqwest::Error>().is_none_or(|http| {
        http.status().is_none_or(|status| {
            status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        })
    })
}

#[derive(Clone, Copy)]
enum PollOutcome {
    Pending,
    Serving,
    Failed,
    Superseded,
}
async fn poll(
    connection: &Connection,
    build: veoveo_types::BuildId,
    upload: Option<&mut UploadedSource>,
) -> Result<PollOutcome> {
    let status = connection.status(build).await?;
    if !status.current {
        println!("build={build} superseded; old intent not resubmitted");
        return Ok(PollOutcome::Superseded);
    }
    // A 202 acknowledges ephemeral work, not durable source-ready bytes. If Hub
    // restarted before materialization committed, resend ONLY this current build's
    // completion with its retained Artifact and fresh native bearer. Server jobs
    // coalesce concurrent completion, and generation fencing rejects supersession.
    if matches!(status.state, BuildState::Preparing)
        && let Some(upload) = upload
        && upload
            .completed_at
            .is_none_or(|at| at.elapsed() >= COMPLETION_RETRY)
    {
        connection.complete_local(build, upload.artifact).await?;
        upload.completed_at = Some(Instant::now());
    }
    match status.state {
        BuildState::Serving { .. } => {
            println!(
                "build={build} deployment={} serving",
                status.accepted.deployment
            );
            Ok(PollOutcome::Serving)
        }
        BuildState::Failed { failure } => {
            println!("build={build} failed: {failure:?}");
            let response = connection
                .http
                .get(connection.url(&format!("applies/{build}/diagnostics"))?)
                .bearer_auth(connection.reference.expose_secret())
                .send()
                .await?;
            if response.status().is_success()
                && let super::protocol::BuildResult::Failed { failure, .. } =
                    response.json().await?
            {
                eprintln!(
                    "{:?}[{}] exit={:?} truncated={}\n{}",
                    failure.phase,
                    failure.index,
                    failure.exit_code,
                    failure.truncated,
                    failure.output
                );
            }
            Ok(PollOutcome::Failed)
        }
        _ => Ok(PollOutcome::Pending),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_mcp_mount_derives_control_origin_without_changing_profile() {
        let args = ApplyArgs {
            definition: "unused.json".into(),
            local: vec![],
            exclude: vec![],
            hub_url: url::Url::parse("https://hub.example/platform/mcp/fixture").unwrap(),
            hub_reference: SecretString::from("opaque-native-reference"),
        };
        let connection = Connection::new(&args).unwrap();
        assert_eq!(
            connection.url("applies/example").unwrap().as_str(),
            "https://hub.example/platform/environments/fixture/applies/example"
        );
        let mut root = args;
        root.hub_url = url::Url::parse("https://hub.example/mcp/fixture").unwrap();
        assert_eq!(
            Connection::new(&root)
                .unwrap()
                .url("applies/example")
                .unwrap()
                .as_str(),
            "https://hub.example/environments/fixture/applies/example"
        );
    }
    #[tokio::test]
    async fn watch_reconnect_replays_same_reservation_but_changed_definition_gets_new_intent() {
        use axum::{Json, Router, extract::State, http::StatusCode, routing::put};
        use std::sync::Arc;
        let seen = Arc::new(tokio::sync::Mutex::new(Vec::<ApplyEnvironment>::new()));
        let app = Router::new()
            .route(
                "/environments/fixture/{environment}",
                put(
                    async |State(seen): State<Arc<tokio::sync::Mutex<Vec<ApplyEnvironment>>>>,
                           Json(request): Json<ApplyEnvironment>| {
                        let mut seen = seen.lock().await;
                        seen.push(request);
                        if seen.len() == 1 {
                            (
                                StatusCode::SERVICE_UNAVAILABLE,
                                Json(Vec::<AcceptedApply>::new()),
                            )
                        } else {
                            (StatusCode::ACCEPTED, Json(Vec::<AcceptedApply>::new()))
                        }
                    },
                ),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("environment.json");
        let mut definition = serde_json::json!({"id":"019a5b30-0000-7000-8000-000000000001","displayName":"First","base":null,"components":{}});
        fs::write(&path, serde_json::to_vec(&definition).unwrap()).unwrap();
        let watcher = tokio::spawn(run(
            ApplyArgs {
                definition: path.clone(),
                local: vec![],
                exclude: vec![],
                hub_url: url::Url::parse(&format!("http://{address}/mcp/fixture")).unwrap(),
                hub_reference: SecretString::from("fixture-reference"),
            },
            true,
        ));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if seen.lock().await.len() >= 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        {
            let seen = seen.lock().await;
            assert_eq!(seen[0], seen[1]);
        }
        definition["displayName"] = "Changed".into();
        fs::write(&path, serde_json::to_vec(&definition).unwrap()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if seen.lock().await.len() >= 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let seen = seen.lock().await;
        assert_ne!(seen[1].idempotency, seen[2].idempotency);
        assert_eq!(seen[2].definition.display_name, "Changed");
        watcher.abort();
        server.abort();
    }
    #[tokio::test]
    async fn accepted_completion_is_resent_for_same_preparing_build_after_hub_restart() {
        use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
        use std::sync::Arc;
        let build = veoveo_types::BuildId::new();
        let environment = veoveo_hub_sdk::EnvironmentId::new();
        let deployment = veoveo_hub_sdk::DeploymentId::new();
        let accepted: AcceptedApply = serde_json::from_value(serde_json::json!({
            "runtime":{"workspace":"fixture","template":"fixture-v1","policyFile":"/fixture.yaml","providers":[],"storage":{"kind":"shared_logical_schema","server":"observatory"}},
            "key":{"owner":{"tenant":"fixture","principal":"owner"},"environment":environment,"component":"observatory"},
            "generation":1,"build":build,"deployment":deployment,"idempotency":"same-reservation",
            "inputs":{"runtimeTemplate":"fixture-v1","definitionDigest":veoveo_types::Sha256Digest::from_bytes([1;32]),"expectedSource":{"digest":veoveo_types::Sha256Digest::from_bytes([2;32]),"size":123},"recipe":{"worker":{"kind":"compiled","output":{"root":"source","path":"service"},"args":[],"cwd":"."}}}
        })).unwrap();
        let status = Arc::new(tokio::sync::Mutex::new(ActivationStatus {
            accepted,
            state: BuildState::Preparing,
            current: true,
            serving: None,
        }));
        let completions = Arc::new(tokio::sync::Mutex::new(Vec::<ArtifactId>::new()));
        type CompletionState = (
            Arc<tokio::sync::Mutex<ActivationStatus>>,
            Arc<tokio::sync::Mutex<Vec<ArtifactId>>>,
        );
        let app = Router::new()
            .route(
                &format!("/environments/fixture/applies/{build}"),
                get(async |State((status, _)): State<CompletionState>| {
                    Json(status.lock().await.clone())
                }),
            )
            .route(
                &format!("/environments/fixture/applies/{build}/complete-local"),
                axum::routing::post(
                    async |State((_, completions)): State<CompletionState>,
                           Json(request): Json<CompleteLocalSnapshot>| {
                        completions.lock().await.push(request.artifact);
                        StatusCode::ACCEPTED
                    },
                ),
            )
            .with_state((status.clone(), completions.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let connection = Connection::from_values(
            url::Url::parse(&format!("http://{address}/mcp/fixture")).unwrap(),
            SecretString::from("fresh-native-reference"),
        )
        .unwrap();
        let artifact = ArtifactId::new();
        // Completion was accepted, but restart happened before SourceReady commit.
        connection.complete_local(build, artifact).await.unwrap();
        let mut upload = UploadedSource {
            artifact,
            completed_at: Some(Instant::now() - COMPLETION_RETRY),
        };
        assert!(matches!(
            poll(&connection, build, Some(&mut upload)).await.unwrap(),
            PollOutcome::Pending
        ));
        assert_eq!(*completions.lock().await, vec![artifact, artifact]);
        // Bounded resend, no duplicate upload/reservation, and no resend after
        // durable source-ready or a newer accepted generation supersedes it.
        assert!(matches!(
            poll(&connection, build, Some(&mut upload)).await.unwrap(),
            PollOutcome::Pending
        ));
        status.lock().await.state = BuildState::SourceReady {
            source: veoveo_hub::activation::MaterializedSource {
                identity: SnapshotIdentity {
                    digest: veoveo_types::Sha256Digest::from_bytes([2; 32]),
                    size: 123,
                },
                directory: RelativePath::parse(format!("sources/{build}")).unwrap(),
            },
        };
        upload.completed_at = None;
        poll(&connection, build, Some(&mut upload)).await.unwrap();
        {
            let mut status = status.lock().await;
            status.current = false;
            status.state = BuildState::Preparing;
        }
        assert!(matches!(
            poll(&connection, build, Some(&mut upload)).await.unwrap(),
            PollOutcome::Superseded
        ));
        assert_eq!(completions.lock().await.len(), 2);
        server.abort();
    }
    #[test]
    fn deterministic_source_snapshot_ignores_mtime_but_tracks_bytes_and_explicit_exclusions() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("source.rs"), b"first source").unwrap();
        fs::create_dir(directory.path().join("cache")).unwrap();
        fs::write(directory.path().join("cache/ignored"), b"not selected").unwrap();
        let excluded = [RelativePath::parse("cache").unwrap()];
        let first = archive(directory.path(), &excluded).unwrap();
        let times = fs::FileTimes::new()
            .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000));
        fs::File::open(directory.path().join("source.rs"))
            .unwrap()
            .set_times(times)
            .unwrap();
        fs::write(
            directory.path().join("cache/ignored"),
            b"unrelated cache change",
        )
        .unwrap();
        assert_eq!(
            archive(directory.path(), &excluded).unwrap().identity,
            first.identity
        );
        let mut tar = tar::Archive::new(fs::File::open(first.file.path()).unwrap());
        let names = tar
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![PathBuf::from("source.rs")]);
        fs::write(directory.path().join("source.rs"), b"second source").unwrap();
        assert_ne!(
            archive(directory.path(), &excluded).unwrap().identity,
            first.identity
        );
    }
}
