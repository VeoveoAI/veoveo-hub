//! Thin reconciliation of accepted durable work in the trusted Hub process. No
//! separate scheduler/inventory. Native OpenShell/s6 retain process ownership.
use super::*;
use crate::activation::{definition_source::DefinitionDocument, source::SourceRecord};
use sqlx::types::Json;
use tokio_util::sync::CancellationToken;
use veoveo_store::{TenantId as DatabaseTenantId, deterministic_tenant_id};

pub(super) fn spawn(
    service: ActivationService,
    cancellation: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut followers = std::collections::BTreeMap::new();
        let mut definitions = std::collections::BTreeMap::new();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => { drain_definitions(&mut definitions).await; return; },
                _ = interval.tick() => {},
            }
            let result = tokio::select! {
                _ = cancellation.cancelled() => { drain_definitions(&mut definitions).await; return; },
                result = recover(&service) => result,
            };
            if result.is_err() {
                // Do not log private provider/native details or inputs.
                tracing::warn!("environment recovery scan unavailable");
            }
            let result = tokio::select! {
                _ = cancellation.cancelled() => { drain_definitions(&mut definitions).await; return; },
                result = refresh_followers(&service, &cancellation, &mut followers) => result,
            };
            if result.is_err() {
                tracing::warn!("native source subscription set unavailable");
            }
            // This task owns child-reader completion on shutdown: cancellation
            // reaches its native child runner, which kills/waits before returning.
            if refresh_definitions(&service, &cancellation, &mut definitions)
                .await
                .is_err()
            {
                tracing::warn!("native definition subscription set unavailable");
            }
        }
    })
}
fn owned_environments(
    service: &ActivationService,
) -> Vec<(veoveo_hub_sdk::EnvironmentId, EnvironmentOwner)> {
    service
        .state
        .catalog
        .current()
        .control_plane()
        .execution
        .as_ref()
        .map(|execution| {
            execution
                .environments
                .iter()
                .map(|(id, record)| (*id, record.owner.clone()))
                .collect()
        })
        .unwrap_or_default()
}

