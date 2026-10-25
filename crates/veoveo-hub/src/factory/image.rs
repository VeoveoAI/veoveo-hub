//! The image builder: BuildKit builds a branch's environment image from a checkout of its
//! `openshell/image/` alone and pushes it to the installation's registry. Hub builds on
//! every push and compares the digests BuildKit returns; its layer cache returns an
//! unchanged image, with the same digest, without building again. The image carries no
//! platform tool: Lore's command line reaches the sandbox from the installation's own
//! image.
//!
//! Builds share no writable cache. Every build names its branch's [`CacheNamespace`],
//! so its `RUN --mount=type=cache` directories are apart from every other branch's, and
//! Hub prunes BuildKit's cache mounts before and after every build, so one lives for one
//! build only. BuildKit keys a cached step by its instruction and inputs, not by what a
//! cache mount held, so a cache mount's content must never outlive its build.

use std::{
    ffi::OsString,
    path::Path,
    process::{ExitStatus, Stdio},
};

use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};
use veoveo_types::{IdProfile, IdProfileSpec, IdentifierError, Sha256Digest};

use crate::execution::BranchName;

/// BuildKit's client, on the runtime image's `PATH`.
const BUILDCTL: &str = "buildctl";

/// The registry path every environment image is named under.
const REPOSITORY_PATH: &str = "factory";

/// How much of the end of BuildKit's output a failed build keeps, in bytes.
pub const FAILURE_OUTPUT_LIMIT: usize = 8 * 1024;

/// The installation's image builder, part of its factory configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageBuilder {
    /// BuildKit's address as `buildctl --addr` takes it, such as
    /// `tcp://buildkitd.veoveo-system.svc.cluster.local:1234`.
    pub buildkit: url::Url,
    /// The registry images are pushed to, by the name BuildKit resolves, such as
    /// `registry.veoveo-system.svc.cluster.local:5000`.
    pub registry: RegistryHost,
}

impl ImageBuilder {
    /// Build the image whose `Dockerfile` and the files it copies are in `context`, with
    /// its cache mounts in `cache`, push it as `<registry>/factory/<name>` and return it
    /// by digest. BuildKit's cache mounts are pruned first, which must succeed, and again
    /// after the build, whatever its outcome; a failure then is logged, and the next
    /// build's first prune removes what it left. Dropping the future stops buildctl,
    /// which ends the build.
    pub async fn build(
        &self,
        context: &Path,
        name: &ImageName,
        cache: &CacheNamespace,
    ) -> Result<PinnedImage, ImageBuildError> {
        self.prune_cache_mounts()
            .await
            .map_err(ImageBuildError::Prune)?;
        let built = self.build_pruned(context, name, cache).await;
        if let Err(error) = self.prune_cache_mounts().await {
            tracing::warn!(
                image = %name,
                %error,
                "BuildKit's cache mounts were not pruned after a build; the next build prunes them first"
            );
        }
        built
    }

    async fn build_pruned(
        &self,
        context: &Path,
        name: &ImageName,
        cache: &CacheNamespace,
    ) -> Result<PinnedImage, ImageBuildError> {
        let repository = self.repository(name);
        let scratch = tempfile::tempdir()?;
        let metadata = scratch.path().join("metadata.json");
        let (status, output) = run(self.arguments(context, &repository, cache, &metadata)).await?;
        if !status.success() {
            return Err(ImageBuildError::Failed { status, output });
        }
        let digest = image_digest(&tokio::fs::read(&metadata).await?)?;
        Ok(PinnedImage(format!("{repository}@{digest}")))
    }

    /// Delete every cache mount BuildKit holds and no build uses now. Layers stay cached.
    async fn prune_cache_mounts(&self) -> Result<(), PruneError> {
        let (status, output) = run(self.prune_arguments()).await?;
        if status.success() {
            Ok(())
        } else {
            Err(PruneError::Failed { status, output })
        }
    }

    fn repository(&self, name: &ImageName) -> String {
        format!("{}/{REPOSITORY_PATH}/{name}", self.registry)
    }

