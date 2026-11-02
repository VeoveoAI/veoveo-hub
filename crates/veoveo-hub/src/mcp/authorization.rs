use crate::contract::HubAction;
use crate::control_plane::{
    HubResourceProjection, OAuthClientRegistration, OAuthClientSurface, PolicyDecision,
    PolicyEffect, PolicyReasonCode, PolicyTarget, TraceId,
};
use rmcp::{
    model::ErrorData as McpError,
    service::{RequestContext, RoleServer},
};
use veoveo_audit_contract::AuditOutcome;
use veoveo_hub_sdk::{CanonicalTaskId, LocalToolName, PromptName, ServerSlug};
use veoveo_store::{deterministic_principal_id, deterministic_tenant_id};

use crate::{
    AuthenticatedSubject, PolicyRequest,
    mcp_support::{
        hub_resource_uri, mcp_internal, mcp_invalid_params, mcp_invalid_request,
        project_upstream_resource, resource_policy_target,
    },
};

use super::HubMcp;
use crate::state::task_decision;

pub(super) struct CanonicalTaskRoute {
    pub(super) hub: HubMcp,
    pub(super) task_id: String,
    pub(super) server: ServerSlug,
    pub(super) subject: AuthenticatedSubject,
}

impl HubMcp {
    pub(super) async fn authorize_canonical_task(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        task_id: &str,
    ) -> Result<CanonicalTaskRoute, McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        self.authorize_canonical_task_with_trace(&subject, action, task_id, Some(trace_id))
            .await
    }

    pub(super) async fn authorize_canonical_task_for_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        task_id: &str,
    ) -> Result<CanonicalTaskRoute, McpError> {
        self.authorize_canonical_task_with_trace(subject, action, task_id, None)
            .await
    }

    async fn authorize_canonical_task_with_trace(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        task_id: &str,
        trace_id: Option<TraceId>,
    ) -> Result<CanonicalTaskRoute, McpError> {
        let canonical_task_id = CanonicalTaskId::parse(task_id)
            .map_err(|error| mcp_invalid_params(format!("invalid canonical task id: {error}")))?;
        let Some(route) = self
            .state
            .task_route(&canonical_task_id, &subject.authority.tenant)
            .await
            .map_err(|error| {
                mcp_internal(format!("failed to read canonical task route: {error}"))
            })?
        else {
            tracing::warn!(%task_id, "canonical task record was not found");
            return Err(mcp_invalid_params("unknown task id"));
        };
        self.authorize_task_route(subject, action, route, trace_id)
            .await
    }

    /// Check a Task's route for `subject` as every Task request does: recorded
    /// ownership, the retained routing capture, current access to the captured
    /// environment, exposure of the deployment and current policy.
    pub(super) async fn authorize_task_route(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        route: crate::state::HubTaskRoute,
        trace_id: Option<TraceId>,
    ) -> Result<CanonicalTaskRoute, McpError> {
        let task_id = route.id.as_str().to_owned();
        let tenant_key = subject.authority.tenant.as_str();
        let expected_tenant = deterministic_tenant_id(tenant_key)
            .map_err(|error| mcp_internal(format!("invalid task tenant: {error}")))?;
        let expected_owner = deterministic_principal_id(tenant_key, subject.actor.id.as_str())
            .map_err(|error| mcp_internal(format!("invalid task owner: {error}")))?;
        let request = task_decision::admit_owner(
            task_decision::Binding {
                tenant: route.tenant,
                owner: route.owner,
                profile: route.profile,
            },
            task_decision::Ownership {
                identity: route.ownership.identity(),
                labels: &route.ownership.labels(),
            },
            task_decision::Task {
                server: route.server,
                deployment: route.deployment,
                canonical: route.id,
                source: route.source_task_id,
                capture: route.execution,
            },
            task_decision::Caller {
                binding: task_decision::Binding {
                    tenant: expected_tenant,
                    owner: expected_owner,
                    profile: self.profile_id.clone(),
                },
                ownership: task_decision::Ownership {
                    identity: crate::state::HubTaskOwnership::current_identity(
                        &subject.actor,
                        &subject.authority,
                    ),
                    labels: &subject.actor.data_labels.iter().collect::<Vec<_>>(),
                },
                delegated: subject.delegation.is_some(),
                capture: subject.execution.clone(),
                payload: subject.clone(),
            },
        )
        .map_err(|refusal| {
            if refusal == task_decision::Refusal::Binding {
                tracing::warn!(
                    %task_id,
                    caller_actor = %subject.actor.id,
                    caller_initiator = %subject.principal.id,
                    caller_profile = %self.profile_id,
                    caller_tenant = ?subject.actor.tenant,
                    "canonical task ownership did not match the authenticated subject"
                );
            }
            mcp_invalid_params("unknown task id")
        })?;
        // Task ownership is checked before using its retained routing capture.
        // Current routing may differ; current domain policy still applies.
        let snapshot = self.live_catalog.snapshot();
        let composition = self
            .state
            .hub_store()
            .composition(&request.task.capture.composition)
            .await
            .map_err(|_| mcp_internal("Task composition storage is unavailable"))?
            .ok_or_else(|| mcp_invalid_params("unknown task id"))?;
        // A linked server's composition selects one runner per link; the Task keeps the
        // runner that created it.
        let deployment = composition
            .components()
            .get(&request.task.server)
            .and_then(|component| {
                component
                    .deployments()
                    .into_iter()
                    .find(|deployment| *deployment == request.task.deployment)
            });
        let request = task_decision::capture(request, composition.context(), deployment)
            .map_err(|_| mcp_invalid_params("unknown task id"))?;
        let hub = self.for_execution(&snapshot, &composition);
        let hub = match hub
            .catalog
            .current()
            .registrations()
            .deployment(&request.task.deployment)
        {
            Some(server) => hub.at(server),
            None => hub,
        };
        let exposed = hub
            .catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .any(|manifest| {
                manifest.slug == request.task.server
                    && manifest.deployment == request.task.deployment
                    && manifest.capabilities.tasks
            });
        if !exposed {
            return Err(mcp_invalid_params("Task deployment is unavailable"));
        }
        let subject = with_task_execution(request.caller, request.task.capture);
        let target = PolicyTarget::Task {
            server: request.task.server.clone(),
            task_id: request.task.canonical,
        };
        let subject = match trace_id {
            Some(trace_id) => {
                hub.authorize_subject_with_trace(&subject, action, target, trace_id)
                    .await?
            }
            None => hub.authorize_subject(&subject, action, target).await?,
        };
        Ok(CanonicalTaskRoute {
            hub,
            task_id: request.task.source,
            server: request.task.server,
            subject,
        })
    }

    pub(super) fn authenticated(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<AuthenticatedSubject, McpError> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| mcp_invalid_request("authenticated HTTP context missing"))?;
        let mut subject = parts
            .extensions
            .get::<AuthenticatedSubject>()
            .cloned()
            .ok_or_else(|| mcp_invalid_request("authenticated subject missing"))?;
        let observation = parts
            .extensions
            .get::<crate::request_observation::RequestObservation>()
            .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))?;
        subject.audit = observation.audit.clone();
        Ok(subject)
    }

    pub(super) async fn authenticated_oauth_client(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<OAuthClientRegistration, McpError> {
        self.state
            .effective_oauth_client(
                &self.catalog.current(),
                &subject.access_token.oauth_client_id,
            )
            .await
            .map_err(|_| mcp_invalid_request("current OAuth registration is unavailable"))?
            .map(|client| client.registration)
            .ok_or_else(|| mcp_invalid_request("authenticated OAuth client is not registered"))
    }

    pub(super) async fn client_allows_task_projection(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<bool, McpError> {
        let client = self.authenticated_oauth_client(subject).await?;
        Ok(client_surface_allows_task_projection(
            client.client_surface,
            client.direct_task_call_adapter,
        ))
    }

    pub(super) async fn client_uses_direct_task_call_adapter(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<bool, McpError> {
        let client = self.authenticated_oauth_client(subject).await?;
        Ok(client.client_surface == OAuthClientSurface::ToolsCompat
            && client.direct_task_call_adapter)
    }

    pub(super) async fn authorize(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        target: PolicyTarget,
    ) -> Result<AuthenticatedSubject, McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        self.authorize_subject_with_trace(&subject, action, target, trace_id)
            .await
    }

    pub(super) async fn authorize_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        target: PolicyTarget,
    ) -> Result<AuthenticatedSubject, McpError> {
        let (subject, decision) = self
            .evaluate_policy_for_subject(subject, action, target)
            .await?;
        if decision.effect == PolicyEffect::Allow {
            Ok(subject)
        } else {
            tracing::warn!(
                profile = %self.profile_id,
                principal = %subject.principal.id,
                action = ?action,
                reason = ?decision.reason,
                "hub policy denied MCP request"
            );
            Err(mcp_invalid_request(
                self.written(&decision.target).denial(decision.reason),
            ))
        }
    }

    async fn authorize_subject_with_trace(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        target: PolicyTarget,
        trace_id: TraceId,
    ) -> Result<AuthenticatedSubject, McpError> {
        let (subject, decision) = self
            .evaluate_policy_for_subject_with_trace(subject, action, target, trace_id)
            .await?;
        if decision.effect == PolicyEffect::Allow {
            Ok(subject)
        } else {
            tracing::warn!(
                profile = %self.profile_id,
                principal = %subject.principal.id,
                action = ?action,
                reason = ?decision.reason,
                "hub policy denied MCP request"
            );
            Err(mcp_invalid_request(
                self.written(&decision.target).denial(decision.reason),
            ))
        }
    }

    pub(super) async fn evaluate_policy_for_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        target: PolicyTarget,
    ) -> Result<(AuthenticatedSubject, PolicyDecision), McpError> {
        let trace_id = TraceId::parse(&subject.audit.trace_id)
            .map_err(|err| mcp_internal(format!("failed to create trace id: {err}")))?;
        self.evaluate_policy_for_subject_with_trace(subject, action, target, trace_id)
            .await
    }

    async fn evaluate_policy_for_subject_with_trace(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        target: PolicyTarget,
        trace_id: TraceId,
    ) -> Result<(AuthenticatedSubject, PolicyDecision), McpError> {
        let decision = self
            .policy_decision(subject, action, &target, trace_id)
            .await?;
        // Owned task status is a polling protocol. Its denials are still records.
        if action != HubAction::TasksGet || decision.effect != PolicyEffect::Allow {
            let detail = crate::audit::mcp_action_detail(action, subject)
                .map_err(|_| mcp_internal("action requires its domain audit producer"))?;
            let target = self
                .written(&target)
                .audit_target()
                .map_err(|_| mcp_internal("invalid audit target"))?;
            let draft = crate::audit::policy_draft(subject, target, detail, &decision)
                .build()
                .map_err(|_| mcp_internal("invalid audit attribution"))?;
            self.state.record_audit(draft).await.map_err(|_| {
                mcp_internal("The hub couldn't commit the audit record. Try again shortly.")
            })?;
        }
        Ok((subject.clone(), decision))
    }

    /// Discovery evaluates item visibility here; the list handler owns its one
    /// request record after aggregation, including responses served from cache.
    pub(super) async fn allows_catalog_targets(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        targets: Vec<PolicyTarget>,
    ) -> Result<Vec<bool>, McpError> {
        let subject = self.authenticated(context)?;
        let trace = trace_id_for_context(context)?;
        let mut admitted = Vec::with_capacity(targets.len());
        for target in targets {
            admitted.push(
                self.policy_decision(&subject, action, &target, trace.clone())
                    .await?
                    .effect
                    == PolicyEffect::Allow,
            );
        }
        Ok(admitted)
    }

    async fn policy_decision(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        target: &PolicyTarget,
        trace_id: TraceId,
    ) -> Result<PolicyDecision, McpError> {
        let catalog = self.catalog.current();
        let managed_admitted = self
            .state
            .oauth_action_admitted(&catalog, subject, action, target)
            .await
            .map_err(|_| mcp_internal("OAuth client authority unavailable"))?;
        let mut decision = catalog.decide(PolicyRequest {
            principal: &subject.principal,
            profile: &self.profile_id,
            action: action.into(),
            target,
            trace_id: &trace_id,
        });
        let route = if managed_admitted
            && decision.effect == PolicyEffect::Allow
            && subject.delegation.is_some()
        {
            delegated_route(
                crate::policy::declared_route_access(catalog.as_ref(), target),
                target,
            )
        } else {
            crate::delegation::authority::Route::Undeclared
        };
        decision = narrow_policy_decision(
            decision,
            managed_admitted,
            subject
                .delegation
                .as_ref()
                .map(|delegation| &delegation.permissions),
            route,
        );
        Ok(decision)
    }

    pub(super) async fn record_discovery(
        &self,
        subject: &AuthenticatedSubject,
        collection: veoveo_hub_sdk::audit::DiscoveryKind,
        mut identities: Vec<String>,
        denied: u32,
    ) -> Result<(), McpError> {
        use sha2::{Digest, Sha256};
        identities.sort();
        let visible = identities
            .len()
            .try_into()
            .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
        // A length-delimited JSON array prevents concatenation collisions between IDs.
        let bytes = serde_json::to_vec(&identities)
            .map_err(|_| mcp_internal("invalid catalog identity"))?;
        let visible_digest = veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        let audit = crate::audit::hub_audit();
        let target = veoveo_hub_sdk::audit::McpTarget::Discovery {
            server: None,
            collection,
        }
        .audit_target()
        .map_err(|_| mcp_internal("invalid discovery target"))?;
        let detail = audit
            .detail(&veoveo_hub_sdk::audit::Discovery {
                collection,
                visible,
                denied,
                visible_digest,
                invocation: Some(subject.audit_invocation()),
            })
            .map_err(|_| mcp_internal("invalid discovery detail"))?;
        let draft = subject
            .audit_draft(detail, AuditOutcome::Success)
            .target(target)
            .build()
            .map_err(|_| mcp_internal("invalid discovery attribution"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required discovery audit unavailable"))
    }

    pub(super) async fn authorize_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<(AuthenticatedSubject, TraceId), McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        let subject = self
            .authorize_subject_with_trace(
                &subject,
                action,
                PolicyTarget::Tool { server, tool },
                trace_id.clone(),
            )
            .await?;
        Ok((subject, trace_id))
    }

    pub(super) async fn authorize_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<AuthenticatedSubject, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.authorize(context, action, target).await
    }

    /// The read handler records a successful read. Denials commit here, before any
    /// upstream request.
    pub(super) async fn admit_resource_read(
        &self,
        context: &RequestContext<RoleServer>,
        projection: &HubResourceProjection,
    ) -> Result<AuthenticatedSubject, McpError> {
        let subject = self.authenticated(context)?;
        let action = HubAction::ResourcesRead;
        // The server's declaration names its own URIs, which a runner's lack its link.
        let target =
            resource_policy_target(projection.server.clone(), projection.upstream_uri.as_str())?;
        let decision = self
            .policy_decision(&subject, action, &target, trace_id_for_context(context)?)
            .await?;
        if decision.effect == PolicyEffect::Deny {
            self.record_policy_denial(&subject, action, target, decision.reason)
                .await?;
            return Err(mcp_invalid_request(
                self.written(&decision.target).denial(decision.reason),
            ));
        }
        Ok(subject)
    }

    /// `skills/get` records its answer. A refusal of the skill's own address commits
    /// here, before any upstream request.
    pub(super) async fn admit_skill_get(
        &self,
        context: &RequestContext<RoleServer>,
        subject: &AuthenticatedSubject,
        server: ServerSlug,
        uri: &str,
    ) -> Result<(), McpError> {
        let action = HubAction::SkillsGet;
        let target = resource_policy_target(server, uri)?;
        let decision = self
            .policy_decision(subject, action, &target, trace_id_for_context(context)?)
            .await?;
        if decision.effect == PolicyEffect::Deny {
            self.record_policy_denial(subject, action, target, decision.reason)
                .await?;
            return Err(mcp_invalid_request(
                self.written(&decision.target).denial(decision.reason),
            ));
        }
        Ok(())
    }

    pub(super) async fn authorize_projected_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        projection: &HubResourceProjection,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize_resource(
            context,
            action,
            projection.server.clone(),
            projection.upstream_uri.as_str(),
        )
        .await
    }

    pub(super) async fn authorize_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: HubAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Prompt { server, prompt })
            .await
    }

    pub(super) async fn record_policy_denial(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        target: PolicyTarget,
        reason: PolicyReasonCode,
    ) -> Result<(), McpError> {
        let written = self.written(&target);
        self.record_written_denial(subject, action, &written, reason)
            .await
    }

    async fn record_written_denial(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        written: &Written,
        reason: PolicyReasonCode,
    ) -> Result<(), McpError> {
        let target = written
            .audit_target()
            .map_err(|_| mcp_internal("invalid audit target"))?;
        let detail = crate::audit::mcp_action_detail(action, subject)
            .map_err(|_| mcp_internal("action requires its domain audit producer"))?;
        let draft = subject
            .audit_draft(detail, AuditOutcome::Denied)
            .target(target);
        let draft = match crate::audit::policy_reason(reason) {
            Some(reason) => draft.reason(reason),
            None => draft,
        }
        .build()
        .map_err(|_| mcp_internal("invalid denial attribution"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required denial audit unavailable"))?;
        Ok(())
    }

    /// `target` as its caller wrote it, in this view: when this view's server is a
    /// runner, its tool with the runner's link and its URIs under it.
    pub(super) fn written(&self, target: &PolicyTarget) -> Written {
        let catalog = self.catalog.current();
        let runner = |server: &ServerSlug| {
            catalog
                .server(server)
                .filter(|registered| registered.runner.is_some())
        };
        match target {
            PolicyTarget::Tool { server, tool } => {
                match runner(server).and_then(|registered| registered.link()) {
                    Some(link) => Written::LinkedTool {
                        server: server.clone(),
                        link: link.clone(),
                        tool: tool.clone(),
                    },
                    None => Written::Target(target.clone()),
                }
            }
            PolicyTarget::Resource { server, uri } => {
                let linked = runner(server).and_then(|registered| {
                    crate::mcp_support::project_upstream_resource_uri_for_hub(
                        registered,
                        uri.as_str(),
                    )
                    .ok()
                });
                match linked {
                    Some(uri) => Written::Target(PolicyTarget::Resource {
                        server: server.clone(),
                        uri,
                    }),
                    None => Written::Target(target.clone()),
                }
            }
            PolicyTarget::ResourceTemplate { server, uri } => {
                let linked = runner(server).and_then(|registered| {
                    veoveo_types::ResourceTemplateUri::new(
                        crate::mcp_support::linked_uri(registered, uri.as_str()).into_owned(),
                    )
                    .ok()
                });
                match linked {
                    Some(uri) => Written::Target(PolicyTarget::ResourceTemplate {
                        server: server.clone(),
                        uri,
                    }),
                    None => Written::Target(target.clone()),
                }
            }
            _ => Written::Target(target.clone()),
        }
    }

    /// Refuse `target`, as its caller wrote it, as one its server did not declare,
    /// recording the denial: a name or URI that names a link the caller holds no grant
    /// on, or no link of the selected branch, reaches no runner and says nothing of
    /// which links exist.
    pub(super) async fn refuse_undeclared(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        written: Written,
    ) -> McpError {
        let reason = PolicyReasonCode::UndeclaredTarget;
        let message = written.denial(reason);
        match self
            .record_written_denial(subject, action, &written, reason)
            .await
        {
            Ok(()) => mcp_invalid_request(message),
            Err(error) => error,
        }
    }

    /// The server, or the runner, a resource URI routes to for `subject`. A linked
    /// server's URI whose link the caller does not reach is refused as a target the
    /// server did not declare, under `action`.
    pub(super) async fn resource_server(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        uri: &str,
    ) -> Result<crate::registration::RegisteredServer, McpError> {
        let catalog = self.catalog.current();
        match catalog.server_for_resource_uri(&self.profile_id, &subject.principal, uri) {
            crate::catalog::ResourceRoute::Server(server) => Ok(server.clone()),
            crate::catalog::ResourceRoute::Unreached(server) => {
                let target = resource_policy_target(server.clone(), uri)?;
                Err(self
                    .refuse_undeclared(subject, action, Written::Target(target))
                    .await)
            }
            crate::catalog::ResourceRoute::Unowned => Err(mcp_invalid_params(format!(
                "resource URI is not exposed: {uri}"
            ))),
        }
    }

    /// `uri` as the server, or the runner, it routes to for `subject` names it.
    pub(super) async fn project_resource_for_upstream(
        &self,
        subject: &AuthenticatedSubject,
        action: HubAction,
        uri: &str,
    ) -> Result<(crate::registration::RegisteredServer, HubResourceProjection), McpError> {
        let server = self.resource_server(subject, action, uri).await?;
        let upstream_uri =
            crate::mcp_support::project_hub_resource_uri_for_upstream(&server, uri, &[])?
                .ok_or_else(|| mcp_invalid_params(format!("resource URI is not exposed: {uri}")))?;
        let projection = HubResourceProjection {
            server: server.slug.clone(),
            hub_uri: hub_resource_uri(uri)?,
            upstream_uri,
        };
        Ok((server, projection))
    }

    pub(super) fn project_upstream_resource(
        &self,
        server: &ServerSlug,
        uri: &str,
    ) -> Result<HubResourceProjection, McpError> {
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(server)
            .ok_or_else(|| mcp_internal(format!("unknown upstream server `{server}`")))?;
        project_upstream_resource(manifest, uri)
    }

    /// The server, or for a linked server the runner the caller reaches first, that
    /// serves `prompt` to `subject`.
    pub(super) fn server_for_prompt(
        &self,
        subject: &AuthenticatedSubject,
        prompt: &str,
    ) -> Result<crate::registration::RegisteredServer, McpError> {
        let prompt = PromptName::parse(prompt)
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        let catalog = self.catalog.current();
        let matches = catalog.prompt_servers(&self.profile_id, &subject.principal, &prompt);
        match matches.as_slice() {
            [server] => Ok((*server).clone()),
            [] => Err(mcp_invalid_params(format!(
                "prompt is not exposed: {prompt}"
            ))),
            _ => Err(mcp_internal(format!(
                "prompt `{prompt}` is ambiguous across profile servers"
            ))),
        }
    }
}

/// The pure request keeps current authority opaque; this adapter changes only execution.
pub(super) fn with_task_execution(
    mut subject: AuthenticatedSubject,
    capture: veoveo_hub_sdk::ExecutionContext,
) -> AuthenticatedSubject {
    subject.execution = Some(capture);
    subject
}

fn trace_id_for_context(context: &RequestContext<RoleServer>) -> Result<TraceId, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))?;
    let observation = parts
        .extensions
        .get::<crate::request_observation::RequestObservation>()
        .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))?;
    TraceId::parse(&observation.audit.trace_id).map_err(|_| mcp_internal("invalid request trace"))
}

