use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};
use veoveo_audit_contract::{AuditActor, AuditContext, AuditPrincipalKind, AuditRequest};
use veoveo_hub::{
    HubCatalog,
    computers::{ComputerOnboarding, ComputerOnboardingContext},
    control_plane::{HubControlPlaneRevision, HubControlPlaneRevisionSource},
    token_profile::{ProfileRequest, TokenContext, TokenIssueError},
};
use veoveo_hub_sdk::{OAuthClientId, Principal, PrincipalKind};
use veoveo_types::WorkContextId;

use super::{
    Association, Mode, Selection, State, Status, WorkspaceStore, client,
    descriptor::WorkspaceDescriptor,
};
use crate::{
    contributions::lore::{Assignment, Deployment, Lore, NativeAction, Repository},
    runtime::{AdminState, build_http_client, replace_catalog, replace_http_client},
};

#[derive(Clone)]
pub(crate) struct WorkspaceService {
    state: AdminState,
    store: WorkspaceStore,
}
struct Authority<'a> {
    browser: Option<&'a ComputerOnboardingContext>,
    principal: Principal,
    client: OAuthClientId,
    work_context: WorkContextId,
    expires_at: DateTime<Utc>,
    audit: AuditContext,
}
#[derive(Debug, thiserror::Error)]
enum WorkspaceDenied {
    #[error("initiating client is not authorized for the selected native resource")]
    Client,
    #[error("existing repository assignment does not authorize workspace publication")]
    Assignment,
}
#[derive(Debug, thiserror::Error)]
#[error("personal workspace already has a different durable selection")]
struct SelectionConflict;
impl WorkspaceService {
    pub fn new(state: AdminState) -> Self {
        Self {
            store: WorkspaceStore::new(state.control_store.platform_store().clone()),
            state,
        }
    }
    fn section(catalog: &HubCatalog) -> Result<Lore> {
        Lore::of(catalog)
    }