    /// buildctl's arguments for building `context` with its cache mounts in `cache`,
    /// pushing it to `repository` and writing the build's metadata to `metadata`.
    fn arguments(
        &self,
        context: &Path,
        repository: &str,
        cache: &CacheNamespace,
        metadata: &Path,
    ) -> Vec<OsString> {
        let local = |name: &str| {
            let mut local = OsString::from(format!("{name}="));
            local.push(context);
            local
        };
        vec![
            "--addr".into(),
            self.buildkit.as_str().into(),
            "build".into(),
            "--progress".into(),
            "plain".into(),
            "--frontend".into(),
            "dockerfile.v0".into(),
            "--opt".into(),
            format!("build-arg:{CACHE_MOUNT_NAMESPACE}={}", cache.0).into(),
            "--local".into(),
            local("context"),
            "--local".into(),
            local("dockerfile"),
            "--output".into(),
            format!("type=image,name={repository},push=true").into(),
            "--metadata-file".into(),
            metadata.into(),
        ]
    }

    /// buildctl's arguments for pruning every cache mount, internal ones included.
    fn prune_arguments(&self) -> Vec<OsString> {
        vec![
            "--addr".into(),
            self.buildkit.as_str().into(),
            "prune".into(),
            "--all".into(),
            "--filter".into(),
            "type==exec.cachemount".into(),
        ]
    }
}

/// Run buildctl with `arguments` and return its status and the end of its progress
/// output, which it writes to standard error.
async fn run(arguments: Vec<OsString>) -> std::io::Result<(ExitStatus, String)> {
    let mut buildctl = Command::new(BUILDCTL)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let progress = buildctl
        .stderr
        .take()
        .expect("buildctl's standard error is piped");
    let output = tail(progress, FAILURE_OUTPUT_LIMIT).await?;
    Ok((buildctl.wait().await?, output))
}

/// The build argument the Dockerfile frontend prefixes to every cache mount's id. A
/// `Dockerfile` cannot set it; only the client's build options do.
const CACHE_MOUNT_NAMESPACE: &str = "BUILDKIT_CACHE_MOUNT_NS";

/// The namespace of one branch's cache mounts: the SHA-256, as 64 lowercase hexadecimal
/// digits, of its Lore repository ID, a zero byte and its branch name. Every branch has
/// its own, of one length, and the same one at every push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheNamespace(String);

