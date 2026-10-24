//! Capture one authenticated request's routing. Headers never carry authority.
use std::collections::BTreeMap;

use veoveo_hub_sdk::{DeploymentId, ResolvedComponent, ResolvedComposition, ServerSlug};

use super::BranchName;
use crate::{AuthenticatedSubject, HubCatalog, HubState, registration::RegistrationSnapshot};

mod kernel;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionAdmissionError {
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

/// The request's routing: the composition it already captured, as a delegated call or
/// a retained Task carries it, or else a new one from the current registrations and
/// the principal's selection, which is retained before it routes anything.
pub async fn admit_execution(
    state: &HubState,
    catalog: &HubCatalog,
    subject: &mut AuthenticatedSubject,
) -> Result<Admitted, ExecutionAdmissionError> {
    let admitted = if let Some(captured) = &subject.execution {
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
        let base = catalog
            .control_plane()
            .execution
            .as_ref()
            .and_then(|execution| execution.base_branch.as_ref());
        let selection = state.hub_store().selection(&subject.principal.id).await?;
        let admitted = route(catalog.registrations(), base, &selection);
        state
            .hub_store()
            .retain_composition(&admitted.composition)
            .await?;
        admitted
    };
    subject.execution = Some(admitted.composition.context());
    Ok(admitted)
}

/// Route each server to its newest registered deployment that is not an instance, then
/// [`select_instances`] over it.
pub(super) fn route(
    registrations: &RegistrationSnapshot,
    base: Option<&BranchName>,
    selection: &BTreeMap<ServerSlug, BranchName>,
) -> Admitted {
    let routed = registrations
        .namespaces()
        .iter()
        .map(|(server, namespace)| {
            (
                server.clone(),
                ResolvedComponent::Deployment {
                    deployment: namespace.routed,
                },
            )
        })
        .collect();
    select_instances(routed, registrations, base, selection)
}

/// Route each server that has registered instances, or that `selection` names, to the
/// instance of its selected branch, or else of the base branch. A server without that
/// instance is unavailable, never served by another branch or by a deployment that is
/// not an instance. Other servers keep their deployment in `routed`.
pub(super) fn select_instances(
    routed: BTreeMap<ServerSlug, ResolvedComponent>,
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
    let mut components = routed;
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
        composition: ResolvedComposition::new(components),
        unavailable,
    }
}

/// The one instance among `served` of the selected branch, or else of the base branch.
fn instance(
    served: &[(&BranchName, DeploymentId)],
    selected: Option<&BranchName>,
    base: Option<&BranchName>,
) -> Result<DeploymentId, Unavailable> {
    kernel::resolve(served, selected, base).map_err(|cause| match cause {
        kernel::Failure::SelectedMissing(branch) => Unavailable::Selected(branch.clone()),
        kernel::Failure::BaseMissing(branch) => Unavailable::Base(branch.clone()),
        kernel::Failure::NoBase => Unavailable::NoBaseBranch,
        kernel::Failure::Ambiguous(branch) => Unavailable::Ambiguous(branch.clone()),
    })
}