type DefinitionWatch = (Generation, CancellationToken, tokio::task::JoinHandle<()>);
async fn drain_definitions(
    definitions: &mut std::collections::BTreeMap<veoveo_hub_sdk::EnvironmentId, DefinitionWatch>,
) {
    for (_, (_, token, task)) in std::mem::take(definitions) {
        token.cancel();
        let _ = task.await;
    }
}
async fn refresh_definitions(
    service: &ActivationService,
    cancellation: &CancellationToken,
    definitions: &mut std::collections::BTreeMap<veoveo_hub_sdk::EnvironmentId, DefinitionWatch>,
) -> Result<()> {
    let mut rows = Vec::new();
    for (environment, owner) in owned_environments(service) {
        if let Some(row) = sqlx::query_file!(
            "src/bin/hub/activation/definition_follow_list.sql",
            environment.as_uuid(),
            owner.tenant.as_str(),
            owner.principal.as_str()
        )
        .fetch_optional(service.state.control_store.platform_store().pool())
        .await?
        {
            rows.push(row);
        }
    }
    let wanted = rows
        .iter()
        .map(|r| (r.document.0.environment, r.document.0.generation))
        .collect::<std::collections::BTreeMap<_, _>>();
    // Cancel superseded readers but retain their JoinHandle until it exits; it
    // must reap any live native CLI before replacement/drop of its temp tree.
    let finished = definitions
        .iter()
        .filter_map(|(id, (generation, token, task))| {
            if wanted.get(id) != Some(generation) {
                token.cancel();
            }
            task.is_finished().then_some(*id)
        })
        .collect::<Vec<_>>();
    for id in finished {
        definitions.remove(&id);
    }
    for row in rows {
        let state = row.document.0;
        if definitions.contains_key(&state.environment) {
            continue;
        }
        let token = cancellation.child_token();
        let task = tokio::spawn(super::definition_follow::run(
            service.clone(),
            state.clone(),
            token.clone(),
        ));
        definitions.insert(state.environment, (state.generation, token, task));
    }
    Ok(())
}
async fn refresh_followers(
    service: &ActivationService,
    cancellation: &CancellationToken,
    followers: &mut std::collections::BTreeMap<
        BuildId,
        (CancellationToken, tokio::task::JoinHandle<()>),
    >,
) -> Result<()> {
    let mut rows = Vec::new();
    for (environment, owner) in owned_environments(service) {
        let mut after = String::new();
        loop {
            let page = sqlx::query_file!(
                "src/bin/hub/activation/follow_list.sql",
                owner.tenant.as_str(),
                owner.principal.as_str(),
                environment.as_uuid(),
                &after,
                ACTIVATION_PAGE_SIZE
            )
            .fetch_all(service.state.control_store.platform_store().pool())
            .await?;
            let Some(last) = page.last() else {
                break;
            };
            after.clone_from(&last.component);
            rows.extend(page);
        }
    }
    let wanted = rows
        .iter()
        .map(|r| r.accepted.0.build)
        .collect::<std::collections::BTreeSet<_>>();
    followers.retain(|id, (token, task)| {
        if !wanted.contains(id) || task.is_finished() {
            token.cancel();
            false
        } else {
            true
        }
    });
    for row in rows {
        let accepted = row.accepted.0;
        if followers.contains_key(&accepted.build) {
            continue;
        }
        let token = cancellation.child_token();
        let task = tokio::spawn(super::follow::run(
            service.clone(),
            accepted.clone(),
            row.document.0,
            token.clone(),
        ));
        followers.insert(accepted.build, (token, task));
    }
    Ok(())
}
async fn recover(service: &ActivationService) -> Result<()> {
    let mut rows = Vec::new();
    for (environment, owner) in owned_environments(service) {
        let mut after = String::new();
        loop {
            let page = sqlx::query_file!(
                "src/bin/hub/activation/recover_list.sql",
                owner.tenant.as_str(),
                owner.principal.as_str(),
                environment.as_uuid(),
                &after,
                ACTIVATION_PAGE_SIZE
            )
            .fetch_all(service.state.control_store.platform_store().pool())
            .await?;
            let Some(last) = page.last() else {
                break;
            };
            after.clone_from(&last.component);
            rows.extend(page);
        }
    }
    let health = service.state.registrations.health().await?;
    for row in rows {
        let accepted = row.accepted.0;
        let source = row.document.0;
        if accepted
            .admit(service.state.catalog.current().control_plane())
            .is_err()
        {
            continue;
        }
        if !recoverable(&row.state.0) {
            continue;
        }
        if matches!(
            row.state.0,
            BuildState::Preparing
                | BuildState::Failed {
                    failure: BuildFailure::Source { .. }
                }
        ) && matches!(
            source.component.source,
            SourceSelector::LocalSnapshot { .. }
        ) {
            // No caller bearer/URL survives restart. An incomplete local transfer
            // requires a fresh authenticated completion, not fabricated authority.
            continue;
        }
        if matches!(row.state.0, BuildState::Serving { .. })
            && health
                .get(&accepted.deployment)
                .is_some_and(|h| h.state == veoveo_hub::contract::HubServerHealthState::Healthy)
        {
            continue;
        }
        // The job acquires the native Singleton before any phase write, then
        // resumes/reissues only this same declaration/key/output.
        // Only typed transient native failures recover at this bounded cadence.
        // Deterministic recipe/start failures need explicit retry/new intent.
        service
            .enqueue(
                accepted.key.owner.clone(),
                source.authority.background_audit(),
                accepted,
                None,
            )
            .await;
    }
    let mut retained = Vec::new();
    let now = Utc::now();
    for (environment, owner) in owned_environments(service) {
        let tenant = deterministic_tenant_id(owner.tenant.as_str())?;
        let mut after = uuid::Uuid::nil();
        loop {
            let page = sqlx::query_file!(
                "src/bin/hub/activation/retained_list.sql",
                owner.tenant.as_str(),
                owner.principal.as_str(),
                environment.as_uuid(),
                after,
                tenant as DatabaseTenantId,
                now,
                ACTIVATION_PAGE_SIZE
            )
            .fetch_all(service.state.control_store.platform_store().pool())
            .await?;
            let Some(last) = page.last() else {
                break;
            };
            after = last.id;
            retained.extend(page);
        }
    }
    for row in retained {
        let accepted = row.accepted.0;
        if health
            .get(&accepted.deployment)
            .is_some_and(|h| h.state == veoveo_hub::contract::HubServerHealthState::Healthy)
        {
            continue;
        }
        let retained = match row.state.0 {
            BuildState::Serving { source, output }
            | BuildState::Registering { source, output }
            | BuildState::Failed {
                failure:
                    BuildFailure::Worker {
                        source,
                        output,
                        code: FailureCode::NativeUnavailable,
                    },
            } => Some((source, output)),
            _ => None,
        };
        if let Some((source, output)) = retained {
            // Current-worker recovery may have entered Registering just before a
            // newer reservation superseded it. Its canonical Tasks retain the
            // same source/output authority; no deterministic failure is retried.
            service.enqueue_retained(accepted, source, output).await;
        }
    }
    Ok(())
}

