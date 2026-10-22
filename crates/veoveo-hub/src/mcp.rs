mod authorization;
mod catalog_pages;
mod completion;
mod discovery;
mod discovery_watch;
mod health;
pub mod http_response;
mod info;
mod native_upstreams;
mod progress;
mod prompts;
mod resource_read;
mod resources;
mod subscriptions;
#[cfg(test)]
pub(crate) mod task_ownership_tests;
mod tasks;
mod tools;
mod upstream;
mod upstream_authorized_http;
mod upstream_connection;
mod upstream_http;
pub use upstream_http::{HubUpstreamHttpClientPool, upstream_client_builder};

use std::{future::Future, sync::Arc};

use rmcp::{
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CancelTaskParams, CompleteRequestParams,
        CompleteResult, ErrorData as McpError, GetPromptRequestParams, GetTaskParams,
        GetTaskResult, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams,
        ReadResourceResponse, ServerConfig, SubscriptionFilter, UpdateTaskParams,
    },
    service::{
        Peer, RequestContext, RoleClient, RoleServer, RunningService, ServiceError,
        SubscriptionContext,
    },
    transport::streamable_http_client::StreamableHttpClientTransportConfig,
};
use sha2::{Digest, Sha256};
use veoveo_hub_sdk::{HubInternalTokenIssuer, HubProfileId, Principal, ServerSlug};
use veoveo_types::InvocationAuthority;

use crate::{
    AuthenticatedSubject, HubCatalogHandle, HubState,
    mcp_support::{mcp_internal, mcp_invalid_params, mcp_invalid_request},
};
use discovery::CatalogDiscoveryCache;
use http_response::{HttpResponseContext, RequestError};
use upstream::{HubUpstreamHandler, HubUpstreamHandlerConfig};
use upstream_authorized_http::HubAuthorizedHttpClient;

pub use health::{HubServerHealth, HubServerHealthState};
pub use tasks::ServerTaskCancellation;

pub(super) const HUB_PAGE_SIZE: usize = 100;

struct RequestUpstream {
    peer: Peer<RoleClient>,
    _running: RunningService<RoleClient, HubUpstreamHandler>,
}

#[derive(Debug, Clone)]
pub struct HubMcp {
    catalog: HubCatalogHandle,
    // Immutable selected views keep this original signal for publication/reconnect hints.
    live_catalog: HubCatalogHandle,
    state: HubState,
    profile_id: HubProfileId,
    internal_token_issuer: HubInternalTokenIssuer,
    upstream_http: HubUpstreamHttpClientPool,
    discovery: Arc<CatalogDiscoveryCache>,
    discovery_watches: Arc<discovery_watch::DiscoveryWatches>,
    progress_tokens: progress::HubProgressTokens,
}

impl HubMcp {
    pub fn new(
        catalog: HubCatalogHandle,
        profile_id: HubProfileId,
        state: HubState,
        internal_token_issuer: HubInternalTokenIssuer,
        upstream_http: HubUpstreamHttpClientPool,
    ) -> Self {
        Self {
            live_catalog: catalog.clone(),
            catalog,
            state,
            profile_id,
            internal_token_issuer,
            upstream_http,
            discovery: Arc::new(CatalogDiscoveryCache::default()),
            discovery_watches: Default::default(),
            progress_tokens: progress::HubProgressTokens::default(),
        }
    }

    /// One authenticated stateless operation, sharing only properly scoped caches.
    pub fn for_execution(
        &self,
        snapshot: &crate::HubCatalogSnapshot,
        composition: &veoveo_hub_sdk::ResolvedComposition,
    ) -> Self {
        Self {
            catalog: snapshot.selected(composition),
            ..self.clone()
        }
    }

    async fn upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<RequestUpstream, RequestError> {
        self.upstream_with_tasks(server_slug, downstream, subject, false)
            .await
    }

    async fn upstream_with_tasks(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
        tasks: bool,
    ) -> Result<RequestUpstream, RequestError> {
        self.connect_upstream(server_slug, Some(downstream), subject, tasks)
            .await
    }

    /// A Tasks-capable upstream for a request the Hub makes itself, outside any MCP
    /// session: the upstream's notifications reach no client.
    async fn detached_upstream(
        &self,
        server_slug: &ServerSlug,
        subject: &AuthenticatedSubject,
    ) -> Result<RequestUpstream, RequestError> {
        self.connect_upstream(server_slug, None, subject, true)
            .await
    }

