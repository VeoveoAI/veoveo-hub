use super::http_response::RequestError;
use crate::contract::HubAction;
use std::{borrow::Cow, time::Instant};
use veoveo_audit_contract::AuditOutcome;
use veoveo_hub_sdk::HubDiscoveryMetadata;
use veoveo_hub_sdk::audit::{ToolCompletion, ToolResultKind};
use veoveo_hub_sdk::hub::HubDiscovery;
use veoveo_hub_sdk::hub::{HubDiscoveryFailure, HubDiscoverySurface};

use crate::control_plane::DiscoveryFailureMode;
use futures::{StreamExt, stream};
use rmcp::{
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResponse, CallToolResult, ClientRequest,
        CreateTaskResult, DetailedTask, ErrorData as McpError, ListToolsResult,
        PaginatedRequestParams, ServerResult, TaskPayload,
    },
    service::{Peer, PeerRequestOptions, RequestContext, RoleClient, RoleServer},
};
use serde_json::Value;
use veoveo_hub_sdk::{LocalToolName, related_task_meta, sanitized_request_meta};

use crate::{
    AuthenticatedSubject,
    mcp_support::{
        mcp_internal, mcp_invalid_params, parse_hub_tool, project_call_tool_resource_uris,
        project_tool_resource_metadata, unexpected_upstream_response,
    },
    state::HubTaskRouteDraft,
};

use super::{
    HubMcp,
    discovery::{
        AdmittedCatalog, DiscoveryCacheKey, MAX_CONCURRENT_DISCOVERY, enforce_complete_discovery,
        isolate_discovery_failures,
    },
    invocation_authorization_fingerprint,
};

// Select before any watch, cache or upstream work, using the same catalog epoch.
fn tool_servers(
    catalog: &crate::HubCatalog,
    profile: &veoveo_hub_sdk::HubProfileId,
) -> Vec<veoveo_hub_sdk::ServerSlug> {
    catalog
        .profile_servers(profile)
        .into_iter()
        .filter(|server| server.capabilities.tools)
        .map(|server| server.slug.clone())
        .collect()
}

