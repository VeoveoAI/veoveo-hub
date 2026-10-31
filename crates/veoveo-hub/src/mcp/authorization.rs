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
use veoveo_store::{
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

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
        let expected_work_context =
            deterministic_work_context_id(tenant_key, subject.authority.work_context.as_str())
                .map_err(|error| mcp_internal(format!("invalid task Work Context: {error}")))?;
        let request = task_decision::admit_owner(
            task_decision::Binding {
                tenant: route.tenant,
                owner: route.owner,
                work_context: route.work_context,
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
                    work_context: expected_work_context,
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
        let deployment = match composition.components().get(&request.task.server) {
            Some(veoveo_hub_sdk::ResolvedComponent::Deployment { deployment }) => Some(*deployment),
            _ => None,
        };
        let request = task_decision::capture(request, composition.context(), deployment)
            .map_err(|_| mcp_invalid_params("unknown task id"))?;
        let hub = self.for_execution(&snapshot, &composition);
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
            Err(mcp_invalid_request(policy_denial_message(&decision)))
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
            Err(mcp_invalid_request(policy_denial_message(&decision)))
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
            let target = crate::audit::policy_target(&target)
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
        let target =
            resource_policy_target(projection.server.clone(), projection.hub_uri.as_str())?;
        let decision = self
            .policy_decision(&subject, action, &target, trace_id_for_context(context)?)
            .await?;
        if decision.effect == PolicyEffect::Deny {
            self.record_policy_denial(&subject, action, target, decision.reason)
                .await?;
            return Err(mcp_invalid_request(policy_denial_message(&decision)));
        }
        Ok(subject)
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
            projection.hub_uri.as_str(),
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
        let target = crate::audit::policy_target(&target)
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

    pub(super) fn server_for_resource(&self, uri: &str) -> Result<ServerSlug, McpError> {
        self.catalog
            .current()
            .server_for_resource_uri(&self.profile_id, uri)
            .map(|server| server.slug.clone())
            .ok_or_else(|| mcp_invalid_params(format!("resource URI is not exposed: {uri}")))
    }

    pub(super) fn project_resource_for_upstream(
        &self,
        uri: &str,
    ) -> Result<HubResourceProjection, McpError> {
        let server = self.server_for_resource(uri)?;
        Ok(HubResourceProjection {
            server,
            hub_uri: hub_resource_uri(uri)?,
            upstream_uri: hub_resource_uri(uri)?,
        })
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

    pub(super) fn server_for_prompt(&self, prompt: &str) -> Result<ServerSlug, McpError> {
        let prompt = PromptName::parse(prompt)
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        let catalog = self.catalog.current();
        let matches = catalog.prompt_servers(&self.profile_id, &prompt);
        match matches.as_slice() {
            [server] => Ok(server.slug.clone()),
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

/// Caller-facing text for a policy denial. The reason code is mapped to words so
/// callers never see a Rust variant name; the full decision stays in the audit log.
fn policy_denial_message(decision: &PolicyDecision) -> String {
    let action = match &decision.target {
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
    };
    let detail = match decision.reason {
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

    use super::{client_surface_allows_task_projection, policy_denial_message};

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
        let message = policy_denial_message(&decision);
        assert!(message.starts_with("You don't have permission to call `upstream-a__route`."));
        assert!(message.contains("missing a scope"));
        assert!(!message.contains("MissingScope"));
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
            "tenants": [], "workContexts": [], "dataLabels": [], "oidcClients": []
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
}
