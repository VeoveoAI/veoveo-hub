//! A person creates a Lore repository and owns it, in one step: `POST
//! /lore/{profile}/repositories` with `{"name": …}` and an optional `"id"`, for any
//! authenticated caller with a tenant, directly or through a server that calls the Hub
//! for them. The Hub creates the repository in the installation's one Lore deployment
//! as the caller, with a Lore login token it issues for them, so Lore records them as its
//! creator. It then publishes one control-plane revision that registers the repository
//! and assigns the caller its owner, and answers once this process renewed the Lore
//! access of the caller's Computers. Calling again with the `id` of a repository Lore
//! records the caller as creating registers it, such as one whose publication failed;
//! nobody else can claim it, and an owner assignment an administrator removed stays
//! removed. Sharing, listing and deletion are separate.
use anyhow::Result;
use axum::{
    Extension, Json, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use veoveo_hub::{
    AuthenticatedSubject, control_plane::HubControlPlaneRevisionSource, http::ProfileAuthState,
    new_hub_control_plane_revision_id, problem::Problem, token_profile::TokenIssueError,
};

use super::{
    Lore, Owned, OwnershipRefused, RepositoryId,
    person::issue_login,
    publication::{Author, Publisher},
    renewal,
    workspace::{
        BranchId,
        client::{Client, NativeRefusal},
    },
};
use crate::runtime::AdminState;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRequest {
    name: String,
    /// The repository to create, or to register when Lore records the caller as having
    /// created it; a new ID when omitted.
    #[serde(default)]
    id: Option<RepositoryId>,
}

#[derive(Serialize)]
struct Created {
    id: RepositoryId,
    name: String,
    /// What the repository is cloned from: `<deployment endpoint>/<id>`.
    remote: String,
}

pub(crate) fn router(state: AdminState, auth: ProfileAuthState) -> Router {
    Router::new()
        .route("/lore/{profile}/repositories", post(create))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            auth,
            veoveo_hub::http::authenticate_profile,
        ))
}

/// 16 random bytes in lowercase hexadecimal, the form of Lore's IDs.
fn random_hex() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("random source: {error}"))?;
    Ok(hex::encode(bytes))
}

async fn create(
    State(state): State<AdminState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    veoveo_http::RequestJson(request): veoveo_http::RequestJson<CreateRequest>,
) -> Response {
    let principal = &subject.principal;
    let Some(tenant) = principal.tenant.as_ref() else {
        return Problem::new(
            StatusCode::FORBIDDEN,
            "repository-creation-forbidden",
            "Repository creation requires a tenant",
        )
        .into_response();
    };
    let catalog = state.catalog.current();
    let deployment = match Lore::of(&catalog)
        .map_err(|error| error.to_string())
        .and_then(|section| {
            section
                .sole_deployment()
                .cloned()
                .map_err(|reason| reason.to_owned())
        }) {
        Ok(deployment) => deployment,
        Err(reason) => {
            tracing::error!("repository creation has no Lore deployment: {reason}");
            return unavailable()
                .detail("the installation registers no single Lore deployment")
                .into_response();
        }
    };
    let ids = (|| -> Result<(RepositoryId, BranchId)> {
        let id = match &request.id {
            Some(id) => id.clone(),
            None => RepositoryId::parse(random_hex()?)?,
        };
        Ok((id, BranchId::parse(random_hex()?)?))
    })();
    let (id, branch) = match ids {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!("failed to choose a repository ID: {error:#}");
            return Problem::internal().into_response();
        }
    };

    // Lore creates it as the caller, or answers the existing repository of that ID and
    // name with its record, whose creator then decides.
    let token = match issue_login(&catalog, &deployment.id, tenant, &principal.id).await {
        Ok(token) => token,
        Err(TokenIssueError::Denied(error)) => {
            tracing::info!(principal = %principal.id, "no Lore token for repository creation: {error:#}");
            return Problem::new(
                StatusCode::FORBIDDEN,
                "repository-creation-forbidden",
                "The Hub issues the caller no Lore token",
            )
            .into_response();
        }
        Err(TokenIssueError::Unavailable(error)) => {
            tracing::error!("failed to issue a Lore token for repository creation: {error:#}");
            return unavailable()
                .detail("the Hub could not issue a Lore token")
                .into_response();
        }
    };
    let created = async {
        Client::connect(&deployment)
            .await?
            .create_repository(&id, &branch, &request.name, "", &token.access_token)
            .await
    }
    .await;
    let repository = match created {
        Ok(repository) => repository,
        Err(error) => return refused_by_lore(&error),
    };
    if hex::encode(&repository.id) != id.as_str() || repository.creator != principal.id.as_str() {
        return Problem::new(
            StatusCode::CONFLICT,
            "repository-claimed",
            "Another person created this repository",
        )
        .detail("Lore records another creator for a repository of this ID")
        .into_response();
    }

    // One revision registers it and assigns the caller its owner.
    let author = Author {
        source: HubControlPlaneRevisionSource::Contribution,
        principal: &principal.id,
        tenant: Some(tenant),
        audit: &subject.audit_context(),
    };
    let published = Publisher::of(&state)
        .publish(
            &author,
            new_hub_control_plane_revision_id,
            || async { Ok(true) },
            |_, section| {
                anyhow::ensure!(
                    section.deployments.iter().any(|d| d.id == deployment.id),
                    "the Lore deployment was withdrawn"
                );
                Ok(section.register_owned(&Owned {
                    deployment: &deployment.id,
                    repository: &id,
                    tenant,
                    principal: &principal.id,
                    title: &repository.name,
                })?)
            },
        )
        .await;
    let published = match published {
        Ok(published) => published,
        Err(error) if error.downcast_ref::<OwnershipRefused>().is_some() => {
            return Problem::new(
                StatusCode::CONFLICT,
                "repository-registered",
                "The repository is registered without the caller as its owner",
            )
            .extension("id", &id)
            .into_response();
        }
        Err(error) => {
            tracing::error!(repository = %id, "created a Lore repository but did not register it: {error:#}");
            return unavailable()
                .detail("The repository was created in Lore but not registered. Call again with this id to register it.")
                .extension("id", &id)
                .into_response();
        }
    };
    // The caller's first push must not meet a token issued before this revision.
    if published.is_some() {
        renewal::after_activation(&state.control_store, state.hub_state.hub_store()).await;
    }
    let mut remote = deployment.endpoint.clone();
    remote.set_path(&format!("/{id}"));
    let status = if published.is_some() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let mut response = (
        status,
        Json(Created {
            id,
            name: repository.name,
            remote: remote.to_string(),
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "repository-creation-unavailable",
        "Repository creation is unavailable",
    )
}

/// Lore's refusal of a creation, as the caller's error.
fn refused_by_lore(error: &anyhow::Error) -> Response {
    match error
        .downcast_ref::<NativeRefusal>()
        .map(|refusal| refusal.0)
    {
        Some(tonic::Code::InvalidArgument) => Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid-repository",
            "Lore refused the repository's name",
        )
        .into_response(),
        Some(tonic::Code::AlreadyExists) => Problem::new(
            StatusCode::CONFLICT,
            "repository-exists",
            "Lore holds a repository of this ID or name already",
        )
        .detail("a repository of this ID has another name, or this name belongs to another ID")
        .into_response(),
        _ => {
            tracing::warn!("Lore did not create a repository: {error:#}");
            Problem::new(
                StatusCode::BAD_GATEWAY,
                "lore-unavailable",
                "Lore did not create the repository",
            )
            .into_response()
        }
    }
}
