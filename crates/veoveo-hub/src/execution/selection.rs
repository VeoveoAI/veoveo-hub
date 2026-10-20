//! Capture one authenticated request's selection. Headers never carry authority.
use std::collections::BTreeMap;

use veoveo_hub_sdk::{EnvironmentSelector, ResolvedComposition};

use super::{EnvironmentBase, ExecutionError};
use crate::{AuthenticatedSubject, HubCatalog, HubState};

#[derive(Debug, thiserror::Error)]
pub enum ExecutionAdmissionError {
    #[error("MCP execution routing requires an explicitly configured default environment")]
    Unconfigured,
    #[error(transparent)]
    Selection(#[from] ExecutionError),
    #[error("captured execution composition is unavailable")]
    MissingCapture,
    #[error("execution composition storage is unavailable: {0}")]
    Storage(#[from] anyhow::Error),
}

pub async fn admit_execution(
    state: &HubState,
    catalog: &HubCatalog,
    subject: &mut AuthenticatedSubject,
    selector: Option<EnvironmentSelector>,
) -> Result<ResolvedComposition, ExecutionAdmissionError> {
    let configuration = catalog
        .control_plane()
        .execution
        .as_ref()
        .ok_or(ExecutionAdmissionError::Unconfigured)?;
    // Execution selection belongs to the authenticated principal, independently of
    // business WorkContext membership/output authority. Delegation retains it.
    let tenant = subject
        .principal
        .tenant
        .as_ref()
        .ok_or(ExecutionError::Forbidden)?;
    let principal = &subject.principal.id;
    let composition = if let Some(captured) = &subject.execution {
        if selector.is_some_and(|selector| selector.environment() != captured.environment) {
            return Err(ExecutionError::Forbidden.into());
        }
        configuration.authorize_capture(captured.environment, tenant, principal)?;
        state
            .hub_store()
            .composition(&captured.composition)
            .await?
            .filter(|composition| composition.context() == *captured)
            .ok_or(ExecutionAdmissionError::MissingCapture)?
    } else {
        let environment = selector.map_or(configuration.default_environment, |selector| {
            selector.environment()
        });
        configuration.authorize_capture(environment, tenant, principal)?;
        let mut pins = BTreeMap::new();
        let mut current = environment;
        // Only the first reachable pin is needed; it already captures inherited targets.
        while let Some(base) = &configuration.environments[&current].base {
            match base {
                EnvironmentBase::Follow { environment } => current = *environment,
                EnvironmentBase::Pin { revision, .. } => {
                    let composition = state
                        .hub_store()
                        .composition(revision)
                        .await?
                        .ok_or(ExecutionAdmissionError::MissingCapture)?;
                    pins.insert(revision.clone(), composition);
                    break;
                }
            }
        }
        let composition = configuration.resolve(environment, tenant, principal, &pins)?;
        state.hub_store().retain_composition(&composition).await?;
        composition
    };
    subject.execution = Some(composition.context());
    Ok(composition)
}
