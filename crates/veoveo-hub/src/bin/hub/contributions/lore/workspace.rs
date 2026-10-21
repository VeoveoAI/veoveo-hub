//! Native personal workspace provisioning, owned by the executable contribution.
mod client;
mod descriptor;
mod service;
mod store;
pub(crate) use service::WorkspaceService;
pub(crate) use store::{Association, Mode, State, WorkspaceStore};
use veoveo_hub::computers::ComputerOnboardingStatus as Status;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{DeploymentId, RepositoryId};

/// An installation default, not an authority to access any repository.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonalWorkspace {
    pub deployment: DeploymentId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Selection {
    pub deployment: DeploymentId,
    /// Omitted means create a repository with durable preallocated native IDs.
    /// Present means use the caller's existing native access, without new rights.
    pub repository: Option<RepositoryId>,
}

pub use crate::native_source_ids::BranchId;
