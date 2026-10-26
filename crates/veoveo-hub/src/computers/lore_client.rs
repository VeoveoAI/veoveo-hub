//! A Computer's own Lore client. Its identifier is the Computer's native name and the Hub
//! keeps only the digest of its secret, which each custody handoff replaces. The gateway
//! refreshes the sandbox's Lore token with it; the Hub issues each token for the
//! Computer's owner.
use anyhow::Result;
use veoveo_types::{PrincipalId, TenantId};

use super::identity::is_native_name;
use crate::{registration, store::HubStore};

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
