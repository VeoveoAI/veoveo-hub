//! Public identifiers for the upstream Tasks the Hub projects. The design is in
//! `task_routes/DESIGN.md`.
use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use veoveo_hub_sdk::{
    CanonicalTaskId, DeploymentId, EnvironmentId, ExecutionContext, HubProfileId, Principal,
    ServerSlug,
};
use veoveo_store::{
    PrincipalId, PrincipalKind, TenantId, WorkContextId, deterministic_principal_id,
    deterministic_tenant_id, deterministic_work_context_id,
};
use veoveo_types::{
    DataLabelId, DelegationId, InvocationAuthority, InvocationProvenance, Sha256Digest,
    TokenIssuer, TokenSubject,
};

use super::HubState;

const DEFAULT_TASK_ROUTE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAX_SOURCE_TASK_ID_BYTES: usize = 512;

#[cfg(test)]
#[path = "task_routes/tests.rs"]
mod tests;

/// A projection to record: who created the upstream Task, in which tenant, Work
/// Context and profile, on which server, under which authority.
#[derive(Clone, Debug)]
pub(crate) struct HubTaskRouteDraft {
    pub tenant: veoveo_types::TenantId,
    pub work_context: veoveo_types::WorkContextId,
    pub profile: HubProfileId,
    pub server: ServerSlug,
    pub deployment: DeploymentId,
    pub execution: ExecutionContext,
    pub source_task_id: String,
    pub authority_digest: Sha256Digest,
    pub ownership: HubTaskOwnership,
    pub ttl_ms: Option<u64>,
    /// The caller a Task of a server with declared uses acts for while it runs.
    pub delegated_caller: Option<veoveo_hub_sdk::DelegatedCaller>,
}

/// The caller one server's Task acts for, on the profile that created it.
#[derive(Clone, Debug)]
pub struct TaskDelegation {
    pub profile: HubProfileId,
    pub caller: veoveo_hub_sdk::DelegatedCaller,
    pub expires_at: DateTime<Utc>,
}

/// The actor and provenance that created a Task. Later access must present the same
/// actor and provenance, and current data labels must contain the recorded ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HubTaskOwnership {
    principal: veoveo_types::PrincipalId,
    kind: PrincipalKind,
    issuer: TokenIssuer,
    subject: TokenSubject,
    provenance: InvocationProvenance,
    data_labels: BTreeSet<DataLabelId>,
}

impl HubTaskOwnership {
    pub(crate) fn from_invocation(actor: &Principal, authority: &InvocationAuthority) -> Self {
        Self {
            principal: actor.id.clone(),
            kind: match actor.kind {
                veoveo_hub_sdk::PrincipalKind::User => PrincipalKind::User,
                veoveo_hub_sdk::PrincipalKind::Service => PrincipalKind::Service,
            },
            issuer: actor.issuer.clone(),
            subject: actor.subject.clone(),
            provenance: authority.provenance.clone(),
            data_labels: actor.data_labels.clone(),
        }
    }

    pub(crate) fn allows(&self, actor: &Principal, authority: &InvocationAuthority) -> bool {
        let mut current = Self::from_invocation(actor, authority);
        if !self.data_labels.is_subset(&current.data_labels) {
            return false;
        }
        current.data_labels = self.data_labels.clone();
        self == &current
    }
}

/// One stored route.
#[derive(Clone, Debug)]
pub(crate) struct HubTaskRoute {
    pub id: CanonicalTaskId,
    pub tenant: TenantId,
    pub owner: PrincipalId,
    pub work_context: WorkContextId,
    pub profile: HubProfileId,
    pub server: ServerSlug,
    pub deployment: DeploymentId,
    pub execution: ExecutionContext,
    pub source_task_id: String,
    pub authority_digest: Sha256Digest,
    pub ownership: HubTaskOwnership,
    pub expires_at: DateTime<Utc>,
}

struct RouteRow {
    id: String,
    tenant: TenantId,
    owner: PrincipalId,
    work_context: WorkContextId,
    profile: String,
    server: String,
    deployment: uuid::Uuid,
    environment: uuid::Uuid,
    composition: String,
    source_task_id: String,
    authority_digest: String,
    owner_key: String,
    owner_kind: PrincipalKind,
    owner_issuer: String,
    owner_subject: String,
    invocation_mode: String,
    initiator: Option<String>,
    delegation_id: Option<String>,
    data_labels: Vec<String>,
    expires_at: DateTime<Utc>,
}

