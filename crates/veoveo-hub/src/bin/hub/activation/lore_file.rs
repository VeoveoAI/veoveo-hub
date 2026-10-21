//! Small native definition-file adapter. Tree metadata bounds the selected file;
//! unchanged Lore performs exact-revision reconstruction. No custom CAS codec.
use super::{lore, source::SourceAuthority, types::*};
use anyhow::{Context, Result, ensure};
use lore_proto::lore::thin_client::v1::{
    NodeType, RevisionTreeRequest, revision_tree_request, revision_tree_response,
    thin_client_service_client::ThinClientServiceClient,
};
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;
use tonic::Request;

pub(super) async fn read(
    state: &crate::runtime::AdminState,
    authority: &SourceAuthority,
    source: &LoreDefinitionSource,
    revision: &NativeRevision,
    cancellation: &CancellationToken,
) -> Result<EnvironmentDefinition> {
    let (deployment, token) =
        lore::credential(state, authority, &source.deployment, &source.repository).await?;
    let mut client = ThinClientServiceClient::new(lore::connect(&deployment).await?);
    let mut request = Request::new(RevisionTreeRequest {
        query: Some(revision_tree_request::Query::Signature(
            hex::decode(revision.as_str())?.into(),
        )),
        path_prefix: definition_parent(&source.path),
        max_depth: Some(1),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    let repository = hex::decode(source.repository.as_str())?;
    request.metadata_mut().insert_bin(
        "lore-partition-bin",
        tonic::metadata::MetadataValue::from_bytes(&repository),
    );
    request.metadata_mut().insert_bin(
        "urc-repository-id-bin",
        tonic::metadata::MetadataValue::from_bytes(&repository),
    );
    let mut tree = client.revision_tree(request).await?.into_inner();
    let mut verified_header = false;
    let mut expected_size = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let response = tokio::select! {
            _ = cancellation.cancelled() => anyhow::bail!("native definition read cancelled"),
            _ = tokio::time::sleep_until(deadline) => anyhow::bail!(super::native::NativeUnavailable),
            response = tree.message() => response?,
        };
        let Some(response) = response else {
            break;
        };
        match response.payload {
            Some(revision_tree_response::Payload::Header(header)) => {
                ensure!(
                    hex::encode(header.signature) == revision.as_str(),
                    "native definition revision differs"
                );
                verified_header = true;
            }
            Some(revision_tree_response::Payload::Node(node))
                if node.path == source.path.as_str() =>
            {
                ensure!(
                    node.node_type == NodeType::File as i32
                        && node.size > 0
                        && node.size <= MAX_DEFINITION_BYTES as u64,
                    "native definition is not a bounded regular file"
                );
                expected_size = Some(node.size);
            }
            _ => {}
        }
        if verified_header && expected_size.is_some() {
            break;
        }
    }
    ensure!(
        verified_header,
        "native definition response omitted revision proof"
    );
    let expected_size = expected_size.context("native definition file missing")?;
    drop(tree);
    let temporary = tempfile::Builder::new()
        .prefix("veoveo-definition-")
        .tempdir()?;
    let repository_path = temporary.path().join("repository");
    let output = temporary.path().join("environment.json");
    let mut remote = deployment.endpoint;
    remote.set_path(&format!("/{}", source.repository));
    let mut clone = native_command(temporary.path());
    clone
        .args(["--access-token", token.as_str(), "repository", "clone"])
        .arg(remote.as_str())
        .arg(&repository_path)
        .args(["--revision", revision.as_str(), "--bare"]);
    run(clone, cancellation).await?;
    let mut write = native_command(temporary.path());
    write
        .current_dir(&repository_path)
        .args(["--access-token", token.as_str(), "--repository"])
        .arg(&repository_path)
        .args([
            "file",
            "write",
            "--path",
            source.path.as_str(),
            "--revision",
            revision.as_str(),
            "--output",
        ])
        .arg(&output);
    run(write, cancellation).await?;
    drop(token);
    // Native I/O parallelism flags are not total transfer limits. The exact file
    // metadata bounds its reconstructed output; enforce actual returned bytes too.
    ensure!(
        std::fs::metadata(&output)?.is_file() && std::fs::metadata(&output)?.len() == expected_size,
        "native definition size differs"
    );
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(
            std::fs::File::open(output)?,
            MAX_DEFINITION_BYTES as u64 + 1,
        ),
        &mut bytes,
    )?;
    ensure!(
        bytes.len() <= MAX_DEFINITION_BYTES,
        "native definition exceeds byte bound"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn definition_parent(path: &veoveo_hub::activation::RelativePath) -> Option<String> {
    std::path::Path::new(path.as_str())
        .parent()
        .and_then(std::path::Path::to_str)
        .filter(|parent| !parent.is_empty() && *parent != ".")
        .map(str::to_owned)
}
fn native_command(root: &std::path::Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/usr/local/bin/lore");
    command
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("LORE_GLOBAL_PATH", root.join("global"))
        .env("LORE_AUTH_PATH", root.join("auth"));
    command
}
async fn drain(mut stream: impl AsyncRead + Unpin) -> Result<()> {
    // Diagnostics may contain native details. Bound/discard, never broadcast them
    // or print argv (which temporarily contains a native resource credential).
    let mut buffer = [0; 4096];
    loop {
        if stream.read(&mut buffer).await? == 0 {
            return Ok(());
        }
    }
}
async fn run(mut command: tokio::process::Command, cancellation: &CancellationToken) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().context("launch native definition reader")?;
    let stdout = child.stdout.take().context("native stdout missing")?;
    let stderr = child.stderr.take().context("native stderr missing")?;
    let status = async {
        let (stdout, stderr, status) = tokio::join!(drain(stdout), drain(stderr), child.wait());
        // Finish child wait even if a pipe read fails, then propagate diagnostics.
        stdout?;
        stderr?;
        Ok::<_, anyhow::Error>(status?)
    };
    let status = tokio::select! {
        result=status=>Some(result?),
        _=cancellation.cancelled()=>None,
        _=tokio::time::sleep(std::time::Duration::from_secs(90))=>None,
    };
    let Some(status) = status else {
        let _ = child.start_kill();
        let _ = child.wait().await; // reap before private temp storage is removed
        anyhow::bail!("native definition read cancelled or timed out");
    };
    ensure!(
        status.success(),
        "native definition read refused or unavailable"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_tree_query_uses_parent_directory_including_repository_root() {
        assert_eq!(
            definition_parent(&veoveo_hub::activation::RelativePath::parse("work.json").unwrap()),
            None
        );
        assert_eq!(
            definition_parent(
                &veoveo_hub::activation::RelativePath::parse("environments/work.json").unwrap()
            ),
            Some("environments".into())
        );
    }
    #[tokio::test]
    async fn cancelled_native_reader_is_killed_and_reaped_before_return() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        let cancellation = CancellationToken::new();
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args([
                "-c",
                "printf '%s' $$ > \"$1\"; exec sleep 60",
                "native-reader-test",
            ])
            .arg(&pid_file);
        let token = cancellation.clone();
        let reader = tokio::spawn(async move { run(command, &token).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !pid_file.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(pid_file).unwrap();
        cancellation.cancel();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), reader)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        let status = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "kill -0 \"$1\" 2>/dev/null",
                "native-reader-test",
                &pid,
            ])
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "reader must not return while its native child survives"
        );
    }
}