impl HubMcp {
    pub(super) async fn handle_list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let snapshot = self.catalog.snapshot();
        let catalog = snapshot.catalog().clone();
        let catalog_generation = snapshot.generation();
        let discovery_failure_mode = catalog
            .profile(&self.profile_id)
            .map(|profile| profile.discovery_failure_mode)
            .unwrap_or(DiscoveryFailureMode::FailClosed);
        let authorization_fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        if discovery_failure_mode == DiscoveryFailureMode::Isolate {
            let (tools, degradation, denied) =
                self.available_tools(context, subject.clone()).await?;
            self.record_discovery(
                &subject,
                veoveo_hub_sdk::audit::DiscoveryKind::Tools,
                tools.iter().map(|tool| tool.name.to_string()).collect(),
                denied,
            )
            .await?;
            let page = super::catalog_pages::page(
                tools,
                request.as_ref(),
                super::execution_composition(&subject)?,
            )?;
            return Ok(ListToolsResult {
                tools: page.items,
                next_cursor: page.next_cursor,
                result_type: Some(rmcp::model::ResultType::COMPLETE),
                ttl_ms: Some(veoveo_hub_sdk::PRIVATE_CATALOG_TTL_MS),
                cache_scope: Some(rmcp::model::CacheScope::Private),
                meta: degradation.into_meta(),
            });
        }
        let results = stream::iter(tool_servers(&catalog, &self.profile_id).into_iter().map(
            |server_slug| {
                let catalog = catalog.clone();
                let context = &context;
                let subject = &subject;
                async move {
                    let key = DiscoveryCacheKey {
                        catalog_generation,
                        principal: subject.actor.id.clone(),
                        authorization_fingerprint,
                        server: server_slug.clone(),
                    };
                    if let Err(error) = self
                        .ensure_discovery_watch(&key, context.peer.clone(), subject)
                        .await
                    {
                        return (server_slug, Err(error));
                    }
                    if let Some(tools) = self.discovery.tools(&key).await {
                        return (server_slug, Ok::<_, McpError>(tools));
                    }
                    let fetch = self.discovery.start_tools(key.clone()).await;
                    let result = async {
                        let tools = self
                            .discover_tools_for_server(&catalog, &server_slug, context, subject)
                            .await?;
                        self.discovery
                            .store_tools(
                                fetch.clone().map(|fetch| fetch.with_denied(tools.denied)),
                                tools.items.clone(),
                            )
                            .await;
                        Ok(tools)
                    }
                    .await;
                    if result.is_err()
                        && let Some(fetch) = fetch
                    {
                        self.discovery
                            .finish_failure(HubDiscoverySurface::Tools, fetch)
                            .await;
                    }
                    (server_slug, result)
                }
            },
        ))
        .buffer_unordered(MAX_CONCURRENT_DISCOVERY)
        .collect::<Vec<_>>()
        .await;
        let denied = results
            .iter()
            .filter_map(|(_, result)| result.as_ref().ok())
            .map(|catalog| catalog.denied)
            .sum();
        let results = results
            .into_iter()
            .map(|(server, result)| (server, result.map(|catalog| catalog.items)))
            .collect();
        let (tools, degradation, errors) =
            isolate_discovery_failures(HubDiscoverySurface::Tools, results);
        for (server, error) in &errors {
            tracing::warn!(%server, %error, "isolated upstream tool discovery failure");
        }
        enforce_complete_discovery(HubDiscoverySurface::Tools, discovery_failure_mode, &errors)?;
        self.record_discovery(
            &subject,
            veoveo_hub_sdk::audit::DiscoveryKind::Tools,
            tools.iter().map(|tool| tool.name.to_string()).collect(),
            denied,
        )
        .await?;
        let page = super::catalog_pages::page(
            tools,
            request.as_ref(),
            super::execution_composition(&subject)?,
        )?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_hub_sdk::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    /// Share independent discovery and a bounded initial-settlement window.
    /// A slow optional server cannot hold the complete federated response.
    pub(super) async fn available_tools(
        &self,
        context: RequestContext<RoleServer>,
        subject: crate::AuthenticatedSubject,
    ) -> Result<(Vec<rmcp::model::Tool>, HubDiscovery, u32), McpError> {
        let snapshot = self.catalog.snapshot();
        let catalog = snapshot.catalog().clone();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let mut keys = Vec::new();
        let mut cached_at_start = std::collections::BTreeMap::new();
        for server_slug in tool_servers(&catalog, &self.profile_id) {
            let key = DiscoveryCacheKey {
                catalog_generation,
                principal: subject.actor.id.clone(),
                authorization_fingerprint,
                server: server_slug.clone(),
            };
            keys.push(key.clone());
            if self.discovery_watch_active(&key).await
                && let Some(items) = self.discovery.tools(&key).await
            {
                cached_at_start.insert(key, items);
                continue;
            }
            let Some(fetch) = self
                .discovery
                .begin(HubDiscoverySurface::Tools, key.clone())
                .await
            else {
                continue;
            };
            let hub = self.clone();
            let catalog = catalog.clone();
            let context = context.clone();
            let subject = subject.clone();
            tokio::spawn(async move {
                let result = async {
                    hub.ensure_discovery_watch(&key, context.peer.clone(), &subject)
                        .await?;
                    hub.discover_tools_for_server(&catalog, &server_slug, &context, &subject)
                        .await
                }
                .await;
                match result {
                    Ok(discovered) => {
                        hub.discovery
                            .finish_tools(fetch.with_denied(discovered.denied), discovered.items)
                            .await;
                    }
                    Err(error) => {
                        hub.discovery
                            .finish_failure(HubDiscoverySurface::Tools, fetch)
                            .await;
                        tracing::warn!(
                            server = %server_slug,
                            %error,
                            "isolated upstream tool discovery failure"
                        );
                    }
                }
            });
        }
        self.discovery
            .settle(HubDiscoverySurface::Tools, &keys)
            .await;
        let mut tools = Vec::new();
        let mut failures = Vec::new();
        let mut denied = 0u32;
        for key in keys {
            let cached = match cached_at_start.remove(&key) {
                Some(items) => Some(items),
                None if self.discovery_watch_active(&key).await => self.discovery.tools(&key).await,
                None => None,
            };
            if let Some(mut cached) = cached {
                denied = denied.saturating_add(cached.denied);
                tools.append(&mut cached.items);
            } else {
                let code = self
                    .discovery
                    .missing_code(HubDiscoverySurface::Tools, &key)
                    .await;
                failures.push(HubDiscoveryFailure {
                    server: key.server,
                    surface: HubDiscoverySurface::Tools,
                    code,
                });
            }
        }
        Ok((tools, HubDiscovery::new(failures), denied))
    }