impl TryFrom<RouteRow> for HubTaskRoute {
    type Error = anyhow::Error;

    fn try_from(row: RouteRow) -> Result<Self> {
        let initiator = row
            .initiator
            .map(veoveo_types::PrincipalId::parse)
            .transpose()?;
        let provenance = match (row.invocation_mode.as_str(), initiator, row.delegation_id) {
            ("direct", Some(initiator), None) => InvocationProvenance::Direct { initiator },
            ("delegated", Some(initiator), Some(delegation_id)) => {
                InvocationProvenance::Delegated {
                    initiator,
                    delegation_id: DelegationId::parse(delegation_id)?,
                }
            }
            ("automated", None, None) => InvocationProvenance::Automated,
            _ => bail!("stored task route provenance is inconsistent"),
        };
        Ok(Self {
            id: CanonicalTaskId::parse(row.id)?,
            tenant: row.tenant,
            owner: row.owner,
            work_context: row.work_context,
            profile: HubProfileId::parse(row.profile)?,
            server: ServerSlug::parse(row.server)?,
            deployment: DeploymentId::parse(row.deployment.to_string())?,
            execution: ExecutionContext {
                environment: EnvironmentId::parse(row.environment.to_string())?,
                composition: serde_json::from_value(serde_json::Value::String(row.composition))?,
            },
            source_task_id: row.source_task_id,
            authority_digest: Sha256Digest::parse(row.authority_digest)?,
            ownership: HubTaskOwnership {
                principal: veoveo_types::PrincipalId::parse(row.owner_key)?,
                kind: row.owner_kind,
                issuer: TokenIssuer::parse(row.owner_issuer)?,
                subject: TokenSubject::parse(row.owner_subject)?,
                provenance,
                data_labels: row
                    .data_labels
                    .into_iter()
                    .map(DataLabelId::parse)
                    .collect::<Result<_, _>>()?,
            },
            expires_at: row.expires_at,
        })
    }
}

/// The identity columns a route's source tuple and authority resolve to.
struct RouteIdentity {
    tenant: TenantId,
    owner: PrincipalId,
    work_context: WorkContextId,
}

impl RouteIdentity {
    fn of(draft: &HubTaskRouteDraft) -> Result<Self> {
        let tenant = draft.tenant.as_str();
        Ok(Self {
            tenant: deterministic_tenant_id(tenant)?,
            owner: deterministic_principal_id(tenant, draft.ownership.principal.as_str())?,
            work_context: deterministic_work_context_id(tenant, draft.work_context.as_str())?,
        })
    }
}

impl HubState {
    /// Delete the routes that expired at or before `now`, one short transaction per
    /// batch. Replicas that run it at once take different routes.
    pub async fn prune_expired_task_routes(
        &self,
        now: DateTime<Utc>,
        batch: veoveo_store::MaintenanceBatch,
    ) -> Result<veoveo_store::Pruned> {
        self.store
            .prune_task_routes(now, batch)
            .await
            .context("failed to prune expired Task routes")
    }