fn client_surface_allows_task_projection(
    surface: OAuthClientSurface,
    direct_task_call_adapter: bool,
) -> bool {
    match surface {
        OAuthClientSurface::FullMcp => true,
        OAuthClientSurface::ToolsCompat => direct_task_call_adapter,
    }
}

/// A policy target as its caller wrote it. Policy decides on a server's own names,
/// which a runner's lack its link; denials and audit records name what the caller
/// named: a runner's tool `{server}__{link}__{tool}`, and its URIs and templates
/// `{server}://{link}/{rest}`.
#[derive(Debug, Clone)]
pub(super) enum Written {
    Target(PolicyTarget),
    LinkedTool {
        server: ServerSlug,
        link: veoveo_hub_sdk::hub::LinkName,
        tool: LocalToolName,
    },
}

impl Written {
    fn audit_target(&self) -> anyhow::Result<veoveo_audit_contract::AuditTarget> {
        match self {
            Self::Target(target) => crate::audit::policy_target(target),
            Self::LinkedTool { server, link, tool } => {
                Ok(veoveo_hub_sdk::audit::McpTarget::LinkedTool {
                    server: server.clone(),
                    link: link.clone(),
                    tool: tool.clone(),
                }
                .audit_target()?)
            }
        }
    }

    fn denial(&self, reason: PolicyReasonCode) -> String {
        match self {
            Self::Target(target) => denial_message(target, reason),
            Self::LinkedTool { server, link, tool } => {
                denial_text(&format!("call `{server}__{link}__{tool}`"), reason)
            }
        }
    }
}

