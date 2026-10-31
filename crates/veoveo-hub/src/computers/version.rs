//! A Computer version: everything a new sandbox gets from the installation, the
//! template, the policy, the Hub's Computer settings and the gateway's supervisor and
//! runtime images, named by a short digest; and what changed between two versions.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use openshell_sdk::raw::proto as p;
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ComputerProvisionSpec;

/// The template annotation that holds a version's release note, written by whoever
/// publishes the template. Its first line is the version's summary.
pub const RELEASE_NOTE: &str = "veoveo.ai/release-note";
/// The domain of a version's digest: its format's URI and a NUL byte.
const DOMAIN: &[u8] = b"https://veoveo.ai/formats/computer-version\0";
/// Hexadecimal digits of the digest that name a version.
const LENGTH: usize = 12;

/// The first twelve hexadecimal digits of the SHA-256 of a version's canonical
/// document: the template's spec, the canonical policy, the Hub's Computer settings
/// that reach a sandbox and the supervisor and runtime images the gateway reports.
/// The same inputs name the same version whatever the template's name, its object
/// identity or the policy file's path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComputerVersion(String);
impl ComputerVersion {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for ComputerVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A version and the summary of its release note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub version: ComputerVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// What a version brings over the one the Computer runs: its release note when that
/// differs from the other's, and the mechanical differences whether or not it has one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsNew {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageChange>,
    /// The supervisor image every new sandbox runs, when the gateway reports another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor: Option<ImageChange>,
    /// The sandbox runtime image every new sandbox runs, when the gateway reports another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_runtime: Option<ImageChange>,
    /// Endpoints the newer version's network rules allow and the older one's do not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_rules_added: Vec<NetworkRule>,
    /// Endpoints the older version's network rules allow and the newer one's do not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_rules_removed: Vec<NetworkRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageChange {
    pub from: String,
    pub to: String,
}

/// A network rule by its name and the endpoints, `host:port` and any path, it gained
/// or lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRule {
    pub name: String,
    pub endpoints: Vec<String>,
}

/// The cause of a Computer's last failed replacement, recorded with it until the next
/// one begins. The Computer keeps the version it had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerFailure {
    pub cause: String,
    pub at: DateTime<Utc>,
    /// The version the replacement was creating.
    pub version: ComputerVersion,
}

/// A completed update the owner has not been shown yet: by a status read or by the
/// start page of the creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateNotice {
    pub from: ComputerVersion,
    pub to: ComputerVersion,
    pub whats_new: WhatsNew,
}

impl ComputerProvisionSpec {
    /// This spec's version.
    pub fn version(&self) -> Result<ComputerVersion> {
        let document = canonical(self)?;
        let bytes = serde_json::to_vec(&document)?;
        let mut digest = Sha256::new();
        digest.update(DOMAIN);
        digest.update(&bytes);
        Ok(ComputerVersion(
            hex::encode(digest.finalize())[..LENGTH].to_owned(),
        ))
    }
    /// The version and the summary of its release note.
    pub fn info(&self) -> Result<VersionInfo> {
        Ok(VersionInfo {
            version: self.version()?,
            summary: self.summary(),
        })
    }
    /// The release note its template carries, if any.
    pub fn release_note(&self) -> Option<String> {
        self.template()
            .ok()?
            .metadata?
            .annotations
            .get(RELEASE_NOTE)
            .map(|note| note.trim().to_owned())
            .filter(|note| !note.is_empty())
    }
    /// The first line of the release note.
    pub fn summary(&self) -> Option<String> {
        self.release_note()?
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_owned)
    }
    pub(crate) fn image(&self) -> Option<String> {
        self.template()
            .ok()?
            .spec?
            .workload
            .map(|workload| workload.image)
            .filter(|image| !image.is_empty())
    }
}

