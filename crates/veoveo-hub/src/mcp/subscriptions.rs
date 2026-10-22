use crate::contract::HubAction;
use std::{collections::BTreeMap, pin::Pin};
use veoveo_hub_sdk::hub::HubDiscoverySurface;

use crate::control_plane::DiscoveryFailureMode;
use futures::{Stream, StreamExt, stream::SelectAll};
use rmcp::{
    model::{ErrorData as McpError, ServerNotification, SubscriptionFilter},
    service::{ServiceError, SubscriptionContext, SubscriptionEnd},
};
use veoveo_hub_sdk::{CanonicalTaskId, CompositionRevision, DeploymentId, ServerSlug};

use crate::mcp_support::{mcp_internal, upstream_error};

use super::{HubMcp, tools::project_detailed_task_resource_uris};

#[cfg(test)]
mod tests;

type SubscriptionRouteKey = (ServerSlug, DeploymentId, CompositionRevision);

#[derive(Default)]
struct UpstreamSubscriptionRoute {
    hub: Option<HubMcp>,
    subject: Option<crate::AuthenticatedSubject>,
    filter: SubscriptionFilter,
    task_ids: BTreeMap<String, String>,
}

type RoutedNotificationStream = Pin<
    Box<
        dyn Stream<
                Item = (
                    SubscriptionRouteKey,
                    bool,
                    Result<Option<ServerNotification>, ServiceError>,
                ),
            > + Send
            + 'static,
    >,
>;

struct OpenedUpstreamSubscription {
    hub: HubMcp,
    task_ids: BTreeMap<String, String>,
    connection: super::RequestUpstream,
    notifications: RoutedNotificationStream,
}