/// Caller-facing text for a policy denial. The reason code is mapped to words so
/// callers never see a Rust variant name; the full decision stays in the audit log.
fn denial_message(target: &PolicyTarget, reason: PolicyReasonCode) -> String {
    denial_text(&target_action(target), reason)
}

fn target_action(target: &PolicyTarget) -> String {
    match target {
        PolicyTarget::Tool { server, tool } => format!("call `{server}__{tool}`"),
        PolicyTarget::Resource { uri, .. } => format!("read `{uri}`"),
        PolicyTarget::ResourceTemplate { uri, .. } => format!("use resource template `{uri}`"),
        PolicyTarget::Prompt { server, prompt } => format!("use prompt `{prompt}` on `{server}`"),
        PolicyTarget::Task { task_id, .. } => format!("access task `{task_id}`"),
        PolicyTarget::PlatformTask { task_id, .. } => format!("access task `{task_id}`"),
        PolicyTarget::Server { server } => format!("use the `{server}` server"),
        PolicyTarget::AppOrigin { server } => format!("open the app origin of `{server}`"),
        PolicyTarget::Hub | PolicyTarget::Owner(_) | PolicyTarget::Unadmitted(_) => {
            "make this request".to_owned()
        }
    }
}

fn denial_text(action: &str, reason: PolicyReasonCode) -> String {
    let detail = match reason {
        PolicyReasonCode::MissingScope | PolicyReasonCode::UnknownScope => {
            " Your access token is missing a scope this profile requires. Ask an administrator \
             to grant it, then sign in again."
        }
        PolicyReasonCode::MissingDataLabel | PolicyReasonCode::UnknownDataLabel => {
            " Your account isn't cleared for the data labels this requires."
        }
        PolicyReasonCode::MissingRole | PolicyReasonCode::MissingGroup => {
            " Your account doesn't have the role or group membership this requires."
        }
        PolicyReasonCode::MissingPrincipalAssurance => {
            " Sign in again with the stronger authentication this profile requires."
        }
        PolicyReasonCode::MissingPrincipal
        | PolicyReasonCode::UnknownPrincipal
        | PolicyReasonCode::MissingTenant
        | PolicyReasonCode::UnknownTenant
        | PolicyReasonCode::UnknownTokenIssuer => {
            " Your identity isn't recognized by this installation. Sign in again."
        }
        PolicyReasonCode::TokenExpired | PolicyReasonCode::TokenNotYetValid => {
            " Your access token isn't currently valid. Sign in again."
        }
        PolicyReasonCode::TokenAudienceMismatch => {
            " Your access token was issued for a different resource. Sign in to this profile."
        }
        PolicyReasonCode::ReplayDetected => " This request was already used. Send a new request.",
        PolicyReasonCode::UnknownProfile
        | PolicyReasonCode::UnknownServer
        | PolicyReasonCode::UnknownTool
        | PolicyReasonCode::UnknownResource
        | PolicyReasonCode::UnknownPrompt
        | PolicyReasonCode::UnknownTask
        | PolicyReasonCode::UndeclaredTarget => " It isn't available in this profile.",
        PolicyReasonCode::MissingPermission => {
            " Your roles don't grant the permission this requires."
        }
        PolicyReasonCode::PolicyDeny | PolicyReasonCode::PolicyAllow => "",
    };
    format!("You don't have permission to {action}.{detail}")
}