impl CacheNamespace {
    pub fn of(repository: &str, branch: &BranchName) -> Self {
        let mut hash = Sha256::new();
        hash.update(repository.as_bytes());
        hash.update([0]);
        hash.update(branch.as_str().as_bytes());
        Self(hex::encode(hash.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What builds an instance's environment image: [`ImageBuilder`] with BuildKit, or a
/// test's own builder.
pub trait Build: Send + Sync {
    /// Build the image whose `Dockerfile` and the files it copies are in `context` as
    /// `name`, with its cache mounts in `cache`, and return it by digest.
    fn build<'a>(
        &'a self,
        context: &'a Path,
        name: &'a ImageName,
        cache: &'a CacheNamespace,
    ) -> BoxFuture<'a, Result<PinnedImage, ImageBuildError>>;
}

impl Build for ImageBuilder {
    fn build<'a>(
        &'a self,
        context: &'a Path,
        name: &'a ImageName,
        cache: &'a CacheNamespace,
    ) -> BoxFuture<'a, Result<PinnedImage, ImageBuildError>> {
        Box::pin(ImageBuilder::build(self, context, name, cache))
    }
}

/// Why an image did not build. The message is what an instance reports as its `build`
/// failure.
#[derive(Debug, thiserror::Error)]
pub enum ImageBuildError {
    #[error("running buildctl: {0}")]
    Io(#[from] std::io::Error),
    /// BuildKit refused or failed the build. `output` is the end of its progress
    /// output, which names the failing step and BuildKit's error.
    #[error("BuildKit failed the build ({status}):\n{output}")]
    Failed { status: ExitStatus, output: String },
    #[error("BuildKit's build metadata names no image digest: {0}")]
    Metadata(#[source] serde_json::Error),
    /// The cache mounts of earlier builds could not be pruned, so nothing was built.
    #[error("BuildKit's cache mounts could not be pruned before the build: {0}")]
    Prune(#[source] PruneError),
}

/// Why BuildKit's cache mounts were not pruned.
#[derive(Debug, thiserror::Error)]
pub enum PruneError {
    #[error("running buildctl: {0}")]
    Io(#[from] std::io::Error),
    #[error("BuildKit failed the prune ({status}):\n{output}")]
    Failed { status: ExitStatus, output: String },
}

/// The pushed image's digest from buildctl's `--metadata-file`.
fn image_digest(metadata: &[u8]) -> Result<Sha256Digest, ImageBuildError> {
    #[derive(Deserialize)]
    struct BuildMetadata {
        #[serde(rename = "containerimage.digest")]
        digest: Sha256Digest,
    }
    serde_json::from_slice::<BuildMetadata>(metadata)
        .map(|metadata| metadata.digest)
        .map_err(ImageBuildError::Metadata)
}

/// The last `limit` bytes of `output`, read to its end, as text that starts at a line
/// when its beginning was cut.
async fn tail(mut output: impl AsyncRead + Unpin, limit: usize) -> std::io::Result<String> {
    let mut kept = Vec::new();
    let mut chunk = vec![0; 16 * 1024];
    let mut cut = false;
    loop {
        let read = output.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        kept.extend_from_slice(&chunk[..read]);
        if kept.len() > 2 * limit {
            kept.drain(..kept.len() - limit);
            cut = true;
        }
    }
    if kept.len() > limit {
        kept.drain(..kept.len() - limit);
        cut = true;
    }
    let mut text = String::from_utf8_lossy(&kept).trim_end().to_owned();
    if cut && let Some(line_end) = text.find('\n') {
        text.drain(..=line_end);
    }
    Ok(text)
}

/// A registry's host with an optional port: the `domain` of the OCI distribution
/// reference grammar, without IPv6 literals.
#[veoveo_types::id(text(RegistryHosts))]
pub struct RegistryHost(String);

/// The name of an environment image below the registry's `factory/` path, such as
/// `time-dev`: `/`-separated path components of the OCI distribution reference grammar,
/// lowercase letters and digits joined by `.`, `_`, `__` or hyphens.
#[veoveo_types::id(text(ImageNames))]
pub struct ImageName(String);

/// An image by digest, `<name>@sha256:<hex>` in the OCI distribution reference grammar,
/// without a tag.
#[veoveo_types::id(text(PinnedImages))]
pub struct PinnedImage(String);

impl PinnedImage {
    /// The image's name, with its registry host when it has one.
    pub fn repository(&self) -> &str {
        self.parts().0
    }

    pub fn digest(&self) -> Sha256Digest {
        Sha256Digest::parse(self.parts().1).expect("a pinned image's digest is admitted")
    }

    fn parts(&self) -> (&str, &str) {
        self.0
            .rsplit_once('@')
            .expect("a pinned image names its digest")
    }
}

#[doc(hidden)]
pub struct RegistryHosts;
impl IdProfile for RegistryHosts {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        if is_domain(value) {
            Ok(())
        } else {
            Err(IdentifierError::new(
                value,
                "must be a registry host name with an optional port",
            ))
        }
    });
}

#[doc(hidden)]
pub struct ImageNames;
impl IdProfile for ImageNames {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        if is_path(value) {
            Ok(())
        } else {
            Err(IdentifierError::new(
                value,
                "must be OCI path components of lowercase letters and digits",
            ))
        }
    });
}

#[doc(hidden)]
pub struct PinnedImages;
impl IdProfile for PinnedImages {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        let pinned = value.split_once('@').is_some_and(|(name, digest)| {
            Sha256Digest::parse(digest).is_ok()
                && match name.split_once('/') {
                    Some((domain, path)) if is_domain(domain) && is_path(path) => true,
                    _ => is_path(name),
                }
        });
        if pinned {
            Ok(())
        } else {
            Err(IdentifierError::new(
                value,
                "must be an image name followed by @sha256: and 64 lowercase hexadecimal digits",
            ))
        }
    });
}

