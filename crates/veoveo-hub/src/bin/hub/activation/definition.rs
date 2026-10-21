//! Owner-admitted native association; it publishes the same EnvironmentDefinition.
use super::*;
use crate::activation::definition_source::{
    self, DefinitionDocument, DefinitionMode, DefinitionSourceState,
};

pub(super) async fn read_source(
    State(service): State<ActivationService>,
    path: Result<
        Path<(veoveo_hub_sdk::HubProfileId, EnvironmentId)>,
        axum::extract::rejection::PathRejection,
    >,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let (_, environment) = match parsed_path(path) {
        Ok(value) => value,
        Err(problem) => return problem.into_response(),
    };
    let result = async {
        let owner = subject_owner(&subject)?;
        let mut tx = service
            .state
            .control_store
            .platform_store()
            .pool()
            .begin()
            .await?;
        sqlx::query_file!("src/bin/hub/activation/definition_snapshot.sql")
            .execute(&mut *tx)
            .await?;
        let control = sqlx::query_file!("src/bin/hub/activation/definition_control.sql")
            .fetch_one(&mut *tx)
            .await?
            .control_plane
            .0;
        ensure!(
            control
                .execution
                .as_ref()
                .and_then(|e| e.environments.get(&environment))
                .is_some_and(|e| e.owner == owner),
            ActivationError::Forbidden
        );
        let source = definition_source::read_on(&mut tx, environment).await?;
        tx.commit().await?;
        Ok::<_, anyhow::Error>(source)
    }
    .await;
    match result {
        Ok(source) => Json(source).into_response(),
        Err(error) => problem(error),
    }
}
pub(super) async fn follow_source(
    State(service): State<ActivationService>,
    path: Result<
        Path<(veoveo_hub_sdk::HubProfileId, EnvironmentId)>,
        axum::extract::rejection::PathRejection,
    >,
    Extension(subject): Extension<AuthenticatedSubject>,
    veoveo_http::RequestJson(request): veoveo_http::RequestJson<FollowDefinition>,
) -> Response {
    let (profile, environment) = match parsed_path(path) {
        Ok(value) => value,
        Err(problem) => return problem.into_response(),
    };
    let result = async {
        ensure!(
            request.source.path.as_str() != ".",
            ActivationError::InvalidPath
        );
        let owner = subject_owner(&subject)?;
        let authority = super::super::source::SourceAuthority::capture(&subject, profile)?;
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
        ensure!(
            current
                .execution
                .as_ref()
                .and_then(|e| e.environments.get(&environment))
                .is_some_and(|e| e.owner == owner),
            ActivationError::Forbidden
        );
        let catalog =
            HubCatalog::from_control_plane(current, service.state.catalog.current().admission())?;
        let _ = crate::activation::lore::credential_in(
            &catalog,
            &authority,
            &request.source.deployment,
            &request.source.repository,
        )
        .await?;
        let previous = sqlx::query_file!(
            "src/bin/hub/activation/definition_source_lock.sql",
            environment.as_uuid()
        )
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| row.document);
        ensure!(
            previous.as_ref().map(|p| p.0.generation) == request.replaces,
            ActivationError::Superseded
        );
        let generation = Generation::new(
            request
                .replaces
                .map_or(Some(1), |g| g.get().checked_add(1))
                .ok_or(ActivationError::InvalidGeneration)?,
        )?;
        let state = DefinitionSourceState {
            environment,
            owner,
            generation,
            authority,
            mode: DefinitionMode::Lore {
                source: request.source,
                observed: None,
            },
        };
        definition_source::persist_on(&mut tx, &state).await?;
        tx.commit().await?;
        Ok::<_, anyhow::Error>(state)
    }
    .await;
    match result {
        Ok(state) => (StatusCode::ACCEPTED, Json(state)).into_response(),
        Err(error) => problem(error),
    }
}
