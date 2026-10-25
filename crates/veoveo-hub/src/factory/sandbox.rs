//! The OpenShell sandbox of one instance: its name, providers, policy, size and driver
//! configuration and the fixed start command, as the Hub creates it.
use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};
use openshell_sdk::raw::proto as p;
use prost_types::{Struct, Value, value::Kind};
use veoveo_types::DeploymentId;

use super::Source;
use crate::{control_plane::FactoryConfiguration, execution::BranchName};

/// The name of the instance's sandbox: the last two groups of its deployment ID, which
/// hold the UUID's random bits. OpenShell routes sandbox names of at most 19 bytes.
pub fn sandbox_name(deployment: DeploymentId) -> String {
    let id = deployment.as_uuid().to_string();
    id[id.len() - 17..].to_owned()
}

/// The provider that carries the instance's registration token into its sandbox.
pub(super) fn registration_provider(sandbox: &str) -> String {
    format!("{sandbox}-registration")
}

/// The provider that carries the instance's Lore access into its sandbox: a token the
/// gateway refreshes with the instance's own client, for the instance's latest pusher.
pub(super) fn lore_provider(sandbox: &str) -> String {
    format!("{sandbox}-lore")
}

/// The providers the instance's sandbox attaches: its registration provider and its Lore
/// provider.
pub(super) fn providers(sandbox: &str) -> Vec<String> {
    vec![registration_provider(sandbox), lore_provider(sandbox)]
}

/// The server setting the registration profile carries the instance's token in.
pub(super) const REGISTRATION_SECRET: &str = "VEOVEO_REGISTRATION_SECRET";

/// The variable the Lore profile carries the placeholder of the instance's Lore token
/// in, which Lore's command line reads as its access token. The gateway refreshes the
/// credential of this key.
pub(super) const LORE_TOKEN: &str = "LORE_ACCESS_TOKEN";

/// Lore's command line, from the installation's Lore image, which the Hub's sandbox and
/// every factory sandbox mount read-only at `/opt/veoveo/lore`. The path is the
/// installation's contract; no image the Hub builds or runs carries Lore.
pub const LORE: &str = "/opt/veoveo/lore/usr/local/bin/lore";

/// The variable Lore's command line reads its identity from. Lore requires an identity
/// with a token it cannot read, such as the provider's placeholder. That identity only
/// names the checkout's local user, and the Lore server authorizes the token alone, so
/// the instance names itself.
pub(super) const LORE_IDENTITY: &str = "LORE_IDENTITY";

/// The fixed start command, run on every start of the sandbox. The first start clones
/// the branch onto the sandbox's persistent disk; every start then resets the checkout
/// to the branch's latest revision, keeping what `.loreignore` lists, such as build
/// output, and replaces itself with the branch's own program, which builds and serves
/// it. `$1` is the repository and `$2` the branch.
///
/// Lore runs from [`LORE`] with the Lore provider's placeholder as its access token, from
/// the sandbox's environment, which the gateway replaces with the real token in Lore's
/// requests.
fn start_command(repository: &url::Url, branch: &BranchName) -> Vec<String> {
    let script = format!(
        r#"set -e
[ -d /sandbox/src/.lore ] || {LORE} clone "$1" /sandbox/src --branch "$2"
cd /sandbox/src
{LORE} sync --reset "$2@LATEST"
exec ./openshell/run"#
    );
    vec![
        "/bin/sh".into(),
        "-c".into(),
        script,
        "factory".into(),
        repository.to_string(),
        branch.to_string(),
    ]
}

/// What the installation's factory template gives every instance's sandbox: its size
/// as Kubernetes limits and its driver configuration, as OpenShell records them in the
/// sandbox's spec. The branch gives the image and the Hub the environment.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Shape {
    resources: Option<Struct>,
    driver_config: Option<Struct>,
}

impl Shape {
    /// The shape of the factory template, which sets only CPU, memory and driver
    /// configuration. OpenShell refuses a create that names a template and sets its own
    /// image, so the Hub creates every sandbox inline from the template's parts.
    pub(super) fn of(template: &p::SandboxWorkloadTemplate) -> Result<Self> {
        let spec = template.spec.as_ref().context("it has no spec")?;
        let workload = spec.workload.clone().unwrap_or_default();
        let resources = workload.resources.unwrap_or_default();
        ensure!(
            workload.image.is_empty() && workload.environment.is_empty() && resources.gpu.is_none(),
            "it may set only CPU, memory and driver configuration: the Hub gives each instance its \
             image and environment, and a GPU is not available to instances"
        );
        Ok(Self {
            resources: limits(&resources.cpu, &resources.memory),
            driver_config: spec.driver_config.clone(),
        })
    }
}

