//! Hub-owned routing of each request: one immutable composition from logical slug to
//! deployment. A slug with factory instances, or one the principal's selection names,
//! routes by branch; any other slug routes to its deployment with live registrations.
//! There are no named environments.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{IdProfile, IdProfileSpec, IdentifierError};

/// Durable routing configuration published with the installation's live control revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionConfiguration {
    /// The branch whose instance serves a server that a principal's selection does not
    /// name. Without it, such a server with instances is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<BranchName>,
}

/// A Lore branch name: 1 to 1,000 bytes, Lore's limit, without control characters.
#[veoveo_types::id(text(LoreBranchNames))]
pub struct BranchName(String);

/// Lore's longest branch name in bytes (`lore-revision/src/branch.rs`, `MAX_NAME_LEN`).
const MAX_BRANCH_NAME_BYTES: usize = 1000;

#[doc(hidden)]
pub struct LoreBranchNames;
impl IdProfile for LoreBranchNames {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        veoveo_types::identifier_syntax::validate_claim_text(value)?;
        if value.len() > MAX_BRANCH_NAME_BYTES {
            return Err(IdentifierError::new(value, "must be at most 1,000 bytes"));
        }
        Ok(())
    });
}

mod selection;
pub use selection::{Admitted, ExecutionAdmissionError, Unavailable, admit_execution};

#[cfg(test)]
mod tests;