    async fn connect_upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Option<Peer<RoleServer>>,
        subject: &AuthenticatedSubject,
        tasks: bool,
    ) -> Result<RequestUpstream, RequestError> {
        let snapshot = self.catalog.snapshot();
        let server = snapshot
            .catalog()
            .server(server_slug)
            .ok_or_else(|| mcp_invalid_params(format!("unknown upstream server `{server_slug}`")))?
            .clone();
        let transport = snapshot
            .catalog()
            .upstream_http()
            .unwrap_or(&self.upstream_http)
            .transport(server.deployment, &server.endpoint)?;
        let mut permissions = crate::policy::granted_permissions(
            &snapshot.catalog().control_plane().authorization,
            &server.authorization,
            &subject.principal,
        );
        if let Some(delegation) = &subject.delegation {
            permissions = delegation.narrow(permissions);
        }
        let authorized_http_client = HubAuthorizedHttpClient::new(
            transport,
            self.internal_token_issuer.clone(),
            self.profile_id.clone(),
            &server,
            subject,
            permissions,
        )?;
        let handler = HubUpstreamHandler::new(HubUpstreamHandlerConfig {
            catalog: self.catalog.clone(),
            profile_id: self.profile_id.clone(),
            principal_id: subject.principal.id.clone(),
            upstream_server: server_slug.clone(),
            downstream,
            progress_tokens: self.progress_tokens.clone(),
            discovery: self.discovery.clone(),
            tasks,
        });
        let running = upstream_connection::discover(
            handler,
            authorized_http_client,
            upstream_transport_config(server.endpoint_url()),
        )
        .await?;
        let peer = running.peer().clone();
        Ok(RequestUpstream {
            peer,
            _running: running,
        })
    }

    async fn idempotent_upstream_request<T, F, Fut>(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
        request: F,
    ) -> Result<T, RequestError>
    where
        F: Fn(Peer<RoleClient>) -> Fut,
        Fut: Future<Output = Result<T, ServiceError>>,
    {
        let _timing = crate::request_observation::StageTimer::start(
            crate::request_observation::RequestStage::Upstream,
        );
        let upstream = self
            .upstream(server_slug, downstream.clone(), subject)
            .await?;
        match request(upstream.peer.clone()).await {
            Ok(result) => Ok(result),
            Err(error) if recoverable_upstream_connection_error(&error) => {
                tracing::warn!(
                    server = %server_slug,
                    principal = %subject.actor.id,
                    %error,
                    "reconnecting a request-scoped upstream MCP client before one idempotent retry"
                );
                drop(upstream);
                let retry = self.upstream(server_slug, downstream, subject).await?;
                request(retry.peer).await.map_err(RequestError::from)
            }
            Err(error) => Err(RequestError::from(error)),
        }
    }
}

fn upstream_transport_config(uri: &str) -> StreamableHttpClientTransportConfig {
    StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
}

fn recoverable_upstream_connection_error(error: &ServiceError) -> bool {
    match error {
        ServiceError::TransportSend(error) => !matches!(
            error
                .error
                .downcast_ref::<rmcp::transport::streamable_http_client::StreamableHttpError<
                    upstream_authorized_http::HubAuthorizedHttpError,
                >>(),
            Some(rmcp::transport::streamable_http_client::StreamableHttpError::HttpResponse { .. })
        ),
        ServiceError::TransportClosed => true,
        // RMCP currently maps an abruptly terminated Streamable HTTP response
        // (including "no close frame received or sent") to its transport-level
        // pseudo JSON-RPC code 0. No conforming MCP application error uses 0.
        ServiceError::McpError(error) => error.code.0 == 0,
        _ => false,
    }
}

fn invocation_authorization_fingerprint(
    actor: &Principal,
    authority: &InvocationAuthority,
) -> Result<[u8; 32], McpError> {
    // `authenticated_at` records when this HTTP request re-verified the bearer
    // token. It changes on every Streamable HTTP request even though the token
    // and its effective authorization are unchanged, so it is excluded from
    // the invocation cache keys and retained admission/retry evidence. Durable
    // Task access uses actor/provenance ownership plus current policy instead.
    let mut stable_actor = actor.clone();
    stable_actor.authenticated_at = None;
    Ok(Sha256::digest(
        serde_json::to_vec(&(stable_actor, authority))
            .map_err(|err| mcp_internal(format!("failed to fingerprint invocation: {err}")))?,
    )
    .into())
}

fn execution_composition(
    subject: &AuthenticatedSubject,
) -> Result<&veoveo_hub_sdk::CompositionRevision, McpError> {
    subject
        .execution
        .as_ref()
        .map(|execution| &execution.composition)
        .ok_or_else(|| mcp_internal("authenticated execution capture missing"))
}