    async fn token(
        &self,
        authority: &Authority<'_>,
        deployment: &Deployment,
        repository: Option<&super::RepositoryId>,
    ) -> Result<String> {
        ensure!(
            authority.expires_at > Utc::now(),
            "workspace caller expired"
        );
        if let Some(browser) = authority.browser {
            browser.recheck().await?;
        }
        let catalog = self.state.catalog.current();
        let active = Self::section(&catalog)?
            .deployments
            .into_iter()
            .find(|d| d.id == deployment.id)
            .context("native deployment withdrawn")?;
        ensure!(
            serde_json::to_value(&active)? == serde_json::to_value(deployment)?,
            "native deployment changed during workspace preparation"
        );
        let context = TokenContext::admit(
            &catalog,
            &deployment.audience,
            &authority.client,
            &authority.principal,
            &authority.work_context,
            &deployment.login_scopes,
            None,
        )
        .map_err(|_| WorkspaceDenied::Client)?
        .expires_before(authority.expires_at);
        let scope = repository.map(|id| deployment.scope_template.render(id));
        let request = match scope.as_deref() {
            Some(scope) => ProfileRequest::Exchange {
                resource: None,
                scope: Some(scope),
            },
            None => ProfileRequest::Login,
        };
        // Ephemeral native RPC credential only. No refresh family or CLI store is
        // fabricated from the Computer's platform-resource grant.
        Ok(crate::contributions::bindings()?
            .profiles
            .issue(context, request, None)
            .await?
            .access_token)
    }
    async fn prepare(
        &self,
        authority: &Authority<'_>,
        selection: Option<Selection>,
    ) -> Result<Option<(Association, Deployment)>> {
        ensure!(
            authority.principal.kind == PrincipalKind::User,
            "workspace requires an authenticated user"
        );
        let tenant = authority
            .principal
            .tenant
            .as_ref()
            .context("workspace tenant missing")?;
        let current = Self::section(&self.state.catalog.current())?;
        let explicit = selection.is_some();
        let existing = self.store.get(tenant, &authority.principal.id).await?;
        let association = match existing {
            Some(association) => {
                if let Some(selection) = &selection {
                    ensure!(
                        selection.deployment == association.deployment
                            && selection
                                .repository
                                .as_ref()
                                .is_none_or(|id| id == &association.repository),
                        SelectionConflict
                    );
                }
                association
            }
            None => {
                let selection = selection.or_else(|| {
                    current.personal_workspace.map(|default| Selection {
                        deployment: default.deployment,
                        repository: None,
                    })
                });
                let Some(selection) = selection else {
                    return Ok(None);
                };
                let deployment = current
                    .deployments
                    .iter()
                    .find(|d| d.id == selection.deployment)
                    .context("selected native workspace deployment unavailable")?;
                // Refuse a caller/client mismatch before durable reservation.
                if explicit {
                    self.token(authority, deployment, selection.repository.as_ref())
                        .await?;
                }
                let association = self
                    .store
                    .reserve(tenant, &authority.principal.id, &selection)
                    .await?;
                if explicit {
                    ensure!(
                        association.deployment == selection.deployment
                            && selection
                                .repository
                                .as_ref()
                                .is_none_or(|id| id == &association.repository),
                        SelectionConflict
                    );
                }
                association
            }
        };
        let deployment = Self::section(&self.state.catalog.current())?
            .deployments
            .into_iter()
            .find(|d| d.id == association.deployment)
            .context("associated native deployment unavailable")?;
        let already_published = self.store.published(&association).await?;
        let creating = association.mode == Mode::Create
            && association.state != State::Ready
            && !already_published;
        let token = self
            .token(
                authority,
                &deployment,
                (!creating).then_some(&association.repository),
            )
            .await?;
        let mut native = client::Client::connect(&deployment).await?;
        let repository = if creating {
            native.create(&association, &token).await?
        } else {
            native.get(&association, &token).await?
        };
        let branch = client::verify(
            &association,
            &repository,
            if creating {
                client::RepositoryProof::Creation
            } else {
                client::RepositoryProof::Association
            },
        )?;
        let association = self.store.verify(&association, &branch).await?;
        if association.mode == Mode::Create
            && association.state != State::Ready
            && !already_published
        {
            self.publish(authority, &association, &deployment, &repository)
                .await?;
        }
        self.store.complete(&association).await?;
        // Re-admit against the current contribution. A concurrently revoked grant
        // must not become a prepared default merely because publication once succeeded.
        self.token(authority, &deployment, Some(&association.repository))
            .await?;
        let association = self
            .store
            .get(tenant, &authority.principal.id)
            .await?
            .context("workspace disappeared")?;
        Ok(Some((association, deployment)))
    }
    async fn publish(
        &self,
        authority: &Authority<'_>,
        association: &Association,
        deployment: &Deployment,
        repository: &lore_proto::lore::model::v1::Repository,
    ) -> Result<()> {
        let _publication = self.state.publication.lock().await;
        for _ in 0..4 {
            if let Some(browser) = authority.browser {
                browser.recheck().await?;
            }
            ensure!(
                authority.expires_at > Utc::now(),
                "workspace caller expired before publication"
            );
            if self.store.published(association).await? {
                return Ok(());
            }
            let previous = self
                .state
                .control_store
                .load_active_revision()
                .await?
                .context("active control plane missing")?;
            let current = HubCatalog::from_control_plane(
                previous.control_plane.clone(),
                self.state.catalog.current().admission(),
            )?;
            let mut section = Self::section(&current)?;
            let active = section
                .deployments
                .iter()
                .find(|d| d.id == association.deployment)
                .context("workspace deployment withdrawn")?;
            ensure!(
                serde_json::to_value(active)? == serde_json::to_value(deployment)?,
                "native deployment changed before workspace publication"
            );
            TokenContext::admit(
                &current,
                &active.audience,
                &authority.client,
                &authority.principal,
                &authority.work_context,
                &active.login_scopes,
                None,
            )
            .map_err(|_| WorkspaceDenied::Client)?;
            merge_created(&mut section, association, &repository.name)?;
            let mut plane = previous.control_plane.clone();
            plane
                .extensions
                .insert("lore".into(), serde_json::to_value(section)?);
            let catalog = HubCatalog::from_control_plane(plane.clone(), current.admission())?;
            self.state.catalog.validate_replacement(&catalog)?;
            let http = build_http_client(&catalog)?;
            let prepared = catalog
                .prepare_runtime(
                    Some(&self.state.catalog.current()),
                    &self.state.public_base_url,
                    http.clone(),
                )
                .await?;
            let revision = HubControlPlaneRevision {
                revision_id: association.publication_id.clone(),
                sha256: hex::encode(prepared.configuration_sha256()),
                source: HubControlPlaneRevisionSource::Contribution,
                applied_at: Utc::now(),
                applied_by: authority.principal.id.clone(),
                tenant: authority.principal.tenant.clone(),
                control_plane: plane,
            };
            match self
                .state
                .control_store
                .record_revision_on(Some(&previous), &revision, &authority.audit)
                .await
            {
                Ok(()) => {
                    replace_http_client(&self.state.http, http);
                    replace_catalog(&self.state.catalog, Arc::new(prepared))?;
                    return Ok(());
                }
                Err(error) if veoveo_hub::is_revision_conflict(&error) => continue,
                Err(error) => return Err(error),
            }
        }
        anyhow::bail!("workspace publication conflicted; retry the retained reservation")
    }
}
fn merge_created(section: &mut Lore, association: &Association, title: &str) -> Result<()> {
    if let Some(existing) = section
        .repositories
        .iter()
        .find(|r| r.deployment == association.deployment && r.id == association.repository)
    {
        ensure!(
            existing.tenant == association.tenant,
            WorkspaceDenied::Assignment
        );
        ensure!(
            section
                .assignments
                .iter()
                .any(|a| a.deployment == association.deployment
                    && a.repository == association.repository
                    && a.tenant == association.tenant
                    && a.principal == association.principal
                    && a.actions.contains(&NativeAction::Owner)),
            WorkspaceDenied::Assignment
        );
    } else {
        section.repositories.push(Repository {
            deployment: association.deployment.clone(),
            id: association.repository.clone(),
            tenant: association.tenant.clone(),
            title: title.into(),
        });
        section.assignments.push(Assignment {
            deployment: association.deployment.clone(),
            repository: association.repository.clone(),
            tenant: association.tenant.clone(),
            principal: association.principal.clone(),
            actions: BTreeSet::from([NativeAction::Owner]),
        });
    }
    Ok(())
}
fn failure(error: &anyhow::Error) -> Status {
    if error.downcast_ref::<WorkspaceDenied>().is_some()
        || error
            .downcast_ref::<TokenIssueError>()
            .is_some_and(TokenIssueError::is_denied)
    {
        Status::Denied
    } else if error.downcast_ref::<SelectionConflict>().is_some() {
        Status::Conflict
    } else if let Some(error) = error.downcast_ref::<client::NativeRefusal>() {
        match error.0 {
            tonic::Code::Unauthenticated => Status::AuthenticationRequired,
            tonic::Code::PermissionDenied => Status::Denied,
            _ => Status::Unavailable,
        }
    } else {
        Status::Unavailable
    }
}
impl ComputerOnboarding for WorkspaceService {
    fn unavailable<'a>(
        &'a self,
        context: &'a ComputerOnboardingContext,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let principal = context.session.principal();
            if let Some(tenant) = &principal.tenant
                && let Some(association) = self.store.get(tenant, &principal.id).await?
            {
                self.store
                    .status(
                        &association,
                        &context.session.computer().id,
                        Status::Unavailable,
                    )
                    .await?;
            }
            Ok(())
        })
    }
    fn status<'a>(
        &'a self,
        context: &'a ComputerOnboardingContext,
    ) -> Pin<Box<dyn Future<Output = Result<Status>> + Send + 'a>> {
        Box::pin(async move {
            let principal = context.session.principal();
            let tenant = principal
                .tenant
                .as_ref()
                .context("workspace tenant missing")?;
            Ok(match self.store.get(tenant, &principal.id).await? {
                Some(association)
                    if association.computer.as_ref() == Some(&context.session.computer().id) =>
                {
                    association.last_status
                }
                Some(_) => Status::Preparing,
                None if Self::section(&self.state.catalog.current())?
                    .personal_workspace
                    .is_some() =>
                {
                    Status::Preparing
                }
                None => Status::Disabled,
            })
        })
    }
    fn ensure<'a>(
        &'a self,
        context: ComputerOnboardingContext,
    ) -> Pin<Box<dyn Future<Output = Result<Status>> + Send + 'a>> {
        Box::pin(async move {
            let principal = context.session.principal().clone();
            let authority = Authority {
                browser: Some(&context),
                audit: AuditContext {
                    actor: AuditActor {
                        principal: principal.id.clone(),
                        kind: AuditPrincipalKind::User,
                        tenant: principal.tenant.clone(),
                        work_context: Some(context.work_context.clone()),
                    },
                    via: None,
                    request: AuditRequest::background(),
                },
                principal,
                client: context.client.clone(),
                work_context: context.work_context.clone(),
                expires_at: context.session.expires_at(),
            };
            if let Some(tenant) = &authority.principal.tenant
                && let Some(association) = self.store.get(tenant, &authority.principal.id).await?
            {
                self.store
                    .status(
                        &association,
                        &context.session.computer().id,
                        Status::Preparing,
                    )
                    .await?;
            }
            let result = async {
                let Some((association, deployment)) = self.prepare(&authority, None).await? else {
                    return Ok(Status::Disabled);
                };
                let descriptor = WorkspaceDescriptor::new(&deployment, &association)?;
                let result = context
                    .exec(
                        vec![
                            "/bin/sh".into(),
                            "-ec".into(),
                            super::descriptor::INSTALL_DESCRIPTOR.into(),
                        ],
                        descriptor.assignments(),
                    )
                    .await?;
                // Ready means the personal repository default and its public
                // descriptor are prepared, not that a user logged in or cloned it.
                Ok(if result.exit_code == 0 {
                    Status::Ready
                } else {
                    Status::Unavailable
                })
            }
            .await;
            let status = match result {
                Ok(status) => status,
                Err(error) => {
                    tracing::warn!(%error, "native personal workspace preparation incomplete");
                    failure(&error)
                }
            };
            if let Some(tenant) = &authority.principal.tenant
                && let Some(association) = self.store.get(tenant, &authority.principal.id).await?
            {
                self.store
                    .status(&association, &context.session.computer().id, status)
                    .await?;
            }
            Ok(status)
        })
    }
}