fn delegated_route<'a>(
    access: Option<&'a veoveo_hub_sdk::registration::RouteAccess>,
    target: &PolicyTarget,
) -> crate::delegation::authority::Route<'a, veoveo_types::PermissionName> {
    use crate::delegation::authority::Route;
    use veoveo_hub_sdk::registration::RouteAccess;
    match access {
        Some(RouteAccess::Authenticated) => Route::Authenticated,
        Some(RouteAccess::Permission(permission)) => Route::Permission(permission),
        None if matches!(
            target,
            PolicyTarget::Server { .. }
                | PolicyTarget::Task { .. }
                | PolicyTarget::PlatformTask { .. }
        ) =>
        {
            Route::Server
        }
        None => Route::Undeclared,
    }
}

fn narrow_policy_decision(
    mut decision: PolicyDecision,
    managed: bool,
    carried: Option<&std::collections::BTreeSet<veoveo_types::PermissionName>>,
    route: crate::delegation::authority::Route<'_, veoveo_types::PermissionName>,
) -> PolicyDecision {
    use crate::delegation::authority;
    // Move the original decision through the shared assembly, retaining every
    // correlation/target field without reconstructing them independently.
    let narrowed = authority::narrow(
        authority::Decision {
            allowed: decision.effect == PolicyEffect::Allow,
            reason: decision.reason,
            granted: std::mem::take(&mut decision.granted),
            metadata: decision,
        },
        managed,
        carried.map(|permissions| (permissions, route)),
        PolicyReasonCode::PolicyDeny,
        PolicyReasonCode::MissingPermission,
    );
    let mut decision = narrowed.metadata;
    decision.effect = if narrowed.allowed {
        PolicyEffect::Allow
    } else {
        PolicyEffect::Deny
    };
    decision.reason = narrowed.reason;
    decision.granted = narrowed.granted;
    decision
}

#[cfg(test)]
mod tests {
    use crate::contract::HubAction;
    use crate::control_plane::{
        OAuthClientSurface, PolicyDecision, PolicyReasonCode, PolicyTarget, TraceId,
    };
    use veoveo_hub_sdk::{HubProfileId, LocalToolName, ServerSlug};

    use super::{Written, client_surface_allows_task_projection, denial_message};

    #[test]
    fn policy_denial_names_the_tool_and_explains_the_reason() {
        let decision = PolicyDecision::deny(
            HubProfileId::parse("operator").unwrap(),
            HubAction::ToolsCall,
            PolicyTarget::Tool {
                server: ServerSlug::parse("upstream-a").unwrap(),
                tool: LocalToolName::parse("route").unwrap(),
            },
            PolicyReasonCode::MissingScope,
            TraceId::parse("policy-denial-message").unwrap(),
        );
        let message = denial_message(&decision.target, decision.reason);
        assert!(message.starts_with("You don't have permission to call `upstream-a__route`."));
        assert!(message.contains("missing a scope"));
        assert!(!message.contains("MissingScope"));
        // A runner's tool is named as the caller wrote it, with its link.
        let linked = Written::LinkedTool {
            server: ServerSlug::parse("ledger").unwrap(),
            link: veoveo_hub_sdk::hub::LinkName::parse("team-b").unwrap(),
            tool: LocalToolName::parse("inspect").unwrap(),
        };
        assert!(
            linked
                .denial(PolicyReasonCode::UndeclaredTarget)
                .starts_with("You don't have permission to call `ledger__team-b__inspect`.")
        );
        assert_eq!(
            linked.audit_target().unwrap().as_str(),
            "veoveo://servers/ledger/links/team-b/tools/inspect"
        );
    }

