//! The one source-facing definition and operation DTOs. Native source vocabulary
//! belongs to this executable, not Core, SDK or generic activation state.
use crate::native_source_ids::{BranchId, DeploymentId, RepositoryId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use veoveo_hub::activation::{
    ApplyIdempotencyKey, RuntimeTemplateRef, ServiceRecipe, SnapshotIdentity,
};
use veoveo_hub::execution::EnvironmentBase;
use veoveo_hub_sdk::{EnvironmentId, ServerSlug};

pub use crate::native_source_ids::NativeRevision;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LoreSelection {
    Follow { branch: BranchId },
    Pin { revision: NativeRevision },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceSelector {
    /// Reserved BEFORE upload. Completion attaches an authorized Artifact ID.
    LocalSnapshot {
        #[serde(flatten)]
        snapshot: SnapshotIdentity,
    },
    Lore {
        deployment: DeploymentId,
        repository: RepositoryId,
        selection: LoreSelection,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentDefinition {
    pub source: SourceSelector,
    pub runtime_template: RuntimeTemplateRef,
    pub recipe: ServiceRecipe,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentDefinition {
    pub id: EnvironmentId,
    pub display_name: String,
    pub base: Option<EnvironmentBase>,
    pub components: BTreeMap<ServerSlug, ComponentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyEnvironment {
    pub idempotency: ApplyIdempotencyKey,
    pub definition: EnvironmentDefinition,
}

/// Completion attaches authorized transfer identity to the already reserved digest,
/// size and generation. It never creates a newer apply when a slow upload finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteLocalSnapshot {
    pub artifact: veoveo_mcp_artifact::contract::ArtifactId,
}

/// Native association to the SAME source-facing environment document, not another
/// manifest. The native repository/branch IDs are stable; path is an ordinary file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoreDefinitionSource {
    pub deployment: DeploymentId,
    pub repository: RepositoryId,
    pub branch: BranchId,
    pub path: veoveo_hub::activation::RelativePath,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FollowDefinition {
    /// Current source selector generation, or None before its first association.
    /// A stale retry cannot restore Follow over a later explicit local apply.
    pub replaces: Option<veoveo_hub::activation::Generation>,
    pub source: LoreDefinitionSource,
}

/// Same bound as private helper input; source configuration is small control data.
pub const MAX_DEFINITION_BYTES: usize = 1024 * 1024;
