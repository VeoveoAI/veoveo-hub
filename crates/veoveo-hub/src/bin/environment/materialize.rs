//! Download privately, verify producer identity, then publish a complete source tree.
use super::protocol::MaterializeRequest;
use anyhow::{Context, Result, ensure};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use veoveo_hub::activation::{MaterializedSource, RelativePath, SnapshotIdentity};
use veoveo_mcp_artifact::contract::TransferMethod;
use veoveo_types::Sha256Digest;

// Actual extraction bounds, independently of untrusted archive header claims.
const MAX_ARCHIVE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_EXPANDED: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

pub async fn run(root: &Path, request: MaterializeRequest) -> Result<MaterializedSource> {
    ensure!(
        request.snapshot.size > 0 && request.snapshot.size <= MAX_ARCHIVE,
        "snapshot size outside materialization bounds"
    );
    ensure!(
        request.transfer.method == TransferMethod::Get,
        "source transfer must be GET"
    );
    let url =
        url::Url::parse(request.transfer.url.as_str()).context("invalid source transfer URL")?;
    ensure!(
        url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
        "source transfer requires HTTPS without userinfo"
    );
    let source_name = format!("sources/{}", request.build);
    let destination = root.join(&source_name);
    fs::create_dir_all(root.join("sources"))?;
    // A marker alone cannot certify a partial extraction: it is written only in the
    // private staging tree immediately before rename. Existing trees are read-only.
    if destination.exists() {
        verify_receipt(&destination, &request.snapshot)?;
        return Ok(MaterializedSource {
            identity: request.snapshot,
            directory: RelativePath::parse(source_name)?,
        });
    }
    let staging = tempfile::Builder::new()
        .prefix(".source-")
        .tempdir_in(root.join("sources"))?;
    let mut archive = tempfile::tempfile()?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    let mut get = client.get(url);
    for (name, value) in &request.transfer.headers {
        get = get.header(name, value);
    }
    // reqwest errors contain the signed URL. Never attach their Display/Debug text.
    let response = get
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("source transfer unavailable"))?;
    ensure!(
        response.status() == reqwest::StatusCode::OK,
        "source transfer refused ({})",
        response.status().as_u16()
    );
    if let Some(size) = response.content_length() {
        ensure!(
            size == request.snapshot.size,
            "source transfer length differs"
        );
    }
    let mut stream = response.bytes_stream();
    let mut size = 0_u64;
    let mut hash = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("source transfer interrupted"))?;
        size = size
            .checked_add(u64::try_from(chunk.len())?)
            .context("source size overflow")?;
        ensure!(
            size <= request.snapshot.size,
            "source transfer exceeds reserved size"
        );
        hash.update(&chunk);
        archive.write_all(&chunk)?;
    }
    ensure!(
        size == request.snapshot.size
            && Sha256Digest::from_bytes(hash.finalize().into()) == request.snapshot.digest,
        "source transfer identity differs"
    );
    use std::io::Seek as _;
    archive.rewind()?;
    unpack(archive, staging.path())?;
    write_receipt(staging.path(), &request.snapshot)?;
    // All writes are confined to a fresh directory. A same-Build concurrent
    // materialization may win; equal completed identity is the only reuse allowed.
    match fs::rename(staging.path(), &destination) {
        Ok(()) => {}
        Err(_) if destination.exists() => verify_receipt(&destination, &request.snapshot)?,
        Err(error) => return Err(error.into()),
    }
    Ok(MaterializedSource {
        identity: request.snapshot,
        directory: RelativePath::parse(source_name)?,
    })
}

fn clean_path(path: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                ensure!(
                    value != ".veoveo-source.json",
                    "reserved source receipt path"
                );
                result.push(value);
            }
            Component::CurDir => {}
            _ => anyhow::bail!("archive path escapes source tree"),
        }
    }
    Ok(result)
}