    #[test]
    fn full_mcp_always_receives_canonical_tasks() {
        assert!(client_surface_allows_task_projection(
            OAuthClientSurface::FullMcp,
            false
        ));
        assert!(client_surface_allows_task_projection(
            OAuthClientSurface::FullMcp,
            true
        ));
    }

    #[test]
    fn tools_compat_requires_explicit_task_projection() {
        assert!(!client_surface_allows_task_projection(
            OAuthClientSurface::ToolsCompat,
            false
        ));
        assert!(client_surface_allows_task_projection(
            OAuthClientSurface::ToolsCompat,
            true
        ));
    }
    #[test]
    fn delegated_routes_require_only_their_declared_access_and_keep_server_boundaries() {
        use std::collections::BTreeSet;
        use veoveo_hub_sdk::registration::RouteAccess;
        use veoveo_types::PermissionName;
        let target = PolicyTarget::Tool {
            server: ServerSlug::parse("upstream-a").unwrap(),
            tool: LocalToolName::parse("route").unwrap(),
        };
        let permission = PermissionName::parse("upstream-a.records.read").unwrap();
        let empty = BTreeSet::new();
        let held = BTreeSet::from([permission.clone()]);
        let admit = |access: Option<RouteAccess>,
                     target: &PolicyTarget,
                     carried: &BTreeSet<PermissionName>,
                     granted: &BTreeSet<PermissionName>,
                     allowed: bool| {
            let mut decision = PolicyDecision::deny(
                HubProfileId::parse("operator").unwrap(),
                HubAction::ToolsCall,
                target.clone(),
                PolicyReasonCode::PolicyAllow,
                TraceId::parse("delegated-policy").unwrap(),
            );
            decision.effect = crate::control_plane::PolicyEffect::Allow;
            decision.granted = granted.clone();
            let mut expected = decision.clone();
            expected.granted = granted.intersection(carried).cloned().collect();
            expected.effect = if allowed {
                crate::control_plane::PolicyEffect::Allow
            } else {
                crate::control_plane::PolicyEffect::Deny
            };
            expected.reason = if allowed {
                PolicyReasonCode::PolicyAllow
            } else {
                PolicyReasonCode::MissingPermission
            };
            let output = super::narrow_policy_decision(
                decision,
                true,
                Some(carried),
                super::delegated_route(access.as_ref(), target),
            );
            assert_eq!(output, expected);
        };
        admit(
            Some(RouteAccess::Authenticated),
            &target,
            &empty,
            &empty,
            true,
        );
        admit(
            Some(RouteAccess::Permission(permission.clone())),
            &target,
            &empty,
            &empty,
            false,
        );
        admit(
            Some(RouteAccess::Permission(permission.clone())),
            &target,
            &held,
            &held,
            true,
        );
        // The current-policy consistency premise matters: this assembly does not
        // invent a new rejection for arbitrary inconsistent Allowed input.
        admit(
            Some(RouteAccess::Permission(permission)),
            &target,
            &held,
            &empty,
            true,
        );
        admit(None, &target, &held, &held, false);
        let server = PolicyTarget::Server {
            server: ServerSlug::parse("upstream-a").unwrap(),
        };
        admit(None, &server, &empty, &empty, false);
        admit(None, &server, &held, &held, true);
    }

