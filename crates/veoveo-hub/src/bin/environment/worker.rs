//! s6 owns child lifetime. Native Exec only prepares/controls its fixed service dirs.
use super::{
    build::{contained, verify_output},
    protocol::WorkerRequest,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use veoveo_hub::activation::WorkerSpec;
use veoveo_hub_sdk::registration::{
    ClientMetadata, RegistrationAccessToken,
    client::{KeyDirectory, RegistrationClient},
};
use veoveo_types::BuildId;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterRequest {
    pub worker: WorkerRequest,
    pub metadata: ClientMetadata,
    pub admission: RegistrationAccessToken,
}
fn key_directory(root: &Path, build: BuildId) -> PathBuf {
    root.join(format!("builds/{build}/keys"))
}

fn command(root: &Path, worker: &WorkerRequest) -> Result<Command> {
    ensure!(
        worker.build.build == worker.identity.build,
        "worker identity belongs to another build"
    );
    ensure!(
        worker.identity.port > 0,
        "worker listen port must be nonzero"
    );
    worker.build.recipe.validate()?;
    verify_output(root, &worker.output)?;
    let source = root.join(worker.build.source.directory.as_str());
    super::materialize::verify_receipt(&source, &worker.build.source.identity)?;
    let (mut command, args, cwd) = match &worker.build.recipe.worker {
        WorkerSpec::Compiled { args, cwd, .. } => (
            Command::new(contained(root, &worker.output.executable)?),
            args,
            cwd,
        ),
        WorkerSpec::Interpreted {
            interpreter,
            entrypoint,
            args,
            cwd,
        } => {
            let mut command = Command::new(interpreter);
            command.arg(contained(&source, entrypoint)?);
            (command, args, cwd)
        }
    };
    command.args(args).current_dir(contained(&source, cwd)?);
    // Activation owns these values. Recipes cannot shadow them with equals or
    // separate argv spellings. No admission secret is needed after saved registration.
    command
        .arg("--deployment-id")
        .arg(worker.identity.deployment.to_string())
        .arg("--key-directory")
        .arg(key_directory(root, worker.identity.build))
        .arg("--port")
        .arg(worker.identity.port.to_string())
        .arg("--public-base-url")
        .arg(worker.identity.public_base_url.as_str())
        .arg("--hub-issuer")
        .arg(worker.identity.issuer.as_str());
    Ok(command)
}

pub async fn describe(root: &Path, worker: &WorkerRequest) -> Result<ClientMetadata> {
    let mut command = command(root, worker)?;
    command.arg("--describe-registration");
    let output = super::process::run(tokio::process::Command::from(command), 1024 * 1024).await?;
    ensure!(
        output.code == Some(0) && !output.truncated,
        "candidate offline registration description failed"
    );
    let metadata: ClientMetadata = serde_json::from_slice(&output.bytes)?;
    metadata.check()?;
    ensure!(
        metadata.registration.deployment == worker.identity.deployment
            && metadata.registration.endpoint == worker.identity.endpoint,
        "candidate registration identity differs from admitted invocation"
    );
    Ok(metadata)
}

/// Also used for trusted reissuance after native orphan collection. The SDK verifies
/// existing saved key/declaration before replacement; no implicit global fallback.
pub async fn register(root: &Path, request: RegisterRequest) -> Result<()> {
    ensure!(
        describe(root, &request.worker).await? == request.metadata,
        "candidate metadata changed before registration"
    );
    stop_existing(root, request.worker.identity.build)?;
    let keys = KeyDirectory::new(key_directory(root, request.worker.identity.build));
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let issuer = url::Url::parse(request.worker.identity.issuer.as_str())?;
    let endpoint = RegistrationClient::discover(&http, &issuer).await?;
    let client = RegistrationClient::new(
        http,
        endpoint,
        secrecy::SecretString::from(request.admission.expose_secret()),
        keys.load_or_generate()?,
        request.metadata,
    )?;
    client.register_and_store(&keys).await?;
    Ok(())
}

pub async fn resume(
    root: &Path,
    worker: &WorkerRequest,
) -> Result<super::protocol::RegistrationState> {
    use veoveo_hub_sdk::registration::client::ClientError;
    let metadata = describe(root, worker).await?;
    let keys = KeyDirectory::new(key_directory(root, worker.identity.build));
    if !keys.registration_path().exists() {
        return Ok(super::protocol::RegistrationState::FreshAdmissionRequired);
    }
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let endpoint =
        RegistrationClient::discover(&http, &url::Url::parse(worker.identity.issuer.as_str())?)
            .await?;
    let client = RegistrationClient::new(
        http,
        endpoint,
        secrecy::SecretString::from("no-initial-admission"),
        keys.load_or_generate()?,
        metadata,
    )?;
    match client.resume_or_register(&keys).await {
        Ok(_) => Ok(super::protocol::RegistrationState::Ready),
        Err(ClientError::FreshAdmissionRequired) => {
            Ok(super::protocol::RegistrationState::FreshAdmissionRequired)
        }
        Err(error) => Err(error.into()),
    }
}
fn stop_existing(root: &Path, build: BuildId) -> Result<()> {
    let directory = root.join(format!("services/worker-{build}"));
    if !directory.exists() {
        return Ok(());
    }
    ensure!(
        Command::new("/usr/bin/s6-svc")
            .arg("-d")
            .arg(&directory)
            .status()?
            .success(),
        "could not stop old worker before fresh registration"
    );
    ensure!(
        Command::new("/usr/bin/s6-svwait")
            .args(["-d", "-t", "25000"])
            .arg(directory)
            .status()?
            .success(),
        "old worker did not stop before fresh registration"
    );
    Ok(())
}

pub fn start(root: &Path, request: &WorkerRequest) -> Result<()> {
    // Verify actual bytes again before installing the service declaration.
    let _ = command(root, request)?;
    ensure!(
        KeyDirectory::new(key_directory(root, request.identity.build))
            .registration_path()
            .is_file(),
        "worker must be admitted and registered before start"
    );
    let directory = root.join(format!("services/worker-{}", request.identity.build));
    fs::create_dir_all(root.join("services"))?;
    let bytes = serde_json::to_vec(request)?;
    let existed = directory.exists();
    if existed {
        ensure!(
            fs::read(directory.join("worker.json"))? == bytes,
            "existing worker declaration differs"
        );
    } else {
        // Dot directories are ignored by s6-svscan. Rename exposes only complete run
        // and identity files; s6 owns the process independently of this Exec caller.
        let staged = tempfile::Builder::new()
            .prefix(".worker-")
            .tempdir_in(root.join("services"))?;
        fs::write(staged.path().join("worker.json"), bytes)?;
        fs::write(
            staged.path().join("run"),
            format!(
                "#!/bin/sh\nexec /usr/local/bin/veoveo-environment worker-exec --build {}\n",
                request.identity.build
            ),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(staged.path().join("run"), fs::Permissions::from_mode(0o500))?;
        }
        fs::rename(staged.path(), &directory)?;
    }
    let status = Command::new("/usr/bin/s6-svscanctl")
        .arg("-a")
        .arg(root.join("services"))
        .status()?;
    ensure!(status.success(), "native worker scan request failed");
    if existed {
        ensure!(
            Command::new("/usr/bin/s6-svc")
                .arg("-u")
                .arg(directory)
                .status()?
                .success(),
            "native worker restart request failed"
        );
    }
    Ok(())
}

#[cfg(unix)]
pub fn exec(root: &Path, build: BuildId) -> Result<()> {
    use std::os::unix::process::CommandExt as _;
    let request: WorkerRequest = serde_json::from_slice(&fs::read(
        root.join(format!("services/worker-{build}/worker.json")),
    )?)?;
    ensure!(
        request.identity.build == build,
        "worker declaration differs from service directory"
    );
    let mut command = command(root, &request)?;
    // Required CLI field, never an authority: saved RFC7592 registration is used.
    command.env(
        "VEOVEO_REGISTRATION_SECRET",
        "deployment-registration-resume",
    );
    Err(command.exec().into())
}
