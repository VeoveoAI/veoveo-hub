//! Hub-owned admission and resolution of execution routing outputs.
//!
//! These are trusted control-plane records, not the source-facing environment manifest.
//! Source adapters later publish admitted deployment outputs through this boundary.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::{
    CompositionRevision, EnvironmentId, ResolvedComponent, ResolvedComposition, ServerSlug,
};
use veoveo_types::{IdProfile, IdProfileSpec, IdentifierError, PrincipalId, TenantId};

/// Durable routing configuration published with the installation's live control revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionConfiguration {
    pub default_environment: EnvironmentId,
    pub environments: BTreeMap<EnvironmentId, EnvironmentRecord>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentOwner {
    pub tenant: TenantId,
    pub principal: PrincipalId,
}

/// An owned environment. Label changes have no routing or permission semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentRecord {
    pub owner: EnvironmentOwner,
    pub display_name: String,
    #[serde(default)]
    pub readers: BTreeSet<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<EnvironmentBase>,
    /// Owned admitted outputs only: absence inherits, explicit unavailable blocks inheritance.
    /// An existing selected deployment retains its identity when temporarily unhealthy.
    pub components: BTreeMap<ServerSlug, ResolvedComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnvironmentBase {
    Follow {
        environment: EnvironmentId,
    },
    Pin {
        environment: EnvironmentId,
        revision: CompositionRevision,
    },
}

impl EnvironmentBase {
    pub fn environment(&self) -> EnvironmentId {
        match self {
            Self::Follow { environment } | Self::Pin { environment, .. } => *environment,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("execution environment is not configured")]
    UnknownEnvironment,
    #[error("execution environment is not authorized for this tenant and principal")]
    Forbidden,
    #[error("execution environment inheritance is cyclic")]
    Cycle,
    #[error("pinned execution composition is not retained")]
    UnknownComposition,
    #[error("pinned execution composition belongs to another environment")]
    PinEnvironmentMismatch,
}

impl EnvironmentRecord {
    pub fn allows(&self, tenant: &TenantId, principal: &PrincipalId) -> bool {
        &self.owner.tenant == tenant
            && (&self.owner.principal == principal || self.readers.contains(principal))
    }
}

impl ExecutionConfiguration {
    /// Check structural edges, including pins. Access revocation must remain publishable;
    /// current inheritance authority is checked when resolving a selection.
    pub fn validate(&self) -> Result<(), ExecutionError> {
        if !self.environments.contains_key(&self.default_environment) {
            return Err(ExecutionError::UnknownEnvironment);
        }
        for (environment, record) in &self.environments {
            let mut seen = BTreeSet::from([*environment]);
            let mut current = record;
            while let Some(base) = &current.base {
                let id = base.environment();
                if !seen.insert(id) {
                    return Err(ExecutionError::Cycle);
                }
                let parent = self
                    .environments
                    .get(&id)
                    .ok_or(ExecutionError::UnknownEnvironment)?;
                current = parent;
            }
        }
        Ok(())
    }

    /// Current access to the selected environment of an already captured operation.
    /// Current base edits cannot reinterpret a retained composition. Component and
    /// data policy are still checked for each operation against current authority.
    pub fn authorize_capture(
        &self,
        environment: EnvironmentId,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<(), ExecutionError> {
        let record = self
            .environments
            .get(&environment)
            .ok_or(ExecutionError::UnknownEnvironment)?;
        if record.allows(tenant, principal) {
            Ok(())
        } else {
            Err(ExecutionError::Forbidden)
        }
    }

    /// Resolve once against one admitted configuration and immutable retained pins.
    /// Current access checks apply even when reading a previously admitted base pin.
    pub fn resolve(
        &self,
        environment: EnvironmentId,
        tenant: &TenantId,
        principal: &PrincipalId,
        pinned: &BTreeMap<CompositionRevision, ResolvedComposition>,
    ) -> Result<ResolvedComposition, ExecutionError> {
        self.validate()?;
        let mut chain = Vec::new();
        let mut current = environment;
        let mut components = BTreeMap::new();
        loop {
            let record = self
                .environments
                .get(&current)
                .ok_or(ExecutionError::UnknownEnvironment)?;
            if !record.allows(tenant, principal) {
                return Err(ExecutionError::Forbidden);
            }
            chain.push(record);
            match &record.base {
                None => break,
                Some(EnvironmentBase::Follow { environment }) => {
                    let parent = self
                        .environments
                        .get(environment)
                        .ok_or(ExecutionError::UnknownEnvironment)?;
                    if !parent.allows(&record.owner.tenant, &record.owner.principal) {
                        return Err(ExecutionError::Forbidden);
                    }
                    current = *environment;
                }
                Some(EnvironmentBase::Pin {
                    environment,
                    revision,
                }) => {
                    let parent = self
                        .environments
                        .get(environment)
                        .ok_or(ExecutionError::UnknownEnvironment)?;
                    if !parent.allows(tenant, principal)
                        || !parent.allows(&record.owner.tenant, &record.owner.principal)
                    {
                        return Err(ExecutionError::Forbidden);
                    }
                    let base = pinned
                        .get(revision)
                        .ok_or(ExecutionError::UnknownComposition)?;
                    if base.context().environment != *environment || base.revision() != revision {
                        return Err(ExecutionError::PinEnvironmentMismatch);
                    }
                    components = base.components().clone();
                    break;
                }
            }
        }
        for record in chain.into_iter().rev() {
            components.extend(record.components.clone());
        }
        Ok(ResolvedComposition::new(environment, components))
    }
}

mod selection;
pub use selection::{Admitted, ExecutionAdmissionError, Unavailable, admit_execution};

#[cfg(test)]
mod tests;
