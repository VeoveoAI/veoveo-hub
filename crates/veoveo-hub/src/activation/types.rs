use std::{collections::BTreeMap, path::PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::{DeploymentId, EnvironmentId, ServerSlug, registration::StorageBinding};
use veoveo_types::{BuildId, PrincipalId, Sha256Digest, TenantId};

use crate::{control_plane::HubControlPlane, execution::EnvironmentOwner};

/// Internal keyset batch size; callers walk every page without truncating intent.
pub const ACTIVATION_PAGE_SIZE: i64 = 64;

#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct RuntimeTemplateRef(String);
#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct ApplyIdempotencyKey(String);

/// Relative paths have no parent/root components, including on another platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct RelativePath(String);
impl RelativePath {
    pub fn parse(value: impl Into<String>) -> Result<Self, ActivationError> {
        let value = value.into();
        if value.is_empty()
            || value.contains(':')
            || value.contains(['\\', '\0'])
            || value.starts_with('/')
            || value.split('/').any(|part| part.is_empty() || part == "..")
        {
            return Err(ActivationError::InvalidPath);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandSpec {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: RelativePath,
}

/// Cargo/npm/etc may write to the persistent cache without using traversal paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "root", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuildOutput {
    Source { path: RelativePath },
    Cache { path: RelativePath },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerSpec {
    Compiled {
        output: BuildOutput,
        args: Vec<String>,
        cwd: RelativePath,
    },
    Interpreted {
        interpreter: String,
        entrypoint: RelativePath,
        args: Vec<String>,
        cwd: RelativePath,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceRecipe {
    #[serde(default)]
    pub check: Vec<CommandSpec>,
    #[serde(default)]
    pub build: Vec<CommandSpec>,
    pub worker: WorkerSpec,
}
impl<'de> Deserialize<'de> for ServiceRecipe {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            check: Vec<CommandSpec>,
            #[serde(default)]
            build: Vec<CommandSpec>,
            worker: WorkerSpec,
        }
        let wire = Wire::deserialize(d)?;
        let recipe = Self {
            check: wire.check,
            build: wire.build,
            worker: wire.worker,
        };
        recipe.validate().map_err(serde::de::Error::custom)?;
        Ok(recipe)
    }
}
fn executable(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\\', '\0']) && !value.split('/').any(|part| part == "..")
}
impl ServiceRecipe {
    pub fn validate(&self) -> Result<(), ActivationError> {
        let mut commands = self.check.iter().chain(&self.build);
        if commands.any(|c| !executable(&c.executable) || c.args.iter().any(|a| a.contains('\0'))) {
            return Err(ActivationError::InvalidRecipe);
        }
        let args = match &self.worker {
            WorkerSpec::Compiled { args, .. } => args,
            WorkerSpec::Interpreted {
                interpreter, args, ..
            } => {
                if !interpreter.starts_with('/') || !executable(interpreter) {
                    return Err(ActivationError::InvalidRecipe);
                }
                args
            }
        };
        const RESERVED: &[&str] = &[
            "--deployment-id",
            "--key-directory",
            "--port",
            "--public-base-url",
            "--hub-issuer",
            "--registration-secret",
            "--describe-registration",
        ];
        if args.iter().any(|a| {
            a.contains('\0')
                || RESERVED
                    .iter()
                    .any(|r| a == r || a.strip_prefix(r).is_some_and(|tail| tail.starts_with('=')))
        }) {
            return Err(ActivationError::InvalidRecipe);
        }
        Ok(())
    }
}

/// Operator-managed live eligibility. No user/group ACL or duplicated native inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeTemplateAdmission {
    pub workspace: String,
    pub template: String,
    pub policy_file: PathBuf,
    /// Trusted native attachments; source definitions cannot mint credentials.
    #[serde(default)]
    pub providers: Vec<String>,
    pub eligible: Vec<RuntimeEligibility>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeEligibility {
    pub environment: EnvironmentId,
    pub component: ServerSlug,
    pub storage: StorageBinding,
}
pub type RuntimeTemplateAdmissions = BTreeMap<RuntimeTemplateRef, RuntimeTemplateAdmission>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationKey {
    pub owner: EnvironmentOwner,
    pub environment: EnvironmentId,
    pub component: ServerSlug,
}
impl ActivationKey {
    /// Fresh current control authority, not the caller's claimed ownership/binding.
    pub fn admit<'a>(
        &self,
        control: &'a HubControlPlane,
        reference: &RuntimeTemplateRef,
    ) -> Result<(&'a RuntimeTemplateAdmission, &'a StorageBinding), ActivationError> {
        let admitted = self.admit_retained(control, reference)?;
        if !control
            .execution
            .as_ref()
            .and_then(|e| e.environments.get(&self.environment))
            .is_some_and(|e| e.components.contains_key(&self.component))
        {
            return Err(ActivationError::ComponentWithdrawn);
        }
        Ok(admitted)
    }
    /// Eligibility remains explicit after source selection changes. This is not
    /// authority by itself: historical recovery additionally requires a retained
    /// canonical Task route and the immutable admitted deployment/native placement.
    pub fn admit_retained<'a>(
        &self,
        control: &'a HubControlPlane,
        reference: &RuntimeTemplateRef,
    ) -> Result<(&'a RuntimeTemplateAdmission, &'a StorageBinding), ActivationError> {
        let execution = control
            .execution
            .as_ref()
            .ok_or(ActivationError::UnknownEnvironment)?;
        let environment = execution
            .environments
            .get(&self.environment)
            .ok_or(ActivationError::UnknownEnvironment)?;
        if environment.owner != self.owner {
            return Err(ActivationError::Forbidden);
        }
        let admission = execution
            .runtime_templates
            .get(reference)
            .ok_or(ActivationError::RuntimeNotAdmitted)?;
        let eligible = admission
            .eligible
            .iter()
            .find(|e| e.environment == self.environment && e.component == self.component)
            .ok_or(ActivationError::RuntimeNotAdmitted)?;
        let StorageBinding::SharedLogicalSchema { server } = &eligible.storage;
        if server != &self.component {
            return Err(ActivationError::RuntimeNotAdmitted);
        }
        Ok((admission, &eligible.storage))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotIdentity {
    /// Producer-owned content identity, independently verified by materialization.
    pub digest: Sha256Digest,
    pub size: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildInputs {
    pub runtime_template: RuntimeTemplateRef,
    pub definition_digest: Sha256Digest,
    /// Local applies reserve this before upload; Lore resolution may fill it later.
    pub expected_source: Option<SnapshotIdentity>,
    pub recipe: ServiceRecipe,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaterializedSource {
    pub identity: SnapshotIdentity,
    pub directory: RelativePath,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedOutput {
    pub digest: Sha256Digest,
    pub executable: RelativePath,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptedApply {
    pub runtime: AdmittedRuntime,
    pub key: ActivationKey,
    pub generation: Generation,
    pub build: BuildId,
    pub deployment: DeploymentId,
    pub idempotency: ApplyIdempotencyKey,
    pub inputs: BuildInputs,
}
/// Positive, checked desired generation. Only the store advances it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Generation(i64);
impl Generation {
    pub fn new(value: i64) -> Result<Self, ActivationError> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(ActivationError::InvalidGeneration)
        }
    }
    pub fn get(self) -> i64 {
        self.0
    }
}
impl<'de> Deserialize<'de> for Generation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(i64::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    Materialization,
    Check,
    Build,
    Registration,
    Readiness,
    NativeUnavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuildFailure {
    Source {
        code: FailureCode,
    },
    Build {
        source: MaterializedSource,
        code: FailureCode,
    },
    Worker {
        source: MaterializedSource,
        output: VerifiedOutput,
        code: FailureCode,
    },
}
/// Required state payloads make incomplete source/output unrepresentable as serving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuildState {
    Preparing,
    SourceReady {
        source: MaterializedSource,
    },
    Building {
        source: MaterializedSource,
    },
    Registering {
        source: MaterializedSource,
        output: VerifiedOutput,
    },
    Serving {
        source: MaterializedSource,
        output: VerifiedOutput,
    },
    Failed {
        failure: BuildFailure,
    },
    Withdrawn,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServingWorker {
    pub build: BuildId,
    pub deployment: DeploymentId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivationStatus {
    pub accepted: AcceptedApply,
    pub state: BuildState,
    pub current: bool,
    pub serving: Option<ServingWorker>,
}

#[derive(Debug, thiserror::Error)]
pub enum ActivationError {
    #[error("component is not owned by this environment")]
    ComponentWithdrawn,
    #[error("environment contains installation-managed components without source definitions")]
    UnmanagedEnvironment,
    #[error("environment definition is invalid")]
    InvalidDefinition,
    #[error("activation generation must be positive")]
    InvalidGeneration,
    #[error("environment is not configured")]
    UnknownEnvironment,
    #[error("environment belongs to another owner or tenant")]
    Forbidden,
    #[error("runtime/storage is not admitted for this environment component")]
    RuntimeNotAdmitted,
    #[error("relative path escapes its root")]
    InvalidPath,
    #[error("worker recipe is invalid or overrides reserved invocation identity")]
    InvalidRecipe,
    #[error("idempotency key is already bound to different inputs")]
    IdempotencyConflict,
    #[error("a newer source apply was accepted")]
    Superseded,
    #[error("activation state transition is invalid")]
    InvalidTransition,
    #[error("materialized source differs from the reserved snapshot")]
    SourceMismatch,
    #[error("build output is immutable")]
    OutputMismatch,
    #[error("activation is not found")]
    NotFound,
}

impl EnvironmentOwner {
    pub fn activation_key(
        &self,
        environment: EnvironmentId,
        component: ServerSlug,
    ) -> ActivationKey {
        ActivationKey {
            owner: self.clone(),
            environment,
            component,
        }
    }
}

/// Derive owner only from the authenticated principal at the public API boundary.
pub fn owner(tenant: &TenantId, principal: &PrincipalId) -> EnvironmentOwner {
    EnvironmentOwner {
        tenant: tenant.clone(),
        principal: principal.clone(),
    }
}

/// Snapshot only the selected native template/storage inputs, not unrelated eligibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdmittedRuntime {
    pub workspace: String,
    pub template: String,
    pub policy_file: PathBuf,
    pub providers: Vec<String>,
    pub storage: StorageBinding,
}
impl AdmittedRuntime {
    pub fn capture(admission: &RuntimeTemplateAdmission, storage: &StorageBinding) -> Self {
        Self {
            workspace: admission.workspace.clone(),
            template: admission.template.clone(),
            policy_file: admission.policy_file.clone(),
            providers: admission.providers.clone(),
            storage: storage.clone(),
        }
    }
}
impl AcceptedApply {
    pub fn admit(&self, control: &HubControlPlane) -> Result<(), ActivationError> {
        let (admission, storage) = self.key.admit(control, &self.inputs.runtime_template)?;
        if AdmittedRuntime::capture(admission, storage) != self.runtime {
            return Err(ActivationError::RuntimeNotAdmitted);
        }
        Ok(())
    }
    pub fn admit_retained(&self, control: &HubControlPlane) -> Result<(), ActivationError> {
        let (admission, storage) = self
            .key
            .admit_retained(control, &self.inputs.runtime_template)?;
        if AdmittedRuntime::capture(admission, storage) != self.runtime {
            return Err(ActivationError::RuntimeNotAdmitted);
        }
        Ok(())
    }
}
