//! Native hints plus authoritative reread. No event ledger/replay assumptions.
use super::*;
use crate::activation::{
    lore,
    source::{self, ControlDocument, SourceRecord},
};
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    service: ActivationService,
    accepted: AcceptedApply,
    record: SourceRecord,
    cancellation: CancellationToken,
) {
    loop {
        let watch = watch(&service, &accepted, &record, &cancellation);
        let result =
            tokio::select! {_ = cancellation.cancelled() => return, result = watch => result};
        match result {
            Ok(()) => return,
            Err(error) if error.is::<ActivationError>() => return,
            Err(_) => {}
        }
        tokio::select! {_ = cancellation.cancelled() => return, _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}}
    }
}
async fn watch(
    service: &ActivationService,
    accepted: &AcceptedApply,
    record: &SourceRecord,
    cancellation: &CancellationToken,
) -> Result<()> {
    let SourceSelector::Lore {
        deployment: id,
        repository,
        selection: LoreSelection::Follow { branch },
    } = &record.component.source
    else {
        anyhow::bail!(ActivationError::InvalidDefinition)
    };
    let (deployment, token) =
        lore::credential(&service.state, &record.authority, id, repository).await?;
    let mut stream = lore::subscribe(&deployment, repository, &token).await?;
    drop(token);
    let mut periodic = tokio::time::interval(std::time::Duration::from_secs(5));
    periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // First tick is immediate AFTER Subscribe response, including reconnects.
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            _ = periodic.tick() => {},
            event = stream.message() => { event?.context("native notification stream ended")?; },
        }
        let status = service
            .store
            .status(&accepted.key.owner, accepted.build)
            .await?;
        ensure!(status.current, ActivationError::Superseded);
        // Re-admit current native grants/client on every read, even when the
        // stream remains healthy. Events are hints and may be silently dropped.
        let (deployment, token) =
            lore::credential(&service.state, &record.authority, id, repository).await?;
        let revision = lore::resolve(&deployment, repository, branch, &token).await?;
        drop(token);
        let mut connection = service
            .state
            .control_store
            .platform_store()
            .pool()
            .acquire()
            .await?;
        let current_record = source::read_on(&mut connection, accepted.build).await?;
        if current_record
            .resolved_revision
            .as_ref()
            .is_none_or(|previous| previous == &revision)
        {
            continue;
        }
        drop(connection);
        if let Some(next) = advance(service, accepted, &current_record, revision).await? {
            service
                .enqueue(
                    next.key.owner.clone(),
                    record.authority.background_audit(),
                    next,
                    None,
                )
                .await;
            return Ok(()); // scan installs the replacement generation's subscription
        }
    }
}
async fn advance(
    service: &ActivationService,
    predecessor: &AcceptedApply,
    record: &SourceRecord,
    revision: NativeRevision,
) -> Result<Option<AcceptedApply>> {
    let mut tx = service
        .state
        .control_store
        .platform_store()
        .pool()
        .begin()
        .await?;
    let current = sqlx::query_file!("src/bin/hub/activation/source_control_lock.sql")
        .fetch_one(&mut *tx)
        .await?
        .control_plane
        .0;
    let guard = sqlx::query_file!(
        "src/bin/hub/activation/source_guard.sql",
        predecessor.build.as_uuid()
    )
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        guard.current && guard.accepted.0 == *predecessor,
        ActivationError::Superseded
    );
    predecessor.admit(&current)?;
    let stored = source::read_on(&mut tx, predecessor.build).await?;
    ensure!(&stored == record, ActivationError::Superseded);
    if stored.resolved_revision.as_ref() == Some(&revision) {
        return Ok(None);
    }
    let SourceSelector::Lore {
        deployment,
        repository,
        selection: LoreSelection::Follow { .. },
    } = &stored.component.source
    else {
        anyhow::bail!(ActivationError::Superseded)
    };
    let catalog =
        HubCatalog::from_control_plane(current, service.state.catalog.current().admission())?;
    let _ = lore::credential_in(&catalog, &stored.authority, deployment, repository).await?;
    // Captured predecessor makes A→B→A a new transition, not a digest-only replay.
    let idempotency = transition_key(predecessor.build, &revision)?;
    let next = source::reserve_resolved_on(
        &mut tx,
        predecessor.key.clone(),
        idempotency,
        stored.component,
        stored.authority,
        Some(revision),
    )
    .await?;
    tx.commit().await?;
    Ok(Some(next))
}
fn transition_key(predecessor: BuildId, revision: &NativeRevision) -> Result<ApplyIdempotencyKey> {
    Ok(ApplyIdempotencyKey::parse(format!(
        "follow-{}",
        hex::encode(Sha256::digest(serde_json::to_vec(&(
            predecessor,
            revision
        ))?))
    ))?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_branch_reads_coalesce_but_later_rewind_is_a_new_transition() {
        let a = NativeRevision::parse("a".repeat(64)).unwrap();
        let b = NativeRevision::parse("b".repeat(64)).unwrap();
        let first = BuildId::new();
        let second = BuildId::new();
        assert_eq!(
            transition_key(first, &b).unwrap(),
            transition_key(first, &b).unwrap()
        );
        assert_ne!(
            transition_key(first, &a).unwrap(),
            transition_key(second, &a).unwrap()
        );
    }
}
