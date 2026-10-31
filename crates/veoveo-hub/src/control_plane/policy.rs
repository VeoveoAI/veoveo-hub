use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_types::ResourceTemplateUri;

use super::*;
use veoveo_hub_sdk::{Principal, PrincipalAssurance, PrincipalKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct PolicySet {
    pub version: PolicyVersion,
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    pub id: PolicyRuleId,
    pub effect: PolicyEffect,
    #[schemars(with = "BTreeSet<super::catalog_schema::RegisteredActionSchema>")]
    pub actions: BTreeSet<veoveo_types::ActionName>,
    #[serde(default)]
    pub profiles: BTreeSet<HubProfileId>,
    #[serde(default)]
    pub protected_resources: BTreeSet<ProtectedResourceId>,
    #[serde(default)]
    pub principal_ids: BTreeSet<PrincipalId>,
    #[serde(default)]
    pub tenant_ids: BTreeSet<TenantId>,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub required_scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub required_data_labels: BTreeSet<DataLabelId>,
    #[serde(default)]
    pub required_assurances: BTreeSet<PrincipalAssurance>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrincipalAuditAttributes {
    pub kind: PrincipalKind,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default)]
    pub assurances: BTreeSet<PrincipalAssurance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_at: Option<DateTime<Utc>>,
}

