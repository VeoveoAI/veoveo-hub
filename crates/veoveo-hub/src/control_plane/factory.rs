//! Trusted installation inputs for factory sandboxes: the one OpenShell sandbox of each
//! instance, which fetches its branch, builds it and serves it.
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::TokenIssuer;

use super::{ComputerGateway, HubControlPlane, HubControlPlaneError};
use crate::factory::image::ImageBuilder;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryConfiguration {
    /// The gateway that runs factory sandboxes. The Hub forwards MCP to them through
    /// `nativeUpstreams`, which must name the same gateway.
    pub gateway: ComputerGateway,
    /// The OpenShell workspace of every factory sandbox. The installation creates it.
    pub workspace: String,
    /// The installation's base policy of every factory sandbox, in OpenShell's format; a
    /// repository's own policy adds network rules to it. Use an immutable versioned file
    /// path for each policy revision.
    pub policy_file: PathBuf,
    /// The CPU limit of every factory sandbox, a Kubernetes quantity such as `8` or `500m`.
    pub cpu: String,
    /// The memory limit of every factory sandbox, a Kubernetes quantity such as `16Gi`.
    pub memory: String,
    /// The loopback port every factory sandbox's server listens on.
    pub port: u16,
    /// The Hub issuer servers register with.
    pub hub_issuer: TokenIssuer,
    /// The global provider profile that carries an instance's registration token into
    /// its sandbox.
    pub registration_profile: String,
    /// The BuildKit daemon and registry that build and hold environment images.
    pub image_builder: ImageBuilder,
    /// Per-user caps; without them, nothing is counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<FactoryCaps>,
}

/// Soft caps: exceeding one warns in the instance's status, unless `enforce` refuses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryCaps {
    /// How many instances may name one principal as their latest pusher.
    pub instances_per_user: u32,
    #[serde(default)]
    pub enforce: bool,
}

impl FactoryConfiguration {
    pub(super) fn validate(&self, plane: &HubControlPlane) -> Result<(), HubControlPlaneError> {
        let invalid = HubControlPlaneError::InvalidFactory;
        if !plane
            .native_upstreams
            .as_ref()
            .is_some_and(|native| native.gateway == self.gateway.endpoint)
        {
            return Err(invalid(
                "the Hub forwards to factory sandboxes through nativeUpstreams on the same gateway",
            ));
        }
        if !self.gateway.certificate_authority.is_absolute()
            || self.gateway.bearer_environment.is_empty()
            || !self.policy_file.is_absolute()
            || self.port == 0
        {
            return Err(invalid(
                "factory requires absolute CA and policy paths, a bearer environment variable and a nonzero port",
            ));
        }
        if !dns_label(&self.workspace) || !dns_label(&self.registration_profile) {
            return Err(invalid(
                "the workspace and the registration profile are lowercase DNS labels",
            ));
        }
        if !cpu_quantity(&self.cpu) || !memory_quantity(&self.memory) {
            return Err(invalid(
                "cpu is a positive number of cores or millicores and memory a positive byte quantity",
            ));
        }
        if self
            .caps
            .as_ref()
            .is_some_and(|caps| caps.instances_per_user == 0)
        {
            return Err(invalid("a cap admits at least one instance"));
        }
        Ok(())
    }
}

fn dns_label(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn cpu_quantity(value: &str) -> bool {
    match value.strip_suffix('m') {
        Some(millicores) => millicores.parse::<u64>().is_ok_and(|v| v > 0),
        None => value.parse::<f64>().is_ok_and(|v| v.is_finite() && v > 0.0),
    }
}

fn memory_quantity(value: &str) -> bool {
    let digits = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (number, suffix) = value.split_at(digits);
    number.parse::<u128>().is_ok_and(|v| v > 0)
        && matches!(
            suffix,
            "" | "Ki" | "Mi" | "Gi" | "Ti" | "Pi" | "Ei" | "K" | "M" | "G" | "T" | "P" | "E"
        )
}