    async fn discover_tools_for_server(
        &self,
        catalog: &crate::HubCatalog,
        server_slug: &veoveo_hub_sdk::ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &crate::AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<rmcp::model::Tool>, McpError> {
        let started = std::time::Instant::now();
        let manifest = catalog
            .server(server_slug)
            .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
        let upstream_tools = self
            .idempotent_upstream_request(
                server_slug,
                context.peer.clone(),
                subject,
                |upstream| async move { upstream.list_all_tools().await },
            )
            .await
            .map_err(RequestError::into_protocol)?;
        let upstream_ms = started.elapsed().as_millis();
        let mut tools = Vec::with_capacity(upstream_tools.len());
        let mut targets = Vec::with_capacity(upstream_tools.len());
        for mut tool in upstream_tools {
            let local_tool = LocalToolName::parse(tool.name.as_ref()).map_err(|err| {
                mcp_internal(format!("upstream exposed invalid tool name: {err}"))
            })?;
            targets.push(crate::control_plane::PolicyTarget::Tool {
                server: server_slug.clone(),
                tool: local_tool.clone(),
            });
            project_tool_resource_metadata(manifest, &mut tool)?;
            let hub_name = catalog
                .project_tool_name(server_slug, &local_tool)
                .map_err(|err| mcp_internal(format!("failed to project tool name: {err}")))?;
            tool.name = Cow::Owned(hub_name.to_string());
            tools.push(tool);
        }
        let authorization_started = std::time::Instant::now();
        let allowed = self
            .allows_catalog_targets(context, HubAction::ToolsList, targets)
            .await?;
        if started.elapsed() >= std::time::Duration::from_millis(100) {
            tracing::info!(server = %server_slug, surface = "tools",
                count = tools.len(), upstream_ms,
                authorization_ms = authorization_started.elapsed().as_millis(),
                total_ms = started.elapsed().as_millis(), "slow MCP catalog discovery");
        }
        let denied: u32 = allowed
            .iter()
            .filter(|allowed| !**allowed)
            .count()
            .try_into()
            .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
        Ok(AdmittedCatalog {
            denied,
            items: tools
                .into_iter()
                .zip(allowed)
                .filter_map(|(tool, allowed)| allowed.then_some(tool))
                .collect(),
        })
    }

    /// The server and tool `name` names. A server this operation's selection serves no
    /// instance of is refused with the cause.
    fn tool_projection(
        &self,
        catalog: &crate::HubCatalog,
        name: &str,
    ) -> Result<crate::HubToolProjection, McpError> {
        let unavailable = veoveo_hub_sdk::hub::HubToolName::parse(name)
            .ok()
            .and_then(|name| crate::HubToolProjection::parse(&name).ok())
            .and_then(|projection| self.unavailable.get_key_value(&projection.server));
        match unavailable {
            Some((server, cause)) => Err(mcp_invalid_params(format!(
                "server `{server}` is unavailable: {cause}"
            ))),
            None => parse_hub_tool(catalog, name),
        }
    }

    pub(super) async fn handle_call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, RequestError> {
        let catalog = self.catalog.current();
        let projection = self.tool_projection(&catalog, &request.name)?;
        let (subject, _) = self
            .authorize_tool(
                &context,
                HubAction::ToolsCall,
                projection.server.clone(),
                projection.tool.clone(),
            )
            .await?;
        let started = Instant::now();
        let response = async {
            let _upstream_timer = crate::request_observation::StageTimer::start(
                crate::request_observation::RequestStage::Upstream,
            );
            restore_request_meta(&mut request, &context.meta);
            request.name = Cow::Owned(projection.tool.to_string());

            let downstream_tasks = context
                .meta
                .client_capabilities()
                .is_some_and(|capabilities| capabilities.supports_tasks());
            let project_tasks =
                downstream_tasks && self.client_allows_task_projection(&subject).await?;
            let direct_adapter = self.client_uses_direct_task_call_adapter(&subject).await?;
            let server_supports_tasks = catalog
                .profile_server(&self.profile_id, &projection.server)
                .is_some_and(|(_, server)| server.capabilities.tasks);
            let effective_tasks = server_supports_tasks && (project_tasks || direct_adapter);
            let downstream_progress_token = context.meta.get_progress_token();
            let upstream = self
                .upstream_with_tasks(
                    &projection.server,
                    context.peer.clone(),
                    &subject,
                    effective_tasks,
                )
                .await?;
            let handle = upstream
                .peer
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(request)),
                    PeerRequestOptions::no_options(),
                )
                .await
                .map_err(RequestError::from)?;
            if let Some(downstream_token) = downstream_progress_token {
                self.progress_tokens
                    .register(
                        &self.profile_id,
                        &subject.principal.id,
                        &projection.server,
                        handle.progress_token.clone(),
                        downstream_token,
                    )
                    .await;
            }
            let upstream_token = handle.progress_token.clone();
            let result = handle.await_response().await.map_err(RequestError::from);
            self.progress_tokens
                .remove_token(
                    &self.profile_id,
                    &subject.principal.id,
                    &projection.server,
                    &upstream_token,
                )
                .await;

            match result? {
                ServerResult::CallToolResult(mut result) => {
                    let manifest = catalog.server(&projection.server).ok_or_else(|| {
                        mcp_internal(format!("unknown tool server `{}`", projection.server))
                    })?;
                    project_call_tool_resource_uris(manifest, &mut result)?;
                    Ok(CallToolResponse::Complete(result))
                }
                ServerResult::InputRequiredResult(result) => {
                    Ok(CallToolResponse::InputRequired(result))
                }
                ServerResult::CreateTaskResult(created) if project_tasks => {
                    let created = self
                        .project_created_task(&subject, &projection.server, created)
                        .await?;
                    Ok(CallToolResponse::Task(created))
                }
                ServerResult::CreateTaskResult(created) if direct_adapter => {
                    let source_task_id = created.task.task_id.clone();
                    let projected = self
                        .project_created_task(&subject, &projection.server, created)
                        .await?;
                    let canonical_task_id = projected.task.task_id;
                    let detailed = await_terminal_task(
                        &upstream.peer,
                        source_task_id,
                        context.ct.clone(),
                        projected.task.poll_interval_ms,
                    )
                    .await?;
                    let mut result = completed_tool_result(detailed)?;
                    let manifest = catalog.server(&projection.server).ok_or_else(|| {
                        mcp_internal(format!("unknown tool server `{}`", projection.server))
                    })?;
                    project_call_tool_resource_uris(manifest, &mut result)?;
                    result.meta = Some(related_task_meta(canonical_task_id));
                    Ok(CallToolResponse::Complete(result))
                }
                ServerResult::CreateTaskResult(_) => {
                    Err(McpError::missing_required_client_capability(
                        rmcp::model::ClientCapabilities::builder()
                            .enable_tasks()
                            .build(),
                    )
                    .into())
                }
                other => Err(unexpected_upstream_response("tools/call", other).into()),
            }
        }
        .await;
        let (result_kind, mcp_error_code) = tool_call_result_kind(&response);
        let failed = matches!(
            result_kind,
            ToolResultKind::ErrorResult | ToolResultKind::ProtocolError
        );
        let audit = crate::audit::hub_audit();
        let completion = ToolCompletion {
            result: result_kind,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or((1u64 << 53) - 1),
            error_code: mcp_error_code,
            invocation: Some(subject.audit_invocation()),
        };
        let draft = veoveo_hub_sdk::audit::McpTarget::Tool {
            server: projection.server,
            tool: projection.tool,
        }
        .audit_target()
        .map_err(anyhow::Error::from)
        .and_then(|target| Ok((target, audit.detail(&completion)?)))
        .and_then(|(target, detail)| {
            let draft = subject.audit_draft(
                detail,
                if failed {
                    AuditOutcome::Failure
                } else {
                    AuditOutcome::Success
                },
            );
            let draft = if failed {
                draft.reason(crate::audit::HubReason::UpstreamFailure.into())
            } else {
                draft
            };
            Ok(draft.target(target).build()?)
        });
        match draft {
            Ok(draft) => {
                self.state
                    .audit_writer()
                    .await
                    .record_completion(draft)
                    .await
            }
            Err(error) => {
                tracing::error!(%error, request_id=%subject.audit.id, "tool completion audit attribution failed")
            }
        }
        response
    }

    async fn project_created_task(
        &self,
        subject: &AuthenticatedSubject,
        server: &veoveo_hub_sdk::ServerSlug,
        mut created: CreateTaskResult,
    ) -> Result<CreateTaskResult, McpError> {
        let authority_digest = veoveo_types::Sha256Digest::from_bytes(
            invocation_authorization_fingerprint(&subject.actor, &subject.authority)?,
        );
        // A server that calls other servers may call them from this Task for its caller.
        // A Task another server's call created delegates nothing further.
        let catalog = self.catalog.current();
        let registered = catalog
            .server(server)
            .ok_or_else(|| mcp_internal("Task deployment is not selected"))?;
        let uses_others = !registered.authorization.uses.is_empty();
        let delegated_caller = if uses_others && subject.delegation.is_none() {
            Some(veoveo_hub_sdk::DelegatedCaller {
                client: subject.access_token.oauth_client_id.clone(),
                actor: subject.actor.clone(),
                authority: subject.authority.clone(),
                request_context: subject
                    .request_context()
                    .map_err(|error| mcp_internal(error.to_string()))?,
                delegation_context: None,
            })
        } else {
            None
        };
        let (canonical, _) = self
            .state
            .create_task_route(HubTaskRouteDraft {
                tenant: subject.authority.tenant.clone(),
                work_context: subject.authority.work_context.clone(),
                profile: self.profile_id.clone(),
                server: server.clone(),
                deployment: registered.deployment,
                execution: subject
                    .execution
                    .clone()
                    .ok_or_else(|| mcp_internal("Task execution capture is missing"))?,
                source_task_id: created.task.task_id.clone(),
                authority_digest,
                ownership: crate::state::HubTaskOwnership::from_invocation(
                    &subject.actor,
                    &subject.authority,
                ),
                ttl_ms: created.task.ttl_ms,
                delegated_caller,
            })
            .await
            .map_err(|error| mcp_internal(format!("failed to persist hub task route: {error}")))?;
        created.task.task_id = canonical.to_string();
        created.meta = Some(related_task_meta(canonical.to_string()));
        Ok(created)
    }
}

