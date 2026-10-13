//! The Gateway's audit vocabulary: the classes it declares under the owner `gateway`, its
//! profile and client targets, and the installation registry that joins them with the
//! MCP vocabulary of `veoveo-sdk-rust` and the platform classes of core. One registry
//! serves the whole process, so every target a registration builds belongs to it.
use std::sync::{Arc, LazyLock};

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_audit_contract::{
    AuditDetail, AuditDetailType, AuditRegistry, AuditRegistryBuilder, AuditTarget,
    AuditTargetOwner, AuditTargetRegistration,
};
use veoveo_sdk_rust::audit::{
    DiscoveryKind, DiscoveryTarget, McpAuditTargets, PromptTarget, ResourceTemplateTarget,
    TaskRouteTarget, ToolTarget,
};
use veoveo_sdk_rust::{
    CanonicalTaskId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, PromptName,
};
use veoveo_types::{OAuthClientId, ResourceTemplateUri, ServerSlug};

use crate::contract::{AuthMethod, AuthReasonCode};

/// An administrative operation of the Gateway's admin API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum AdministrativeOperation {
    ControlPlane,
    JwtRevoke,
    JwtPrune,
    ServerHealth,
    TaskCancel,
}

/// Whether an administrative operation reads or changes state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum AdministrativeAccess {
    Read,
    Write,
}

/// Why an admitted administrative operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum AdminOperationFailure {
    BuildHttpClient,
    CancelTask,
    ControlPlaneSha,
    ExpiredRevocation,
    InvalidControlPlane,
    LatestRevisionRead,
    PersistControlPlaneRevision,
    PersistJwtRevocation,
    PruneJwtRevocations,
    RevisionId,
    TaskOwnership,
    TaskRoute,
}

/// What an authentication event did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum AuthenticationActivity {
    Issue,
    Refresh,
    DuplicateRefresh,
    Revoke,
    Replay,
    CredentialDenial,
}

/// A change a control-plane publication made to an identity record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum AccountActivity {
    Create,
    Update,
    Delete,
}

/// A step in a server's registration with the Gateway. An expiry records a
/// registration the Gateway removed because its login held no live process instance
/// for the grace period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum RegistrationActivity {
    /// The first registration of a slug pinned its namespace to the server's key.
    ClaimNamespace,
    Register,
    /// An RFC 7592 update changed the registered declarations.
    Update,
    Deregister,
    Expire,
}

/// The admin API admitted or refused an operation: class `gateway.admin_admission`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminAdmission {
    pub profile: GatewayProfileId,
    pub operation: AdministrativeOperation,
    pub access: AdministrativeAccess,
}
impl AuditDetailType for AdminAdmission {
    const CLASS: &'static str = "gateway.admin_admission";
}

/// An admitted administrative operation finished: class `gateway.admin_completion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminCompletion {
    pub profile: GatewayProfileId,
    pub operation: AdministrativeOperation,
    pub access: AdministrativeAccess,
    pub failure: Option<AdminOperationFailure>,
}
impl AuditDetailType for AdminCompletion {
    const CLASS: &'static str = "gateway.admin_completion";
}

/// A token was issued, refreshed, revoked or refused: class `gateway.authentication`.
/// The session family names the refresh-token family a browser session renews.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Authentication {
    pub activity: AuthenticationActivity,
    pub method: AuthMethod,
    pub reason: AuthReasonCode,
    pub session_family: Option<GatewayRefreshFamilyId>,
}
impl AuditDetailType for Authentication {
    const CLASS: &'static str = "gateway.authentication";
}

/// A control-plane publication created, changed or deleted an identity record: class
/// `gateway.account_change`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountChange {
    pub activity: AccountActivity,
}
impl AuditDetailType for AccountChange {
    const CLASS: &'static str = "gateway.account_change";
}

/// A server registration step: class `gateway.registration`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub activity: RegistrationActivity,
}
impl AuditDetailType for Registration {
    const CLASS: &'static str = "gateway.registration";
}