/// The URL the Hub reaches the instance's server at, through native forwarding to the
/// sandbox's loopback port.
pub(super) fn public_base_url(sandbox: &str, port: u16) -> String {
    format!("http://{sandbox}.internal:{port}")
}

/// The base policy with a repository's own network rules added. A repository adds rules
/// under names of its own and changes nothing else.
pub(super) fn policy(
    base: &p::sandbox::v1::SandboxPolicy,
    repository: Option<&str>,
) -> Result<p::sandbox::v1::SandboxPolicy> {
    let mut policy = base.clone();
    if let Some(repository) = repository {
        let own = openshell_policy::parse_sandbox_policy(repository).map_err(|error| {
            anyhow::anyhow!("the repository's openshell/policy.yaml is invalid: {error}")
        })?;
        ensure!(
            own.filesystem.is_none()
                && own.landlock.is_none()
                && own.process.is_none()
                && own.network_middlewares.is_empty(),
            "the repository's openshell/policy.yaml may only add network policies"
        );
        for (name, rule) in own.network_policies {
            ensure!(
                !policy.network_policies.contains_key(&name),
                "the repository's openshell/policy.yaml redefines the installation's network policy `{name}`"
            );
            policy.network_policies.insert(name, rule);
        }
    }
    openshell_policy::validate_and_canonicalize_sandbox_policy(policy)
        .map_err(|error| anyhow::anyhow!("the factory sandbox policy is invalid: {error}"))
}