impl HubMcp {
    pub(super) fn accepted_subscriptions(
        &self,
        requested: &SubscriptionFilter,
    ) -> SubscriptionFilter {
        let mut accepted = SubscriptionFilter::new();
        accepted.tools_list_changed = requested.tools_list_changed;
        accepted.prompts_list_changed = requested.prompts_list_changed;
        accepted.resources_list_changed = requested.resources_list_changed;
        let resources = requested
            .resource_subscriptions
            .iter()
            .flatten()
            .filter(|uri| self.project_resource_for_upstream(uri).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        if !resources.is_empty() {
            accepted.resource_subscriptions = Some(resources);
        }
        let task_ids = requested
            .task_ids
            .iter()
            .flatten()
            .filter(|task_id| CanonicalTaskId::parse((*task_id).clone()).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        if !task_ids.is_empty() {
            accepted.task_ids = Some(task_ids);
        }
        accepted
    }

    pub(super) async fn handle_listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context();
        let subject = self.authenticated(request_context)?;
        let accepted = context.accepted().clone();
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let mut discovery_changes = self.discovery.subscribe();
        let mut catalog_changes = self.live_catalog.subscribe();
        let mut selection_changes = self.state.selection_changes.subscribe();
        let composition = super::execution_composition(&subject)?.clone();

        // A catalog listener primes discovery on the same replica that owns the
        // stream. Each server completes independently and wakes the caller through
        // the shared cache without delaying subscription acknowledgement.
        if accepted.resources_list_changed == Some(true) {
            let _ = self
                .available_resources(request_context.clone(), subject.clone())
                .await?;
        }
        let isolate_tools = snapshot
            .catalog()
            .profile(&self.profile_id)
            .is_some_and(|profile| profile.discovery_failure_mode == DiscoveryFailureMode::Isolate);
        if accepted.tools_list_changed == Some(true) && isolate_tools {
            let _ = self
                .available_tools(request_context.clone(), subject.clone())
                .await?;
        }
        let mut routes = BTreeMap::<SubscriptionRouteKey, UpstreamSubscriptionRoute>::new();

        if accepted.tools_list_changed == Some(true)
            || accepted.prompts_list_changed == Some(true)
            || accepted.resources_list_changed == Some(true)
        {
            for manifest in snapshot.catalog().profile_servers(&self.profile_id) {
                let mut filter =
                    super::discovery_watch::catalog_change_filter(&manifest.capabilities);
                if accepted.tools_list_changed != Some(true) {
                    filter.tools_list_changed = None;
                }
                if accepted.resources_list_changed != Some(true) {
                    filter.resources_list_changed = None;
                }
                if accepted.prompts_list_changed != Some(true) {
                    filter.prompts_list_changed = None;
                }
                if !subscription_filter_is_empty(&filter) {
                    routes
                        .entry((
                            manifest.slug.clone(),
                            manifest.deployment,
                            composition.clone(),
                        ))
                        .or_default()
                        .filter = filter;
                }
            }
        }

        for uri in accepted.resource_subscriptions.iter().flatten() {
            let projection = self.project_resource_for_upstream(uri)?;
            self.authorize_projected_resource(
                request_context,
                HubAction::SubscriptionsListen,
                &projection,
            )
            .await?;
            let deployment = snapshot
                .catalog()
                .server(&projection.server)
                .ok_or_else(|| mcp_internal("resource deployment is unavailable"))?
                .deployment;
            routes
                .entry((projection.server, deployment, composition.clone()))
                .or_default()
                .filter
                .resource_subscriptions
                .get_or_insert_default()
                .push(projection.upstream_uri.to_string());
        }

        if accepted
            .task_ids
            .as_ref()
            .is_some_and(|ids| !ids.is_empty())
            && !self.client_allows_task_projection(&subject).await?
        {
            return Err(McpError::missing_required_client_capability(
                rmcp::model::ClientCapabilities::builder()
                    .enable_tasks()
                    .build(),
            ));
        }
        for canonical_task_id in accepted.task_ids.iter().flatten() {
            let route = self
                .authorize_canonical_task_for_subject(
                    &subject,
                    HubAction::SubscriptionsListen,
                    canonical_task_id,
                )
                .await?;
            let selected = route.hub.catalog.current();
            let deployment = selected
                .server(&route.server)
                .ok_or_else(|| mcp_internal("Task deployment is unavailable"))?
                .deployment;
            let revision = super::execution_composition(&route.subject)?.clone();
            let upstream_route = routes
                .entry((route.server, deployment, revision))
                .or_default();
            upstream_route.hub = Some(route.hub);
            upstream_route.subject = Some(route.subject);
            upstream_route
                .filter
                .task_ids
                .get_or_insert_default()
                .push(route.task_id.clone());
            upstream_route
                .task_ids
                .insert(route.task_id, canonical_task_id.clone());
        }

        routes.retain(|_, route| !subscription_filter_is_empty(&route.filter));
        let hub = self.clone();
        let downstream = request_context.peer.clone();
        let subscription_subject = subject.clone();
        let mut pending = futures::stream::iter(routes.into_iter().map(move |(server, route)| {
            let hub = hub.clone();
            let downstream = downstream.clone();
            let subject = subscription_subject.clone();
            async move {
                let required = route.filter.resource_subscriptions.is_some()
                    || route.filter.task_ids.is_some();
                let hub = route.hub.clone().unwrap_or(hub);
                let subject = route.subject.clone().unwrap_or(subject);
                let result = hub
                    .open_upstream_subscription(
                        server.clone(),
                        route,
                        downstream,
                        subject,
                        required,
                    )
                    .await;
                (server, required, result)
            }
        }))
        .buffer_unordered(super::discovery::MAX_CONCURRENT_DISCOVERY);
        let mut pending_done = false;
        let mut notifications = SelectAll::<RoutedNotificationStream>::new();
        let mut task_routes =
            BTreeMap::<SubscriptionRouteKey, (HubMcp, BTreeMap<String, String>)>::new();
        if *catalog_changes.borrow_and_update() != catalog_generation {
            notify_catalog_change(&context, &accepted).await?;
            return Ok(());
        }
        let mut upstream_connections = Vec::new();
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                opened = pending.next(), if !pending_done => {
                    match opened {
                        Some((server, _, Ok(opened))) => {
                            task_routes.insert(server, (opened.hub, opened.task_ids));
                            notifications.push(opened.notifications);
                            upstream_connections.push(opened.connection);
                        }
                        Some((_server, true, Err(error))) => return Err(error),
                        Some((server, false, Err(error))) => {
                            tracing::warn!(server = %server.0, deployment = %server.1, %error, "isolated upstream catalog-listener failure");
                        }
                        None => pending_done = true,
                    }
                }
                update = notifications.next(), if !notifications.is_empty() => {
                    let Some(update) = update else { continue; };
                    let (server, _required, update) = update;
                    match update {
                        Ok(Some(mut notification)) => {
                            let (hub, routes) = task_routes.get(&server)
                                .ok_or_else(|| mcp_internal("subscription route is unavailable"))?;
                            hub.project_subscription_notification(
                                &server.0,
                                Some(routes),
                                &mut notification,
                            ).await?;
                            context.sink().send(notification).await.map_err(|error| {
                                mcp_internal(format!("failed to forward subscription notification: {error}"))
                            })?;
                        }
                        Ok(None) => {
                            // An established listener that loses its source cannot
                            // keep claiming a live catalog. Reconnect re-admits all
                            // filters. Complete this listen normally so the client
                            // can re-listen and read current state; an ended source
                            // is not an upstream application error.
                            return Ok(());
                        }
                        Err(error) => return Err(upstream_error(error)),
                    }
                }
                change = catalog_changes.changed() => {
                    if change.is_err() { return Ok(()); }
                    // Finish normally after a hint, as when an upstream source ends.
                    // Re-listening re-admits current selection/access/placement; Task
                    // filters independently retain their original exact captures.
                    catalog_changes.borrow_and_update();
                    notify_catalog_change(&context, &accepted).await?;
                    return Ok(());
                }
                change = selection_changes.recv() => {
                    use tokio::sync::broadcast::error::RecvError;
                    match change {
                        Ok(principal) if principal != subject.principal.id => {}
                        Err(RecvError::Closed) => return Ok(()),
                        // The principal's selection changed, or this listen missed
                        // changes: finish with a hint, as after a publication.
                        Ok(_) | Err(RecvError::Lagged(_)) => {
                            notify_catalog_change(&context, &accepted).await?;
                            return Ok(());
                        }
                    }
                }
                change = discovery_changes.recv() => {
                    match change {
                        Ok(change) if change.belongs_to(
                            catalog_generation,
                            &subject.actor.id,
                            &authorization_fingerprint,
                        ) => match change.surface {
                            HubDiscoverySurface::Resources
                            | HubDiscoverySurface::ResourceTemplates
                                if accepted.resources_list_changed == Some(true) => {
                                    context.sink().notify_resource_list_changed().await.map_err(|error| {
                                        mcp_internal(format!("failed to publish resource catalog change: {error}"))
                                    })?;
                                }
                            HubDiscoverySurface::Tools
                                if accepted.tools_list_changed == Some(true) => {
                                    context.sink().notify_tool_list_changed().await.map_err(|error| {
                                        mcp_internal(format!("failed to publish tool catalog change: {error}"))
                                    })?;
                                }
                            _ => {}
                        },
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "hub discovery changes lagged");
                            if accepted.resources_list_changed == Some(true) {
                                context.sink().notify_resource_list_changed().await.map_err(|error| {
                                    mcp_internal(format!("failed to publish resource catalog resync: {error}"))
                                })?;
                            }
                            if accepted.tools_list_changed == Some(true) {
                                context.sink().notify_tool_list_changed().await.map_err(|error| {
                                    mcp_internal(format!("failed to publish tool catalog resync: {error}"))
                                })?;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
            }
        }
    }

    async fn open_upstream_subscription(
        &self,
        key: SubscriptionRouteKey,
        route: UpstreamSubscriptionRoute,
        downstream: rmcp::service::Peer<rmcp::service::RoleServer>,
        subject: crate::AuthenticatedSubject,
        required: bool,
    ) -> Result<OpenedUpstreamSubscription, McpError> {
        let server = &key.0;
        let needs_tasks = route.filter.task_ids.is_some();
        let upstream = self
            .upstream_with_tasks(server, downstream, &subject, needs_tasks)
            .await
            .map_err(super::http_response::RequestError::into_protocol)?;
        let observed = upstream
            .peer
            .peer_info()
            .ok_or_else(|| mcp_internal(format!("upstream `{server}` was not discovered")))?;
        let effective = route.filter.supported_by(&observed.capabilities);
        if effective != route.filter {
            return Err(mcp_internal(format!(
                "upstream `{server}` does not support the accepted subscription filter"
            )));
        }
        let subscription = upstream
            .peer
            .listen(effective)
            .await
            .map_err(upstream_error)?;
        if subscription.acknowledged() != &route.filter {
            return Err(mcp_internal(format!(
                "upstream `{server}` narrowed the accepted subscription filter"
            )));
        }
        Ok(OpenedUpstreamSubscription {
            hub: self.clone(),
            task_ids: route.task_ids,
            connection: upstream,
            notifications: Box::pin(routed_notifications(key, required, subscription)),
        })
    }

    async fn project_subscription_notification(
        &self,
        server: &ServerSlug,
        task_routes: Option<&BTreeMap<String, String>>,
        notification: &mut ServerNotification,
    ) -> Result<(), McpError> {
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(server)
            .ok_or_else(|| mcp_internal(format!("unknown subscription server `{server}`")))?;
        match notification {
            // Subscription notifications are not delivered to ClientHandler.
            // Invalidate before waking a client that will immediately list again.
            ServerNotification::ResourceListChangedNotification(_) => {
                self.discovery.invalidate_resource_surfaces(server).await;
            }
            ServerNotification::PromptListChangedNotification(_) => {
                self.discovery.invalidate_prompts(server).await;
            }
            ServerNotification::ToolListChangedNotification(_) => {
                self.discovery.invalidate_tools(server).await;
            }
            ServerNotification::ResourceUpdatedNotification(update) => {
                update.params.uri = self
                    .project_upstream_resource(server, &update.params.uri)?
                    .hub_uri
                    .to_string();
            }
            ServerNotification::TaskStatusNotification(update) => {
                let source_id = update.params.task.task.task_id.clone();
                let canonical = task_routes
                    .and_then(|routes| routes.get(&source_id))
                    .ok_or_else(|| mcp_internal("upstream notified an unrequested task"))?;
                project_detailed_task_resource_uris(manifest, &mut update.params.task)?;
                update.params.task.task.task_id = canonical.clone();
            }
            _ => {}
        }
        Ok(())
    }
}

fn subscription_filter_is_empty(filter: &SubscriptionFilter) -> bool {
    filter.tools_list_changed != Some(true)
        && filter.prompts_list_changed != Some(true)
        && filter.resources_list_changed != Some(true)
        && filter
            .resource_subscriptions
            .as_ref()
            .is_none_or(Vec::is_empty)
        && filter.task_ids.as_ref().is_none_or(Vec::is_empty)
}

fn routed_notifications(
    key: SubscriptionRouteKey,
    required: bool,
    mut subscription: rmcp::service::Subscription,
) -> impl Stream<
    Item = (
        SubscriptionRouteKey,
        bool,
        Result<Option<ServerNotification>, ServiceError>,
    ),
> + Send
+ 'static {
    async_stream::stream! {
        loop {
            match subscription.next().await {
                Ok(Some(notification)) => yield (key.clone(), required, Ok(Some(notification))),
                Ok(None) => {
                    let reason = match subscription.end() {
                        Some(SubscriptionEnd::Graceful(_)) => "graceful",
                        Some(SubscriptionEnd::Abrupt) => "abrupt",
                        Some(SubscriptionEnd::Cancelled) => "cancelled",
                        Some(SubscriptionEnd::Lagged { .. }) => "lagged",
                        _ => "unknown",
                    };
                    tracing::info!(server = %key.0, deployment = %key.1, reason, "upstream subscription ended; completing downstream listen for re-subscription");
                    yield (key.clone(), required, Ok(None));
                    break;
                }
                Err(error) => {
                    yield (key.clone(), required, Err(error));
                    break;
                }
            }
        }
    }
}

// Hints contain no authority; the caller re-lists/re-listens under current admission.
async fn notify_catalog_change(
    context: &SubscriptionContext,
    accepted: &SubscriptionFilter,
) -> Result<(), McpError> {
    if accepted.tools_list_changed == Some(true) {
        context
            .sink()
            .notify_tool_list_changed()
            .await
            .map_err(|error| {
                mcp_internal(format!("failed to publish tool catalog change: {error}"))
            })?;
    }
    if accepted.prompts_list_changed == Some(true) {
        context
            .sink()
            .notify_prompt_list_changed()
            .await
            .map_err(|error| {
                mcp_internal(format!("failed to publish prompt catalog change: {error}"))
            })?;
    }
    if accepted.resources_list_changed == Some(true) {
        context
            .sink()
            .notify_resource_list_changed()
            .await
            .map_err(|error| {
                mcp_internal(format!(
                    "failed to publish resource catalog change: {error}"
                ))
            })?;
    }
    Ok(())
}
