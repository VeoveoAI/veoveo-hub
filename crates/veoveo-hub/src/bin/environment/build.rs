//! Shared compiler cache is locked until output capture, not merely compilation.
//! Actual executable paths are immutable BuildId paths, never mutable target files.
//! Recipes are foreground build/check commands, not background daemons. Native
//! Exec owns their process-group deadline/cancellation; serving children belong to s6.
use super::protocol::{BuildRequest, BuildResult, CommandFailure, CommandPhase};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};
use veoveo_hub::activation::{BuildOutput, CommandSpec, RelativePath, VerifiedOutput, WorkerSpec};
use veoveo_types::Sha256Digest;

const LOG_BYTES: usize = 16 * 1024;

pub async fn run(root: &Path, request: BuildRequest) -> Result<BuildResult> {
    let build = request.build;
    let result = build_locked(root, request).await?;
    let directory = root.join("diagnostics");
    fs::create_dir_all(&directory)?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    serde_json::to_writer(&mut file, &result)?;
    file.as_file().sync_all()?;
    file.persist(directory.join(format!("{build}.json")))?;
    Ok(result)
}
async fn build_locked(root: &Path, request: BuildRequest) -> Result<BuildResult> {
    request.recipe.validate()?;
    ensure!(
        request.source.directory.as_str() == format!("sources/{}", request.build),
        "source directory differs from build"
    );
    let source = root.join(request.source.directory.as_str());
    super::materialize::verify_receipt(&source, &request.source.identity)?;
    fs::create_dir_all(root.join("cache"))?;
    fs::create_dir_all(root.join("builds"))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("cache/.build.lock"))?;
    lock.lock()?;
    let destination = root.join(format!("builds/{}", request.build));
    if destination.exists() {
        let saved: BuildRequest =
            serde_json::from_slice(&fs::read(destination.join("input.json"))?)?;
        ensure!(
            serde_json::to_vec(&saved)? == serde_json::to_vec(&request)?,
            "existing BuildId has different inputs"
        );
        let output: VerifiedOutput =
            serde_json::from_slice(&fs::read(destination.join("output.json"))?)?;
        verify_output(root, &output)?;
        return Ok(BuildResult::Ready {
            output,
            check_ms: 0,
            build_ms: 0,
        });
    }
    let staging = tempfile::Builder::new()
        .prefix(".build-")
        .tempdir_in(root.join("builds"))?;
    let check_start = Instant::now();
    for (index, command) in request.recipe.check.iter().enumerate() {
        if let Some(failure) = command_run(
            root,
            &source,
            command,
            CommandPhase::Check,
            index,
            staging.path(),
        )
        .await?
        {
            return Ok(BuildResult::Failed {
                failure,
                check_ms: elapsed(check_start),
                build_ms: 0,
            });
        }
    }
    let check_ms = elapsed(check_start);
    let build_start = Instant::now();
    for (index, command) in request.recipe.build.iter().enumerate() {
        if let Some(failure) = command_run(
            root,
            &source,
            command,
            CommandPhase::Build,
            index,
            staging.path(),
        )
        .await?
        {
            return Ok(BuildResult::Failed {
                failure,
                check_ms,
                build_ms: elapsed(build_start),
            });
        }
    }
    let (output_path, digest) = match &request.recipe.worker {
        WorkerSpec::Compiled { output, .. } => {
            let candidate = match output {
                BuildOutput::Source { path } => contained(&source, path)?,
                BuildOutput::Cache { path } => contained(&root.join("cache"), path)?,
            };
            ensure!(candidate.is_file(), "build output is not a regular file");
            let captured = staging.path().join("service");
            fs::copy(candidate, &captured)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&captured, fs::Permissions::from_mode(0o500))?;
            }
            let digest = digest_file(&captured)?;
            (
                RelativePath::parse(format!("builds/{}/service", request.build))?,
                digest,
            )
        }
        WorkerSpec::Interpreted { entrypoint, .. } => {
            // The interpreter is admitted by native policy; all source/imports stay
            // in this immutable source tree rather than a shared live checkout.
            let entry = contained(&source, entrypoint)?;
            (
                RelativePath::parse(format!(
                    "{}/{}",
                    request.source.directory.as_str(),
                    entrypoint.as_str()
                ))?,
                digest_file(&entry)?,
            )
        }
    };
    let output = VerifiedOutput {
        digest,
        executable: output_path,
    };
    fs::write(
        staging.path().join("input.json"),
        serde_json::to_vec(&request)?,
    )?;
    fs::write(
        staging.path().join("output.json"),
        serde_json::to_vec(&output)?,
    )?;
    fs::rename(staging.path(), destination)?;
    // The exclusive native filesystem lock is held through verified output capture.
    drop(lock);
    Ok(BuildResult::Ready {
        output,
        check_ms,
        build_ms: elapsed(build_start),
    })
}

pub(super) fn contained(root: &Path, relative: &RelativePath) -> Result<PathBuf> {
    let path = root.join(relative.as_str()).canonicalize()?;
    ensure!(
        path.starts_with(root.canonicalize()?),
        "recipe path resolves outside its admitted root"
    );
    Ok(path)
}
pub(super) fn digest_file(path: &Path) -> Result<Sha256Digest> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}
pub(super) fn verify_output(root: &Path, output: &VerifiedOutput) -> Result<()> {
    ensure!(
        digest_file(&contained(root, &output.executable)?)? == output.digest,
        "captured executable bytes changed"
    );
    Ok(())
}
fn elapsed(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

async fn command_run(
    root: &Path,
    source: &Path,
    spec: &CommandSpec,
    phase: CommandPhase,
    index: usize,
    _staging: &Path,
) -> Result<Option<CommandFailure>> {
    let cwd = contained(source, &spec.cwd)?;
    let mut command = tokio::process::Command::new(&spec.executable);
    command.args(&spec.args).current_dir(cwd);
    command
        .env("CARGO_TARGET_DIR", root.join("cache/target"))
        .env("CARGO_HOME", root.join("cache/cargo"));
    let result = super::process::run(command, LOG_BYTES).await?;
    if result.code == Some(0) {
        return Ok(None);
    }
    Ok(Some(CommandFailure {
        phase,
        index,
        exit_code: result.code,
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&result.bytes),
            String::from_utf8_lossy(&result.stderr)
        ),
        truncated: result.truncated || result.stderr_truncated,
    }))
}

#[cfg(test)]
#[path = "build/tests.rs"]
mod tests;
