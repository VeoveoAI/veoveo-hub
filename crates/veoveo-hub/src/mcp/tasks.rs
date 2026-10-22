use super::http_response::RequestError;
use crate::contract::HubAction;
use rmcp::{
    model::{CancelTaskParams, ErrorCode, GetTaskParams, GetTaskResult, UpdateTaskParams},
    service::{RequestContext, RoleServer},
};
use veoveo_hub_sdk::ServerSlug;

use crate::AuthenticatedSubject;
use crate::mcp_support::{mcp_internal, mcp_invalid_params};

/// What cancelling a server's Task by the identifier it issued came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerTaskCancellation {
    /// The deployment holding the Task accepted `tasks/cancel`.
    Cancelled,
    /// No unexpired route of the caller's names the Task.
    Unrouted,
    /// The route's recorded ownership, its captured environment or the deployment's
    /// exposure refuses the caller.
    Unowned,
    /// Current policy refuses the caller.
    Denied,
}

use super::{
    HubMcp,
    tools::{project_detailed_task_resource_uris, rewrite_detailed_task_id},
};

impl HubMcp {
    pub(super) async fn handle_get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, RequestError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject).await? {
            return Err(mcp_invalid_params("unknown method").into());
        }
        let canonical_task_id = request.task_id.clone();
        let route = self
            .authorize_canonical_task(&context, HubAction::TasksGet, &canonical_task_id)
            .await?;
        let upstream = route
            .hub
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut upstream_request = GetTaskParams::new(route.task_id);
        upstream_request.meta = request.meta;
        let mut result = upstream
            .peer
            .get_task(upstream_request)
            .await
            .map_err(RequestError::from)?;
        let catalog = route.hub.catalog.current();
        let manifest = catalog
            .server(&route.server)
            .ok_or_else(|| mcp_internal(format!("unknown task server `{}`", route.server)))?;
        project_detailed_task_resource_uris(manifest, &mut result.task)?;
        rewrite_detailed_task_id(&mut result.task, &canonical_task_id);
        Ok(result)
    }

    pub(super) async fn handle_update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), RequestError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject).await? {
            return Err(mcp_invalid_params("unknown method").into());
        }
        let route = self
            .authorize_canonical_task(&context, HubAction::TasksUpdate, &request.task_id)
            .await?;
        let upstream = route
            .hub
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut upstream_request = UpdateTaskParams::new(route.task_id, request.input_responses);
        upstream_request.meta = request.meta;
        upstream
            .peer
            .update_task(upstream_request)
            .await
            .map_err(RequestError::from)
    }

    /// Cancel the caller's Task of `server` by the identifier the server issued, outside
    /// any MCP session: find the Task's route, check it as `tasks/cancel` does, and
    /// forward `tasks/cancel` to the deployment that holds the Task, which owns the Task
    /// state and checks ownership again.
    pub async fn cancel_server_task(
        &self,
        subject: &AuthenticatedSubject,
        server: &ServerSlug,
        source_task_id: &str,
    ) -> Result<ServerTaskCancellation, rmcp::model::ErrorData> {
        let Some(route) = self
            .state
            .owned_task_route(
                server,
                source_task_id,
                &self.profile_id,
                &subject.authority.tenant,
                &subject.actor.id,
            )
            .await
            .map_err(|error| mcp_internal(format!("failed to read the Task's route: {error}")))?
        else {
            return Ok(ServerTaskCancellation::Unrouted);
        };
        let route = match self
            .authorize_task_route(subject, HubAction::TasksCancel, route, None)
            .await
        {
            Ok(route) => route,
            Err(error) if error.code == ErrorCode::INVALID_PARAMS => {
                return Ok(ServerTaskCancellation::Unowned);
            }
            Err(error) if error.code == ErrorCode::INVALID_REQUEST => {
                return Ok(ServerTaskCancellation::Denied);
            }
            Err(error) => return Err(error),
        };
        let upstream = route
            .hub
            .detached_upstream(&route.server, &route.subject)
            .await
            .map_err(RequestError::into_protocol)?;
        upstream
            .peer
            .cancel_task(CancelTaskParams::new(route.task_id))
            .await
            .map_err(|error| RequestError::from(error).into_protocol())?;
        Ok(ServerTaskCancellation::Cancelled)
    }

    pub(super) async fn handle_cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), RequestError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject).await? {
            return Err(mcp_invalid_params("unknown method").into());
        }
        let route = self
            .authorize_canonical_task(&context, HubAction::TasksCancel, &request.task_id)
            .await?;
        let upstream = route
            .hub
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut upstream_request = CancelTaskParams::new(route.task_id);
        upstream_request.meta = request.meta;
        upstream
            .peer
            .cancel_task(upstream_request)
            .await
            .map_err(RequestError::from)
    }
}