fn discovery_authorization_fingerprint(
    subject: &AuthenticatedSubject,
) -> Result<[u8; 32], McpError> {
    let authority = invocation_authorization_fingerprint(&subject.actor, &subject.authority)?;
    Ok(Sha256::digest(
        // Composition is part of cache identity, not business/data authority.
        serde_json::to_vec(&(
            authority,
            &subject.access_token.oauth_client_id,
            execution_composition(subject)?,
        ))
        .map_err(|_| mcp_internal("failed to fingerprint discovery authority"))?,
    )
    .into())
}

impl ServerHandler for HubMcp {
    fn get_info(&self) -> ServerConfig {
        self.handle_get_info()
    }

    /// The Hub serves the `2026-07-28` lifecycle only: every request carries its
    /// own protocol metadata, so no older version's session methods apply.
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        veoveo_hub_sdk::final_protocol_versions()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        request_observation(&context)?
            .scope(self.handle_list_tools(request, context))
            .await
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response.finish(
            request_observation(&context)?
                .scope(self.handle_call_tool(request, context))
                .await,
        )
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        request_observation(&context)?
            .scope(self.handle_list_resources(request, context))
            .await
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        request_observation(&context)?
            .scope(self.handle_list_resource_templates(request, context))
            .await
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response
            .finish(
                request_observation(&context)?
                    .scope(self.handle_read_resource(request, context))
                    .await,
            )
            .map(Into::into)
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(self.accepted_subscriptions(requested))
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        self.handle_listen(context).await
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        request_observation(&context)?
            .scope(self.handle_list_prompts(request, context))
            .await
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response
            .finish(
                request_observation(&context)?
                    .scope(self.handle_get_prompt(request, context))
                    .await,
            )
            .map(Into::into)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response.finish(
            request_observation(&context)?
                .scope(self.handle_complete(request, context))
                .await,
        )
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response.finish(
            request_observation(&context)?
                .scope(self.handle_get_task(request, context))
                .await,
        )
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response.finish(
            request_observation(&context)?
                .scope(self.handle_update_task(request, context))
                .await,
        )
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let response = HttpResponseContext::from_request(&context)?;
        response.finish(
            request_observation(&context)?
                .scope(self.handle_cancel_task(request, context))
                .await,
        )
    }
}

fn request_observation(
    context: &RequestContext<RoleServer>,
) -> Result<crate::request_observation::RequestObservation, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| {
            parts
                .extensions
                .get::<crate::request_observation::RequestObservation>()
        })
        .cloned()
        .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Utc;
    use veoveo_hub_sdk::{PrincipalKind, TokenIssuer, TokenSubject};
    use veoveo_types::{
        AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, RoleId, ScopeName,
        TenantId, WorkContextId,
    };
    use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

    use super::*;

    pub(super) fn principal() -> Principal {
        Principal {
            id: PrincipalId::parse("issuer#subject").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://identity.example").unwrap(),
            subject: TokenSubject::parse("subject").unwrap(),
            tenant: None,
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::parse("operator").unwrap()]),
            scopes: BTreeSet::from([ScopeName::parse("tools:call").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        }
    }

    pub(super) fn authority() -> InvocationAuthority {
        InvocationAuthority {
            work_context: WorkContextId::parse("mission").unwrap(),
            tenant: TenantId::parse("tenant").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(PrincipalId::parse("issuer#subject").unwrap()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: PrincipalId::parse("issuer#subject").unwrap(),
            },
        }
    }

    #[test]
    fn upstream_fingerprint_covers_actor_and_authority() {
        let baseline = principal();
        let mut changed = baseline.clone();
        changed
            .roles
            .insert(RoleId::parse("administrator").unwrap());
        let mut reverified = baseline.clone();
        reverified.authenticated_at = Some(Utc::now());

        assert_eq!(
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap(),
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap()
        );
        assert_eq!(
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap(),
            invocation_authorization_fingerprint(&reverified, &authority()).unwrap(),
            "HTTP bearer re-verification time is audit metadata, not authorization identity"
        );
        assert_ne!(
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap(),
            invocation_authorization_fingerprint(&changed, &authority()).unwrap()
        );
    }

    #[test]
    fn abrupt_upstream_transport_error_is_recoverable_for_idempotent_requests() {
        let error = ServiceError::McpError(McpError::new(
            rmcp::model::ErrorCode(0),
            "no close frame received or sent",
            None,
        ));
        assert!(recoverable_upstream_connection_error(&error));
    }

    #[test]
    fn application_upstream_error_is_not_retried() {
        let error = ServiceError::McpError(McpError::internal_error("application failure", None));
        assert!(!recoverable_upstream_connection_error(&error));
    }
}