    /// Record the public route of an upstream Task, or return the route an earlier or
    /// concurrent projection of the same Task recorded. A retry never extends the
    /// original expiry or changes its authority; a mismatching or expired route is
    /// refused.
    pub(crate) async fn create_task_route(
        &self,
        draft: HubTaskRouteDraft,
    ) -> Result<(CanonicalTaskId, HubTaskRoute)> {
        ensure!(
            !draft.source_task_id.is_empty()
                && draft.source_task_id.len() <= MAX_SOURCE_TASK_ID_BYTES,
            "source task id must contain 1 to {MAX_SOURCE_TASK_ID_BYTES} bytes"
        );
        let capture = self
            .store
            .composition(&draft.execution.composition)
            .await?
            .context("Task execution composition is not retained")?;
        ensure!(
            capture.context() == draft.execution
                && capture.components().get(&draft.server)
                    == Some(&veoveo_hub_sdk::ResolvedComponent::Deployment {
                        deployment: draft.deployment
                    }),
            "Task deployment is not selected by its captured composition"
        );
        let now = Utc::now();
        let ttl = draft
            .ttl_ms
            .unwrap_or(DEFAULT_TASK_ROUTE_TTL_MS)
            .try_into()
            .ok()
            .and_then(TimeDelta::try_milliseconds)
            .context("task route TTL exceeds the supported range")?;
        let expires_at = now
            .checked_add_signed(ttl)
            .context("task route expiry exceeds the supported range")?;
        let identity = RouteIdentity::of(&draft)?;
        if let Some(route) = self.task_route_for_source(&draft, &identity).await? {
            return reusable(route, &draft, &identity, now);
        }
        let ownership = &draft.ownership;
        self.store
            .platform()
            .ensure_identity(
                draft.tenant.as_str(),
                ownership.principal.as_str(),
                ownership.issuer.as_str(),
                ownership.subject.as_str(),
                ownership.kind,
            )
            .await
            .context("failed to record the task route owner")?;
        let id = new_task_route_id()?;
        let (mode, initiator, delegation_id) = match &ownership.provenance {
            InvocationProvenance::Direct { initiator } => ("direct", Some(initiator), None),
            InvocationProvenance::Delegated {
                initiator,
                delegation_id,
            } => ("delegated", Some(initiator), Some(delegation_id)),
            InvocationProvenance::Automated => ("automated", None, None),
        };
        let data_labels = ownership
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let delegated_caller = draft
            .delegated_caller
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .context("failed to encode the Task's delegated caller")?;
        sqlx::query_file!(
            "src/state/task_routes/create.sql",
            id.as_str(),
            identity.tenant as TenantId,
            identity.owner as PrincipalId,
            identity.work_context as WorkContextId,
            draft.profile.as_str(),
            draft.server.as_str(),
            draft.source_task_id,
            draft.authority_digest.as_str(),
            ownership.kind as PrincipalKind,
            ownership.issuer.as_str(),
            ownership.subject.as_str(),
            mode,
            initiator.map(|initiator| initiator.as_str()),
            delegation_id.map(|delegation| delegation.as_str()),
            &data_labels,
            now,
            expires_at,
            delegated_caller,
            draft.deployment.as_uuid(),
            draft.execution.environment.as_uuid(),
            draft.execution.composition.digest().as_str(),
        )
        .execute(self.store.platform().pool())
        .await
        .context("failed to record the task route")?;
        let route = self
            .task_route_for_source(&draft, &identity)
            .await?
            .ok_or_else(|| anyhow!("the recorded task route is missing"))?;
        reusable(route, &draft, &identity, now)
    }

    /// The unexpired route with this public identifier in the caller's tenant.
    pub(crate) async fn task_route(
        &self,
        canonical: &CanonicalTaskId,
        tenant: &veoveo_types::TenantId,
    ) -> Result<Option<HubTaskRoute>> {
        let tenant = deterministic_tenant_id(tenant.as_str())?;
        sqlx::query_file_as!(
            RouteRow,
            "src/state/task_routes/route.sql",
            canonical.as_str(),
            tenant as TenantId,
            Utc::now(),
        )
        .fetch_optional(self.store.platform().pool())
        .await
        .context("failed to read the task route")?
        .map(HubTaskRoute::try_from)
        .transpose()
    }

    /// The caller `server`'s Task `source_task_id` acts for, while its route lives.
    pub async fn task_delegation(
        &self,
        server: &ServerSlug,
        deployment: DeploymentId,
        source_task_id: &str,
    ) -> Result<Option<TaskDelegation>> {
        let rows = sqlx::query_file!(
            "src/state/task_routes/delegation.sql",
            server.as_str(),
            source_task_id,
            Utc::now(),
            deployment.as_uuid(),
        )
        .fetch_all(self.store.platform().pool())
        .await
        .context("failed to read the Task's delegation")?;
        let mut rows = rows.into_iter();
        let (Some(row), None) = (rows.next(), rows.next()) else {
            // No live route, or an identifier two routes name, delegates nothing.
            return Ok(None);
        };
        Ok(Some(TaskDelegation {
            profile: HubProfileId::parse(row.profile)?,
            caller: serde_json::from_value(row.delegated_caller)
                .context("the stored delegated caller does not decode")?,
            expires_at: row.expires_at,
        }))
    }

