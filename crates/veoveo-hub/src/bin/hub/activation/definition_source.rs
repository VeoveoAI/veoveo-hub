//! Definition-file selector ownership. Slow native reads are outside transactions;
//! publication compares the captured source generation on the control transaction.
use super::{
    source::{ControlDocument, SourceAuthority},
    types::*,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, types::Json};
use veoveo_hub::{
    activation::{ActivationError, Generation},
    execution::EnvironmentOwner,
};
use veoveo_hub_sdk::EnvironmentId;

pub(super) type DefinitionDocument = sqlx::types::Json<DefinitionSourceState>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DefinitionMode {
    Local,
    Lore {
        source: LoreDefinitionSource,
        observed: Option<NativeRevision>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DefinitionSourceState {
    pub environment: EnvironmentId,
    pub owner: EnvironmentOwner,
    pub generation: Generation,
    pub authority: SourceAuthority,
    pub mode: DefinitionMode,
}
pub(super) enum DefinitionOrigin {
    Local,
    Follow {
        expected: Box<DefinitionSourceState>,
        revision: NativeRevision,
    },
}
pub(super) async fn read_on(
    connection: &mut PgConnection,
    environment: EnvironmentId,
) -> Result<Option<DefinitionSourceState>> {
    Ok(sqlx::query_file!(
        "src/bin/hub/activation/definition_source_read.sql",
        environment.as_uuid()
    )
    .fetch_optional(connection)
    .await?
    .map(|r| r.document.0))
}
pub(super) async fn prepare_on(
    connection: &mut PgConnection,
    owner: &EnvironmentOwner,
    environment: EnvironmentId,
    authority: &SourceAuthority,
    origin: &DefinitionOrigin,
) -> Result<DefinitionSourceState> {
    let previous = sqlx::query_file!(
        "src/bin/hub/activation/definition_source_lock.sql",
        environment.as_uuid()
    )
    .fetch_optional(&mut *connection)
    .await?
    .map(|r| r.document.0);
    if let Some(previous) = &previous {
        ensure!(&previous.owner == owner, ActivationError::Forbidden);
    }
    let mode = match origin {
        DefinitionOrigin::Local => DefinitionMode::Local,
        DefinitionOrigin::Follow { expected, revision } => {
            ensure!(
                previous.as_ref() == Some(expected.as_ref()),
                ActivationError::Superseded
            );
            let DefinitionMode::Lore { source, .. } = &expected.mode else {
                anyhow::bail!(ActivationError::Superseded)
            };
            let current = sqlx::query_file!("src/bin/hub/activation/definition_control.sql")
                .fetch_one(&mut *connection)
                .await?
                .control_plane
                .0;
            let catalog = veoveo_hub::HubCatalog::from_control_plane(
                current,
                crate::bindings::catalog_admission()?,
            )?;
            let _ = super::lore::credential_in(
                &catalog,
                authority,
                &source.deployment,
                &source.repository,
            )
            .await?;
            DefinitionMode::Lore {
                source: source.clone(),
                observed: Some(revision.clone()),
            }
        }
    };
    Ok(DefinitionSourceState {
        environment,
        owner: owner.clone(),
        authority: authority.clone(),
        generation: advance(previous.as_ref().map(|p| p.generation))?,
        mode,
    })
}
fn advance(previous: Option<Generation>) -> Result<Generation> {
    Ok(Generation::new(
        previous
            .map_or(Some(1), |g| g.get().checked_add(1))
            .ok_or(ActivationError::InvalidGeneration)?,
    )?)
}
pub(super) async fn persist_on(
    connection: &mut PgConnection,
    state: &DefinitionSourceState,
) -> Result<()> {
    let updated = sqlx::query_file!(
        "src/bin/hub/activation/definition_source_write.sql",
        state.environment.as_uuid(),
        state.owner.tenant.as_str(),
        state.owner.principal.as_str(),
        state.generation.get(),
        Json(state) as _
    )
    .execute(connection)
    .await?;
    ensure!(updated.rows_affected() == 1, ActivationError::Forbidden);
    Ok(())
}