/// `domain := host [':' port]`, where `host` is dot-separated labels of letters, digits
/// and inner hyphens.
fn is_domain(value: &str) -> bool {
    let (host, port) = match value.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (value, None),
    };
    port.is_none_or(|port| !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()))
        && host.split('.').all(|label| {
            let label = label.as_bytes();
            label.first().is_some_and(u8::is_ascii_alphanumeric)
                && label.last().is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        })
}

/// `path := path-component ['/' path-component]*`.
fn is_path(value: &str) -> bool {
    value.split('/').all(is_path_component)
}

/// `path-component := [a-z0-9]+ (separator [a-z0-9]+)*`, `separator := [_.] | __ | [-]+`.
fn is_path_component(value: &str) -> bool {
    let alphanumeric = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    let bytes = value.as_bytes();
    bytes.first().is_some_and(alphanumeric)
        && bytes.last().is_some_and(alphanumeric)
        && value
            .split(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            .all(|separator| {
                matches!(separator, "" | "." | "_" | "__")
                    || separator.bytes().all(|byte| byte == b'-')
            })
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use anyhow::{Context, ensure};

    use super::*;

    const DIGEST: &str = "sha256:8dad0d15745459ae06213192eba0680a68cbb02e71757d16a739f3f9e34c71b4";
    const REPOSITORY: &str = "0194b726b34e72b0b45550b88a967076";

    fn builder() -> ImageBuilder {
        serde_json::from_value(serde_json::json!({
            "buildkit": "tcp://buildkitd.veoveo-system.svc.cluster.local:1234",
            "registry": "registry.veoveo-system.svc.cluster.local:5000",
        }))
        .unwrap()
    }

    #[test]
    fn buildctl_builds_the_context_alone_and_pushes_it_under_factory() {
        let builder = builder();
        let repository = builder.repository(&ImageName::parse("time-dev").unwrap());
        let cache = CacheNamespace::of(REPOSITORY, &BranchName::parse("dev").unwrap());
        let namespace = format!("build-arg:BUILDKIT_CACHE_MOUNT_NS={}", cache.as_str());
        let arguments = builder.arguments(
            Path::new("/tmp/checkout/openshell/image"),
            &repository,
            &cache,
            Path::new("/tmp/build/metadata.json"),
        );
        assert_eq!(
            arguments,
            [
                "--addr",
                "tcp://buildkitd.veoveo-system.svc.cluster.local:1234",
                "build",
                "--progress",
                "plain",
                "--frontend",
                "dockerfile.v0",
                "--opt",
                namespace.as_str(),
                "--local",
                "context=/tmp/checkout/openshell/image",
                "--local",
                "dockerfile=/tmp/checkout/openshell/image",
                "--output",
                "type=image,name=registry.veoveo-system.svc.cluster.local:5000/factory/time-dev,push=true",
                "--metadata-file",
                "/tmp/build/metadata.json",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn every_build_prunes_only_cache_mounts() {
        assert_eq!(
            builder().prune_arguments(),
            [
                "--addr",
                "tcp://buildkitd.veoveo-system.svc.cluster.local:1234",
                "prune",
                "--all",
                "--filter",
                "type==exec.cachemount",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn each_branch_has_its_own_cache_namespace_of_one_length() {
        let branch = |name: &str| BranchName::parse(name).unwrap();
        let dev = CacheNamespace::of(REPOSITORY, &branch("dev"));
        assert_eq!(dev, CacheNamespace::of(REPOSITORY, &branch("dev")));
        let others = [
            CacheNamespace::of(REPOSITORY, &branch("main")),
            CacheNamespace::of(REPOSITORY, &branch("dev2")),
            CacheNamespace::of("0194b726b34e72b0b45550b88a967077", &branch("dev")),
        ];
        for namespace in others.iter().chain([&dev]) {
            assert_eq!(namespace.as_str().len(), 64);
            assert!(
                namespace
                    .as_str()
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            );
        }
        for other in &others {
            assert_ne!(&dev, other);
        }
        // SHA-256 of the repository ID, a zero byte and the branch name.
        assert_eq!(
            CacheNamespace::of("r", &branch("b")).as_str(),
            hex::encode(Sha256::digest(b"r\0b"))
        );
    }

    #[test]
    fn the_configuration_names_buildkit_and_the_registry_only() {
        let mut configuration = serde_json::to_value(builder()).unwrap();
        assert_eq!(
            configuration,
            serde_json::json!({
                "buildkit": "tcp://buildkitd.veoveo-system.svc.cluster.local:1234",
                "registry": "registry.veoveo-system.svc.cluster.local:5000",
            })
        );
        // No build context is configured: environment images copy no platform tool.
        configuration["lore"] =
            format!("registry.veoveo-system.svc.cluster.local:5000/lore@{DIGEST}").into();
        assert!(serde_json::from_value::<ImageBuilder>(configuration.clone()).is_err());
        configuration.as_object_mut().unwrap().remove("lore");
        configuration["insecure"] = true.into();
        assert!(serde_json::from_value::<ImageBuilder>(configuration).is_err());
    }

    #[test]
    fn the_digest_is_read_from_buildctl_metadata() {
        // Written by buildctl v0.34.1 for an image it pushed.
        let metadata = r#"{
  "containerimage.config.digest": "sha256:6c0e153064f5d496e7162b8e846f7d25f87fc950462f8d535fb9997726d8d371",
  "containerimage.descriptor": {
    "mediaType": "application/vnd.oci.image.manifest.v1+json",
    "digest": "sha256:0603da33cbeb3a2e0ba47d24912bff917f3a9f4289802ef5f6fa5b1164cb1485",
    "size": 481,
    "platform": {
      "architecture": "amd64",
      "os": "linux"
    }
  },
  "containerimage.digest": "sha256:0603da33cbeb3a2e0ba47d24912bff917f3a9f4289802ef5f6fa5b1164cb1485",
  "image.name": "registry.veoveo-probe.svc.cluster.local:5000/lore"
}"#;
        assert_eq!(
            image_digest(metadata.as_bytes()).unwrap(),
            Sha256Digest::parse(
                "sha256:0603da33cbeb3a2e0ba47d24912bff917f3a9f4289802ef5f6fa5b1164cb1485"
            )
            .unwrap()
        );
        for metadata in [
            r#"{"image.name": "registry/factory/time-dev"}"#,
            r#"{"containerimage.digest": "sha256:8DAD"}"#,
            "",
        ] {
            assert!(matches!(
                image_digest(metadata.as_bytes()),
                Err(ImageBuildError::Metadata(_))
            ));
        }
    }

    #[test]
    fn names_follow_the_distribution_reference_grammar() {
        for valid in [
            "registry.veoveo-system.svc.cluster.local:5000",
            "localhost",
            "10.43.200.10:5000",
            "Registry-1.example",
        ] {
            assert!(RegistryHost::parse(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "registry:", "registry:50a", "-registry", "a..b", "a/b"] {
            assert!(RegistryHost::parse(invalid).is_err(), "{invalid}");
        }
        for valid in ["time-dev", "time/dev--jose_x", "a.b__c", "0"] {
            assert!(ImageName::parse(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "", "Time", "time/", "/time", "-time", "time-", "a___b", "a._b", "a:b", "a,b",
        ] {
            assert!(ImageName::parse(invalid).is_err(), "{invalid}");
        }
        let pinned = PinnedImage::parse(format!(
            "registry.veoveo-system.svc.cluster.local:5000/factory/time-dev@{DIGEST}"
        ))
        .unwrap();
        assert_eq!(
            pinned.repository(),
            "registry.veoveo-system.svc.cluster.local:5000/factory/time-dev"
        );
        assert_eq!(pinned.digest(), Sha256Digest::parse(DIGEST).unwrap());
        assert!(PinnedImage::parse(format!("veoveo/lore@{DIGEST}")).is_ok());
        for invalid in [
            "veoveo/lore".to_owned(),
            format!("veoveo/lore:v1@{DIGEST}"),
            format!("localhost:5000@{DIGEST}"),
            format!("veoveo/lore@{}", DIGEST.to_uppercase()),
            "veoveo/lore@sha256:8dad".to_owned(),
        ] {
            assert!(PinnedImage::parse(&invalid).is_err(), "{invalid}");
        }
    }

    #[tokio::test]
    async fn a_failure_keeps_the_end_of_the_output_from_a_line_start() {
        let short = "#1 [internal] load build definition\nerror: failed to solve\n";
        assert_eq!(
            tail(short.as_bytes(), 1024).await.unwrap(),
            short.trim_end()
        );
        let long: String = (0..10_000).map(|step| format!("#{step} DONE\n")).collect();
        let kept = tail(long.as_bytes(), 1024).await.unwrap();
        assert!(kept.len() <= 1024);
        assert!(kept.starts_with('#'), "{kept}");
        assert!(kept.ends_with("#9999 DONE"), "{kept}");
        let unbroken = "x".repeat(5000);
        assert_eq!(tail(unbroken.as_bytes(), 1024).await.unwrap().len(), 1024);
    }

    /// A deployed check: builds an environment image twice against a running BuildKit
    /// and its registry, then a `Dockerfile` BuildKit refuses, and finds no cache mount
    /// left after each build. The file named by `VEOVEO_IMAGE_BUILDER_TEST_CONFIG` holds
    /// `{"builder": …, "context": …, "name": …}`.
    #[tokio::test]
    #[ignore = "requires a BuildKit daemon and its registry"]
    async fn an_unchanged_image_builds_again_with_the_same_digest() -> anyhow::Result<()> {
        #[derive(Deserialize)]
        struct Probe {
            builder: ImageBuilder,
            context: std::path::PathBuf,
            name: ImageName,
        }
        let path = std::env::var("VEOVEO_IMAGE_BUILDER_TEST_CONFIG")?;
        let probe: Probe = serde_json::from_slice(&std::fs::read(path)?)?;
        let cache = CacheNamespace::of(REPOSITORY, &BranchName::parse("probe")?);
        let cache_mounts = || async {
            let listed = Command::new(BUILDCTL)
                .args(["--addr", probe.builder.buildkit.as_str(), "du", "--filter"])
                .arg("type==exec.cachemount")
                .args(["--format", "{{json .}}"])
                .output()
                .await?;
            ensure!(listed.status.success(), "buildctl du failed");
            // buildctl writes the records as one JSON array, or `null` when there are none.
            let records: Option<Vec<serde_json::Value>> = serde_json::from_slice(&listed.stdout)?;
            anyhow::Ok(records.map_or(0, |records| records.len()))
        };
        let started = Instant::now();
        let first = probe
            .builder
            .build(&probe.context, &probe.name, &cache)
            .await?;
        let first_took = started.elapsed();
        ensure!(cache_mounts().await? == 0, "a build leaves no cache mount");
        let started = Instant::now();
        let second = probe
            .builder
            .build(&probe.context, &probe.name, &cache)
            .await?;
        let second_took = started.elapsed();
        eprintln!("first build {first} in {first_took:?}");
        eprintln!("second build {second} in {second_took:?}");
        ensure!(first == second, "an unchanged image keeps its digest");
        let refused = tempfile::tempdir()?;
        std::fs::write(
            refused.path().join("Dockerfile"),
            "FROM scratch\nNOT_AN_INSTRUCTION here\n",
        )?;
        let started = Instant::now();
        let error = probe
            .builder
            .build(refused.path(), &probe.name, &cache)
            .await
            .err()
            .context("BuildKit refuses an unknown instruction")?;
        eprintln!("refused in {:?}: {error}", started.elapsed());
        ensure!(matches!(error, ImageBuildError::Failed { .. }));
        ensure!(error.to_string().contains("unknown instruction"), "{error}");
        ensure!(
            cache_mounts().await? == 0,
            "a failed build leaves no cache mount"
        );
        Ok(())
    }
}