/// A caller read the status of a Task the Gateway routes: class `gateway.task_status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskStatusRead {}
impl AuditDetailType for TaskStatusRead {
    const CLASS: &'static str = "gateway.task_status";
}

/// Target `{"kind":"profile","profile":…}`: a Gateway profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileTarget {
    pub profile: GatewayProfileId,
}

/// Target `{"kind":"client","client":…}`: a registered OAuth client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientTarget {
    pub client: OAuthClientId,
}

/// The wire shape of the Gateway's targets: a closed object tagged by `kind`.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TargetWire {
    Profile { profile: GatewayProfileId },
    Client { client: OAuthClientId },
}

impl Serialize for ProfileTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TargetWire::Profile {
            profile: self.profile.clone(),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ProfileTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match TargetWire::deserialize(deserializer)? {
            TargetWire::Profile { profile } => Ok(Self { profile }),
            TargetWire::Client { .. } => Err(serde::de::Error::custom(
                "expected a Gateway audit target of kind `profile`",
            )),
        }
    }
}
impl Serialize for ClientTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TargetWire::Client {
            client: self.client.clone(),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ClientTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match TargetWire::deserialize(deserializer)? {
            TargetWire::Client { client } => Ok(Self { client }),
            TargetWire::Profile { .. } => Err(serde::de::Error::custom(
                "expected a Gateway audit target of kind `client`",
            )),
        }
    }
}

#[derive(JsonSchema)]
#[allow(dead_code, reason = "Only the target schema reads this field set")]
struct ProfileFields {
    profile: GatewayProfileId,
}

#[derive(JsonSchema)]
#[allow(dead_code, reason = "Only the target schema reads this field set")]
struct ClientFields {
    /// Core's audit target schemas admit no `not` keyword, which the identifier's own
    /// schema uses, so the target declares the client as a non-empty string. Decoding
    /// still admits it through `OAuthClientId`.
    #[schemars(with = "String", length(min = 1))]
    client: OAuthClientId,
}

/// A target's schema: its fields inlined, the constant `kind` discriminator, and no
/// further properties.
fn tagged_schema<F: JsonSchema>(kind: &str) -> schemars::Schema {
    let mut schema = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<F>();
    let object = schema.as_object_mut().expect("target fields are an object");
    object
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("target properties")
        .insert(
            "kind".into(),
            serde_json::json!({"type": "string", "const": kind}),
        );
    object
        .entry("required")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .expect("target required fields")
        .push(serde_json::json!("kind"));
    object.insert(
        "additionalProperties".into(),
        serde_json::Value::Bool(false),
    );
    schema
}

impl JsonSchema for ProfileTarget {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "GatewayProfileTarget".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        tagged_schema::<ProfileFields>(Self::KIND)
    }
}
impl JsonSchema for ClientTarget {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "GatewayClientTarget".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        tagged_schema::<ClientFields>(Self::KIND)
    }
}

impl AuditTargetOwner for ProfileTarget {
    const KIND: &'static str = "profile";
}
impl AuditTargetOwner for ClientTarget {
    const KIND: &'static str = "client";
}

/// The installation's audit registry and the target registrations the Gateway records
/// with.
pub struct GatewayAudit {
    registry: Arc<AuditRegistry>,
    mcp: McpAuditTargets,
    profile: AuditTargetRegistration<ProfileTarget>,
    client: AuditTargetRegistration<ClientTarget>,
}

static GATEWAY_AUDIT: LazyLock<GatewayAudit> = LazyLock::new(|| {
    GatewayAudit::build(AuditRegistry::builder()).expect("the Gateway's audit vocabulary is closed")
});

/// The process's audit registry and target registrations.
pub fn gateway_audit() -> &'static GatewayAudit {
    &GATEWAY_AUDIT
}

