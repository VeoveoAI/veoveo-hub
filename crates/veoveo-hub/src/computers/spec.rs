//! Durable admitted per-Computer defaults. Native policy/template use protobuf,
//! not a second workload schema. This contains only secret locators, never grants.
use crate::{contract::SecretReference, control_plane::Computers};
use anyhow::{Context, Result, ensure};
use openshell_sdk::raw::proto as p;
use prost::Message;
use prost_types::{Struct, value::Kind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PersistenceBackend {
    Docker,
    Kubernetes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerProvisionSpec {
    pub(crate) config: Computers,
    pub(crate) model_secret: SecretReference,
    pub(crate) hub_url: String,
    pub(crate) backend: PersistenceBackend,
    policy: Vec<u8>,
    template: Vec<u8>,
}
impl ComputerProvisionSpec {
    pub(crate) fn new(
        config: Computers,
        model_secret: SecretReference,
        hub_url: String,
        backend: PersistenceBackend,
        policy: &p::sandbox::v1::SandboxPolicy,
        template: &p::SandboxWorkloadTemplate,
    ) -> Self {
        Self {
            config,
            model_secret,
            hub_url,
            backend,
            policy: policy.encode_to_vec(),
            template: template.encode_to_vec(),
        }
    }
    pub(crate) fn policy(&self) -> Result<p::sandbox::v1::SandboxPolicy> {
        p::sandbox::v1::SandboxPolicy::decode(self.policy.as_slice())
            .context("stored Computer policy is invalid")
    }
    pub(crate) fn template(&self) -> Result<p::SandboxWorkloadTemplate> {
        p::SandboxWorkloadTemplate::decode(self.template.as_slice())
            .context("stored Computer template is invalid")
    }
    #[cfg(test)]
    pub(crate) fn fixture(config: Computers) -> Self {
        Self::new(
            config.clone(),
            crate::contract::SecretReference {
                id: config.model_provider.secret,
                source: crate::contract::SecretSource::Env,
                purpose: crate::contract::SecretPurpose::ProviderApiKey,
                locator: crate::contract::SecretLocator::parse("UNUSED_FIXTURE_KEY").unwrap(),
                owner: crate::contract::SecretOwner::Hub,
                rotation_hint: None,
                metadata: serde_json::Value::Null,
            },
            "https://hub.example/mcp/fixture".into(),
            PersistenceBackend::Docker,
            &Default::default(),
            &Default::default(),
        )
    }
}
fn structure<'a>(parent: &'a Struct, key: &str) -> Result<Option<&'a Struct>> {
    match parent.fields.get(key).map(|v| &v.kind) {
        None => Ok(None),
        Some(Some(Kind::StructValue(value))) => Ok(Some(value)),
        _ => anyhow::bail!("native Computer persistence field {key} must be an object"),
    }
}
fn string<'a>(parent: &'a Struct, key: &str) -> Option<&'a str> {
    match parent.fields.get(key).and_then(|v| v.kind.as_ref()) {
        Some(Kind::StringValue(value)) => Some(value),
        _ => None,
    }
}
/// Validate the selected native backend, not an unrelated driver-config key.
/// Reusable Computer templates must keep Kubernetes's automatic per-sandbox
/// /sandbox PVC. A static explicit claim could share owner working data.
pub(crate) fn persistence(
    driver: &p::ComputeDriverInfo,
    template: &p::SandboxWorkloadTemplate,
) -> Result<PersistenceBackend> {
    let capabilities = driver
        .capabilities
        .as_ref()
        .context("Computer driver capabilities missing")?;
    let config = template
        .spec
        .as_ref()
        .context("native template has no spec")?
        .driver_config
        .as_ref();
    match capabilities.driver_name.as_str() {
        "docker" => {
            let docker = config
                .map(|config| structure(config, &driver.name))
                .transpose()?
                .flatten();
            ensure!(
                docker
                    .and_then(|v| v.fields.get("persistent_workspace"))
                    .is_some_and(|v| matches!(v.kind, Some(Kind::BoolValue(true)))),
                "Computer requires native Docker persistent workspace storage"
            );
            Ok(PersistenceBackend::Docker)
        }
        "kubernetes" => {
            let kube = config
                .map(|config| structure(config, &driver.name))
                .transpose()?
                .flatten();
            if let Some(kube) = kube {
                let agent = structure(kube, "containers")?
                    .map(|v| structure(v, "agent"))
                    .transpose()?
                    .flatten();
                if let Some(value) = agent.and_then(|v| v.fields.get("volume_mounts")) {
                    let Some(Kind::ListValue(mounts)) = value.kind.as_ref() else {
                        anyhow::bail!("Computer volume mounts must be an array")
                    };
                    for mount in &mounts.values {
                        let Some(Kind::StructValue(mount)) = mount.kind.as_ref() else {
                            anyhow::bail!("Computer volume mount must be an object")
                        };
                        let path =
                            string(mount, "mount_path").context("Computer mount path missing")?;
                        ensure!(
                            !std::path::Path::new(path).starts_with("/sandbox"),
                            "Computer requires native per-sandbox workspace PVC; explicit workspace mounts are not reusable owner defaults"
                        );
                    }
                }
            }
            Ok(PersistenceBackend::Kubernetes)
        }
        _ => anyhow::bail!("Computer persistence is unsupported for selected native driver"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn template(json: serde_json::Value) -> p::SandboxWorkloadTemplate {
        fn value(v: serde_json::Value) -> prost_types::Value {
            prost_types::Value {
                kind: Some(match v {
                    serde_json::Value::Object(map) => Kind::StructValue(Struct {
                        fields: map.into_iter().map(|(k, v)| (k, value(v))).collect(),
                    }),
                    serde_json::Value::Array(items) => Kind::ListValue(prost_types::ListValue {
                        values: items.into_iter().map(value).collect(),
                    }),
                    serde_json::Value::Bool(v) => Kind::BoolValue(v),
                    serde_json::Value::String(v) => Kind::StringValue(v),
                    _ => panic!("fixture value type"),
                }),
            }
        }
        let Some(Kind::StructValue(config)) = value(json).kind else {
            unreachable!()
        };
        p::SandboxWorkloadTemplate {
            spec: Some(p::SandboxWorkloadTemplateSpec {
                driver_config: Some(config),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn driver(name: &str) -> p::ComputeDriverInfo {
        p::ComputeDriverInfo {
            name: name.into(),
            capabilities: Some(p::ComputeDriverCapabilities {
                driver_name: name.into(),
                ..Default::default()
            }),
        }
    }
    #[test]
    fn selected_backend_requires_actual_whole_workspace_persistence() {
        let empty = template(serde_json::json!({}));
        assert_eq!(
            persistence(&driver("kubernetes"), &empty).unwrap(),
            PersistenceBackend::Kubernetes
        );
        assert!(persistence(&driver("docker"), &empty).is_err());
        let docker = template(serde_json::json!({"docker":{"persistent_workspace":true}}));
        assert_eq!(
            persistence(&driver("docker"), &docker).unwrap(),
            PersistenceBackend::Docker
        );
        assert!(
            persistence(&driver("unknown"), &docker).is_err(),
            "an unrelated docker key is no proof for an unknown backend"
        );
        let mut config = serde_json::json!({"kubernetes":{
            "volumes":[{"name":"work", "persistent_volume_claim":{"claim_name":"fixture", "read_only":false}}],
            "containers":{"agent":{"volume_mounts":[{"name":"work", "mount_path":"/sandbox", "read_only":false}]}}
        }});
        assert!(
            persistence(&driver("kubernetes"), &template(config.clone())).is_err(),
            "a fixed whole-workspace claim in a reusable template could share private owner data"
        );
        config["kubernetes"]["containers"]["agent"]["volume_mounts"][0]["mount_path"] =
            "/sandbox/home".into();
        assert!(
            persistence(&driver("kubernetes"), &template(config.clone())).is_err(),
            "a child mount disables the default whole workspace PVC"
        );
        config["kubernetes"]["containers"]["agent"]["volume_mounts"][0]["mount_path"] =
            "/etc/fixture".into();
        config["kubernetes"]["containers"]["agent"]["volume_mounts"][0]["read_only"] = true.into();
        config["kubernetes"]["volumes"][0]["persistent_volume_claim"]["read_only"] = true.into();
        assert_eq!(
            persistence(&driver("kubernetes"), &template(config)).unwrap(),
            PersistenceBackend::Kubernetes,
            "read-only configuration PVCs outside the workspace retain native automatic persistence"
        );
    }
}