impl From<&Principal> for PrincipalAuditAttributes {
    fn from(principal: &Principal) -> Self {
        Self {
            kind: principal.kind,
            groups: principal.groups.clone(),
            roles: principal.roles.clone(),
            scopes: principal.scopes.clone(),
            data_labels: principal.data_labels.clone(),
            assurances: principal.assurances.clone(),
            authenticated_at: principal.authenticated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    pub effect: PolicyEffect,
    pub reason: PolicyReasonCode,
    pub evaluated_at: DateTime<Utc>,
    pub profile: HubProfileId,
    #[schemars(with = "super::catalog_schema::RegisteredActionSchema")]
    pub action: veoveo_types::ActionName,
    pub target: PolicyTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_version: Option<PolicyVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<PolicyRuleId>,
    /// For a registered server's target: the permission its declaration requires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<veoveo_types::PermissionName>,
    /// For an allowed server target: every permission the caller holds in that server's
    /// namespace, forwarded in the signed internal identity.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub granted: BTreeSet<veoveo_types::PermissionName>,
    pub trace_id: TraceId,
}

impl PolicyDecision {
    pub fn deny(
        profile: HubProfileId,
        action: impl Into<veoveo_types::ActionName>,
        target: PolicyTarget,
        reason: PolicyReasonCode,
        trace_id: TraceId,
    ) -> Self {
        Self {
            effect: PolicyEffect::Deny,
            reason,
            evaluated_at: Utc::now(),
            profile,
            action: action.into(),
            target,
            principal: None,
            tenant: None,
            policy_version: None,
            rule_id: None,
            permission: None,
            granted: BTreeSet::new(),
            trace_id,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyReasonCode {
    PolicyAllow,
    PolicyDeny,
    UnknownProfile,
    UnknownServer,
    UnknownTool,
    UnknownResource,
    UnknownPrompt,
    UnknownTask,
    UnknownPrincipal,
    UnknownScope,
    UnknownDataLabel,
    UnknownTenant,
    UnknownTokenIssuer,
    MissingPrincipal,
    MissingTenant,
    MissingGroup,
    MissingRole,
    MissingScope,
    MissingDataLabel,
    MissingPrincipalAssurance,
    TokenAudienceMismatch,
    TokenExpired,
    TokenNotYetValid,
    ReplayDetected,
    /// The target is not in the registered server's declaration.
    UndeclaredTarget,
    /// No role the caller holds grants the target's required permission.
    MissingPermission,
}

impl PolicyReasonCode {
    /// The code's wire spelling, which audit records carry as their reason.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PolicyAllow => "policy_allow",
            Self::PolicyDeny => "policy_deny",
            Self::UnknownProfile => "unknown_profile",
            Self::UnknownServer => "unknown_server",
            Self::UnknownTool => "unknown_tool",
            Self::UnknownResource => "unknown_resource",
            Self::UnknownPrompt => "unknown_prompt",
            Self::UnknownTask => "unknown_task",
            Self::UnknownPrincipal => "unknown_principal",
            Self::UnknownScope => "unknown_scope",
            Self::UnknownDataLabel => "unknown_data_label",
            Self::UnknownTenant => "unknown_tenant",
            Self::UnknownTokenIssuer => "unknown_token_issuer",
            Self::MissingPrincipal => "missing_principal",
            Self::MissingTenant => "missing_tenant",
            Self::MissingGroup => "missing_group",
            Self::MissingRole => "missing_role",
            Self::MissingScope => "missing_scope",
            Self::MissingDataLabel => "missing_data_label",
            Self::MissingPrincipalAssurance => "missing_principal_assurance",
            Self::TokenAudienceMismatch => "token_audience_mismatch",
            Self::TokenExpired => "token_expired",
            Self::TokenNotYetValid => "token_not_yet_valid",
            Self::ReplayDetected => "replay_detected",
            Self::UndeclaredTarget => "undeclared_target",
            Self::MissingPermission => "missing_permission",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "kind",
    deny_unknown_fields
)]
pub enum PolicyTarget {
    Hub,
    Server {
        server: ServerSlug,
    },
    Tool {
        server: ServerSlug,
        tool: LocalToolName,
    },
    Resource {
        server: ServerSlug,
        uri: ResourceUri,
    },
    /// A declaration target for template discovery or argument completion.
    /// ```compile_fail
    /// use veoveo_hub_sdk::{PolicyTarget, ServerSlug};
    /// use veoveo_types::ResourceUri;
    /// PolicyTarget::ResourceTemplate {
    ///     server: ServerSlug::parse("example").unwrap(),
    ///     uri: ResourceUri::new("example://items/literal").unwrap(),
    /// };
    /// ```
    ResourceTemplate {
        server: ServerSlug,
        uri: ResourceTemplateUri,
    },
    Prompt {
        server: ServerSlug,
        prompt: PromptName,
    },
    Task {
        server: ServerSlug,
        task_id: CanonicalTaskId,
    },
    /// A native platform Task addressed by the administration API.
    /// MCP forwarding uses `Task` and its opaque hub route identity.
    PlatformTask {
        server: ServerSlug,
        task_id: veoveo_types::TaskId,
    },
    /// A request on the server's app origin, behind its `app_origin` requirement.
    AppOrigin {
        server: ServerSlug,
    },
    #[serde(untagged, skip_deserializing)]
    #[schemars(skip)]
    Owner(crate::contract::AdmittedPolicyTarget),
    #[serde(untagged)]
    #[schemars(skip)]
    Unadmitted(DecodedPolicyTarget),
}

/// A decoded owner target cannot authorize execution before registry admission.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedPolicyTarget(serde_json::Value);
impl std::fmt::Debug for DecodedPolicyTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DecodedPolicyTarget([REDACTED])")
    }
}
impl Serialize for DecodedPolicyTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for DecodedPolicyTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = veoveo_types::UniqueJsonValue::deserialize(deserializer)?.0;
        if !value.is_object()
            || value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_none()
        {
            return Err(serde::de::Error::custom(
                "policy target requires an object with a kind",
            ));
        }
        Ok(Self(value))
    }
}
impl PolicyTarget {
    pub fn admit(
        self,
        registry: &crate::contract::CatalogRegistry,
    ) -> Result<Self, veoveo_types::ExtensionError> {
        match self {
            Self::Unadmitted(target) => registry.admit_target(target.0).map(Self::Owner),
            Self::Owner(target) => {
                registry.check_target(&target)?;
                Ok(Self::Owner(target))
            }
            kernel => Ok(kernel),
        }
    }
}

impl PolicyTarget {
    pub fn kind(&self) -> &str {
        match self {
            Self::Hub => "hub",
            Self::Server { .. } => "server",
            Self::Tool { .. } => "tool",
            Self::Resource { .. } => "resource",
            Self::ResourceTemplate { .. } => "resource_template",
            Self::Prompt { .. } => "prompt",
            Self::Task { .. } => "task",
            Self::PlatformTask { .. } => "platform_task",
            Self::AppOrigin { .. } => "app_origin",
            Self::Owner(target) => target.kind().as_str(),
            Self::Unadmitted(_) => "unadmitted",
        }
    }
    pub fn server(&self) -> Option<&ServerSlug> {
        match self {
            Self::Server { server }
            | Self::Tool { server, .. }
            | Self::Resource { server, .. }
            | Self::ResourceTemplate { server, .. }
            | Self::Prompt { server, .. }
            | Self::Task { server, .. }
            | Self::PlatformTask { server, .. }
            | Self::AppOrigin { server } => Some(server),
            _ => None,
        }
    }
}
