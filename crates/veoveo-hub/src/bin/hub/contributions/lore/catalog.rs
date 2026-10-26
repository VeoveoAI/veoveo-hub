//! Lore's repository listing. Lore's `auth_service` catalog asks
//! `UrcAuthApi.LookupUserPermissions` with the caller's token whenever something lists,
//! and the Hub answers that one method as gRPC on its own listener: the person's
//! assignments on the token's deployment in the active control plane. Lore still
//! authorizes every request from the token alone, so nothing else reaches the Hub.
use std::{future::Future, pin::Pin};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    response::Response,
    routing::post,
};
use lore_proto::auth::{
    LookupUserPermissionsRequest, LookupUserPermissionsResponse, ResourcePermission,
};
use tonic::{
    Status,
    server::{Grpc, UnaryService},
};
use tonic_prost::ProstCodec;
use veoveo_hub::{BearerToken, HubCatalog, JwtAuthConfig, JwtVerifier};
use veoveo_hub_sdk::Principal;
use veoveo_types::{PrincipalId, TenantId};

use super::{Deployment, Lore, NativeAction};
use crate::{
    http_util::{allowed_hub_jwt_algorithms, load_jwks},
    runtime::{AppState, current_http_client},
};

/// The gRPC path of `epic_urc.UrcAuthApi/LookupUserPermissions`.
const LOOKUP: &str = "/epic_urc.UrcAuthApi/LookupUserPermissions";

pub(crate) fn router(state: AppState) -> Router {
    Router::new().route(LOOKUP, post(lookup)).with_state(state)
}

async fn lookup(State(state): State<AppState>, request: Request) -> Response {
    let mut grpc = Grpc::new(ProstCodec::<
        LookupUserPermissionsResponse,
        LookupUserPermissionsRequest,
    >::default());
    grpc.unary(Lookup(state), request).await.map(Body::new)
}

struct Lookup(AppState);

impl UnaryService<LookupUserPermissionsRequest> for Lookup {
    type Response = LookupUserPermissionsResponse;
    type Future =
        Pin<Box<dyn Future<Output = Result<tonic::Response<Self::Response>, Status>> + Send>>;

    fn call(&mut self, request: tonic::Request<LookupUserPermissionsRequest>) -> Self::Future {
        let state = self.0.clone();
        Box::pin(async move {
            let answer = answer(&state, request).await;
            if let Err(status) = &answer
                && status.code() != tonic::Code::Unavailable
            {
                tracing::info!(
                    code = ?status.code(),
                    "refused a Lore repository listing: {}",
                    status.message()
                );
            }
            answer.map(tonic::Response::new)
        })
    }
}

async fn answer(
    state: &AppState,
    request: tonic::Request<LookupUserPermissionsRequest>,
) -> Result<LookupUserPermissionsResponse, Status> {
    let catalog = state.catalog.current();
    let section = Lore::of(&catalog).map_err(|error| {
        tracing::error!("the Lore section cannot answer a repository listing: {error:#}");
        Status::unavailable("the Lore deployments are unavailable")
    })?;
    let bearer = request
        .metadata()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| BearerToken::from_authorization_header(value).ok())
        .ok_or_else(|| Status::unauthenticated("a Lore bearer token is required"))?;
    let (deployment, principal) = verify(state, &catalog, &section, &bearer).await?;
    let tenant = principal
        .tenant
        .as_ref()
        .ok_or_else(|| Status::unauthenticated("the Lore token names no tenant"))?;
    listing(
        &section,
        deployment,
        tenant,
        &principal.id,
        request.get_ref(),
    )
}

/// The deployment whose authorization server signed `bearer` for its audience, and the
/// person the token names: a Lore token this Hub issued, a login or a repository token.
async fn verify<'a>(
    state: &AppState,
    catalog: &HubCatalog,
    section: &'a Lore,
    bearer: &BearerToken,
) -> Result<(&'a Deployment, Principal), Status> {
    let http = current_http_client(&state.http);
    for deployment in &section.deployments {
        let Some(server) = catalog.authorization_server(&deployment.authorization_server) else {
            continue;
        };
        let keys = load_jwks(&http, &server.jwks).await.map_err(|error| {
            tracing::error!(
                deployment = %deployment.id,
                "the keys of a Lore deployment's authorization server are unavailable: {error:#}"
            );
            Status::unavailable("the Lore token keys are unavailable")
        })?;
        let verifier = JwtAuthConfig::new(
            server.issuer.clone(),
            deployment.audience.clone(),
            Default::default(),
            allowed_hub_jwt_algorithms(),
        )
        .map(|config| JwtVerifier::new(config, keys))
        .and_then(|verifier| verifier.with_extensions(state.token_profiles.claim_registry()))
        .map_err(|error| {
            tracing::error!(
                deployment = %deployment.id,
                "a Lore token verifier cannot be built: {error}"
            );
            Status::unavailable("the Lore token verifier is unavailable")
        })?;
        if let Ok(verified) = verifier.verify(bearer)
            && verified.purpose.is_some()
        {
            return Ok((deployment, verified.principal));
        }
    }
    Err(Status::unauthenticated(
        "the token is no Lore token this Hub issued",
    ))
}

/// One page of `principal`'s entries on `deployment` in `tenant`, as Lore asks for them:
/// one per assignment, in resource-ID order, whose resource ID begins with the filter
/// and `-` (every entry for an empty filter), after the page token's resource ID.
pub(super) fn listing(
    section: &Lore,
    deployment: &Deployment,
    tenant: &TenantId,
    principal: &PrincipalId,
    request: &LookupUserPermissionsRequest,
) -> Result<LookupUserPermissionsResponse, Status> {
    if request.context_filter.is_some() {
        return Err(Status::invalid_argument("context_filter is not supported"));
    }
    let limit = match request.page_size {
        None | Some(0) => None,
        Some(size) => Some(
            usize::try_from(size)
                .map_err(|_| Status::invalid_argument("page_size must not be negative"))?,
        ),
    };
    let after = match request.page_token.as_deref() {
        None | Some("") => None,
        Some(token) => {
            deployment.resource_id_template.parse(token).map_err(|_| {
                Status::invalid_argument("page_token names no repository of the deployment")
            })?;
            Some(token)
        }
    };
    let prefix = format!("{}-", request.resource_filter);
    let mut entries = section
        .assignments
        .iter()
        .filter(|assignment| {
            assignment.deployment == deployment.id
                && &assignment.tenant == tenant
                && &assignment.principal == principal
        })
        .map(|assignment| ResourcePermission {
            resource_id: deployment
                .resource_id_template
                .render(&assignment.repository),
            permission: assignment
                .actions
                .iter()
                .map(|action| action.as_str().to_owned())
                .collect(),
        })
        .filter(|entry| {
            request.resource_filter.is_empty() || entry.resource_id.starts_with(&prefix)
        })
        .filter(|entry| after.is_none_or(|after| entry.resource_id.as_str() > after))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.resource_id.cmp(&b.resource_id));
    let next_page_token = match limit {
        Some(limit) if entries.len() > limit => {
            entries.truncate(limit);
            entries.last().map(|entry| entry.resource_id.clone())
        }
        _ => None,
    };
    Ok(LookupUserPermissionsResponse {
        resource_permission: entries,
        next_page_token,
    })
}

impl NativeAction {
    /// The action's name, as a token's `permission` carries it.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Obliterate => "obliterate",
            Self::Migrate => "migrate",
        }
    }
}