/// The create request of the instance's sandbox.
pub(super) fn create_request(
    config: &FactoryConfiguration,
    deployment: DeploymentId,
    branch: &BranchName,
    source: &Source,
) -> p::CreateSandboxRequest {
    let name = sandbox_name(deployment);
    p::CreateSandboxRequest {
        workspace_scope: super::scope(&config.workspace),
        name: name.clone(),
        spec: Some(p::SandboxSpec {
            environment: HashMap::from([
                ("VEOVEO_DEPLOYMENT_ID".into(), deployment.to_string()),
                (LORE_IDENTITY.into(), deployment.to_string()),
                (
                    "VEOVEO_PUBLIC_BASE_URL".into(),
                    public_base_url(&name, config.port),
                ),
                ("VEOVEO_PORT".into(), config.port.to_string()),
                ("VEOVEO_HUB_ISSUER".into(), config.hub_issuer.to_string()),
            ]),
            template: Some(p::SandboxTemplate {
                image: source.image.as_str().into(),
                resources: source.shape.resources.clone(),
                driver_config: source.shape.driver_config.clone(),
                ..Default::default()
            }),
            policy: Some(source.policy.clone()),
            providers: providers(&name),
            command: start_command(&source.remote, branch),
            restart_policy: p::SandboxRestartPolicy::Always as i32,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Whether the sandbox runs this source: its image, under its policy, with its shape, as
/// OpenShell recorded its create request. OpenShell records a created sandbox's policy in
/// its canonical form, which [`policy`] returns, so an unchanged policy compares equal,
/// and its size and driver configuration as they were sent.
pub(super) fn runs(sandbox: &p::Sandbox, source: &Source) -> bool {
    sandbox.spec.as_ref().is_some_and(|spec| {
        spec.template.as_ref().is_some_and(|template| {
            template.image == source.image.as_str()
                && template.resources == source.shape.resources
                && template.driver_config == source.shape.driver_config
        }) && spec.policy.as_ref() == Some(&source.policy)
    })
}

/// Kubernetes resource limits, which OpenShell's Kubernetes driver applies to the
/// sandbox's workload container, as OpenShell makes them of a template's CPU and memory:
/// the quantities it names, or none.
fn limits(cpu: &str, memory: &str) -> Option<Struct> {
    let fields = [("cpu", cpu), ("memory", memory)]
        .into_iter()
        .filter(|(_, quantity)| !quantity.is_empty())
        .map(|(name, quantity)| {
            (
                name.to_owned(),
                Value {
                    kind: Some(Kind::StringValue(quantity.into())),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    (!fields.is_empty()).then(|| Struct {
        fields: BTreeMap::from([(
            "limits".into(),
            Value {
                kind: Some(Kind::StructValue(Struct { fields })),
            },
        )]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_names_are_routable_dns_labels_from_the_deployment() {
        let deployment = DeploymentId::new();
        let name = sandbox_name(deployment);
        assert_eq!(name.len(), 17);
        assert!(deployment.to_string().ends_with(&name));
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        );
        assert!(!name.starts_with('-') && !name.contains("--"));
        assert_eq!(sandbox_name(deployment), name);
    }

    #[test]
    fn the_start_command_passes_repository_and_branch_as_arguments() {
        let repository = url::Url::parse("grpcs://lore.internal:41337/repository").unwrap();
        let branch = BranchName::parse("dev/jose/it's").unwrap();
        let command = start_command(&repository, &branch);
        assert_eq!(command[..2], ["/bin/sh", "-c"]);
        assert_eq!(
            command[2],
            "set -e\n\
             [ -d /sandbox/src/.lore ] || /opt/veoveo/lore/usr/local/bin/lore clone \"$1\" /sandbox/src --branch \"$2\"\n\
             cd /sandbox/src\n\
             /opt/veoveo/lore/usr/local/bin/lore sync --reset \"$2@LATEST\"\n\
             exec ./openshell/run"
        );
        assert_eq!(
            command[3..],
            [
                "factory",
                "grpcs://lore.internal:41337/repository",
                "dev/jose/it's"
            ]
        );
    }

    #[test]
    fn the_factory_template_gives_size_and_driver_configuration_only() {
        let driver_config = Struct {
            fields: BTreeMap::from([(
                "kubernetes".into(),
                Value {
                    kind: Some(Kind::StructValue(Struct::default())),
                },
            )]),
        };
        let template = |workload: p::SandboxWorkloadConfig| p::SandboxWorkloadTemplate {
            spec: Some(p::SandboxWorkloadTemplateSpec {
                workload: Some(workload),
                driver_config: Some(driver_config.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let sized = |cpu: &str, memory: &str| p::SandboxWorkloadConfig {
            resources: Some(p::SandboxResources {
                cpu: cpu.into(),
                memory: memory.into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let shape = Shape::of(&template(sized("4", "8Gi"))).unwrap();
        assert_eq!(shape.resources, limits("4", "8Gi"));
        assert_eq!(shape.driver_config.as_ref(), Some(&driver_config));
        let text = |value: &str| Value {
            kind: Some(Kind::StringValue(value.into())),
        };
        assert_eq!(
            limits("500m", ""),
            Some(Struct {
                fields: BTreeMap::from([(
                    "limits".into(),
                    Value {
                        kind: Some(Kind::StructValue(Struct {
                            fields: BTreeMap::from([("cpu".into(), text("500m"))]),
                        })),
                    },
                )]),
            })
        );
        assert_eq!(
            Shape::of(&template(Default::default())).unwrap().resources,
            None
        );
        for refused in [
            p::SandboxWorkloadConfig {
                image: "registry.internal:5000/factory-base@sha256:0".into(),
                ..sized("4", "8Gi")
            },
            p::SandboxWorkloadConfig {
                environment: HashMap::from([("A".into(), "b".into())]),
                ..sized("4", "8Gi")
            },
            p::SandboxWorkloadConfig {
                resources: Some(p::SandboxResources {
                    gpu: Some(Default::default()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ] {
            assert!(Shape::of(&template(refused)).is_err());
        }
        assert!(Shape::of(&p::SandboxWorkloadTemplate::default()).is_err());
    }

    #[test]
    fn a_repository_policy_only_adds_network_rules_of_its_own() {
        let base = openshell_policy::parse_sandbox_policy(
            "version: 1\nnetwork_policies:\n  internet:\n    endpoints:\n      - {host: \"**.*.*\", ports: [443]}\n    binaries:\n      - path: /**\n",
        )
        .unwrap();
        let added = policy(
            &base,
            Some("version: 1\nnetwork_policies:\n  packages:\n    endpoints:\n      - {host: packages.example, port: 443}\n    binaries:\n      - path: /**\n"),
        )
        .unwrap();
        let mut names = added.network_policies.keys().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["internet", "packages"]);
        // OpenShell records a created sandbox's policy canonicalized again, which leaves
        // it unchanged, so a push with the same policy compares equal.
        assert_eq!(
            openshell_policy::validate_and_canonicalize_sandbox_policy(added.clone()).unwrap(),
            added
        );
        assert!(
            policy(
                &base,
                Some("version: 1\nnetwork_policies:\n  internet:\n    endpoints:\n      - {host: packages.example, port: 443}\n    binaries:\n      - path: /**\n"),
            )
            .is_err()
        );
        assert!(
            policy(
                &base,
                Some("version: 1\nfilesystem_policy:\n  read_write: [/]\n")
            )
            .is_err()
        );
        assert_eq!(
            policy(&base, None).unwrap(),
            openshell_policy::validate_and_canonicalize_sandbox_policy(base).unwrap()
        );
    }
}
