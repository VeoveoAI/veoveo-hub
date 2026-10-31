//! Trusted installation inputs for factory sandboxes: the one OpenShell sandbox of each
//! instance, which fetches its branch, builds it and serves it.
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::TokenIssuer;

use super::{ComputerGateway, ComputerTemplate, HubControlPlane, HubControlPlaneError};
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
    /// The installation's factory template: the OpenShell workload template whose CPU,
    /// memory and driver configuration every factory sandbox gets, with its branch's
    /// image. The Hub reads it on every push.
    pub template: ComputerTemplate,
    /// The loopback port every factory sandbox's server listens on.
    pub port: u16,
    /// The Hub issuer servers register with.
    pub hub_issuer: TokenIssuer,
    /// The Lore server as factory sandboxes reach it, such as
    /// `grpcs://lore.veoveo-system.svc.cluster.local:41337`. An instance's start command
    /// clones `<loreServer>/<repository ID>`.
    pub lore_server: url::Url,
    /// The global provider profile that carries an instance's registration token into
    /// its sandbox.
    pub registration_profile: String,
    /// The global provider profile that carries an instance's Lore access into its
    /// sandbox. The gateway refreshes it with the instance's own Hub client, whose tokens
    /// act as the instance's latest pusher in its repository.
    pub lore_profile: String,
    /// The BuildKit daemon and registry that build and hold environment images.
    pub image_builder: ImageBuilder,
    /// Per-user caps; without them, nothing is counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<FactoryCaps>,
    /// The installation's providers a branch may request in `openshell/providers.yaml`,
    /// each with the repositories granted it. An instance gets only those it requests
    /// that are granted to its repository.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<FactoryProviderGrant>,
}

/// One of the installation's providers in the factory workspace, such as
/// `vv-factory-model`, and the Lore repositories whose instances may have it attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryProviderGrant {
    pub provider: String,
    /// Lore repository IDs: 32 lowercase hexadecimal digits each.
    pub repositories: Vec<String>,
}

impl FactoryConfiguration {
    /// The providers among `requested` granted to `repository`, and those not granted,
    /// each sorted and once.
    pub fn granted_providers(
        &self,
        repository: &str,
        requested: &[String],
    ) -> (Vec<String>, Vec<String>) {
        let requested: std::collections::BTreeSet<&String> = requested.iter().collect();
        let (granted, ungranted): (Vec<&String>, Vec<&String>) =
            requested.into_iter().partition(|name| {
                self.providers.iter().any(|grant| {
                    &&grant.provider == name
                        && grant
                            .repositories
                            .iter()
                            .any(|granted| granted == repository)
                })
            });
        (
            granted.into_iter().cloned().collect(),
            ungranted.into_iter().cloned().collect(),
        )
    }
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
        let lore = &self.lore_server;
        if lore.host_str().is_none()
            || !lore.username().is_empty()
            || lore.password().is_some()
            || !matches!(lore.path(), "" | "/")
            || lore.query().is_some()
            || lore.fragment().is_some()
        {
            return Err(invalid(
                "the Lore server is a host and port without credentials, path, query or fragment",
            ));
        }
        if !dns_label(&self.workspace)
            || !dns_label(&self.template.workspace)
            || !dns_label(&self.template.name)
            || !dns_label(&self.registration_profile)
            || !dns_label(&self.lore_profile)
        {
            return Err(invalid(
                "the workspaces, the template and the provider profiles are lowercase DNS labels",
            ));
        }
        if self
            .caps
            .as_ref()
            .is_some_and(|caps| caps.instances_per_user == 0)
        {
            return Err(invalid("a cap admits at least one instance"));
        }
        let mut named = std::collections::BTreeSet::new();
        for grant in &self.providers {
            if !dns_label(&grant.provider)
                || !named.insert(grant.provider.as_str())
                || grant.repositories.is_empty()
                || !grant.repositories.iter().all(|repository| {
                    repository.len() == 32
                        && repository
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
            {
                return Err(invalid(
                    "a provider grant names one provider by a lowercase DNS label, once, and Lore repository IDs of 32 lowercase hexadecimal digits",
                ));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_gets_only_the_requested_providers_granted_it() {
        let mut configuration: FactoryConfiguration = serde_json::from_value(serde_json::json!({
            "gateway": {"endpoint": "https://gateway.example/", "certificateAuthority": "/ca.pem", "bearerEnvironment": "BEARER"},
            "workspace": "vv-factory",
            "policyFile": "/policy.yaml",
            "template": {"workspace": "vv-factory", "name": "factory"},
            "port": 8800,
            "hubIssuer": "https://hub.example",
            "loreServer": "grpcs://lore.example:41337",
            "registrationProfile": "vv-factory-registration",
            "loreProfile": "vv-factory-lore",
            "imageBuilder": {"buildkit": "tcp://buildkitd.internal:1234", "registry": "registry.internal:5000"},
            "providers": [
                {"provider": "vv-factory-model", "repositories": ["0123456789abcdef0123456789abcdef"]}
            ]
        }))
        .unwrap();
        let requested = [
            "mail".to_owned(),
            "vv-factory-model".to_owned(),
            "mail".to_owned(),
        ];
        assert_eq!(
            configuration.granted_providers("0123456789abcdef0123456789abcdef", &requested),
            (vec!["vv-factory-model".to_owned()], vec!["mail".to_owned()])
        );
        assert_eq!(
            configuration.granted_providers("fedcba9876543210fedcba9876543210", &requested),
            (
                Vec::new(),
                vec!["mail".to_owned(), "vv-factory-model".to_owned()]
            )
        );
        configuration.providers[0].repositories = vec!["not-a-repository".into()];
        let plane: HubControlPlane =
            serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap();
        assert!(configuration.validate(&plane).is_err());
    }
}
