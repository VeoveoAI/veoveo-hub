use super::http_response::RequestError;
use crate::contract::HubAction;
use crate::control_plane::PolicyTarget;
use rmcp::{
    model::{CompleteRequestParams, CompleteResult, Reference},
    service::{RequestContext, RoleServer},
};
use veoveo_hub_sdk::PromptName;

use crate::mcp_support::{
    mcp_invalid_params, mcp_invalid_request, resource_template_policy_target,
};

use super::HubMcp;

impl HubMcp {
    /// Complete on the server, or the runner, the reference names. A runner completes
    /// its own resource template, named without its link.
    pub(super) async fn handle_complete(
        &self,
        mut request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, RequestError> {
        let subject = self.authenticated(&context)?;
        let routed = match &mut request.r#ref {
            Reference::Resource(reference) => {
                let routed = self
                    .resource_server(&subject, HubAction::CompletionComplete, &reference.uri)
                    .await?;
                reference.uri = crate::mcp_support::unlinked_uri(&routed, &reference.uri)
                    .ok_or_else(|| {
                        mcp_invalid_params(format!(
                            "resource template is not exposed: {}",
                            reference.uri
                        ))
                    })?
                    .into_owned();
                routed
            }
            Reference::Prompt(reference) => self.server_for_prompt(&subject, &reference.name)?,
            _ => return Err(mcp_invalid_params("unsupported completion reference kind").into()),
        };
        Box::pin(
            self.at(&routed)
                .complete_routed(request, context, routed.slug),
        )
        .await
    }

    async fn complete_routed(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
        server: veoveo_hub_sdk::ServerSlug,
    ) -> Result<CompleteResult, RequestError> {
        let catalog = self.catalog.current();
        let (_profile, manifest) = catalog
            .profile_server(&self.profile_id, &server)
            .ok_or_else(|| mcp_invalid_params(format!("server `{server}` is not registered")))?;
        if !manifest.capabilities.completions {
            return Err(mcp_invalid_request("profile does not expose completions").into());
        }
        let target = match &request.r#ref {
            Reference::Resource(reference) => {
                resource_template_policy_target(server.clone(), &reference.uri)?
            }
            Reference::Prompt(reference) => {
                let prompt = PromptName::parse(reference.name.clone()).map_err(|err| {
                    mcp_invalid_params(format!("invalid completion prompt: {err}"))
                })?;
                PolicyTarget::Prompt {
                    server: server.clone(),
                    prompt,
                }
            }
            _ => return Err(mcp_invalid_params("unsupported completion reference kind").into()),
        };
        let subject = self
            .authorize(&context, HubAction::CompletionComplete, target)
            .await?;
        self.idempotent_upstream_request(&server, context.peer.clone(), &subject, |upstream| {
            let request = request.clone();
            async move { upstream.complete(request).await }
        })
        .await
    }
}
