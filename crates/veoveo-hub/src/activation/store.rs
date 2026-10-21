use anyhow::{Context, Result, ensure};
use chrono::Utc;
use sqlx::{Connection as _, PgConnection, types::Json};
use veoveo_hub_sdk::{DeploymentId, ResolvedComponent};
use veoveo_store::PlatformStore;
use veoveo_types::BuildId;

use super::*;
use crate::{control_plane::HubControlPlane, execution::EnvironmentOwner};

#[derive(Clone)]
pub struct ActivationStore {
    store: PlatformStore,
}
impl ActivationStore {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    /// Control head first, activation head second: all reservation/publication uses
    /// this lock order, including across Hub processes. No slow I/O holds the locks.
    pub async fn reserve(
        &self,
        key: ActivationKey,
        idempotency: ApplyIdempotencyKey,
        inputs: BuildInputs,
    ) -> Result<AcceptedApply> {
        let mut tx = self.store.pool().begin().await?;
        let accepted = Self::reserve_on(&mut tx, key, idempotency, inputs).await?;
        tx.commit().await?;
        Ok(accepted)
    }

    /// Append a reservation to a caller-owned publication transaction. This method
    /// still locks/rereads current control itself; native source rows can use this
    /// same connection without an untyped state extension or second transaction.
    pub async fn reserve_on(
        connection: &mut PgConnection,
        key: ActivationKey,
        idempotency: ApplyIdempotencyKey,
        inputs: BuildInputs,
    ) -> Result<AcceptedApply> {
        inputs.recipe.validate()?;
        let control = sqlx::query_file!("src/activation/current.sql")
            .fetch_one(&mut *connection)
            .await?
            .control_plane
            .0;
        let (template, storage) = key.admit(&control, &inputs.runtime_template)?;
        let runtime = AdmittedRuntime::capture(template, storage);
        let environment = key.environment.as_uuid();
        if let Some(row) = sqlx::query_file!(
            "src/activation/replay.sql",
            key.owner.tenant.as_str(),
            key.owner.principal.as_str(),
            environment,
            key.component.as_str(),
            idempotency.as_str()
        )
        .fetch_optional(&mut *connection)
        .await?
        {
            row.accepted.0.admit(&control)?;
            ensure!(
                row.accepted.0.inputs == inputs,
                ActivationError::IdempotencyConflict
            );
            return Ok(row.accepted.0);
        }
        let previous = sqlx::query_file!(
            "src/activation/head.sql",
            key.owner.tenant.as_str(),
            key.owner.principal.as_str(),
            environment,
            key.component.as_str()
        )
        .fetch_optional(&mut *connection)
        .await?;
        let generation = previous
            .map_or(Some(1), |head| head.generation.checked_add(1))
            .context("activation generation exhausted")?;
        let accepted = AcceptedApply {
            runtime,
            key,
            idempotency,
            inputs,
            generation: Generation::new(generation)?,
            build: BuildId::new(),
            deployment: DeploymentId::new(),
        };
        let build = accepted.build.as_uuid();
        let deployment = accepted.deployment.as_uuid();
        sqlx::query_file!(
            "src/activation/insert.sql",
            build,
            deployment,
            accepted.key.owner.tenant.as_str(),
            accepted.key.owner.principal.as_str(),
            environment,
            accepted.key.component.as_str(),
            generation,
            accepted.idempotency.as_str(),
            Json(&accepted) as _,
            Utc::now()
        )
        .execute(&mut *connection)
        .await?;
        sqlx::query_file!(
            "src/activation/reserve.sql",
            accepted.key.owner.tenant.as_str(),
            accepted.key.owner.principal.as_str(),
            environment,
            accepted.key.component.as_str(),
            generation,
            build
        )
        .execute(&mut *connection)
        .await?;
        Ok(accepted)
    }

    /// Reads never materialize source, create a worker, or change desired intent.
    /// Admission of a new candidate is another phase write: same control/head
    /// transaction and exact current Registering generation, never a stale job.
    pub(crate) async fn admit_worker_on(
        connection: &mut PgConnection,
        control: &HubControlPlane,
        accepted: &AcceptedApply,
    ) -> Result<()> {
        accepted.admit(control)?;
        lock_current(connection, accepted).await?;
        let state = read(connection, &accepted.key.owner, accepted.build).await?;
        ensure!(
            state.accepted == *accepted && matches!(state.state, BuildState::Registering { .. }),
            ActivationError::InvalidTransition
        );
        Ok(())
    }
    pub async fn status(
        &self,
        owner: &EnvironmentOwner,
        build: BuildId,
    ) -> Result<ActivationStatus> {
        let mut tx = self.store.pool().begin().await?;
        sqlx::query_file!("src/activation/snapshot.sql")
            .execute(&mut *tx)
            .await?;
        let status = read(&mut tx, owner, build).await?;
        let current = sqlx::query_file!("src/activation/current_read.sql")
            .fetch_one(&mut *tx)
            .await?
            .control_plane
            .0;
        let record = current
            .execution
            .as_ref()
            .and_then(|e| e.environments.get(&status.accepted.key.environment))
            .ok_or(ActivationError::UnknownEnvironment)?;
        ensure!(&record.owner == owner, ActivationError::Forbidden);
        tx.commit().await?;
        Ok(status)
    }

