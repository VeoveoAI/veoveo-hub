//! Exact-revision materialization through the unchanged native CLI. The credential
//! arrives on private Exec stdin; only this child argv briefly contains it. Never
//! log the command, native stderr, or private request. No credential is persisted.
use super::protocol::{LoreSource, MaterializeLoreRequest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use veoveo_hub::activation::{MaterializedSource, RelativePath};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    source: LoreSource,
    materialized: MaterializedSource,
}
fn validate(source: &LoreSource) -> Result<()> {
    ensure!(
        source.remote.scheme() == "grpcs"
            && source.remote.host_str().is_some()
            && source.remote.username().is_empty()
            && source.remote.password().is_none()
            && source.remote.query().is_none()
            && source.remote.fragment().is_none()
            && source.remote.path() == format!("/{}", source.repository),
        "native source requires the exact admitted repository URL"
    );
    Ok(())
}
pub async fn run(root: &Path, request: MaterializeLoreRequest) -> Result<MaterializedSource> {
    validate(&request.source)?;
    ensure!(
        !request.access_token.is_empty(),
        "native resource token missing"
    );
    let parent = root.join("sources");
    fs::create_dir_all(&parent)?;
    let relative = RelativePath::parse(format!("sources/{}", request.build))?;
    let destination = root.join(relative.as_str());
    if destination.exists() {
        let receipt: Receipt =
            serde_json::from_slice(&fs::read(destination.join(".veoveo-lore.json"))?)?;
        ensure!(
            receipt.source == request.source,
            "existing build has different native provenance"
        );
        super::materialize::verify_receipt(&destination, &receipt.materialized.identity)?;
        return Ok(receipt.materialized);
    }
    let temporary = tempfile::Builder::new()
        .prefix(".native-source-")
        .tempdir_in(&parent)?;
    let checkout = temporary.path().join("checkout");
    let mut command = tokio::process::Command::new("/usr/local/bin/lore");
    command
        .args([
            "--access-token",
            request.access_token.as_str(),
            "repository",
            "clone",
        ])
        .arg(request.source.remote.as_str())
        .arg(&checkout)
        .args([
            "--revision",
            request.source.revision.as_str(),
            "--no-tracking",
        ]);
    // Native Exec's outer deadline owns this foreground child and descendants.
    let result = super::process::run(command, 16 * 1024).await?;
    ensure!(
        result.code == Some(0),
        "native exact-revision clone refused or unavailable"
    );
    drop(request.access_token);
    // Canonical selected bytes have a separate consumer-owned digest. Native
    // revision IDs are provenance, never reinterpreted as a content digest.
    let archive = super::client::archive(&checkout, &[])?;
    let staged = temporary.path().join("source");
    fs::create_dir(&staged)?;
    super::materialize::unpack(fs::File::open(archive.file.path())?, &staged)?;
    let materialized = MaterializedSource {
        identity: archive.identity,
        directory: relative,
    };
    super::materialize::write_receipt(&staged, &materialized.identity)?;
    fs::write(
        staged.join(".veoveo-lore.json"),
        serde_json::to_vec(&Receipt {
            source: request.source,
            materialized: materialized.clone(),
        })?,
    )?;
    fs::File::open(&staged)?.sync_all()?;
    match fs::rename(&staged, &destination) {
        Ok(()) => {}
        Err(error) => return Err(error.into()),
    }
    fs::File::open(&parent)?.sync_all()?;
    Ok(materialized)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_remote_must_bind_the_exact_repository_without_token_url_fields() {
        let mut source = LoreSource {
            remote: url::Url::parse(
                "grpcs://native.example:41337/11111111111111111111111111111111",
            )
            .unwrap(),
            repository: crate::native_source_ids::RepositoryId::parse(
                "11111111111111111111111111111111",
            )
            .unwrap(),
            revision: crate::native_source_ids::NativeRevision::parse("a".repeat(64)).unwrap(),
        };
        assert!(validate(&source).is_ok());
        source.remote.set_path("/22222222222222222222222222222222");
        assert!(validate(&source).is_err());
        source.remote.set_path("/11111111111111111111111111111111");
        source.remote.set_query(Some("token=secret"));
        assert!(validate(&source).is_err());
    }
}
