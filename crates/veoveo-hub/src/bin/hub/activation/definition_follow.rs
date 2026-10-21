//! Native configuration-file notifications feed the ordinary definition publisher.
//! Local publication changes its selector generation in that same transaction.
use super::*;
use crate::activation::{
    definition_source::{self, DefinitionMode, DefinitionSourceState},
    lore, lore_file,
    source::DefinitionAppend,
};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    service: ActivationService,
    expected: DefinitionSourceState,
    cancellation: CancellationToken,
) {
    loop {
        if cancellation.is_cancelled() {
            return;
        }
        match watch(&service, &expected, &cancellation).await {
            Ok(()) => return,
            Err(error) if error.is::<ActivationError>() => return,
            Err(_) => {
                tracing::warn!(environment=%expected.environment,"native environment definition follow unavailable; previous serving selection retained")
            }
        }
        tokio::select! {_=cancellation.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_secs(5))=>{}}
    }
}
async fn watch(
    service: &ActivationService,
    expected: &DefinitionSourceState,
    cancellation: &CancellationToken,
) -> Result<()> {
    let DefinitionMode::Lore { source, observed } = &expected.mode else {
        anyhow::bail!(ActivationError::Superseded)
    };
    let (deployment, token) = lore::credential(
        &service.state,
        &expected.authority,
        &source.deployment,
        &source.repository,
    )
    .await?;
    let mut stream = lore::subscribe(&deployment, &source.repository, &token).await?;
    drop(token);
    let mut periodic = tokio::time::interval(std::time::Duration::from_secs(5));
    periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _=cancellation.cancelled()=>return Ok(()),
            _=periodic.tick()=>{},
            event=stream.message()=>{event?.context("native definition subscription ended")?;},
        }
        let mut connection = service
            .state
            .control_store
            .platform_store()
            .pool()
            .acquire()
            .await?;
        ensure!(
            definition_source::read_on(&mut connection, expected.environment)
                .await?
                .as_ref()
                == Some(expected),
            ActivationError::Superseded
        );
        drop(connection);
        let (deployment, token) = lore::credential(
            &service.state,
            &expected.authority,
            &source.deployment,
            &source.repository,
        )
        .await?;
        let revision =
            lore::resolve(&deployment, &source.repository, &source.branch, &token).await?;
        drop(token);
        if observed.as_ref() == Some(&revision) {
            continue;
        }
        // Do not wrap/drop this future on cancellation: the native reader owns
        // killing and reaping its foreground child before temporary bytes are removed.
        let definition = lore_file::read(
            &service.state,
            &expected.authority,
            source,
            &revision,
            cancellation,
        )
        .await?;
        ensure!(
            definition.id == expected.environment,
            ActivationError::InvalidDefinition
        );
        let append = DefinitionAppend::from_follow(expected.clone(), revision, definition.clone())?;
        service
            .publish_definition(
                append,
                expected.owner.clone(),
                definition,
                expected.authority.background_audit(),
            )
            .await?;
        return Ok(()); // source generation advanced atomically; scan adopts it
    }
}
