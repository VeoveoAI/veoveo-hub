//! One accepted activation, using existing native execution and publication paths.
use super::*;
use crate::activation::{
    native::{self, protocol},
    types::CompleteLocalSnapshot,
};
use std::time::Instant;
use veoveo_hub::control_plane::NativeUpstreamBinding;
use veoveo_hub_sdk::{
    ResolvedComponent, TokenIssuer,
    client::{HubClient, StaticBearer},
    registration::ServerEndpoint,
};
use veoveo_mcp_artifact::contract::CreateDownloadRequest;
use veoveo_store::{Singleton, SingletonName};

/// Ephemeral caller-authorized transport input. Never stored with activation state.
pub(super) struct LocalTransfer {
    pub profile: veoveo_hub_sdk::HubProfileId,
    pub completion: CompleteLocalSnapshot,
    pub bearer: veoveo_hub::BearerToken,
}
impl ActivationService {
    pub(super) async fn activate(
        &self,
        owner: veoveo_hub::execution::EnvironmentOwner,
        audit: veoveo_audit_contract::AuditContext,
        build: BuildId,
        transfer: Option<LocalTransfer>,
    ) -> Result<()> {
        let status = self.store.status(&owner, build).await?;
        ensure!(status.current, ActivationError::Superseded);
        let accepted = status.accepted;
        let role = SingletonName::parse(format!(
            "activation.{}.{}",
            accepted.key.environment.as_uuid().simple(),
            accepted.key.component
        ))?;
        let mut role = Singleton::acquire(self.state.control_store.platform_store(), role).await?;
        let result = self
            .activate_owned(&mut role, &owner, &audit, &accepted, transfer)
            .await;
        if let Err(error) = &result {
            // If the role connection ended, this write also fails. Unclassified
            // deterministic errors are phase failures, never implicit retry authority.
            let code = if native::transport_failure(error) {
                FailureCode::NativeUnavailable
            } else {
                match self.store.status(&owner, build).await.map(|s| s.state) {
                    Ok(BuildState::Preparing) => FailureCode::Materialization,
                    Ok(BuildState::SourceReady { .. } | BuildState::Building { .. }) => {
                        FailureCode::Build
                    }
                    _ => FailureCode::Registration,
                }
            };
            let _ = ActivationStore::fail_on(role.connection(), &accepted, code).await;
        }
        role.release().await?;
        result
    }
    async fn activate_owned(
        &self,
        role: &mut Singleton,
        owner: &veoveo_hub::execution::EnvironmentOwner,
        audit: &veoveo_audit_contract::AuditContext,
        accepted: &AcceptedApply,
        transfer: Option<LocalTransfer>,
    ) -> Result<()> {
        let build = accepted.build;
        let fresh = self.store.status(owner, build).await?;
        ensure!(
            fresh.current && fresh.accepted == *accepted,
            ActivationError::Superseded
        );
        let active = self
            .state
            .control_store
            .load_active_revision()
            .await?
            .context("authoritative control missing")?;
        accepted.admit(&active.control_plane)?;
        let runtime =
            native::Runtime::ensure(&self.state.catalog.current(), role.connection(), accepted)
                .await?;
        runtime.wait_ready().await?;
        let mut status = self.store.status(owner, build).await?;
        if matches!(status.state, BuildState::Serving { .. }) {
            ActivationStore::recover_worker_on(role.connection(), accepted).await?;
            status = self.store.status(owner, build).await?;
        }
        if matches!(status.state, BuildState::Failed { .. }) {
            ActivationStore::retry_on(role.connection(), accepted).await?;
            status = self.store.status(owner, build).await?;
        }
        if matches!(status.state, BuildState::Preparing) {
            let source_start = Instant::now();
            let record = crate::activation::source::read_on(role.connection(), build).await?;
            let materialize = match &record.component.source {
                SourceSelector::LocalSnapshot { .. } => {
                    self.materialize_local(
                        accepted,
                        &runtime,
                        transfer
                            .context("local source completion requires fresh caller authority")?,
                    )
                    .await
                }
                SourceSelector::Lore { .. } => {
                    crate::activation::lore::materialize(
                        &self.state,
                        role.connection(),
                        accepted,
                        record,
                        &runtime,
                    )
                    .await
                }
            };
            let source = match materialize {
                Ok(source) => source,
                Err(error) => {
                    ActivationStore::fail_on(
                        role.connection(),
                        accepted,
                        if native::transport_failure(&error) {
                            FailureCode::NativeUnavailable
                        } else {
                            FailureCode::Materialization
                        },
                    )
                    .await?;
                    return Err(error);
                }
            };
            ActivationStore::source_ready_on(role.connection(), accepted, source).await?;
            tracing::info!(%build,deployment=%accepted.deployment,runtime=%runtime.identity.native_id,source_ms=source_start.elapsed().as_millis(),"activation source ready");
            status = self.store.status(owner, build).await?;
        }
        if let BuildState::SourceReady { source } | BuildState::Building { source } = status.state {
            ActivationStore::begin_build_on(role.connection(), accepted).await?;
            let result: protocol::BuildResult = runtime
                .invoke(
                    "build",
                    &protocol::BuildRequest {
                        build,
                        source,
                        recipe: accepted.inputs.recipe.clone(),
                    },
                )
                .await?;
            match result {
                protocol::BuildResult::Ready {
                    output,
                    check_ms,
                    build_ms,
                } => {
                    ActivationStore::output_ready_on(role.connection(), accepted, output).await?;
                    tracing::info!(%build,deployment=%accepted.deployment,check_ms,build_ms,"activation output captured");
                }
                protocol::BuildResult::Failed { failure, .. } => {
                    let code = match failure.phase {
                        protocol::CommandPhase::Check => FailureCode::Check,
                        protocol::CommandPhase::Build => FailureCode::Build,
                    };
                    ActivationStore::fail_on(role.connection(), accepted, code).await?;
                    // Owner retrieval reads the retained bounded diagnostics via
                    // native Exec; compiler output is not broadcast in Hub logs.
                    anyhow::bail!("activation recipe failed; bounded owner diagnostics retained");
                }
            }
            status = self.store.status(owner, build).await?;
        }
        let BuildState::Registering { source, output } = status.state else {
            ensure!(
                matches!(status.state, BuildState::Serving { .. }),
                ActivationError::InvalidTransition
            );
            return Ok(());
        };
        let placement = runtime.placement(role.connection(), build).await?;
        let worker = self.worker_request(accepted, placement.port, source, output)?;
        let binding = NativeUpstreamBinding {
            endpoint: worker.identity.endpoint.clone(),
            workspace: runtime.identity.workspace.clone(),
            sandbox: runtime.identity.name.clone(),
            port: placement.port,
        };
        let metadata: veoveo_hub_sdk::registration::ClientMetadata =
            runtime.invoke("describe", &worker).await?;
        ensure!(
            metadata.registration.slug == accepted.key.component
                && metadata.registration.deployment == accepted.deployment
                && metadata.registration.endpoint == binding.endpoint,
            ActivationError::InvalidTransition
        );
        self.publish_native(role.connection(), audit, accepted, Some(binding.clone()))
            .await?;
        let start = Instant::now();
        if matches!(
            runtime
                .invoke::<_, protocol::RegistrationState>("resume", &worker)
                .await?,
            protocol::RegistrationState::FreshAdmissionRequired
        ) {
            let grant = self
                .state
                .registrations
                .admit_activation_deployment_on(
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
            // register_and_store is explicit trusted same-key fresh admission, including
            // recovery after orphan collection. The ordinary binary then resumes it.
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
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
        // Existing SDK application readiness + exact live registration gate
        // selection. MCP behavior is qualified through ordinary selected sessions.
        self.probe_candidate(role.connection(), accepted, &worker)
            .await?;
        let start_ready_ms = start.elapsed().as_millis();
        let publication = Instant::now();
        self.publish_native(role.connection(), audit, accepted, None)
            .await?;
        tracing::info!(%build,deployment=%accepted.deployment,runtime=%runtime.identity.native_id,start_ready_ms,publication_ms=publication.elapsed().as_millis(),"activation selected");
        Ok(())
    }
    pub(super) fn worker_request(
        &self,
        accepted: &AcceptedApply,
        port: u16,
        source: MaterializedSource,
        output: VerifiedOutput,
    ) -> Result<protocol::WorkerRequest> {
        let build = accepted.build;
        let public_base_url = url::Url::parse(&format!(
            "http://worker-{}.localhost:{}",
            accepted.deployment.as_uuid().simple(),
            port
        ))?;
        let endpoint =
            ServerEndpoint::new(public_base_url.join(&format!("{}/mcp", accepted.key.component))?)?;
        Ok(protocol::WorkerRequest {
            build: protocol::BuildRequest {
                build,
                source,
                recipe: accepted.inputs.recipe.clone(),
            },
            output,
            identity: protocol::WorkerIdentity {
                build,
                deployment: accepted.deployment,
                endpoint,
                public_base_url,
                issuer: TokenIssuer::parse(&self.state.public_base_url)?,
                port,
            },
        })
    }
    async fn materialize_local(
        &self,
        accepted: &AcceptedApply,
        runtime: &native::Runtime,
        transfer: LocalTransfer,
    ) -> Result<MaterializedSource> {
        let build = accepted.build;
        let LocalTransfer {
            profile,
            completion,
            bearer,
        } = transfer;
        let snapshot = accepted
            .inputs
            .expected_source
            .clone()
            .context("apply is not a reserved local snapshot")?;
        let catalog = self.state.catalog.current();
        let endpoint = catalog
            .profile(&profile)
            .context("source transfer profile withdrawn")?
            .protected_resource
            .clone();
        // Source transfer is a platform control operation. It deliberately uses
        // this profile's explicit default composition, never the new candidate's
        // application environment (whose Artifact selection may not exist yet).
        async {
            let client = HubClient::new(
                url::Url::parse(endpoint.as_str())?,
                crate::runtime::current_http_client(&self.state.http),
                StaticBearer::new(bearer.as_str()),
            );
            let signed = client
                .call(CreateDownloadRequest {
                    artifact_id: completion.artifact,
                })
                .await?;
            ensure!(
                signed.artifact.artifact_id == completion.artifact
                    && signed
                        .artifact
                        .size
                        .is_some_and(|s| s.bytes() == snapshot.size),
                ActivationError::SourceMismatch
            );
            drop(client);
            drop(bearer);
            let source: MaterializedSource = runtime
                .invoke(
                    "materialize",
                    &protocol::MaterializeRequest {
                        build,
                        snapshot,
                        transfer: signed.get,
                    },
                )
                .await?;
            Ok::<_, anyhow::Error>(source)
        }
        .await
    }
    pub(super) async fn enqueue(
        &self,
        owner: veoveo_hub::execution::EnvironmentOwner,
        audit: veoveo_audit_contract::AuditContext,
        accepted: AcceptedApply,
        transfer: Option<LocalTransfer>,
    ) {
        let build = accepted.build;
        if !self.jobs.lock().await.insert(build) {
            return;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            let result = tokio::select! {
                _ = worker.cancellation.cancelled() => Ok(()),
                result = worker.activate(owner, audit, build, transfer) => result,
            };
            if let Err(error) = result {
                // Native/provider-controlled errors and URLs do not enter broadcast logs.
                tracing::warn!(%build, "activation operation failed; owner status/diagnostics available");
                drop(error);
            }
            worker.jobs.lock().await.remove(&build);
        });
    }
    async fn probe_candidate(
        &self,
        connection: &mut sqlx::PgConnection,
        accepted: &AcceptedApply,
        worker: &protocol::WorkerRequest,
    ) -> Result<()> {
        self.state.registrations.refresh().await?;
        let wait = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            loop {
                let status = self
                    .store
                    .status(&accepted.key.owner, accepted.build)
                    .await?;
                ensure!(status.current, ActivationError::Superseded);
                let active = self
                    .state
                    .control_store
                    .load_active_revision()
                    .await?
                    .context("authoritative control missing")?;
                accepted.admit(&active.control_plane)?;
                let registered = self
                    .state
                    .registrations
                    .health()
                    .await?
                    .get(&accepted.deployment)
                    .is_some_and(|health| {
                        health.state == veoveo_hub::contract::HubServerHealthState::Healthy
                    });
                if registered
                    && self
                        .state
                        .catalog
                        .current()
                        .native_application_ready(accepted.deployment, &worker.identity.endpoint)
                        .await?
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        })
        .await;
        match wait {
            Ok(result) => result,
            Err(_) => {
                ActivationStore::fail_on(connection, accepted, FailureCode::Readiness).await?;
                anyhow::bail!("candidate application readiness did not become available")
            }
        }
    }
    async fn publish_native(
        &self,
        connection: &mut sqlx::PgConnection,
        audit: &veoveo_audit_contract::AuditContext,
        accepted: &AcceptedApply,
        placement: Option<NativeUpstreamBinding>,
    ) -> Result<()> {
        let _publication = self.state.publication.lock().await;
        let active = self
            .state
            .control_store
            .load_active_revision()
            .await?
            .context("authoritative control missing")?;
        accepted.admit(&active.control_plane)?;
        let mut proposed = active.control_plane.clone();
        if let Some(binding) = &placement {
            let native = proposed
                .native_upstreams
                .as_mut()
                .context("native transport not configured")?;
            if let Some(existing) = native
                .bindings
                .iter()
                .find(|v| v.endpoint == binding.endpoint)
            {
                ensure!(existing == binding, ActivationError::RuntimeNotAdmitted);
            } else {
                native.bindings.push(binding.clone());
            }
        } else {
            *proposed
                .execution
                .as_mut()
                .and_then(|e| e.environments.get_mut(&accepted.key.environment))
                .and_then(|e| e.components.get_mut(&accepted.key.component))
                .ok_or(ActivationError::ComponentWithdrawn)? = ResolvedComponent::Deployment {
                deployment: accepted.deployment,
            };
        }
        let catalog =
            HubCatalog::from_control_plane(proposed, self.state.catalog.current().admission())?;
        self.state.catalog.validate_replacement(&catalog)?;
        let http = build_http_client(&catalog)?;
        let catalog = Arc::new(
            catalog
                .prepare_runtime(
                    Some(&self.state.catalog.current()),
                    &self.state.public_base_url,
                    http.clone(),
                )
                .await?,
        );
        let revision = HubControlPlaneRevision {
            revision_id: veoveo_hub::new_hub_control_plane_revision_id()?,
            sha256: crate::audit::control_plane_sha256(catalog.control_plane())?,
            source: HubControlPlaneRevisionSource::Contribution,
            applied_at: Utc::now(),
            applied_by: accepted.key.owner.principal.clone(),
            tenant: Some(accepted.key.owner.tenant.clone()),
            control_plane: catalog.control_plane().clone(),
        };
        if let Some(binding) = placement {
            self.state
                .control_store
                .record_activation_placement_on_connection(
                    connection,
                    Some(&active),
                    &revision,
                    audit,
                    &ActivationPlacement {
                        accepted: accepted.clone(),
                        binding,
                    },
                )
                .await?;
        } else {
            self.state
                .control_store
                .record_activation_on_connection(
                    connection,
                    Some(&active),
                    &revision,
                    audit,
                    &ActivationPublication {
                        accepted: accepted.clone(),
                    },
                )
                .await?;
        }
        replace_catalog(&self.state.catalog, catalog)?;
        replace_http_client(&self.state.http, http);
        Ok(())
    }
}