    /// Whether `server`'s Task in core's Task runtime is queued, running or waiting for
    /// `caller`. An identifier that is no runtime Task identifier names no live Task. The
    /// Task lives in the database of its deployment: the installation database, or the
    /// database of an instance's data environment, read on a connection of its own.
    pub async fn task_live(
        &self,
        server: &ServerSlug,
        deployment: DeploymentId,
        source_task_id: &str,
        caller: &veoveo_hub_sdk::DelegatedCaller,
    ) -> Result<bool> {
        let Ok(task) = uuid::Uuid::parse_str(source_task_id) else {
            return Ok(false);
        };
        // The runtime keys a Task by its owner's tenant claim, or `installation`.
        let tenant = caller
            .actor
            .tenant
            .as_ref()
            .map_or("installation", |tenant| tenant.as_str());
        let platform = self.store.platform();
        let live = async |pool: &sqlx::PgPool| {
            sqlx::query_file_scalar!(
                "src/state/task_routes/task_live.sql",
                server.as_str(),
                task,
                tenant,
                deployment.as_uuid(),
            )
            .fetch_one(pool)
            .await
            .context("failed to read the Task's status")
        };
        match crate::data_environment::of_deployment(platform, deployment).await? {
            None => live(platform.pool()).await,
            Some(environment) => {
                let store = crate::data_environment::connect(platform, &environment).await?;
                let answer = live(store.pool()).await;
                store.close().await;
                answer
            }
        }
    }

    /// The unexpired route of `actor`'s Task of `server` on `profile` in `tenant`, by
    /// the identifier the server issued. An identifier two routes name has none.
    pub(crate) async fn owned_task_route(
        &self,
        server: &ServerSlug,
        source_task_id: &str,
        profile: &HubProfileId,
        tenant: &veoveo_types::TenantId,
        actor: &veoveo_types::PrincipalId,
    ) -> Result<Option<HubTaskRoute>> {
        let tenant_key = tenant.as_str();
        let rows = sqlx::query_file_as!(
            RouteRow,
            "src/state/task_routes/owned.sql",
            server.as_str(),
            source_task_id,
            deterministic_principal_id(tenant_key, actor.as_str())? as PrincipalId,
            profile.as_str(),
            deterministic_tenant_id(tenant_key)? as TenantId,
            Utc::now(),
        )
        .fetch_all(self.store.platform().pool())
        .await
        .context("failed to read the Task's route")?;
        let mut rows = rows.into_iter();
        let (Some(row), None) = (rows.next(), rows.next()) else {
            return Ok(None);
        };
        HubTaskRoute::try_from(row).map(Some)
    }

    async fn task_route_for_source(
        &self,
        draft: &HubTaskRouteDraft,
        identity: &RouteIdentity,
    ) -> Result<Option<HubTaskRoute>> {
        sqlx::query_file_as!(
            RouteRow,
            "src/state/task_routes/for_source.sql",
            draft.server.as_str(),
            draft.source_task_id,
            identity.owner as PrincipalId,
            draft.profile.as_str(),
            identity.tenant as TenantId,
            draft.deployment.as_uuid(),
        )
        .fetch_optional(self.store.platform().pool())
        .await
        .context("failed to read the task route of the upstream Task")?
        .map(HubTaskRoute::try_from)
        .transpose()
    }
}

fn reusable(
    route: HubTaskRoute,
    draft: &HubTaskRouteDraft,
    identity: &RouteIdentity,
    now: DateTime<Utc>,
) -> Result<(CanonicalTaskId, HubTaskRoute)> {
    ensure!(
        route.tenant == identity.tenant
            && route.owner == identity.owner
            && route.work_context == identity.work_context
            && route.profile == draft.profile
            && route.server == draft.server
            && route.deployment == draft.deployment
            && route.source_task_id == draft.source_task_id
            && route.authority_digest == draft.authority_digest
            && route.ownership == draft.ownership
            && route.expires_at > now,
        "the existing task route does not admit this retry"
    );
    Ok((route.id.clone(), route))
}

fn new_task_route_id() -> Result<CanonicalTaskId> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy).map_err(|error| anyhow!("task route entropy: {error}"))?;
    Ok(CanonicalTaskId::parse(format!(
        "gtr_{}",
        URL_SAFE_NO_PAD.encode(entropy)
    ))?)
}
