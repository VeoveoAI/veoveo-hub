//! Capture one authenticated request's selection. Headers never carry authority.
use std::collections::BTreeMap;

use veoveo_hub_sdk::{
    DeploymentId, EnvironmentSelector, ResolvedComponent, ResolvedComposition, ServerSlug,
};

use super::{BranchName, EnvironmentBase, ExecutionError};
use crate::{AuthenticatedSubject, HubCatalog, HubState, registration::RegistrationSnapshot};

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

/// Why a request's selection routes no instance of a server.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unavailable {
    #[error("the selected branch `{0}` has no registered instance")]
    Selected(BranchName),
    #[error("the base branch `{0}` has no registered instance")]
    Base(BranchName),
    #[error("several instances of branch `{0}` are registered")]
    Ambiguous(BranchName),
    #[error("the installation names no base branch")]
    NoBaseBranch,
}

/// One request's admitted routing: the captured composition, and why it routes no
/// instance of a server that has instances or that the principal's selection names.
/// A captured composition carries no causes; its routing never changes.
#[derive(Debug, Clone)]
pub struct Admitted {
    pub composition: ResolvedComposition,
    pub unavailable: BTreeMap<ServerSlug, Unavailable>,
}

pub async fn admit_execution(
    state: &HubState,
    catalog: &HubCatalog,
    subject: &mut AuthenticatedSubject,
    selector: Option<EnvironmentSelector>,
) -> Result<Admitted, ExecutionAdmissionError> {
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
    let admitted = if let Some(captured) = &subject.execution {
        if selector.is_some_and(|selector| selector.environment() != captured.environment) {
            return Err(ExecutionError::Forbidden.into());
        }
        configuration.authorize_capture(captured.environment, tenant, principal)?;
        let composition = state
            .hub_store()
            .composition(&captured.composition)
            .await?
            .filter(|composition| composition.context() == *captured)
            .ok_or(ExecutionAdmissionError::MissingCapture)?;
        Admitted {
            composition,
            unavailable: BTreeMap::new(),
        }
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
        let configured = configuration.resolve(environment, tenant, principal, &pins)?;
        let selection = state.hub_store().selection(principal).await?;
        let admitted = select_instances(
            configured,
            catalog.registrations(),
            configuration.base_branch.as_ref(),
            &selection,
        );
        state
            .hub_store()
            .retain_composition(&admitted.composition)
            .await?;
        admitted
    };
    subject.execution = Some(admitted.composition.context());
    Ok(admitted)
}

/// Route each server that has registered instances, or that `selection` names, to the
/// instance of its selected branch, or else of the base branch. A server without that
/// instance is unavailable, never served by another branch or by the configured
/// component. Other servers keep their configured component.
pub(super) fn select_instances(
    configured: ResolvedComposition,
    registrations: &RegistrationSnapshot,
    base: Option<&BranchName>,
    selection: &BTreeMap<ServerSlug, BranchName>,
) -> Admitted {
    let mut instances = BTreeMap::<&ServerSlug, Vec<_>>::new();
    for server in registrations.iter() {
        if let Some(branch) = &server.branch {
            instances
                .entry(&server.slug)
                .or_default()
                .push((branch, server.deployment));
        }
    }
    for server in selection.keys() {
        instances.entry(server).or_default();
    }
    let mut components = configured.components().clone();
    let mut unavailable = BTreeMap::new();
    for (server, served) in instances {
        let component = match instance(&served, selection.get(server), base) {
            Ok(deployment) => ResolvedComponent::Deployment { deployment },
            Err(cause) => {
                unavailable.insert(server.clone(), cause);
                ResolvedComponent::Unavailable {}
            }
        };
        components.insert(server.clone(), component);
    }
    Admitted {
        composition: ResolvedComposition::new(configured.context().environment, components),
        unavailable,
    }
}

/// The one instance among `served` of the selected branch, or else of the base branch.
fn instance(
    served: &[(&BranchName, DeploymentId)],
    selected: Option<&BranchName>,
    base: Option<&BranchName>,
) -> Result<DeploymentId, Unavailable> {
    let (branch, missing) = match (selected, base) {
        (Some(branch), _) => (branch, Unavailable::Selected(branch.clone())),
        (None, Some(branch)) => (branch, Unavailable::Base(branch.clone())),
        (None, None) => return Err(Unavailable::NoBaseBranch),
    };
    let mut matching = served.iter().filter(|(served, _)| *served == branch);
    match (matching.next(), matching.next()) {
        (Some((_, deployment)), None) => Ok(*deployment),
        (None, _) => Err(missing),
        (Some(_), Some(_)) => Err(Unavailable::Ambiguous(branch.clone())),
    }
}
