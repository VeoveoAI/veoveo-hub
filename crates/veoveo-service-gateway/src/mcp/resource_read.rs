//! Resource delivery rechecks authority on every read and commits its audit record
//! before returning content.
use super::http_response::RequestError;
use super::{GatewayMcp, discovery::DiscoveryCacheKey};
use crate::control_plane::GatewayResourceProjection;
use crate::{
    AuthenticatedSubject,
    mcp_support::{
        mcp_internal, mcp_invalid_params, project_gateway_resource_uri_for_upstream,
        project_read_resource_result,
    },
};
use rmcp::{
    ServiceError,
    model::{CacheScope, ErrorData as McpError, ReadResourceRequestParams, ReadResourceResult},
    service::{Peer, RequestContext, RoleClient, RoleServer},
};
use veoveo_audit_contract::{AuditOutcome, AuditReason, AuditTarget};
use veoveo_sdk_rust::audit::{McpReadMethod, Read};
use veoveo_sdk_rust::gateway::GatewayDiscoverySurface;
use veoveo_sdk_rust::parse_gateway_task_resource_uri;

impl GatewayMcp {
    async fn finish_resource_read(
        &self,
        subject: &AuthenticatedSubject,
        projection: &GatewayResourceProjection,
        result: Result<ReadResourceResult, RequestError>,
    ) -> Result<ReadResourceResult, RequestError> {
        let (outcome, reason) = match &result {
            Ok(_) => (AuditOutcome::Succeeded, AuditReason::Accepted),
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::INVALID_REQUEST => {
                (AuditOutcome::Denied, AuditReason::PolicyDenied)
            }
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::INVALID_PARAMS => {
                (AuditOutcome::Failed, AuditReason::InvalidRequest)
            }
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::RESOURCE_NOT_FOUND => {
                (AuditOutcome::Failed, AuditReason::NotFound)
            }
            Err(_) => (AuditOutcome::Failed, AuditReason::UpstreamFailure),
        };
        let detail = crate::audit::gateway_audit()
            .detail(&Read {
                method: McpReadMethod::ResourceRead,
            })
            .map_err(|_| mcp_internal("invalid resource read audit detail"))?;
        let draft = subject
            .audit_draft(
                AuditTarget::Resource {
                    uri: projection.gateway_uri.clone(),
                },
                detail,
                outcome,
                reason,
            )
            .map_err(|_| mcp_internal("invalid resource read audit attribution"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required resource read audit unavailable"))?;
        result
    }
    pub(super) async fn handle_read_resource(
        &self,
        mut request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, RequestError> {
        if let Some(task_id) = parse_gateway_task_resource_uri(&request.uri) {
            return self
                .read_task_status_resource(task_id, &request.uri, &context)
                .await;
        }
        let server = self.server_for_resource(&request.uri)?;
        let projection = self.project_resource_for_upstream(&request.uri)?;
        let subject = self.admit_resource_read(&context, &projection).await?;
        let result = async {
            // The canonical request context owns metadata.
            request.meta = Some(context.meta.clone());
            let snapshot = self.catalog.snapshot();
            let catalog = snapshot.catalog();
            let manifest = catalog
                .server(&server)
                .ok_or_else(|| mcp_internal(format!("unknown resource server `{server}`")))?;
            let key = DiscoveryCacheKey {
                catalog_generation: snapshot.generation(),
                principal: subject.actor.id.clone(),
                authorization_fingerprint: super::discovery_authorization_fingerprint(&subject)?,
                server: server.clone(),
            };
            self.ensure_discovery_watch(&key, context.peer.clone(), &subject)
                .await?;
            let routes = match self.discovery.resource_routes(&key).await {
                Some(routes) => routes,
                None => {
                    if let Some(fetch) = self
                        .discovery
                        .begin(GatewayDiscoverySurface::Resources, key.clone())
                        .await
                    {
                        match self
                            .discover_resources_for_server(catalog, &server, &context, &subject)
                            .await
                        {
                            Ok(routes) => {
                                self.discovery
                                    .finish_resource_routes(
                                        fetch.with_denied(routes.denied),
                                        routes.items,
                                    )
                                    .await
                            }
                            Err(error) => {
                                self.discovery
                                    .finish_failure(GatewayDiscoverySurface::Resources, fetch)
                                    .await;
                                return Err(error.into());
                            }
                        }
                    } else {
                        self.discovery
                            .settle(
                                GatewayDiscoverySurface::Resources,
                                std::slice::from_ref(&key),
                            )
                            .await;
                    }
                    // Read back through the cache's generation and notification fences.
                    self.discovery.resource_routes(&key).await.ok_or_else(|| {
                        mcp_internal(format!(
                            "resource discovery for `{server}` is unavailable; retry the read"
                        ))
                    })?
                }
            };
            let upstream_uri = routes
                .iter()
                .find(|route| route.resource.uri == request.uri)
                .map(|route| route.upstream_uri.clone());
            let Some(upstream_uri) = upstream_uri.or(project_gateway_resource_uri_for_upstream(
                manifest,
                &request.uri,
                &[],
            )?) else {
                return Err(mcp_invalid_params(format!(
                    "resource URI is not exposed: {}",
                    request.uri
                ))
                .into());
            };
            let projection = GatewayResourceProjection {
                server,
                gateway_uri: projection.gateway_uri.clone(),
                upstream_uri,
            };
            request.uri = projection.upstream_uri.to_string();
            let mut result = self
                .idempotent_upstream_request(
                    &projection.server,
                    context.peer.clone(),
                    &subject,
                    |upstream| {
                        let request = request.clone();
                        async move { read_upstream(upstream, request).await }
                    },
                )
                .await?;
            prepare_delivery(&mut result, &projection)?;
            Ok(result)
        }
        .await;
        self.finish_resource_read(&subject, &projection, result)
            .await
    }
}

fn prepare_delivery(
    result: &mut ReadResourceResult,
    projection: &GatewayResourceProjection,
) -> Result<(), McpError> {
    project_read_resource_result(result, projection)?;
    // SDK read caches key by URI only. Every delivered read must recheck
    // authority and commit its own audit record before it reaches a caller.
    result.ttl_ms = Some(0);
    result.cache_scope = Some(CacheScope::Private);
    Ok(())
}

async fn read_upstream(
    upstream: Peer<RoleClient>,
    request: ReadResourceRequestParams,
) -> Result<ReadResourceResult, ServiceError> {
    upstream.read_resource(request).await
}

#[cfg(test)]
mod tests;