fn tool_call_result_kind(
    response: &Result<CallToolResponse, RequestError>,
) -> (ToolResultKind, Option<i32>) {
    match response {
        Ok(CallToolResponse::Complete(result)) if result.is_error == Some(true) => {
            (ToolResultKind::ErrorResult, None)
        }
        Ok(CallToolResponse::Complete(_)) => (ToolResultKind::Complete, None),
        Ok(CallToolResponse::InputRequired(_)) => (ToolResultKind::InputRequired, None),
        Ok(CallToolResponse::Task(_)) => (ToolResultKind::TaskCreated, None),
        Ok(_) => (ToolResultKind::OtherResponse, None),
        Err(error) => (ToolResultKind::ProtocolError, Some(error.protocol().code.0)),
    }
}

fn restore_request_meta(
    request: &mut CallToolRequestParams,
    context_meta: &rmcp::model::RequestMetaObject,
) {
    if context_meta.is_empty() {
        return;
    }
    request
        .meta
        .get_or_insert_with(rmcp::model::RequestMetaObject::new)
        .extend(sanitized_request_meta(context_meta));
}

async fn await_terminal_task(
    peer: &Peer<RoleClient>,
    task_id: String,
    cancellation: tokio_util::sync::CancellationToken,
    initial_poll_interval_ms: Option<u64>,
) -> Result<DetailedTask, RequestError> {
    let mut poll_interval_ms = initial_poll_interval_ms.unwrap_or(1_000).clamp(100, 30_000);
    loop {
        let current = peer
            .get_task(rmcp::model::GetTaskParams::new(task_id.clone()))
            .await
            .map_err(RequestError::from)?
            .task;
        if current.status().is_terminal() {
            return Ok(current);
        }
        poll_interval_ms = current
            .task
            .poll_interval_ms
            .unwrap_or(poll_interval_ms)
            .clamp(100, 30_000);
        tokio::select! {
            () = cancellation.cancelled() => {
                let _ = peer.cancel_task(rmcp::model::CancelTaskParams::new(task_id)).await;
                return Err(McpError::invalid_request("task wait was cancelled", None).into());
            }
            () = tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)) => {}
        }
    }
}