/// Extract files/directories first, links last. No archive write can follow a link
/// introduced by an earlier archive entry. Hard links target regular extracted files.
pub(super) fn unpack(reader: impl Read, destination: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut paths = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut links = Vec::new();
    let mut expanded = 0_u64;
    for (index, entry) in archive.entries()?.enumerate() {
        ensure!(index < MAX_ENTRIES, "snapshot contains too many entries");
        let mut entry = entry?;
        let path = clean_path(&entry.path()?)?;
        if path.as_os_str().is_empty() {
            ensure!(
                entry.header().entry_type().is_dir(),
                "empty archive file path"
            );
            continue;
        }
        ensure!(paths.insert(path.clone()), "duplicate archive path");
        let kind = entry.header().entry_type();
        let target = destination.join(&path);
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if kind.is_symlink() || kind.is_hard_link() {
            let link = entry
                .link_name()?
                .context("archive link without target")?
                .into_owned();
            ensure!(!link.is_absolute(), "absolute archive link");
            links.push((path, link, kind.is_hard_link()));
            continue;
        }
        ensure!(kind.is_file(), "unsupported archive special entry");
        files.insert(path.clone());
        expanded = expanded
            .checked_add(entry.size())
            .context("expanded source size overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED,
            "snapshot expanded size exceeds bound"
        );
        fs::create_dir_all(target.parent().context("archive file parent missing")?)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        ensure!(
            std::io::copy(&mut entry, &mut output)? == entry.size(),
            "truncated archive file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            // Preserve executable bits, never setuid/setgid or writable-by-other.
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(0o600 | (entry.header().mode()? & 0o111)),
            )?;
        }
    }
    // Validate the entire planned link graph before creating any link. In
    // particular, never create a child through a previously introduced symlink.
    let link_paths: BTreeSet<_> = links.iter().map(|(path, _, _)| path.clone()).collect();
    for (path, link, hard) in &links {
        ensure!(
            !path
                .ancestors()
                .skip(1)
                .any(|parent| link_paths.contains(parent)),
            "archive link parent is another archive link"
        );
        if *hard {
            let original = clean_path(link)?;
            ensure!(
                files.contains(&original),
                "hard link target is not an extracted regular file"
            );
            ensure!(
                destination
                    .join(original)
                    .canonicalize()?
                    .starts_with(destination.canonicalize()?),
                "hard link target escapes source tree"
            );
        }
    }
    for (path, link, hard) in &links {
        let target = destination.join(path);
        fs::create_dir_all(target.parent().context("archive link parent missing")?)?;
        if *hard {
            let original = destination.join(clean_path(link)?);
            fs::hard_link(original, target)?;
        } else {
            // Lexically normalize against the symlink's parent before creating it.
            let mut relative = path.parent().unwrap_or(Path::new("")).to_path_buf();
            for part in link.components() {
                match part {
                    Component::ParentDir => {
                        ensure!(relative.pop(), "archive link escapes source tree")
                    }
                    Component::CurDir => {}
                    Component::Normal(p) => relative.push(p),
                    _ => anyhow::bail!("invalid archive link"),
                }
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, target)?;
            #[cfg(not(unix))]
            anyhow::bail!("native source links require a Unix runtime");
        }
    }
    for (path, _, _) in links {
        let resolved = destination
            .join(path)
            .canonicalize()
            .context("unresolvable archive link")?;
        ensure!(
            resolved.starts_with(destination.canonicalize()?),
            "archive link escapes source tree"
        );
    }
    Ok(())
}
pub(super) fn verify_receipt(destination: &Path, identity: &SnapshotIdentity) -> Result<()> {
    let receipt: SnapshotIdentity =
        serde_json::from_slice(&fs::read(destination.join(".veoveo-source.json"))?)?;
    ensure!(
        &receipt == identity,
        "existing source directory has another identity"
    );
    Ok(())
}
pub(super) fn write_receipt(destination: &Path, identity: &SnapshotIdentity) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination.join(".veoveo-source.json"))?;
    serde_json::to_writer(&mut file, identity)?;
    file.sync_all()?;
    File::open(destination)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "materialize/tests.rs"]
mod tests;
