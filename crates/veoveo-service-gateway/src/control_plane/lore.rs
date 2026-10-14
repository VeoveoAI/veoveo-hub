//! Lore as a protected resource of the installation. The Gateway issues Lore access
//! tokens whose `lore_resources` claim carries each client's repository grants, and the
//! Lore server enforces them with its per-repository authorizer.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_sdk_rust::{OAuthClientId, ProtectedResourceId};
use veoveo_types::{IdProfile, IdProfileSpec, IdentifierError};

use super::{AuthorizationServerId, GatewayControlPlaneError, OAuthClientRegistration};

/// The Lore server an installation runs and which clients may use which of its
/// repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoreResource {
    /// The Lore server's URL, such as `lores://vv-lore.internal:41337`. Lore tokens carry it as
    /// their audience.
    pub resource: ProtectedResourceId,
    /// The authorization server that issues Lore tokens.
    pub authorization_server: AuthorizationServerId,
    pub grants: Vec<LoreGrant>,
}

/// One client's access to Lore repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoreGrant {
    pub client: OAuthClientId,
    /// Every repository, present and future.
    #[serde(default, skip_serializing_if = "is_false")]
    pub all_repositories: bool,
    /// The repositories the client reaches. Empty until the installation creates them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub repositories: BTreeSet<LoreRepositoryId>,
    /// Privileged Lore actions on those repositories. Reading and pushing to
    /// unprotected branches need none.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub actions: BTreeSet<LoreAction>,
    /// Marks the client as a Lore service account, which Lore lets push to protected
    /// branches. The installation's merger holds it; environments do not.
    #[serde(default, skip_serializing_if = "is_false")]
    pub service_account: bool,
}

/// A privileged action the Lore server checks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, veoveo_types::Vocabulary)]
pub enum LoreAction {
    #[vocabulary(rename = "admin")]
    Admin,
    #[vocabulary(rename = "migrate")]
    Migrate,
    #[vocabulary(rename = "obliterate")]
    Obliterate,
    #[vocabulary(rename = "owner")]
    Owner,
}

/// A Lore repository ID: 32 lowercase hexadecimal digits, as Lore prints it.
#[veoveo_types::id(text(LoreRepositoryIds))]
pub struct LoreRepositoryId(String);

#[doc(hidden)]
pub struct LoreRepositoryIds;
impl IdProfile for LoreRepositoryIds {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_repository_id(value));
}

fn validate_repository_id(value: &str) -> Result<(), IdentifierError> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(IdentifierError::new(
        value,
        "must be 32 lowercase hexadecimal digits, as Lore prints a repository ID",
    ))
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One entry of a Lore token's `lore_resources` claim, in the shape Lore's
/// per-repository authorizer reads by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoreResourceGrant {
    pub resource_id: String,
    pub permission: Vec<String>,
}

/// Lore renders a repository as `urc-{id}` and every repository as `urc-*` in its
/// default `resource_id_template` and `resource_wildcard`.
const LORE_RESOURCE_PREFIX: &str = "urc-";

impl LoreResource {
    pub fn grant(&self, client: &OAuthClientId) -> Option<&LoreGrant> {
        self.grants.iter().find(|grant| &grant.client == client)
    }

    /// Every grant names a known client of this authorization server that may
    /// request Lore tokens, once; and every client that may request them holds a grant.
    pub(super) fn validate(
        &self,
        authorization_servers: &BTreeSet<&AuthorizationServerId>,
        clients: &[OAuthClientRegistration],
    ) -> Result<(), GatewayControlPlaneError> {
        let invalid = |reason: String| Err(GatewayControlPlaneError::InvalidLore { reason });
        if !authorization_servers.contains(&self.authorization_server) {
            return invalid(format!(
                "authorization server `{}` is not configured",
                self.authorization_server
            ));
        }
        let clients: BTreeMap<_, _> = clients.iter().map(|client| (&client.id, client)).collect();
        let mut granted = BTreeSet::new();
        for grant in &self.grants {
            if !granted.insert(&grant.client) {
                return invalid(format!("client `{}` holds two grants", grant.client));
            }
            if grant.all_repositories && !grant.repositories.is_empty() {
                return invalid(format!(
                    "client `{}` names repositories and every repository at once",
                    grant.client
                ));
            }
            let Some(client) = clients.get(&grant.client) else {
                return invalid(format!("client `{}` is not configured", grant.client));
            };
            if client.authorization_server != self.authorization_server
                || !client.allowed_resources.contains(&self.resource)
            {
                return invalid(format!(
                    "client `{}` must belong to `{}` and allow the resource `{}`",
                    grant.client, self.authorization_server, self.resource
                ));
            }
        }
        if let Some(client) = clients.values().find(|client| {
            client.allowed_resources.contains(&self.resource) && !granted.contains(&client.id)
        }) {
            return invalid(format!(
                "client `{}` allows the Lore resource and holds no grant",
                client.id
            ));
        }
        Ok(())
    }
}

impl LoreGrant {
    /// The grant as Lore reads it from a token.
    pub fn resource_grants(&self) -> Vec<LoreResourceGrant> {
        let permission: Vec<String> = self.actions.iter().map(ToString::to_string).collect();
        let resource = |id: &str| LoreResourceGrant {
            resource_id: format!("{LORE_RESOURCE_PREFIX}{id}"),
            permission: permission.clone(),
        };
        if self.all_repositories {
            return vec![resource("*")];
        }
        self.repositories
            .iter()
            .map(|id| resource(id.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(json: serde_json::Value) -> LoreGrant {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn grants_render_in_lores_default_resource_shape() {
        let environment = grant(serde_json::json!({
            "client": "lore-vv-mcp-time",
            "repositories": ["01a1397efdb27152b7259d027ce59c45"]
        }));
        assert_eq!(
            serde_json::to_value(environment.resource_grants()).unwrap(),
            serde_json::json!([{ "resource_id": "urc-01a1397efdb27152b7259d027ce59c45", "permission": [] }])
        );
        let merger = grant(serde_json::json!({
            "client": "lore-admin",
            "allRepositories": true,
            "actions": ["owner", "admin"],
            "serviceAccount": true
        }));
        assert_eq!(
            serde_json::to_value(merger.resource_grants()).unwrap(),
            serde_json::json!([{ "resource_id": "urc-*", "permission": ["admin", "owner"] }])
        );
        assert!(
            grant(serde_json::json!({ "client": "lore-vv-mcp-time" }))
                .resource_grants()
                .is_empty()
        );
    }

    #[test]
    fn repository_ids_and_actions_are_admitted_on_entry() {
        for bad in [
            "01A1397EFDB27152B7259D027CE59C45",
            "01a1397e",
            "urc-01a1397efdb27152b7259d027ce59c4",
        ] {
            assert!(LoreRepositoryId::parse(bad).is_err(), "{bad}");
        }
        assert!(
            serde_json::from_value::<LoreGrant>(serde_json::json!({
                "client": "lore-admin",
                "actions": ["push-protected"]
            }))
            .is_err()
        );
    }
}
