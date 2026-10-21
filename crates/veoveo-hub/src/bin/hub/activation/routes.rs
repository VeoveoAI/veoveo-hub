use super::types::*;
#[path = "definition.rs"]
mod definition;
#[path = "definition_follow.rs"]
mod definition_follow;
#[path = "follow.rs"]
mod follow;
#[path = "operation.rs"]
mod operation;
#[path = "reconcile.rs"]
mod reconcile;
use super::source::ControlDocument;
use crate::runtime::{AdminState, build_http_client, replace_catalog, replace_http_client};
use anyhow::{Context, Result, ensure};
use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;

use std::sync::Arc;
use veoveo_hub::{
    AuthenticatedSubject, HubCatalog,
    activation::*,
    control_plane::{HubControlPlaneRevision, HubControlPlaneRevisionSource},
    execution::EnvironmentOwner,
};
use veoveo_hub_sdk::{EnvironmentId, PrincipalKind};
use veoveo_types::BuildId;

#[derive(Clone)]
pub(crate) struct ActivationService {
    state: AdminState,
    store: ActivationStore,
    jobs: Arc<tokio::sync::Mutex<std::collections::BTreeSet<BuildId>>>,
    cancellation: tokio_util::sync::CancellationToken,
}
impl ActivationService {
    pub fn new(state: AdminState, cancellation: tokio_util::sync::CancellationToken) -> Self {
        Self {
            store: ActivationStore::new(state.control_store.platform_store().clone()),
            state,
            jobs: Arc::new(tokio::sync::Mutex::new(Default::default())),
            cancellation,
        }
    }
    pub fn spawn_reconciliation(
        &self,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        reconcile::spawn(self.clone(), cancellation)
    }
    pub fn router(self, auth: veoveo_hub::http::ProfileAuthState) -> Router {
        Router::new()
            .route(
                "/environments/{profile}/{environment}",
                get(read_environment).put(update_environment),
            )
            .route(
                "/environments/{profile}/{environment}/source",
                get(definition::read_source).post(definition::follow_source),
            )
            .route("/environments/{profile}/applies/{build}", get(read_apply))
            .route(
                "/environments/{profile}/applies/{build}/retry",
                post(retry_apply),
            )
            .route(
                "/environments/{profile}/applies/{build}/diagnostics",
                get(operation::read_diagnostics),
            )
            .route(
                "/environments/{profile}/applies/{build}/complete-local",
                post(operation::complete_local),
            )
            .layer(axum::extract::DefaultBodyLimit::max(MAX_DEFINITION_BYTES))
            .with_state(self)
            .layer(axum::middleware::from_fn_with_state(
                auth,
                veoveo_hub::http::authenticate_profile,
            ))
    }
    async fn update(
        &self,
        subject: &AuthenticatedSubject,
        profile: veoveo_hub_sdk::HubProfileId,
        request: ApplyEnvironment,
    ) -> Result<Vec<AcceptedApply>> {
        let definition = request.definition.clone();
        let definitions = super::source::DefinitionAppend::new(subject, profile, request)?;
        let owner = subject_owner(subject)?;
        self.publish_definition(definitions, owner, definition, subject.audit_context())
            .await
    }
    pub(super) async fn publish_definition(
        &self,
        definitions: super::source::DefinitionAppend,
        owner: EnvironmentOwner,
        definition: EnvironmentDefinition,
        audit: veoveo_audit_contract::AuditContext,
    ) -> Result<Vec<AcceptedApply>> {
        for component in definition.components.values() {
            component.recipe.validate()?;
        }
        let change = EnvironmentPublication {
            owner: owner.clone(),
            environment: definition.id,
            display_name: definition.display_name,
            base: definition.base,
            components: definition.components.keys().cloned().collect(),
        };
        // This mutex orders this process's replacement; the transaction guard below
        // enforces actual cross-replica authority and withdrawal fencing.
        let _publication = self.state.publication.lock().await;
        let active = self
            .state
            .control_store
            .load_active_revision()
            .await?
            .context("no authoritative control")?;
        if let Some(existing) = active
            .control_plane
            .execution
            .as_ref()
            .and_then(|e| e.environments.get(&change.environment))
        {
            ensure!(existing.owner == owner, ActivationError::Forbidden);
            let mut connection = self
                .state
                .control_store
                .platform_store()
                .pool()
                .acquire()
                .await?;
            if let Some(replayed) = definitions.replay_on(&mut connection).await? {
                return Ok(replayed);
            }
        }
        let proposed = change.project(&active.control_plane)?;
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
            applied_by: owner.principal.clone(),
            tenant: Some(owner.tenant.clone()),
            control_plane: catalog.control_plane().clone(),
        };
        self.state
            .control_store
            .record_source_environment_on(Some(&active), &revision, &audit, &change, &definitions)
            .await?;
        // No await separates durable commit and adoption.
        replace_catalog(&self.state.catalog, catalog)?;
        replace_http_client(&self.state.http, http);
        Ok(definitions.accepted())
    }
}
fn subject_owner(subject: &AuthenticatedSubject) -> Result<EnvironmentOwner> {
    ensure!(
        subject.principal.kind == PrincipalKind::User && subject.delegation.is_none(),
        ActivationError::Forbidden
    );
    Ok(EnvironmentOwner {
        tenant: subject
            .principal
            .tenant
            .clone()
            .ok_or(ActivationError::Forbidden)?,
        principal: subject.principal.id.clone(),
    })
}
fn problem(error: anyhow::Error) -> Response {
    let (status, code) = match error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ActivationError>())
    {
        Some(ActivationError::UnknownEnvironment | ActivationError::NotFound) => {
            (StatusCode::NOT_FOUND, "activation-not-found")
        }
        Some(ActivationError::Forbidden) => (StatusCode::FORBIDDEN, "activation-forbidden"),
        Some(ActivationError::RuntimeNotAdmitted) => {
            (StatusCode::CONFLICT, "activation-runtime-not-admitted")
        }
        Some(
            ActivationError::Superseded
            | ActivationError::IdempotencyConflict
            | ActivationError::InvalidTransition,
        ) => (StatusCode::CONFLICT, "activation-conflict"),
        Some(_) => (StatusCode::BAD_REQUEST, "activation-invalid-input"),
        None if veoveo_hub::is_revision_conflict(&error) => {
            (StatusCode::CONFLICT, "activation-control-revision-changed")
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "activation-unavailable"),
    };
    tracing::warn!(code, "environment operation refused or unavailable");
    veoveo_hub::problem::Problem::new(status, code, "Environment operation did not complete")
        .into_response()
}
fn parsed_path<T>(
    path: Result<Path<T>, axum::extract::rejection::PathRejection>,
) -> Result<T, veoveo_hub::problem::Problem> {
    path.map(|Path(value)| value).map_err(|_| {
        veoveo_hub::problem::Problem::new(
            StatusCode::BAD_REQUEST,
            "activation-invalid-path",
            "Invalid environment operation path",
        )
    })
}
async fn update_environment(
    State(service): State<ActivationService>,
    path: Result<
        Path<(veoveo_hub_sdk::HubProfileId, EnvironmentId)>,
        axum::extract::rejection::PathRejection,
    >,
    Extension(subject): Extension<AuthenticatedSubject>,
    veoveo_http::RequestJson(request): veoveo_http::RequestJson<ApplyEnvironment>,
) -> Response {
    let (profile, environment) = match parsed_path(path) {
        Ok(value) => value,
        Err(problem) => return problem.into_response(),
    };
    if environment != request.definition.id {
        return problem(ActivationError::InvalidDefinition.into());
    }
    match service.update(&subject, profile, request).await {
        Ok(accepted) => (StatusCode::ACCEPTED, Json(accepted)).into_response(),
        Err(e) => problem(e),
    }
}
async fn read_environment(
    State(service): State<ActivationService>,
    path: Result<Path<(String, EnvironmentId)>, axum::extract::rejection::PathRejection>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let (_profile, environment) = match parsed_path(path) {
        Ok(value) => value,
        Err(problem) => return problem.into_response(),
    };
    let result = async {
        let owner = subject_owner(&subject)?;
        let mut transaction = service
            .state
            .control_store
            .platform_store()
            .pool()
            .begin()
            .await?;
        sqlx::query_file!("src/bin/hub/activation/definition_snapshot.sql")
            .execute(&mut *transaction)
            .await?;
        let current = sqlx::query_file!("src/bin/hub/activation/definition_control.sql")
            .fetch_one(&mut *transaction)
            .await?
            .control_plane
            .0;
        let record = current
            .execution
            .as_ref()
            .and_then(|e| e.environments.get(&environment))
            .ok_or(ActivationError::UnknownEnvironment)?;
        ensure!(record.owner == owner, ActivationError::Forbidden);
        let stored = super::source::definitions(&mut transaction, &owner, environment).await?;
        let mut components = std::collections::BTreeMap::new();
        for component in record.components.keys() {
            components.insert(
                component.clone(),
                stored
                    .get(component)
                    .cloned()
                    .ok_or(ActivationError::UnmanagedEnvironment)?,
            );
        }
        let definition = EnvironmentDefinition {
            id: environment,
            display_name: record.display_name.clone(),
            base: record.base.clone(),
            components,
        };
        transaction.commit().await?;
        Ok::<_, anyhow::Error>(definition)
    }
    .await;
    match result {
        Ok(definition) => Json(definition).into_response(),
        Err(e) => problem(e),
    }
}
async fn read_apply(
    State(service): State<ActivationService>,
    path: Result<Path<(String, BuildId)>, axum::extract::rejection::PathRejection>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let (_profile, build) = match parsed_path(path) {
        Ok(value) => value,
        Err(problem) => return problem.into_response(),
    };
    let result = async { service.store.status(&subject_owner(&subject)?, build).await }.await;
    match result {
        Ok(status) => Json(status).into_response(),
        Err(e) => problem(e),
    }
}
async fn retry_apply(
    State(service): State<ActivationService>,
    path: Result<Path<(String, BuildId)>, axum::extract::rejection::PathRejection>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let (_profile, build) = match parsed_path(path) {
        Ok(value) => value,
        Err(problem) => return problem.into_response(),
    };
    let result = async {
        let status = service
            .store
            .status(&subject_owner(&subject)?, build)
            .await?;
        service.store.retry(&status.accepted).await?;
        let resumed = service
            .store
            .status(&status.accepted.key.owner.clone(), build)
            .await?;
        // Materialized input/output is retained on the native PVC. Retrying it
        // must run real work, not leave a phase transition waiting for re-upload.
        // Preparing local source still needs complete-local with fresh authority.
        let mut connection = service
            .state
            .control_store
            .platform_store()
            .pool()
            .acquire()
            .await?;
        let record = super::source::read_on(&mut connection, build).await?;
        drop(connection);
        if !matches!(resumed.state, BuildState::Preparing)
            || matches!(record.component.source, SourceSelector::Lore { .. })
        {
            service
                .enqueue(
                    status.accepted.key.owner.clone(),
                    subject.audit_context(),
                    status.accepted,
                    None,
                )
                .await;
        }
        Ok::<_, anyhow::Error>(resumed)
    }
    .await;
    match result {
        Ok(status) => (StatusCode::ACCEPTED, Json(status)).into_response(),
        Err(e) => problem(e),
    }
}