pub(super) fn completed_tool_result(task: DetailedTask) -> Result<CallToolResult, McpError> {
    match task.payload {
        TaskPayload::Completed { result } => {
            serde_json::from_value(Value::Object(result)).map_err(|error| {
                mcp_internal(format!(
                    "upstream task result was not a tool result: {error}"
                ))
            })
        }
        TaskPayload::Failed { error } => {
            let error = serde_json::from_value(Value::Object(error)).map_err(|decode| {
                mcp_internal(format!("upstream task error was malformed: {decode}"))
            })?;
            Err(error)
        }
        TaskPayload::Cancelled => Err(McpError::invalid_request("task was cancelled", None)),
        TaskPayload::Working | TaskPayload::InputRequired { .. } => {
            Err(mcp_internal("task result requested before completion"))
        }
        _ => Err(mcp_internal(
            "upstream returned an unsupported task payload",
        )),
    }
}

pub(super) fn project_detailed_task_resource_uris(
    manifest: &crate::registration::RegisteredServer,
    task: &mut DetailedTask,
) -> Result<(), McpError> {
    let TaskPayload::Completed { result } = &mut task.payload else {
        return Ok(());
    };
    let mut tool_result: CallToolResult = serde_json::from_value(Value::Object(result.clone()))
        .map_err(|error| {
            mcp_internal(format!(
                "upstream completed task result was not a tool result: {error}"
            ))
        })?;
    project_call_tool_resource_uris(manifest, &mut tool_result)?;
    *result = serde_json::to_value(tool_result)
        .map_err(|error| mcp_internal(format!("failed to encode projected task result: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| mcp_internal("projected task result was not an object"))?;
    Ok(())
}

pub(super) fn rewrite_detailed_task_id(task: &mut DetailedTask, canonical_task_id: &str) {
    task.task.task_id = canonical_task_id.to_owned();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_audit_distinguishes_domain_and_protocol_failures() {
        let domain_failure = Ok(CallToolResponse::Complete(CallToolResult::error(vec![])));
        assert_eq!(
            tool_call_result_kind(&domain_failure),
            (ToolResultKind::ErrorResult, None)
        );
        let protocol_failure = Err(McpError::invalid_params("bad arguments", None).into());
        assert_eq!(
            tool_call_result_kind(&protocol_failure),
            (
                ToolResultKind::ProtocolError,
                Some(rmcp::model::ErrorCode::INVALID_PARAMS.0)
            )
        );
    }

    #[test]
    fn request_context_metadata_is_restored_before_upstream_projection() {
        let mut request = CallToolRequestParams::new("upstream__forecast");
        let mut context_meta = rmcp::model::RequestMetaObject::new();
        context_meta.set_traceparent("00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01");

        restore_request_meta(&mut request, &context_meta);

        assert_eq!(
            request
                .meta
                .as_ref()
                .and_then(|meta| meta.get_traceparent()),
            context_meta.get_traceparent()
        );
    }
}
