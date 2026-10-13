//! The Gateway's audit vocabulary: the classes it declares under the owner `gateway`,
//! each with the OCSF class it maps to, and the installation registry that joins them
//! with the MCP vocabulary of `veoveo-sdk-rust` and the platform classes of core.
use std::sync::{Arc, LazyLock};

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_audit_contract::{
    AuditDetail, AuditDetailDeclaration, AuditDetailType, AuditOcsfClass, AuditRegistry,
    AuditRegistryBuilder,
};
use veoveo_sdk_rust::GatewayProfileId;
use veoveo_sdk_rust::GatewayRefreshFamilyId;
use veoveo_sdk_rust::audit::AuditInvocation;
use veoveo_types::{DataLabelId, OAuthClientId, ScopeName};

use crate::contract::AuthMethod;

/// OCSF Account Change (3001) with an activity this class does not fix.
const OCSF_ACCOUNT_CHANGE: AuditOcsfClass = AuditOcsfClass {
    class_uid: 3001,
    activity_id: 99,
};
/// OCSF Authentication (3002) with an activity this class does not fix.
const OCSF_AUTHENTICATION: AuditOcsfClass = AuditOcsfClass {
    class_uid: 3002,
    activity_id: 99,
};
/// OCSF Authentication (3002), Service Ticket Request (4): a token for another service.
const OCSF_SERVICE_TICKET: AuditOcsfClass = AuditOcsfClass {
    class_uid: 3002,
    activity_id: 4,
};
/// OCSF Entity Management (3004) with an activity this class does not fix.
const OCSF_ENTITY_MANAGEMENT: AuditOcsfClass = AuditOcsfClass {
    class_uid: 3004,
    activity_id: 99,
};

/// An administrative operation of the Gateway's admin API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum AdministrativeOperation {
    ControlPlane,
    JwtRevoke,
    JwtPrune,
    ServerHealth,
    TaskCancel,
    UsageRead,
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

/// What a registered server presented as the subject of a token exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum ExchangeSubjectKind {
    /// The forwarded token of a call the server is handling.
    Request,
    /// One of the server's Tasks.
    Task,
}

/// Why the Gateway refused a token exchange, as its OAuth error names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum ExchangeRefusal {
    InvalidRequest,
    UnauthorizedClient,
    InvalidGrant,
    InvalidTarget,
    InvalidAuthorizationDetails,
    ServerError,
}

/// The admin API admitted or refused an operation: class `gateway.admin_admission`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminAdmission {
    pub profile: GatewayProfileId,
    pub operation: AdministrativeOperation,
    pub access: AdministrativeAccess,
    pub invocation: AuditInvocation,
}
impl AuditDetailType for AdminAdmission {
    const CLASS: &'static str = "gateway.admin_admission";
}

/// An admitted administrative operation finished: class `gateway.admin_completion`. A
/// failure's reason is the record's reason code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminCompletion {
    pub profile: GatewayProfileId,
    pub operation: AdministrativeOperation,
    pub access: AdministrativeAccess,
    pub duration_ms: u64,
    pub invocation: AuditInvocation,
}
impl AuditDetailType for AdminCompletion {
    const CLASS: &'static str = "gateway.admin_completion";
}

/// A token was issued, refreshed, revoked or refused: class `gateway.authentication`.
/// The record's reason is the authentication reason code. The session family names the
/// refresh-token family a browser session renews, the client, scopes and data labels
/// are the authenticated caller's, and the duration is how long the decision took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Authentication {
    pub activity: AuthenticationActivity,
    pub method: AuthMethod,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_family: Option<GatewayRefreshFamilyId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<OAuthClientId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
}
impl AuditDetailType for Authentication {
    const CLASS: &'static str = "gateway.authentication";
    const OCSF: AuditOcsfClass = OCSF_AUTHENTICATION;
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
    const OCSF: AuditOcsfClass = OCSF_ACCOUNT_CHANGE;
}

/// A server registration step: class `gateway.registration`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub activity: RegistrationActivity,
}
impl AuditDetailType for Registration {
    const CLASS: &'static str = "gateway.registration";
    const OCSF: AuditOcsfClass = OCSF_ENTITY_MANAGEMENT;
}