fn recoverable(state: &BuildState) -> bool {
    match state {
        BuildState::Failed {
            failure:
                BuildFailure::Source { code }
                | BuildFailure::Build { code, .. }
                | BuildFailure::Worker { code, .. },
        } => *code == FailureCode::NativeUnavailable,
        BuildState::Withdrawn => false,
        _ => true,
    }
}
impl ActivationService {
    async fn enqueue_retained(
        &self,
        accepted: AcceptedApply,
        source: MaterializedSource,
        output: VerifiedOutput,
    ) {
        let build = accepted.build;
        if !self.jobs.lock().await.insert(build) {
            return;
        }
        let service = self.clone();
        tokio::spawn(async move {
            let result = tokio::select! {
                _ = service.cancellation.cancelled() => Ok(()),
                result = service.recover_retained(&accepted, source, output) => result,
            };
            if result.is_err() {
                tracing::warn!(%build, "retained worker recovery refused or unavailable");
            }
            service.jobs.lock().await.remove(&build);
        });
    }
    async fn recover_retained(
        &self,
        accepted: &AcceptedApply,
        source: MaterializedSource,
        output: VerifiedOutput,
    ) -> Result<()> {
        use crate::activation::native::{self, protocol};
        use veoveo_store::{Singleton, SingletonName};
        let name = SingletonName::parse(format!(
            "activation.{}.{}",
            accepted.key.environment.as_uuid().simple(),
            accepted.key.component
        ))?;
        let mut role = Singleton::acquire(self.state.control_store.platform_store(), name).await?;
        let current = self
            .state
            .control_store
            .load_active_revision()
            .await?
            .context("authoritative control missing")?;
        accepted.admit_retained(&current.control_plane)?;
        let tenant = deterministic_tenant_id(accepted.key.owner.tenant.as_str())?;
        ensure!(
            sqlx::query_file!(
                "src/registration/store/retained_task.sql",
                tenant as DatabaseTenantId,
                accepted.deployment.as_uuid(),
                Utc::now()
            )
            .fetch_one(role.connection())
            .await?
            .retained,
            "worker no longer has retained canonical Tasks"
        );
        let runtime =
            native::Runtime::retained(&self.state.catalog.current(), role.connection(), accepted)
                .await?;
        runtime.wait_ready().await?;
        let placement = runtime
            .retained_placement(role.connection(), accepted.build)
            .await?;
        let worker = self.worker_request(accepted, placement.port, source, output)?;
        // Existing placement must still be explicitly published. Retained provenance
        // never reconstructs withdrawn authority or selects an old component.
        let binding = veoveo_hub::control_plane::NativeUpstreamBinding {
            endpoint: worker.identity.endpoint.clone(),
            workspace: runtime.identity.workspace.clone(),
            sandbox: runtime.identity.name.clone(),
            port: placement.port,
        };
        ensure!(
            current
                .control_plane
                .native_upstreams
                .as_ref()
                .is_some_and(|n| n.bindings.contains(&binding)),
            "retained placement withdrawn"
        );
        if matches!(
            runtime
                .invoke::<_, protocol::RegistrationState>("resume", &worker)
                .await?,
            protocol::RegistrationState::FreshAdmissionRequired
        ) {
            let metadata: veoveo_hub_sdk::registration::ClientMetadata =
                runtime.invoke("describe", &worker).await?;
            let grant = self
                .state
                .registrations
                .admit_retained_deployment_on(
                    role.connection(),
                    veoveo_hub::registration::DeploymentAdmissionRequest {
                        environment: accepted.key.environment,
                        metadata: metadata.clone(),
                        storage: accepted.runtime.storage.clone(),
                        runtime: binding,
                        expires_at: Utc::now() + chrono::Duration::minutes(5),
                    },
                    accepted,
                )
                .await?;
            #[derive(serde::Serialize)]
            struct Register<'a> {
                worker: &'a protocol::WorkerRequest,
                metadata: veoveo_hub_sdk::registration::ClientMetadata,
                admission: veoveo_hub_sdk::registration::RegistrationAccessToken,
            }
            runtime
                .invoke::<_, ()>(
                    "register",
                    &Register {
                        worker: &worker,
                        metadata,
                        admission: grant.initial_access_token,
                    },
                )
                .await?;
        }
        runtime.invoke::<_, ()>("start", &worker).await?;
        // No source/build/phase/route publication occurs in this path. Actual Task
        // reads still re-admit original capture and current domain ACLs in E2/Core.
        role.release().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_native_transport_failure_is_automatically_retried() {
        for code in [
            FailureCode::Build,
            FailureCode::Check,
            FailureCode::Materialization,
            FailureCode::Registration,
            FailureCode::Readiness,
        ] {
            assert!(!recoverable(&BuildState::Failed {
                failure: BuildFailure::Source { code }
            }));
        }
        assert!(recoverable(&BuildState::Failed {
            failure: BuildFailure::Source {
                code: FailureCode::NativeUnavailable
            }
        }));
        assert!(recoverable(&BuildState::Building {
            source: MaterializedSource {
                identity: SnapshotIdentity {
                    digest: veoveo_types::Sha256Digest::from_bytes([0; 32]),
                    size: 1
                },
                directory: RelativePath::parse("sources/fixture").unwrap()
            }
        }));
        assert!(!recoverable(&BuildState::Withdrawn));
    }
}
