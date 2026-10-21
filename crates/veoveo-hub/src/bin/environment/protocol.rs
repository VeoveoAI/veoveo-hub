//! Private stdin protocol shared by the native runtime helper and Hub adapter.
//! Signed transfers and registration admission never appear in argv or durable state.
use serde::{Deserialize, Serialize};
use veoveo_hub::activation::{MaterializedSource, ServiceRecipe, SnapshotIdentity, VerifiedOutput};
use veoveo_hub_sdk::{DeploymentId, TokenIssuer, registration::ServerEndpoint};
use veoveo_mcp_artifact::contract::SignedTransfer;
use veoveo_types::BuildId;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaterializeRequest {
    pub build: BuildId,
    pub snapshot: SnapshotIdentity,
    pub transfer: SignedTransfer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildRequest {
    pub build: BuildId,
    pub source: MaterializedSource,
    pub recipe: ServiceRecipe,
}

/// Values chosen by activation, never supplied by a source recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerIdentity {
    pub build: BuildId,
    pub deployment: DeploymentId,
    pub endpoint: ServerEndpoint,
    pub public_base_url: url::Url,
    pub issuer: TokenIssuer,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerRequest {
    pub build: BuildRequest,
    pub output: VerifiedOutput,
    pub identity: WorkerIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandFailure {
    pub phase: CommandPhase,
    pub index: usize,
    pub exit_code: Option<i32>,
    /// Bounded combined stdout/stderr tail. No transfer or admission inputs logged.
    pub output: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPhase {
    Check,
    Build,
}

/// Surface the SDK's existing saved-registration result, never an authority grant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationState {
    Ready,
    FreshAdmissionRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuildResult {
    Ready {
        output: VerifiedOutput,
        check_ms: u64,
        build_ms: u64,
    },
    Failed {
        failure: CommandFailure,
        check_ms: u64,
        build_ms: u64,
    },
}

/// Immutable native source provenance belongs to the executable adapter, not Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoreSource {
    pub remote: url::Url,
    pub repository: crate::native_source_ids::RepositoryId,
    pub revision: crate::native_source_ids::NativeRevision,
}

/// Private Exec stdin only. Deliberately not Debug and never persisted. Native
/// Lore's existing access-token argv is scoped to this owner runtime's child.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaterializeLoreRequest {
    pub build: BuildId,
    pub source: LoreSource,
    pub access_token: String,
}