/// What `to` brings over `from`, the version the Computer runs. `to`'s note is listed
/// unless `from` has the same one, as a version changed only through the policy or the
/// Hub's Computer settings does; it is listed when `from` has none or its template
/// cannot be read.
pub fn whats_new(from: &ComputerProvisionSpec, to: &ComputerProvisionSpec) -> Result<WhatsNew> {
    let note = to
        .release_note()
        .filter(|note| from.release_note().as_ref() != Some(note));
    let image = match (from.image(), to.image()) {
        (Some(from), Some(to)) if from != to => Some(ImageChange { from, to }),
        _ => None,
    };
    // An older version recorded before the gateway reported its images names none.
    let changed = |from: &str, to: &str| {
        (!to.is_empty() && from != to).then(|| ImageChange {
            from: from.to_owned(),
            to: to.to_owned(),
        })
    };
    let supervisor = changed(&from.runtime.supervisor_image, &to.runtime.supervisor_image);
    let sandbox_runtime = changed(
        &from.runtime.sandbox_runtime_image,
        &to.runtime.sandbox_runtime_image,
    );
    let before = endpoints(&from.policy()?);
    let after = endpoints(&to.policy()?);
    Ok(WhatsNew {
        note,
        image,
        supervisor,
        sandbox_runtime,
        network_rules_added: rules(after.difference(&before)),
        network_rules_removed: rules(before.difference(&after)),
    })
}

/// Every endpoint a policy's network rules allow, by rule name.
fn endpoints(policy: &p::sandbox::v1::SandboxPolicy) -> BTreeSet<(String, String)> {
    let mut endpoints = BTreeSet::new();
    for (key, rule) in &policy.network_policies {
        let name = if rule.name.is_empty() {
            key.clone()
        } else {
            rule.name.clone()
        };
        for endpoint in &rule.endpoints {
            let host = if endpoint.host.is_empty() {
                endpoint.allowed_ips.join(",")
            } else {
                endpoint.host.clone()
            };
            let ports = if endpoint.ports.is_empty() {
                vec![endpoint.port]
            } else {
                endpoint.ports.clone()
            };
            for port in ports {
                endpoints.insert((name.clone(), format!("{host}:{port}{}", endpoint.path)));
            }
        }
    }
    endpoints
}

fn rules<'a>(pairs: impl Iterator<Item = &'a (String, String)>) -> Vec<NetworkRule> {
    let mut rules = BTreeMap::<&str, Vec<String>>::new();
    for (rule, endpoint) in pairs {
        rules.entry(rule).or_default().push(endpoint.clone());
    }
    rules
        .into_iter()
        .map(|(name, endpoints)| NetworkRule {
            name: name.to_owned(),
            endpoints,
        })
        .collect()
}

/// The canonical document a version digests. Protobuf maps encode in no fixed order,
/// so the policy enters in OpenShell's canonical JSON and the template's environment
/// as a sorted map beside the rest of its spec, whose encoding has no map left. The
/// handbook, the forwarding capacity, the gateway connection and the names of the
/// template and the policy file are left out: none reaches the sandbox itself. The
/// supervisor and sandbox runtime images the gateway's driver reports are in: every new
/// sandbox runs them.
fn canonical(spec: &ComputerProvisionSpec) -> Result<serde_json::Value> {
    let mut settings = serde_json::to_value(&spec.config)?;
    let object = settings
        .as_object_mut()
        .context("Computer settings are not an object")?;
    for key in [
        "handbook",
        "forwardingConnectionLimit",
        "collectAfterSeconds",
        "gateway",
        "template",
        "policyFile",
    ] {
        object.remove(key);
    }
    let mut template = spec.template()?.spec.unwrap_or_default();
    let environment: BTreeMap<String, String> = template
        .workload
        .as_mut()
        .map(|workload| {
            std::mem::take(&mut workload.environment)
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    let policy = openshell_policy::sandbox_policy_to_json_value(&spec.policy()?)
        .map_err(|error| anyhow::anyhow!("Computer policy has no canonical form: {error}"))?;
    let mut document = serde_json::json!({
        "settings": settings,
        "modelSecret": spec.model_secret,
        "hubUrl": spec.hub_url,
        "backend": spec.backend,
        "policy": policy,
        "template": {
            "spec": hex::encode(template.encode_to_vec()),
            "environment": environment,
        },
    });
    // A gateway that reports no images leaves the document as it was before it could.
    if !spec.runtime.is_empty() {
        document["runtime"] = serde_json::to_value(&spec.runtime)?;
    }
    Ok(sorted(document))
}

/// `value` with every object's members in key order, whatever order its map keeps.
fn sorted(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let members: BTreeMap<_, _> = object
                .into_iter()
                .map(|(key, value)| (key, sorted(value)))
                .collect();
            serde_json::Value::Object(members.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sorted).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests;