impl WorkspaceService {
    pub fn router(self, auth: veoveo_hub::http::ProfileAuthState) -> axum::Router {
        axum::Router::new()
            .route("/personal-workspace/{profile}", axum::routing::post(select))
            .with_state(self)
            .layer(axum::middleware::from_fn_with_state(
                auth,
                veoveo_hub::http::authenticate_profile,
            ))
    }
}
async fn select(
    axum::extract::State(service): axum::extract::State<WorkspaceService>,
    axum::extract::Extension(subject): axum::extract::Extension<veoveo_hub::AuthenticatedSubject>,
    veoveo_http::RequestJson(selection): veoveo_http::RequestJson<Selection>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if subject.delegation.is_some() || subject.principal.kind != PrincipalKind::User {
        return veoveo_hub::problem::Problem::new(
            axum::http::StatusCode::FORBIDDEN,
            "personal-workspace-forbidden",
            "Personal workspace requires a direct authenticated user",
        )
        .into_response();
    }
    let authority = Authority {
        browser: None,
        principal: subject.principal.clone(),
        client: subject.access_token.oauth_client_id.clone(),
        work_context: subject.authority.work_context.clone(),
        expires_at: subject.access_token.expires_at,
        audit: subject.audit_context(),
    };
    match service.prepare(&authority, Some(selection)).await {
        Ok(Some((association, _))) => {
            let mut response = axum::Json(association).into_response();
            response.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("private, no-store"),
            );
            response
        }
        Ok(None) => veoveo_hub::problem::Problem::new(
            axum::http::StatusCode::NOT_FOUND,
            "personal-workspace-unconfigured",
            "No personal workspace selected",
        )
        .into_response(),
        Err(error) => {
            let status = failure(&error);
            tracing::warn!(%error, "personal workspace selection refused or unavailable");
            let (http, code) = match status {
                Status::Denied => (
                    axum::http::StatusCode::FORBIDDEN,
                    "personal-workspace-denied",
                ),
                Status::Conflict => (
                    axum::http::StatusCode::CONFLICT,
                    "personal-workspace-conflict",
                ),
                Status::AuthenticationRequired => (
                    axum::http::StatusCode::UNAUTHORIZED,
                    "personal-workspace-authentication-required",
                ),
                _ => (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "personal-workspace-unavailable",
                ),
            };
            veoveo_hub::problem::Problem::new(
                http,
                code,
                "Personal workspace preparation did not complete",
            )
            .detail("Check native authentication or retry the retained workspace selection.")
            .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::lore::{DeploymentId, RepositoryId};
    use veoveo_types::{PrincipalId, TenantId};

    #[test]
    fn native_creator_publication_preserves_others_and_never_repairs_revoked_owner() {
        let association = Association {
            tenant: TenantId::parse("tenant-a").unwrap(),
            principal: PrincipalId::parse("alice").unwrap(),
            deployment: DeploymentId::parse("source").unwrap(),
            repository: RepositoryId::parse("11111111111111111111111111111111").unwrap(),
            default_branch: Some(
                super::super::BranchId::parse("22222222222222222222222222222222").unwrap(),
            ),
            name: "workspace-one".into(),
            publication_id: veoveo_hub::new_hub_control_plane_revision_id().unwrap(),
            last_status: Status::Preparing,
            computer: None,
            mode: Mode::Create,
            state: State::Verified,
        };
        let unrelated = Repository {
            deployment: association.deployment.clone(),
            id: RepositoryId::parse("33333333333333333333333333333333").unwrap(),
            tenant: association.tenant.clone(),
            title: "Shared".into(),
        };
        let mut section = Lore {
            repositories: vec![unrelated.clone()],
            ..Default::default()
        };
        merge_created(&mut section, &association, "Workspace").unwrap();
        assert_eq!(
            serde_json::to_value(&section.repositories[0]).unwrap(),
            serde_json::to_value(unrelated).unwrap()
        );
        assert_eq!(section.assignments.len(), 1);
        assert_eq!(section.assignments[0].principal, association.principal);
        assert_eq!(
            section.assignments[0].actions,
            BTreeSet::from([NativeAction::Owner])
        );
        let published = serde_json::to_value(&section).unwrap();
        merge_created(&mut section, &association, "Workspace").unwrap();
        assert_eq!(serde_json::to_value(&section).unwrap(), published);
        section.assignments[0].actions.clear();
        assert!(merge_created(&mut section, &association, "Workspace").is_err());
        assert!(section.assignments[0].actions.is_empty());
    }
}