/// A registered server exchanged a token to call other servers for its caller, or was
/// refused: class `gateway.token_exchange`. The record's actor is the caller once the
/// subject is known, its target the server that asked, and a refusal's reason the OAuth
/// error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenExchange {
    pub subject: Option<ExchangeSubjectKind>,
    /// How many permissions the server requested, and how many the token carries.
    pub requested: u32,
    pub granted: u32,
}
impl AuditDetailType for TokenExchange {
    const CLASS: &'static str = "gateway.token_exchange";
    const OCSF: AuditOcsfClass = OCSF_SERVICE_TICKET;
}

/// A caller read the status of a Task the Gateway routes: class `gateway.task_status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskStatusRead {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<AuditInvocation>,
}
impl AuditDetailType for TaskStatusRead {
    const CLASS: &'static str = "gateway.task_status";
    const OCSF: AuditOcsfClass = AuditOcsfClass::api_activity(2);
}

/// The installation's audit registry the Gateway records with.
pub struct GatewayAudit {
    registry: Arc<AuditRegistry>,
}

static GATEWAY_AUDIT: LazyLock<GatewayAudit> = LazyLock::new(|| {
    GatewayAudit::build(AuditRegistry::builder()).expect("the Gateway's audit vocabulary is closed")
});

/// The process's audit registry.
pub fn gateway_audit() -> &'static GatewayAudit {
    &GATEWAY_AUDIT
}

impl GatewayAudit {
    /// Join the MCP vocabulary and the Gateway's classes to `builder`.
    pub fn build(mut builder: AuditRegistryBuilder) -> Result<Self> {
        veoveo_sdk_rust::audit::register(&mut builder)?;
        Self::declare(&mut builder)?;
        Ok(Self {
            registry: Arc::new(builder.build()),
        })
    }

    /// A registry for reading every owner's records: the Gateway's own vocabulary, then
    /// each class stored in `audit.detail_type` that it does not declare itself. The
    /// Gateway's own declaration wins over a stored one of the same class, and a stored
    /// declaration the registry cannot admit is skipped so it cannot stop the Gateway
    /// from reading the rest.
    pub fn reader_registry(
        stored: impl IntoIterator<Item = AuditDetailDeclaration>,
    ) -> Result<Arc<AuditRegistry>> {
        let mut builder = AuditRegistry::builder();
        veoveo_sdk_rust::audit::register(&mut builder)?;
        Self::declare(&mut builder)?;
        for declaration in stored {
            let class = declaration.class().clone();
            if let Err(error) = builder.declaration(declaration) {
                tracing::warn!(%class, %error, "audit reader skips a stored class declaration");
            }
        }
        Ok(Arc::new(builder.build()))
    }

    fn declare(builder: &mut AuditRegistryBuilder) -> Result<()> {
        builder.declare::<AdminAdmission>()?;
        builder.declare::<AdminCompletion>()?;
        builder.declare::<Authentication>()?;
        builder.declare::<AccountChange>()?;
        builder.declare::<Registration>()?;
        builder.declare::<TaskStatusRead>()?;
        builder.declare::<TokenExchange>()?;
        Ok(())
    }

    pub fn registry(&self) -> &Arc<AuditRegistry> {
        &self.registry
    }

    /// Admit a detail against its class's declaration.
    pub fn detail<T: AuditDetailType>(&self, value: &T) -> Result<AuditDetail> {
        Ok(self.registry.detail(value)?)
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
                "task_status",
                "token_exchange"
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
    fn each_gateway_class_names_its_ocsf_class() {
        assert_eq!(Authentication::OCSF.class_uid, 3002);
        assert_eq!(TokenExchange::OCSF, OCSF_SERVICE_TICKET);
        assert_eq!(AccountChange::OCSF.class_uid, 3001);
        assert_eq!(Registration::OCSF.class_uid, 3004);
        assert_eq!(TaskStatusRead::OCSF, AuditOcsfClass::api_activity(2));
        assert_eq!(AdminAdmission::OCSF, AuditOcsfClass::API_ACTIVITY_OTHER);
        let declared: Vec<_> = gateway_audit()
            .registry()
            .declarations()
            .filter(|declaration| declaration.class().owner() == "gateway")
            .map(|declaration| declaration.ocsf().class_uid)
            .collect();
        assert_eq!(declared.len(), 7);
    }
}