    pub async fn source_ready(
        &self,
        accepted: &AcceptedApply,
        source: MaterializedSource,
    ) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::source_ready_on(&mut connection, accepted, source).await
    }
    /// Writes through the caller's owned native activation Singleton connection.
    pub async fn source_ready_on(
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
        source: MaterializedSource,
    ) -> Result<()> {
        let expected_directory = format!("sources/{}", accepted.build);
        ensure!(
            source.directory.as_str() == expected_directory,
            ActivationError::InvalidPath
        );
        if let Some(expected) = &accepted.inputs.expected_source {
            ensure!(
                expected == &source.identity,
                ActivationError::SourceMismatch
            );
        }
        Self::transition_on(connection, accepted, |status| {
            match &status.state {
                BuildState::Preparing => status.state = BuildState::SourceReady { source },
                BuildState::SourceReady { source: previous } if previous == &source => {}
                _ => return Err(ActivationError::InvalidTransition.into()),
            }
            Ok(())
        })
        .await
    }
    pub async fn begin_build(&self, accepted: &AcceptedApply) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::begin_build_on(&mut connection, accepted).await
    }
    /// Writes through the caller's owned native activation Singleton connection.
    pub async fn begin_build_on(
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
    ) -> Result<()> {
        Self::transition_on(connection, accepted, |status| {
            let source = match &status.state {
                BuildState::SourceReady { source } | BuildState::Building { source } => {
                    source.clone()
                }
                _ => return Err(ActivationError::InvalidTransition.into()),
            };
            status.state = BuildState::Building { source };
            Ok(())
        })
        .await
    }
    pub async fn output_ready(
        &self,
        accepted: &AcceptedApply,
        output: VerifiedOutput,
    ) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::output_ready_on(&mut connection, accepted, output).await
    }
    /// Writes through the caller's owned native activation Singleton connection.
    pub async fn output_ready_on(
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
        output: VerifiedOutput,
    ) -> Result<()> {
        if matches!(&accepted.inputs.recipe.worker, WorkerSpec::Compiled { .. }) {
            ensure!(
                output.executable.as_str() == format!("builds/{}/service", accepted.build),
                ActivationError::InvalidPath
            );
        }
        Self::transition_on(connection, accepted, |status| {
            let source = match &status.state {
                BuildState::Building { source } => source.clone(),
                BuildState::Registering {
                    source,
                    output: previous,
                } => {
                    ensure!(previous == &output, ActivationError::OutputMismatch);
                    source.clone()
                }
                _ => return Err(ActivationError::InvalidTransition.into()),
            };
            status.state = BuildState::Registering { source, output };
            Ok(())
        })
        .await
    }
    /// Recover the same retained worker after native process/registration loss.
    /// The serving pointer and selected deployment stay unchanged; normal current
    /// owner/admission/generation checks still fence the subsequent resume/reissue.
    pub async fn recover_worker(&self, accepted: &AcceptedApply) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::recover_worker_on(&mut connection, accepted).await
    }
    /// Writes through the caller's owned native activation Singleton connection.
    pub async fn recover_worker_on(
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
    ) -> Result<()> {
        Self::transition_on(connection, accepted, |status| {
            status.state = match &status.state {
                BuildState::Serving { source, output } => BuildState::Registering {
                    source: source.clone(),
                    output: output.clone(),
                },
                BuildState::Registering { .. } => return Ok(()),
                _ => return Err(ActivationError::InvalidTransition.into()),
            };
            Ok(())
        })
        .await
    }
    pub async fn fail(&self, accepted: &AcceptedApply, code: FailureCode) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::fail_on(&mut connection, accepted, code).await
    }
    /// Writes through the caller's owned native activation Singleton connection.
    pub async fn fail_on(
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
        code: FailureCode,
    ) -> Result<()> {
        Self::transition_on(connection, accepted, |status| {
            let failure = match &status.state {
                BuildState::Preparing => BuildFailure::Source { code },
                BuildState::SourceReady { source } | BuildState::Building { source } => {
                    BuildFailure::Build {
                        source: source.clone(),
                        code,
                    }
                }
                BuildState::Registering { source, output } => BuildFailure::Worker {
                    source: source.clone(),
                    output: output.clone(),
                    code,
                },
                BuildState::Failed { .. } => return Ok(()),
                _ => return Err(ActivationError::InvalidTransition.into()),
            };
            status.state = BuildState::Failed { failure };
            Ok(())
        })
        .await
    }
    /// Fresh current authority is checked by transition. Retry preserves accepted
    /// inputs/generation and any verified immutable source/output; it cannot revive
    /// a superseded apply or silently turn changed input into the same build.
    pub async fn retry(&self, accepted: &AcceptedApply) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::retry_on(&mut connection, accepted).await
    }
    /// Writes through the caller's owned native activation Singleton connection.
    pub async fn retry_on(connection: &mut PgConnection, accepted: &AcceptedApply) -> Result<()> {
        Self::transition_on(connection, accepted, |status| {
            status.state = match &status.state {
                BuildState::Failed {
                    failure: BuildFailure::Source { .. },
                } => BuildState::Preparing,
                BuildState::Failed {
                    failure: BuildFailure::Build { source, .. },
                } => BuildState::SourceReady {
                    source: source.clone(),
                },
                BuildState::Failed {
                    failure: BuildFailure::Worker { source, output, .. },
                } => BuildState::Registering {
                    source: source.clone(),
                    output: output.clone(),
                },
                BuildState::Withdrawn => return Err(ActivationError::InvalidTransition.into()),
                state => state.clone(),
            };
            Ok(())
        })
        .await
    }

    async fn transition_on(
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
        change: impl FnOnce(&mut ActivationStatus) -> Result<()>,
    ) -> Result<()> {
        let mut tx = connection.begin().await?;
        let control = sqlx::query_file!("src/activation/current.sql")
            .fetch_one(&mut *tx)
            .await?
            .control_plane
            .0;
        accepted.admit(&control)?;
        lock_current(&mut tx, accepted).await?;
        let mut status = read(&mut tx, &accepted.key.owner, accepted.build).await?;
        ensure!(
            status.accepted == *accepted,
            ActivationError::InvalidTransition
        );
        change(&mut status)?;
        write(&mut tx, &status).await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(super) async fn read(
    connection: &mut PgConnection,
    owner: &EnvironmentOwner,
    build: BuildId,
) -> Result<ActivationStatus> {
    let row = sqlx::query_file!(
        "src/activation/read.sql",
        build.as_uuid(),
        owner.tenant.as_str(),
        owner.principal.as_str()
    )
    .fetch_optional(connection)
    .await?
    .ok_or(ActivationError::NotFound)?;
    let serving = match (row.serving_build, row.serving_deployment) {
        (Some(build), Some(deployment)) => Some(ServingWorker {
            build: BuildId::parse(build.to_string())?,
            deployment: DeploymentId::parse(deployment.to_string())?,
        }),
        (None, None) => None,
        _ => anyhow::bail!("inconsistent serving worker identity"),
    };
    Ok(ActivationStatus {
        accepted: row.accepted.0,
        state: row.state.0,
        current: row.current,
        serving,
    })
}
async fn lock_current(connection: &mut PgConnection, accepted: &AcceptedApply) -> Result<()> {
    let key = &accepted.key;
    let head = sqlx::query_file!(
        "src/activation/head.sql",
        key.owner.tenant.as_str(),
        key.owner.principal.as_str(),
        key.environment.as_uuid(),
        key.component.as_str()
    )
    .fetch_optional(connection)
    .await?
    .ok_or(ActivationError::NotFound)?;
    ensure!(
        head.generation == accepted.generation.get() && head.desired == accepted.build.as_uuid(),
        ActivationError::Superseded
    );
    Ok(())
}
async fn write(connection: &mut PgConnection, status: &ActivationStatus) -> Result<()> {
    sqlx::query_file!(
        "src/activation/transition.sql",
        status.accepted.build.as_uuid(),
        Json(&status.state) as _,
        Utc::now()
    )
    .execute(connection)
    .await?;
    Ok(())
}

/// Passed only by the trusted activation path after app readiness and registration.
/// The live transaction repeats every authority/generation/deployment check.
#[derive(Clone, Debug)]
pub struct ActivationPublication {
    pub accepted: AcceptedApply,
}

/// Trusted native preparation publishes exact placement before scoped registration,
/// without selecting the candidate or altering any environment/source authority.
pub struct ActivationPlacement {
    pub accepted: AcceptedApply,
    pub binding: crate::control_plane::NativeUpstreamBinding,
}
impl ActivationPlacement {
    pub(crate) async fn place_on(
        &self,
        connection: &mut PgConnection,
        current: &HubControlPlane,
        proposed: &HubControlPlane,
    ) -> Result<()> {
        self.accepted.admit(current)?;
        ensure!(
            self.binding.workspace == self.accepted.runtime.workspace && self.binding.port > 0,
            ActivationError::RuntimeNotAdmitted
        );
        lock_current(connection, &self.accepted).await?;
        let status = read(connection, &self.accepted.key.owner, self.accepted.build).await?;
        ensure!(
            status.accepted == self.accepted
                && matches!(status.state, BuildState::Registering { .. }),
            ActivationError::InvalidTransition
        );
        let mut expected = current.clone();
        let native = expected
            .native_upstreams
            .as_mut()
            .ok_or(ActivationError::RuntimeNotAdmitted)?;
        if let Some(existing) = native
            .bindings
            .iter()
            .find(|b| b.endpoint == self.binding.endpoint)
        {
            ensure!(
                existing == &self.binding,
                ActivationError::RuntimeNotAdmitted
            );
        } else {
            native.bindings.push(self.binding.clone());
        }
        ensure!(
            serde_json::to_value(expected)? == serde_json::to_value(proposed)?,
            ActivationError::InvalidTransition
        );
        Ok(())
    }
}
impl ActivationPublication {
    pub async fn select_on(
        &self,
        connection: &mut PgConnection,
        current: &HubControlPlane,
        control: &HubControlPlane,
    ) -> Result<()> {
        let accepted = &self.accepted;
        accepted.admit(current)?;
        accepted.admit(control)?;
        let (_, binding) = accepted
            .key
            .admit(current, &accepted.inputs.runtime_template)?;
        let (_, proposed_binding) = accepted
            .key
            .admit(control, &accepted.inputs.runtime_template)?;
        ensure!(
            proposed_binding == binding,
            ActivationError::RuntimeNotAdmitted
        );
        lock_current(connection, accepted).await?;
        let mut status = read(connection, &accepted.key.owner, accepted.build).await?;
        ensure!(
            status.accepted == *accepted,
            ActivationError::InvalidTransition
        );
        let (source, output) = match &status.state {
            BuildState::Registering { source, output } => (source.clone(), output.clone()),
            _ => return Err(ActivationError::InvalidTransition.into()),
        };
        let component = control
            .execution
            .as_ref()
            .and_then(|e| e.environments.get(&accepted.key.environment))
            .and_then(|e| e.components.get(&accepted.key.component));
        ensure!(
            component
                == Some(&ResolvedComponent::Deployment {
                    deployment: accepted.deployment
                }),
            ActivationError::InvalidTransition
        );
        let registered = sqlx::query_file!(
            "src/activation/registered.sql",
            accepted.deployment.as_uuid(),
            accepted.key.component.as_str()
        )
        .fetch_optional(&mut *connection)
        .await?
        .context("candidate deployment not admitted")?;
        ensure!(
            registered.registered
                && registered.storage.0 == *binding
                && registered.environment == Some(accepted.key.environment.as_uuid())
                && registered.owner_tenant.as_deref() == Some(accepted.key.owner.tenant.as_str())
                && registered.owner_principal.as_deref()
                    == Some(accepted.key.owner.principal.as_str()),
            ActivationError::Forbidden
        );
        let runtime = registered
            .runtime
            .context("candidate native binding missing")?
            .0;
        ensure!(
            runtime.endpoint == registered.metadata.0.registration.endpoint,
            ActivationError::RuntimeNotAdmitted
        );
        ensure!(
            current
                .native_upstreams
                .as_ref()
                .is_some_and(|n| n.bindings.contains(&runtime)),
            ActivationError::RuntimeNotAdmitted
        );
        ensure!(
            control
                .native_upstreams
                .as_ref()
                .is_some_and(|n| n.bindings.contains(&runtime)),
            ActivationError::RuntimeNotAdmitted
        );
        sqlx::query_file!(
            "src/activation/select.sql",
            accepted.key.owner.tenant.as_str(),
            accepted.key.owner.principal.as_str(),
            accepted.key.environment.as_uuid(),
            accepted.key.component.as_str(),
            accepted.build.as_uuid(),
            accepted.generation.get()
        )
        .execute(&mut *connection)
        .await?;
        status.state = BuildState::Serving { source, output };
        status.serving = Some(ServingWorker {
            build: accepted.build,
            deployment: accepted.deployment,
        });
        write(connection, &status).await?;
        Ok(())
    }
}
