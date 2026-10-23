//! When a control-plane revision becomes active, every Computer whose owner's Lore
//! assignments it changed gets fresh Lore access at once: Hub asks the gateway to renew
//! the Computer's Lore credential, and the gateway mints a token with the owner's
//! current assignments behind the sandbox's unchanged placeholder. Whichever Hub
//! process first sees the revision active, by publishing it, starting on it or adopting
//! it, claims its activation, so this happens once per installation.
use std::collections::BTreeSet;

use anyhow::Result;
use veoveo_hub::{
    HubControlStore, computers::ComputerOwner, control_plane::HubControlPlane, store::HubStore,
};

use super::Lore;

/// Claim the active revision's activation and renew the Lore access of every Computer
/// whose owner's assignments it changed. Best effort: a failure is logged and never
/// fails or delays the revision's publication or adoption.
pub(crate) async fn after_activation(control_store: &HubControlStore, store: &HubStore) {
    let activation = match control_store.claim_activation().await {
        Ok(Some(activation)) => activation,
        Ok(None) => return,
        Err(error) => {
            tracing::error!("failed to claim a control-plane activation: {error:#}");
            return;
        }
    };
    let revision = &activation.revision;
    let owners = match changed_owners(activation.previous.as_ref(), &activation.active) {
        Ok(owners) => owners,
        Err(error) => {
            tracing::error!(%revision, "failed to compare Lore assignments: {error:#}");
            return;
        }
    };
    if owners.is_empty() {
        return;
    }
    match veoveo_hub::computers::renew_lore_access(store, &owners).await {
        Ok(renewal) => tracing::info!(
            %revision,
            owners = owners.len(),
            renewed = renewal.renewed,
            failed = renewal.failed,
            "renewed the Lore access of Computers whose owners' assignments changed"
        ),
        Err(error) => {
            tracing::error!(%revision, "failed to renew Computers' Lore access: {error:#}");
        }
    }
}

/// The owners whose Lore assignments differ between two configurations: an assignment
/// added, removed or given other actions.
pub(crate) fn changed_owners(
    previous: Option<&HubControlPlane>,
    active: &HubControlPlane,
) -> Result<BTreeSet<ComputerOwner>> {
    let assignments = |plane: &HubControlPlane| -> Result<BTreeSet<_>> {
        Ok(Lore::of_plane(plane)?
            .assignments
            .into_iter()
            .map(|a| (a.tenant, a.principal, a.deployment, a.repository, a.actions))
            .collect())
    };
    let before = previous.map(assignments).transpose()?.unwrap_or_default();
    let after = assignments(active)?;
    Ok(before
        .symmetric_difference(&after)
        .map(|(tenant, principal, ..)| ComputerOwner {
            tenant: tenant.clone(),
            principal: principal.clone(),
        })
        .collect())
}
