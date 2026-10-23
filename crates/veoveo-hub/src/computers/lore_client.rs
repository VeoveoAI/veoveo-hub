//! A Computer's own Lore client. Its identifier is the Computer's native name and the Hub
//! keeps only the digest of its secret, which each custody handoff replaces. The gateway
//! refreshes the sandbox's Lore token with it; the Hub issues each token for the
//! Computer's owner.
use std::collections::BTreeSet;

use anyhow::Result;
use uuid::Uuid;
use veoveo_types::{PrincipalId, TenantId};

use super::{
    gateway,
    identity::is_native_name,
    provisioning::{LORE_PROVIDER, rotate},
};
use crate::{control_plane::ComputerGateway, registration, store::HubStore};

/// The person a Computer's own Lore client acts for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComputerOwner {
    pub tenant: TenantId,
    pub principal: PrincipalId,
}

/// The owner of the Computer whose own Lore client is `client` with this secret, or none
/// when no Computer's client has both.
pub async fn lore_client(
    store: &HubStore,
    client: &str,
    secret: &str,
) -> Result<Option<ComputerOwner>> {
    if !is_native_name(client) {
        return Ok(None);
    }
    Ok(store
        .computer_of_lore_client(&registration::digest(secret))
        .await?
        .filter(|(id, _, _)| id.native_name() == client)
        .map(|(_, tenant, principal)| ComputerOwner { tenant, principal }))
}

/// How many of the owners' Computers renewed their Lore access, and how many failed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoreRenewal {
    pub renewed: usize,
    pub failed: usize,
}

/// Renew the Lore credential of each owner's Computer now, on the gateway it was
/// provisioned on: the gateway mints a token with the owner's current assignments
/// behind the sandbox's unchanged placeholder. Best effort: a Computer that cannot renew
/// is logged and the others still renew. A stopped Computer renews as well, since the
/// gateway keeps its provider and refreshes it on its own schedule; a Computer whose
/// custody handoff has not completed gets a new token when the handoff rotates it
/// before the sandbox starts.
pub async fn renew_lore_access(
    store: &HubStore,
    owners: &BTreeSet<ComputerOwner>,
) -> Result<LoreRenewal> {
    let mut renewal = LoreRenewal::default();
    let mut gateways: Vec<(ComputerGateway, openshell_sdk::OpenShellClient)> = Vec::new();
    for owner in owners {
        let Some(computer) = store
            .owned_computer(&owner.tenant, &owner.principal)
            .await?
        else {
            continue;
        };
        if computer.handoff.is_some() || !computer.ready {
            continue;
        }
        let config = &computer.provisioning.config;
        let client = match gateways.iter().find(|(known, _)| known == &config.gateway) {
            Some((_, client)) => Ok(client.clone()),
            None => gateway::connect(&config.gateway).await.inspect(|client| {
                gateways.push((config.gateway.clone(), client.clone()));
            }),
        };
        let renewed = match client {
            Ok(client) => {
                rotate(
                    &client,
                    &computer.native_name(),
                    LORE_PROVIDER,
                    &config.lore_provider.credential_key,
                    Uuid::now_v7().to_string(),
                    "renew Lore credential",
                )
                .await
            }
            Err(error) => Err(error),
        };
        match renewed {
            Ok(()) => renewal.renewed += 1,
            Err(error) => {
                renewal.failed += 1;
                tracing::warn!(
                    computer = %computer.id,
                    principal = %owner.principal,
                    "a Computer's Lore access did not renew after its owner's assignments changed: {error:#}"
                );
            }
        }
    }
    Ok(renewal)
}