    #[test]
    fn actual_registered_policy_narrows_current_grants() {
        use crate::control_plane::{HubControlPlane, PolicyEffect};
        use std::collections::BTreeSet;
        use veoveo_hub_sdk::registration::{
            Access, AuthorizationDeclaration, PermissionDeclaration, Requirements, RouteAccess,
        };
        use veoveo_types::{PermissionName, PolicyVersion};
        // A complete, validated local catalog fixture; these URLs are never fetched.
        // This test owns every registered-policy-fixture name below.
        let plane: HubControlPlane = serde_json::from_value(serde_json::json!({
            "identityProviders": [{ "id": "registered-policy-idp", "issuer": "https://registered-policy.example", "jwks": { "source": "remote", "jwksUri": "https://registered-policy.example/keys" } }],
            "authorizationServers": [{ "id": "registered-policy-as", "issuer": "https://registered-policy.example", "jwks": { "source": "remote", "jwksUri": "https://registered-policy.example/keys" }, "tokenEndpoint": "https://registered-policy.example/token" }],
            "profiles": [{ "id": "registered-policy", "identityProvider": "registered-policy-idp", "authorizationServer": "registered-policy-as", "protectedResource": "https://registered-policy.example/mcp", "policyVersion": "2026-10-30", "authModes": ["oauth_client_credentials"] }],
            "authorization": {
                "roles": [{ "name": "registered-policy-reader", "access": ["read"] }],
                "grants": [{ "role": "registered-policy-reader", "subject": { "principal": "registered-policy-caller" } }]
            },
            "policies": [{ "version": "2026-10-30", "rules": [] }],
            "tenants": [], "dataLabels": [], "oidcClients": []
        })).unwrap();
        let slug = ServerSlug::parse("registered-policy-fixture").unwrap();
        let permission = |action: &str| {
            PermissionName::parse(format!("registered-policy-fixture.records.{action}")).unwrap()
        };
        let read = permission("read");
        let inspect = permission("inspect");
        let write = permission("write");
        let resource_uri = |authority: &str| {
            veoveo_types::ResourceUri::new(format!("{slug}://{authority}/record")).unwrap()
        };
        let template_uri = |authority: &str, pattern: &str| {
            veoveo_types::ResourceTemplateUri::new(format!("{slug}://{authority}/{pattern}"))
                .unwrap()
        };
        let exact_resource = resource_uri("exact");
        let agreed_resource = resource_uri("agreed");
        let mixed_resource = resource_uri("mixed");
        let different_resource = resource_uri("different");
        let authenticated_resource = resource_uri("authenticated");
        let direct_template = template_uri("direct", "{name}");
        let authenticated_prompt = veoveo_hub_sdk::PromptName::parse("authenticated").unwrap();
        let read_prompt = veoveo_hub_sdk::PromptName::parse("read").unwrap();
        let declaration = AuthorizationDeclaration {
            permissions: [
                (read.clone(), Access::Read),
                (inspect.clone(), Access::Read),
                (write.clone(), Access::Write),
            ]
            .into_iter()
            .map(|(permission, access)| PermissionDeclaration {
                permission,
                resource: veoveo_types::ResourceTypeName::parse(
                    "registered-policy-fixture.records",
                )
                .unwrap(),
                access,
                risks: BTreeSet::new(),
            })
            .collect(),
            requirements: Requirements {
                tools: [
                    (
                        LocalToolName::parse("authenticated").unwrap(),
                        RouteAccess::Authenticated,
                    ),
                    (
                        LocalToolName::parse("read").unwrap(),
                        RouteAccess::Permission(read.clone()),
                    ),
                    (
                        LocalToolName::parse("write").unwrap(),
                        RouteAccess::Permission(write.clone()),
                    ),
                ]
                .into(),
                resources: [(
                    exact_resource.clone(),
                    RouteAccess::Permission(read.clone()),
                )]
                .into(),
                resource_templates: [
                    (
                        template_uri("exact", "{name}"),
                        RouteAccess::Permission(write.clone()),
                    ),
                    (
                        template_uri("exact", "{+tail}"),
                        RouteAccess::Permission(inspect.clone()),
                    ),
                    (
                        template_uri("agreed", "{name}"),
                        RouteAccess::Permission(read.clone()),
                    ),
                    (
                        template_uri("agreed", "{+tail}"),
                        RouteAccess::Permission(read.clone()),
                    ),
                    (template_uri("mixed", "{name}"), RouteAccess::Authenticated),
                    (
                        template_uri("mixed", "{+tail}"),
                        RouteAccess::Permission(read.clone()),
                    ),
                    (
                        template_uri("different", "{name}"),
                        RouteAccess::Permission(read.clone()),
                    ),
                    (
                        template_uri("different", "{+tail}"),
                        RouteAccess::Permission(inspect.clone()),
                    ),
                    (
                        template_uri("authenticated", "{name}"),
                        RouteAccess::Authenticated,
                    ),
                    (
                        direct_template.clone(),
                        RouteAccess::Permission(inspect.clone()),
                    ),
                ]
                .into(),
                prompts: [
                    (authenticated_prompt.clone(), RouteAccess::Authenticated),
                    (read_prompt.clone(), RouteAccess::Permission(read.clone())),
                ]
                .into(),
                app_origin: None,
            },
            uses: Vec::new(),
            calls_hub: false,
        };
        declaration
            .check(
                &slug,
                &veoveo_types::ResourceScheme::parse("registered-policy-fixture").unwrap(),
            )
            .unwrap();
        let catalog = crate::policy::PolicyCatalog::new(plane, crate::test_catalog::registry())
            .unwrap()
            .with_server(slug.clone(), declaration.clone());
        let caller = veoveo_hub_sdk::Principal {
            id: veoveo_types::PrincipalId::parse("registered-policy-caller").unwrap(),
            kind: veoveo_hub_sdk::PrincipalKind::User,
            issuer: veoveo_hub_sdk::TokenIssuer::parse("https://registered-policy.example")
                .unwrap(),
            subject: veoveo_hub_sdk::TokenSubject::parse("registered-policy-subject").unwrap(),
            tenant: None,
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let mut other = caller.clone();
        other.id = veoveo_types::PrincipalId::parse("registered-policy-other").unwrap();
        let profile = HubProfileId::parse("registered-policy").unwrap();
        let trace = TraceId::parse("registered-policy-downstream").unwrap();
        let tool = |name| PolicyTarget::Tool {
            server: slug.clone(),
            tool: LocalToolName::parse(name).unwrap(),
        };
        let held = BTreeSet::from([read.clone(), inspect.clone()]);
        let empty = BTreeSet::new();
        // (target, action, caller, managed, carried, effect, reason, required, grants)
        for (target, action, principal, managed, carried, effect, reason, required, granted) in [
            (
                tool("authenticated"),
                HubAction::ToolsCall,
                &other,
                true,
                empty.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                None,
                empty.clone(),
            ),
            (
                tool("authenticated"),
                HubAction::ToolsCall,
                &caller,
                true,
                empty.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                None,
                empty.clone(),
            ),
            (
                tool("read"),
                HubAction::ToolsCall,
                &caller,
                true,
                BTreeSet::from([read.clone(), inspect.clone(), write.clone()]),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                Some(read.clone()),
                held.clone(),
            ),
            (
                tool("read"),
                HubAction::ToolsCall,
                &caller,
                true,
                BTreeSet::from([read.clone()]),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                Some(read.clone()),
                BTreeSet::from([read.clone()]),
            ),
            (
                tool("read"),
                HubAction::ToolsCall,
                &caller,
                true,
                BTreeSet::from([inspect.clone()]),
                PolicyEffect::Deny,
                PolicyReasonCode::MissingPermission,
                Some(read.clone()),
                BTreeSet::from([inspect.clone()]),
            ),
            (
                tool("write"),
                HubAction::ToolsCall,
                &caller,
                true,
                BTreeSet::from([write.clone()]),
                PolicyEffect::Deny,
                PolicyReasonCode::MissingPermission,
                Some(write.clone()),
                empty.clone(),
            ),
            (
                PolicyTarget::Server {
                    server: slug.clone(),
                },
                HubAction::ResourcesList,
                &caller,
                true,
                BTreeSet::from([inspect.clone()]),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                None,
                BTreeSet::from([inspect.clone()]),
            ),
            (
                PolicyTarget::Server {
                    server: slug.clone(),
                },
                HubAction::ResourcesList,
                &caller,
                true,
                empty.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::MissingPermission,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::Task {
                    server: slug.clone(),
                    task_id: veoveo_hub_sdk::CanonicalTaskId::parse(
                        uuid::Uuid::now_v7().to_string(),
                    )
                    .unwrap(),
                },
                HubAction::TasksGet,
                &caller,
                true,
                BTreeSet::from([inspect.clone()]),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                None,
                BTreeSet::from([inspect.clone()]),
            ),
            (
                PolicyTarget::PlatformTask {
                    server: slug.clone(),
                    task_id: veoveo_types::TaskId::new(),
                },
                HubAction::TasksGet,
                &caller,
                true,
                empty.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::MissingPermission,
                None,
                empty.clone(),
            ),
            (
                tool("missing"),
                HubAction::ToolsCall,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::UnknownTool,
                None,
                empty.clone(),
            ),
            (
                tool("read"),
                HubAction::ToolsCall,
                &caller,
                false,
                empty.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::PolicyDeny,
                Some(read.clone()),
                held.clone(),
            ),
            (
                PolicyTarget::Resource {
                    server: slug.clone(),
                    uri: exact_resource.clone(),
                },
                HubAction::ResourcesRead,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                Some(read.clone()),
                held.clone(),
            ),
            (
                PolicyTarget::Resource {
                    server: slug.clone(),
                    uri: agreed_resource.clone(),
                },
                HubAction::ResourcesRead,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                Some(read.clone()),
                held.clone(),
            ),
            (
                PolicyTarget::Resource {
                    server: slug.clone(),
                    uri: mixed_resource.clone(),
                },
                HubAction::ResourcesRead,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::UndeclaredTarget,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::Resource {
                    server: slug.clone(),
                    uri: different_resource.clone(),
                },
                HubAction::ResourcesRead,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::UndeclaredTarget,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::Resource {
                    server: slug.clone(),
                    uri: authenticated_resource.clone(),
                },
                HubAction::ResourcesRead,
                &other,
                true,
                empty.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::ResourceTemplate {
                    server: slug.clone(),
                    uri: direct_template.clone(),
                },
                HubAction::ResourcesTemplatesList,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                Some(inspect.clone()),
                held.clone(),
            ),
            (
                PolicyTarget::Prompt {
                    server: slug.clone(),
                    prompt: authenticated_prompt.clone(),
                },
                HubAction::PromptsGet,
                &other,
                true,
                empty.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::Prompt {
                    server: slug.clone(),
                    prompt: read_prompt.clone(),
                },
                HubAction::PromptsGet,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                Some(read.clone()),
                held.clone(),
            ),
            (
                PolicyTarget::Resource {
                    server: slug.clone(),
                    uri: resource_uri("missing"),
                },
                HubAction::ResourcesRead,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::UndeclaredTarget,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::ResourceTemplate {
                    server: slug.clone(),
                    uri: template_uri("missing", "{name}"),
                },
                HubAction::ResourcesTemplatesList,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::UndeclaredTarget,
                None,
                empty.clone(),
            ),
            (
                PolicyTarget::Prompt {
                    server: slug.clone(),
                    prompt: veoveo_hub_sdk::PromptName::parse("missing").unwrap(),
                },
                HubAction::PromptsGet,
                &caller,
                true,
                held.clone(),
                PolicyEffect::Deny,
                PolicyReasonCode::UnknownPrompt,
                None,
                empty.clone(),
            ),
        ] {
            let before = chrono::Utc::now();
            let base = crate::policy::decide(
                &catalog,
                crate::policy::PolicyRequest {
                    principal,
                    profile: &profile,
                    action: action.into(),
                    target: &target,
                    trace_id: &trace,
                },
            );
            let after = chrono::Utc::now();
            assert!(before <= base.evaluated_at && base.evaluated_at <= after);
            let sampled = base.evaluated_at;
            let access = crate::policy::declared_route_access(&catalog, &target);
            let route = super::delegated_route(access, &target);
            let output = super::narrow_policy_decision(base, managed, Some(&carried), route);
            assert_eq!(
                output,
                PolicyDecision {
                    effect,
                    reason,
                    evaluated_at: sampled,
                    profile: profile.clone(),
                    action: action.into(),
                    target,
                    principal: Some(principal.id.clone()),
                    tenant: None,
                    policy_version: Some(PolicyVersion::parse("2026-10-30").unwrap()),
                    rule_id: None,
                    permission: required,
                    granted,
                    trace_id: trace.clone(),
                }
            );
        }
        // Direct SDK observations supplement the full actual-policy assertions above.
        use veoveo_hub_sdk::registration::RequirementTarget;
        for (target, expected) in [
            (
                RequirementTarget::Resource(&exact_resource),
                Some(RouteAccess::Permission(read.clone())),
            ),
            (
                RequirementTarget::Resource(&agreed_resource),
                Some(RouteAccess::Permission(read.clone())),
            ),
            (RequirementTarget::Resource(&mixed_resource), None),
            (RequirementTarget::Resource(&different_resource), None),
            (
                RequirementTarget::Resource(&authenticated_resource),
                Some(RouteAccess::Authenticated),
            ),
            (
                RequirementTarget::ResourceTemplate(&direct_template),
                Some(RouteAccess::Permission(inspect.clone())),
            ),
            (
                RequirementTarget::Prompt(&authenticated_prompt),
                Some(RouteAccess::Authenticated),
            ),
            (
                RequirementTarget::Prompt(&read_prompt),
                Some(RouteAccess::Permission(read)),
            ),
        ] {
            assert_eq!(declaration.access(target).cloned(), expected);
        }
    }

    #[test]
    fn delegated_policy_assembly_preserves_prior_denial_and_non_delegated_decisions() {
        use crate::control_plane::PolicyEffect;
        use crate::delegation::authority::Route;
        use std::collections::BTreeSet;
        use veoveo_types::PermissionName;
        let mut denied = PolicyDecision::deny(
            HubProfileId::parse("operator").unwrap(),
            HubAction::ToolsCall,
            PolicyTarget::Hub,
            PolicyReasonCode::MissingScope,
            TraceId::parse("delegated-policy-denied").unwrap(),
        );
        // Even an unusual populated denied decision must not be normalized away.
        denied.granted =
            BTreeSet::from([PermissionName::parse("upstream-a.records.read").unwrap()]);
        let empty = BTreeSet::new();
        assert_eq!(
            super::narrow_policy_decision(denied.clone(), true, Some(&empty), Route::Undeclared),
            denied
        );
        let mut allowed = denied.clone();
        allowed.effect = PolicyEffect::Allow;
        allowed.reason = PolicyReasonCode::PolicyAllow;
        assert_eq!(
            super::narrow_policy_decision(allowed.clone(), true, None, Route::Undeclared),
            allowed
        );
        for original in [allowed, denied] {
            let mut expected = original.clone();
            expected.effect = PolicyEffect::Deny;
            expected.reason = PolicyReasonCode::PolicyDeny;
            assert_eq!(
                super::narrow_policy_decision(original, false, Some(&empty), Route::Authenticated),
                expected
            );
        }
    }

    // These synthetic admitted-subject fixtures exercise native catalog admission,
    // actual policy/narrowing and the real discovery cache. Token signatures,
    // RFC 8693 endpoint authentication, HTTP dispatch and watches are not executed.
    // The source exchange/admission path preserves caller fields and carries its
    // separately signed permission ceiling in SubjectDelegation.
    fn discovery_catalog() -> (crate::HubCatalog, ServerSlug) {
        use crate::control_plane::{
            GrantSubject, InstallationAuthorization, InstallationRole, RoleGrant,
        };
        use std::{collections::BTreeSet, sync::Arc};
        use veoveo_hub_sdk::registration::{
            Access, AuthorizationDeclaration, PermissionDeclaration, Requirements, RouteAccess,
        };
        use veoveo_types::{GroupId, PermissionName, ResourceTypeName, RoleId};

        let mut plane: crate::control_plane::HubControlPlane =
            serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap();
        plane.authorization = InstallationAuthorization {
            roles: vec![
                InstallationRole {
                    name: RoleId::parse("cache-reader").unwrap(),
                    access: BTreeSet::from([Access::Read]),
                    risks: BTreeSet::new(),
                },
                InstallationRole {
                    name: RoleId::parse("cache-writer").unwrap(),
                    access: BTreeSet::from([Access::Write]),
                    risks: BTreeSet::new(),
                },
            ],
            grants: vec![
                RoleGrant {
                    role: RoleId::parse("cache-reader").unwrap(),
                    subject: GrantSubject::Principal(super::super::tests::principal().id),
                    tenant: None,
                },
                RoleGrant {
                    role: RoleId::parse("cache-writer").unwrap(),
                    subject: GrantSubject::Group(GroupId::parse("cache-writers").unwrap()),
                    tenant: None,
                },
            ],
            overrides: Vec::new(),
        };
        let permission = |name: &str, access| PermissionDeclaration {
            permission: PermissionName::parse(name).unwrap(),
            resource: ResourceTypeName::parse("cache-fixture.records").unwrap(),
            access,
            risks: BTreeSet::new(),
        };
        let mut requirements = Requirements::default();
        requirements.tools.insert(
            LocalToolName::parse("common").unwrap(),
            RouteAccess::Permission(PermissionName::parse("cache-fixture.records.read").unwrap()),
        );
        requirements.tools.insert(
            LocalToolName::parse("extra").unwrap(),
            RouteAccess::Permission(PermissionName::parse("cache-fixture.records.write").unwrap()),
        );
        let mut server = crate::test_catalog::registered_server(
            "cache-fixture",
            "https://cache-fixture.test/mcp",
        );
        server.authorization = Arc::new(AuthorizationDeclaration {
            permissions: vec![
                permission("cache-fixture.records.read", Access::Read),
                permission("cache-fixture.records.write", Access::Write),
            ],
            requirements,
            uses: Vec::new(),
            calls_hub: false,
        });
        server.capabilities =
            crate::registration::ServerCapabilities::from_declaration(&server.authorization);
        let slug = server.slug.clone();
        let catalog = crate::HubCatalog::from_control_plane(plane, crate::test_catalog::binding())
            .unwrap()
            .with_registrations(crate::registration::RegistrationSnapshot::new([server]));
        (catalog, slug)
    }

    fn discovery_subject(catalog: &crate::HubCatalog, writer: bool) -> crate::AuthenticatedSubject {
        use chrono::{TimeDelta, Utc};
        use std::collections::BTreeSet;
        use veoveo_hub_sdk::{AccessTokenSubject, OAuthClientId, ProtectedResourceId};
        use veoveo_types::{DelegationId, GroupId, InvocationMode, ScopeName, TenantId};

        let mut principal = super::super::tests::principal();
        principal.tenant = Some(TenantId::parse("tenant-a").unwrap());
        principal.scopes = BTreeSet::from([ScopeName::parse("operator:use").unwrap()]);
        if writer {
            principal
                .groups
                .insert(GroupId::parse("cache-writers").unwrap());
        }
        let access_token = AccessTokenSubject {
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::parse("operator-delegated").unwrap(),
            session_family: None,
            audience: ProtectedResourceId::parse("https://veoveo.example/mcp/operator").unwrap(),
            invocation_mode: InvocationMode::Delegated,
            initiator: Some(principal.id.clone()),
            delegation_id: Some(DelegationId::parse("cache-delegation").unwrap()),
            scopes: principal.scopes.clone(),
            jwt_id: None,
            issued_at: Utc::now(),
            not_before: None,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        };
        let mut subject = catalog
            .resolve_authenticated_subject(crate::VerifiedAccessToken {
                purpose: None,
                extensions: Default::default(),
                access_token,
                principal,
                principal_display_name: None,
            })
            .unwrap();
        subject.execution = Some(veoveo_hub_sdk::hosting::testing::execution_context());
        subject
            .request_context()
            .unwrap()
            .validate_for(&subject.actor, &subject.authority)
            .unwrap();
        subject
    }

    fn discovered_for(
        catalog: &crate::HubCatalog,
        server: &ServerSlug,
        subject: &crate::AuthenticatedSubject,
    ) -> super::super::discovery::AdmittedCatalog<rmcp::model::Tool> {
        use crate::control_plane::PolicyEffect;
        let mut items = Vec::new();
        let mut denied = 0;
        for name in ["common", "extra"] {
            let target = PolicyTarget::Tool {
                server: server.clone(),
                tool: LocalToolName::parse(name).unwrap(),
            };
            let mut decision = catalog.decide(crate::PolicyRequest {
                principal: &subject.principal,
                profile: &HubProfileId::parse("operator").unwrap(),
                action: HubAction::ToolsList.into(),
                target: &target,
                trace_id: &TraceId::parse("cache-isolation-regression").unwrap(),
            });
            let route = if decision.effect == PolicyEffect::Allow && subject.delegation.is_some() {
                super::delegated_route(
                    crate::policy::declared_route_access(catalog, &target),
                    &target,
                )
            } else {
                crate::delegation::authority::Route::Undeclared
            };
            // This installed-client fixture has current OAuth admission. The
            // separate dynamic OAuth-owner/DB check is outside this unit test.
            decision = super::narrow_policy_decision(
                decision,
                true,
                subject.delegation.as_ref().map(|info| &info.permissions),
                route,
            );
            if decision.effect == PolicyEffect::Allow {
                items.push(rmcp::model::Tool::new(
                    format!("{server}__{name}"),
                    format!("{name} synthetic descriptor"),
                    std::sync::Arc::default(),
                ));
            } else {
                denied += 1;
            }
        }
        super::super::discovery::AdmittedCatalog { items, denied }
    }

    fn discovery_key(
        subject: &crate::AuthenticatedSubject,
        server: &ServerSlug,
    ) -> super::super::discovery::DiscoveryCacheKey {
        super::super::discovery::DiscoveryCacheKey {
            catalog_generation: 7,
            principal: subject.actor.id.clone(),
            authorization_fingerprint: super::super::discovery_authorization_fingerprint(subject)
                .unwrap(),
            server: server.clone(),
            link: None,
        }
    }

    async fn assert_cache_population_orders(
        catalog: &crate::HubCatalog,
        server: &ServerSlug,
        broad: &crate::AuthenticatedSubject,
        narrow: &crate::AuthenticatedSubject,
    ) {
        use super::super::discovery::CatalogDiscoveryCache;
        let broad_expected = discovered_for(catalog, server, broad);
        let narrow_expected = discovered_for(catalog, server, narrow);
        assert_eq!((broad_expected.items.len(), broad_expected.denied), (2, 0));
        assert_eq!(
            (narrow_expected.items.len(), narrow_expected.denied),
            (1, 1)
        );
        assert_eq!(narrow_expected.items, broad_expected.items[..1]);
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        for (first, second, first_expected, second_expected) in [
            (broad, narrow, &broad_expected, &narrow_expected),
            (narrow, broad, &narrow_expected, &broad_expected),
        ] {
            let cache = CatalogDiscoveryCache::default();
            let first_key = discovery_key(first, server);
            let fetch = cache
                .start_tools(first_key.clone())
                .await
                .unwrap()
                .with_denied(first_expected.denied);
            cache
                .store_tools(Some(fetch), first_expected.items.clone())
                .await;
            assert_eq!(cache.tools(&first_key).await.as_ref(), Some(first_expected));

            // Fresh authentication timestamps and audit observations do not
            // change effective authority and must retain positive cache reuse.
            let mut reverified = first.clone();
            reverified.actor.authenticated_at = Some(chrono::Utc::now());
            reverified.principal.authenticated_at = reverified.actor.authenticated_at;
            reverified.audit = veoveo_audit_contract::AuditRequest::background();
            let reverified_key = discovery_key(&reverified, server);
            assert_eq!(reverified_key, first_key);
            assert_eq!(
                cache.tools(&reverified_key).await.as_ref(),
                Some(first_expected)
            );

            // This is the production cache-hit/fresh-admission decision. Compare
            // the entire descriptor payload and denied count, not merely names.
            let second_key = discovery_key(second, server);
            let observed = match cache.tools(&second_key).await {
                Some(cached) => cached,
                None => {
                    let admitted = discovered_for(catalog, server, second);
                    let fetch = cache
                        .start_tools(second_key)
                        .await
                        .unwrap()
                        .with_denied(admitted.denied);
                    cache.store_tools(Some(fetch), admitted.items.clone()).await;
                    admitted
                }
            };
            actual.push(observed);
            expected.push(second_expected.clone());
        }
        assert_eq!(
            actual, expected,
            "both broad-first and narrow-first discovery must retain current admission"
        );
    }

    #[tokio::test]
    async fn discovery_cache_separates_delegation_ceilings_in_both_fill_orders() {
        use std::collections::BTreeSet;
        use veoveo_hub_sdk::ActingServer;
        use veoveo_types::PermissionName;
        let (catalog, server) = discovery_catalog();
        let mut broad = discovery_subject(&catalog, true);
        let context = broad.request_context().unwrap();
        broad.delegation = Some(crate::delegation::SubjectDelegation {
            acting_server: ActingServer {
                deployment: veoveo_hub_sdk::hosting::testing::deployment_id(),
                server: ServerSlug::parse("cache-forwarder").unwrap(),
                client: veoveo_hub_sdk::OAuthClientId::parse("cache-forwarder-0001").unwrap(),
            },
            permissions: BTreeSet::from([
                PermissionName::parse("cache-fixture.records.read").unwrap(),
                PermissionName::parse("cache-fixture.records.write").unwrap(),
            ]),
            task: None,
            request_context: context,
            context: None,
            expires_at: broad.access_token.expires_at,
        });
        let mut narrow = broad.clone();
        narrow
            .delegation
            .as_mut()
            .unwrap()
            .permissions
            .remove(&PermissionName::parse("cache-fixture.records.write").unwrap());
        assert_cache_population_orders(&catalog, &server, &broad, &narrow).await;
    }

    #[tokio::test]
    async fn discovery_cache_separates_current_principal_groups_in_both_fill_orders() {
        let (catalog, server) = discovery_catalog();
        let broad = discovery_subject(&catalog, true);
        let mut narrow = discovery_subject(&catalog, false);
        // Bind both to the same retained routing context; native delegated
        // admission preserves the user's differing groups but clears actor groups.
        narrow.execution = broad.execution.clone();
        assert_eq!(broad.actor, narrow.actor);
        assert_eq!(broad.authority, narrow.authority);
        assert_ne!(broad.principal.groups, narrow.principal.groups);
        assert_cache_population_orders(&catalog, &server, &broad, &narrow).await;
    }
}