pub(super) async fn complete_local(
    State(service): State<ActivationService>,
    path: Result<
        Path<(veoveo_hub_sdk::HubProfileId, BuildId)>,
        axum::extract::rejection::PathRejection,
    >,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: axum::http::HeaderMap,
    veoveo_http::RequestJson(completion): veoveo_http::RequestJson<CompleteLocalSnapshot>,
) -> Response {
    let Path((profile, build)) = match path {
        Ok(path) => path,
        Err(_) => return problem(ActivationError::InvalidDefinition.into()),
    };
    let prepare = async {
        let owner = subject_owner(&subject)?;
        let status = service.store.status(&owner, build).await?;
        ensure!(status.current, ActivationError::Superseded);
        ensure!(
            status.accepted.inputs.expected_source.is_some(),
            ActivationError::InvalidDefinition
        );
        let bearer = veoveo_hub::BearerToken::from_authorization_header(
            headers
                .get(axum::http::header::AUTHORIZATION)
                .context("source caller authorization missing")?
                .to_str()?,
        )?;
        Ok::<_, anyhow::Error>((status, bearer))
    }
    .await;
    let (status, bearer) = match prepare {
        Ok(value) => value,
        Err(error) => return problem(error),
    };
    service
        .enqueue(
            subject_owner(&subject).expect("owner checked above"),
            subject.audit_context(),
            status.accepted.clone(),
            Some(LocalTransfer {
                profile,
                completion,
                bearer,
            }),
        )
        .await;
    (StatusCode::ACCEPTED, Json(status)).into_response()
}

pub(super) async fn read_diagnostics(
    State(service): State<ActivationService>,
    path: Result<
        Path<(veoveo_hub_sdk::HubProfileId, BuildId)>,
        axum::extract::rejection::PathRejection,
    >,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Path((_, build)) = match path {
        Ok(path) => path,
        Err(_) => return problem(ActivationError::InvalidDefinition.into()),
    };
    let read = async {
        let owner = subject_owner(&subject)?;
        let status = service.store.status(&owner, build).await?;
        let mut connection = service
            .state
            .control_store
            .platform_store()
            .pool()
            .acquire()
            .await?;
        let runtime = native::Runtime::existing(
            &service.state.catalog.current(),
            &mut connection,
            &status.accepted,
        )
        .await?;
        drop(connection);
        runtime
            .invoke::<_, protocol::BuildResult>("diagnostics", &build)
            .await
    }
    .await;
    match read {
        Ok(diagnostics) => Json(diagnostics).into_response(),
        Err(error) => problem(error),
    }
}
