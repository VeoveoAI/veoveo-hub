//! Lore's repository creation and deletion. Lore's `auth_service` catalog lists from the
//! Hub's grants (`catalog.rs`), so Lore tells the same endpoint of each repository it
//! creates or deletes, with the caller's token: `RebacApi.CreateResource` before it
//! stores a repository and `DeleteResource` before it removes one. The Hub answers both
//! as gRPC on its own listener. Creation registers the repository in the caller's tenant
//! and makes them its owner, as `POST /lore/{profile}/repositories` does, so `lore
//! repository create` and that route make one record, and Lore stores nothing the Hub
//! refused. Deletion is the Hub's decision: an owner deletes, and the repository's
//! registration and every assignment on it go with it.
use std::{future::Future, pin::Pin};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    response::Response,
    routing::post,
};
use lore_proto::rebac::{
    CreateResourceRequest, CreateResourceResponse, DeleteResourceRequest, DeleteResourceResponse,
};
use tonic::{
    Status,
    server::{Grpc, UnaryService},
};
use tonic_prost::ProstCodec;
use veoveo_audit_contract::{AuditActor, AuditContext, AuditPrincipalKind};
use veoveo_hub::{
    control_plane::HubControlPlaneRevisionSource, http::auth_support::audit_request,
    new_hub_control_plane_revision_id,
};

use super::{
    Lore, Owned, OwnershipRefused, RepositoryId,
    catalog::TokenCaller,
    publication::{Author, Publisher},
    sharing::{Caller, Refusal},
};
use crate::runtime::{AdminState, AppState};

/// The gRPC paths of `ucs.auth.RebacApi`'s two methods.
const CREATE: &str = "/ucs.auth.RebacApi/CreateResource";
const DELETE: &str = "/ucs.auth.RebacApi/DeleteResource";

/// What the two methods use: the token check of the listing, and the publication of
/// the owner routes.
#[derive(Clone)]
struct Resources {
    app: AppState,
    admin: AdminState,
}

pub(crate) fn router(app: AppState, admin: AdminState) -> Router {
    Router::new()
        .route(CREATE, post(create))
        .route(DELETE, post(delete))
        .with_state(Resources { app, admin })
}

async fn create(State(state): State<Resources>, request: Request) -> Response {
    let mut grpc =
        Grpc::new(ProstCodec::<CreateResourceResponse, CreateResourceRequest>::default());
    grpc.unary(Create(state), request).await.map(Body::new)
}

async fn delete(State(state): State<Resources>, request: Request) -> Response {
    let mut grpc =
        Grpc::new(ProstCodec::<DeleteResourceResponse, DeleteResourceRequest>::default());
    grpc.unary(Delete(state), request).await.map(Body::new)
}

type Answer<T> = Pin<Box<dyn Future<Output = Result<tonic::Response<T>, Status>> + Send>>;

struct Create(Resources);

impl UnaryService<CreateResourceRequest> for Create {
    type Response = CreateResourceResponse;
    type Future = Answer<Self::Response>;

    fn call(&mut self, request: tonic::Request<CreateResourceRequest>) -> Self::Future {
        let state = self.0.clone();
        Box::pin(async move {
            logged("creation", created(&state, request).await).map(tonic::Response::new)
        })
    }
}

struct Delete(Resources);

impl UnaryService<DeleteResourceRequest> for Delete {
    type Response = DeleteResourceResponse;
    type Future = Answer<Self::Response>;

    fn call(&mut self, request: tonic::Request<DeleteResourceRequest>) -> Self::Future {
        let state = self.0.clone();
        Box::pin(async move {
            logged("deletion", deleted(&state, request).await).map(tonic::Response::new)
        })
    }
}

fn logged<T>(what: &str, answer: Result<T, Status>) -> Result<T, Status> {
    if let Err(status) = &answer
        && status.code() != tonic::Code::Unavailable
    {
        tracing::info!(
            code = ?status.code(),
            "refused a Lore repository {what}: {}",
            status.message()
        );
    }
    answer
}

/// Register the repository Lore is about to create, with its creator as its owner.
async fn created(
    state: &Resources,
    request: tonic::Request<CreateResourceRequest>,
) -> Result<CreateResourceResponse, Status> {
    let caller = TokenCaller::of(&state.app, request.metadata()).await?;
    let request = request.into_inner();
    let id = repository(&caller, &request.resource_id)?;
    publish(state, &caller, |section| {
        Ok(section.register_owned(&Owned {
            deployment: &caller.deployment.id,
            repository: &id,
            tenant: &caller.tenant,
            principal: &caller.principal,
            title: &request.resource_name,
        })?)
    })
    .await
    .map_err(|error| {
        if error.downcast_ref::<OwnershipRefused>().is_some() {
            Status::permission_denied("The repository is registered without you as its owner")
        } else {
            tracing::error!(repository = %id, "did not register a repository Lore creates: {error:#}");
            Status::unavailable("The Hub could not register the repository")
        }
    })?;
    Ok(CreateResourceResponse {})
}

/// Remove the repository Lore is about to delete, when its caller owns it.
async fn deleted(
    state: &Resources,
    request: tonic::Request<DeleteResourceRequest>,
) -> Result<DeleteResourceResponse, Status> {
    let caller = TokenCaller::of(&state.app, request.metadata()).await?;
    let id = repository(&caller, &request.get_ref().resource_id)?;
    publish(state, &caller, |section| {
        Ok(section.delete_repository(
            &caller.deployment.id,
            &id,
            Caller {
                tenant: &caller.tenant,
                principal: &caller.principal,
            },
        )?)
    })
    .await
    .map_err(|error| {
        if error.downcast_ref::<Refusal>().is_some() {
            Status::permission_denied("Only an owner deletes the repository")
        } else {
            tracing::error!(repository = %id, "did not remove a repository Lore deletes: {error:#}");
            Status::unavailable("The Hub could not remove the repository")
        }
    })?;
    Ok(DeleteResourceResponse {})
}

/// The repository a resource ID names on the caller's deployment.
fn repository(caller: &TokenCaller, resource_id: &str) -> Result<RepositoryId, Status> {
    caller
        .deployment
        .resource_id_template
        .parse(resource_id)
        .map_err(|_| Status::invalid_argument("resource_id names no repository of the deployment"))
}

/// Publish what `change` makes of the Lore section, as the caller.
async fn publish(
    state: &Resources,
    caller: &TokenCaller,
    mut change: impl FnMut(&mut Lore) -> anyhow::Result<bool>,
) -> anyhow::Result<()> {
    let audit = AuditContext {
        actor: AuditActor {
            principal: caller.principal.clone(),
            kind: AuditPrincipalKind::User,
            tenant: Some(caller.tenant.clone()),
            work_context: None,
        },
        via: None,
        request: audit_request(),
    };
    let author = Author {
        source: HubControlPlaneRevisionSource::Contribution,
        principal: &caller.principal,
        tenant: Some(&caller.tenant),
        audit: &audit,
    };
    Publisher::of(&state.admin)
        .publish(
            &author,
            new_hub_control_plane_revision_id,
            || async { Ok(true) },
            |_, section| {
                anyhow::ensure!(
                    section
                        .deployments
                        .iter()
                        .any(|d| d.id == caller.deployment.id),
                    "the Lore deployment was withdrawn"
                );
                change(section)
            },
        )
        .await?;
    Ok(())
}