impl GatewayAudit {
    /// Join the MCP vocabulary and the Gateway's classes and targets to `builder`.
    pub fn build(mut builder: AuditRegistryBuilder) -> Result<Self> {
        let mcp = veoveo_sdk_rust::audit::register(&mut builder)?;
        Self::declare(&mut builder)?;
        let profile = builder.register_target()?;
        let client = builder.register_target()?;
        Ok(Self {
            registry: Arc::new(builder.build()),
            mcp,
            profile,
            client,
        })
    }

    fn declare(builder: &mut AuditRegistryBuilder) -> Result<()> {
        builder.declare::<AdminAdmission>()?;
        builder.declare::<AdminCompletion>()?;
        builder.declare::<Authentication>()?;
        builder.declare::<AccountChange>()?;
        builder.declare::<Registration>()?;
        builder.declare::<TaskStatusRead>()?;
        Ok(())
    }

    pub fn registry(&self) -> &Arc<AuditRegistry> {
        &self.registry
    }

    /// Admit a detail against its class's declaration.
    pub fn detail<T: AuditDetailType>(&self, value: &T) -> Result<AuditDetail> {
        Ok(self.registry.detail(value)?)
    }

    pub fn tool(&self, server: ServerSlug, tool: LocalToolName) -> Result<AuditTarget> {
        Ok(self
            .mcp
            .tool
            .target(&self.registry, ToolTarget { server, tool })?)
    }

    pub fn resource_template(
        &self,
        server: ServerSlug,
        uri: ResourceTemplateUri,
    ) -> Result<AuditTarget> {
        Ok(self
            .mcp
            .resource_template
            .target(&self.registry, ResourceTemplateTarget { server, uri })?)
    }

    pub fn prompt(&self, server: ServerSlug, prompt: PromptName) -> Result<AuditTarget> {
        Ok(self
            .mcp
            .prompt
            .target(&self.registry, PromptTarget { server, prompt })?)
    }

    pub fn discovery(
        &self,
        server: Option<ServerSlug>,
        collection: DiscoveryKind,
    ) -> Result<AuditTarget> {
        Ok(self
            .mcp
            .discovery
            .target(&self.registry, DiscoveryTarget { server, collection })?)
    }

    pub fn task_route(&self, server: ServerSlug, route: CanonicalTaskId) -> Result<AuditTarget> {
        Ok(self
            .mcp
            .task_route
            .target(&self.registry, TaskRouteTarget { server, route })?)
    }

    pub fn profile(&self, profile: GatewayProfileId) -> Result<AuditTarget> {
        Ok(self
            .profile
            .target(&self.registry, ProfileTarget { profile })?)
    }

    pub fn client(&self, client: OAuthClientId) -> Result<AuditTarget> {
        Ok(self
            .client
            .target(&self.registry, ClientTarget { client })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_audit_contract::{AuditDetailDeclaration, AuditRegistry};

    #[test]
    fn the_gateway_vocabulary_is_closed_and_owned_by_the_gateway() {
        let audit = gateway_audit();
        let gateway: Vec<_> = audit
            .registry()
            .declarations()
            .map(AuditDetailDeclaration::class)
            .filter(|class| class.owner() == "gateway")
            .map(|class| class.name().to_owned())
            .collect();
        assert_eq!(
            gateway,
            [
                "account_change",
                "admin_admission",
                "admin_completion",
                "authentication",
                "registration",
                "task_status"
            ]
        );
        assert!(
            audit
                .registry()
                .declarations()
                .any(|declaration| declaration.class().as_str() == "mcp.tool_completion")
        );
        // A second registry built the same way is a different identity.
        assert!(GatewayAudit::build(AuditRegistry::builder()).is_ok());
    }

    #[test]
    fn profile_and_client_targets_round_trip_through_the_registry() {
        let audit = gateway_audit();
        let profile = audit
            .profile(GatewayProfileId::parse("operator").unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(&profile).unwrap(),
            serde_json::json!({"kind": "profile", "profile": "operator"})
        );
        let client = audit
            .client(OAuthClientId::parse("console").unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(&client).unwrap(),
            serde_json::json!({"kind": "client", "client": "console"})
        );
    }
}
