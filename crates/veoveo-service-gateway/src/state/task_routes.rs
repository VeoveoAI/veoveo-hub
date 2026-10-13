//! Public identifiers for the upstream Tasks the Gateway projects. The design is in
//! `task_routes/DESIGN.md`.
use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use veoveo_platform_store::{
    PrincipalId, PrincipalKind, TenantId, WorkContextId, deterministic_principal_id,
    deterministic_tenant_id, deterministic_work_context_id,
};
use veoveo_sdk_rust::{CanonicalTaskId, GatewayProfileId, Principal, ServerSlug};
use veoveo_types::{
    DataLabelId, DelegationId, InvocationAuthority, InvocationProvenance, Sha256Digest,
    TokenIssuer, TokenSubject,
};

use super::GatewayState;

const DEFAULT_TASK_ROUTE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const MAX_SOURCE_TASK_ID_BYTES: usize = 512;

#[cfg(test)]
#[path = "task_routes/tests.rs"]
mod tests;

/// A projection to record: who created the upstream Task, in which tenant, Work
/// Context and profile, on which server, under which authority.
#[derive(Clone, Debug)]
pub(crate) struct GatewayTaskRouteDraft {
    pub tenant: veoveo_types::TenantId,
    pub work_context: veoveo_types::WorkContextId,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub source_task_id: String,
    pub authority_digest: Sha256Digest,
    pub ownership: GatewayTaskOwnership,
    pub ttl_ms: Option<u64>,
}

/// The actor and provenance that created a Task. Later access must present the same
/// actor and provenance, and current data labels must contain the recorded ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GatewayTaskOwnership {
    principal: veoveo_types::PrincipalId,
    kind: PrincipalKind,
    issuer: TokenIssuer,
    subject: TokenSubject,
    provenance: InvocationProvenance,
    data_labels: BTreeSet<DataLabelId>,
}

impl GatewayTaskOwnership {
    pub(crate) fn from_invocation(actor: &Principal, authority: &InvocationAuthority) -> Self {
        Self {
            principal: actor.id.clone(),
            kind: match actor.kind {
                veoveo_sdk_rust::PrincipalKind::User => PrincipalKind::User,
                veoveo_sdk_rust::PrincipalKind::Service => PrincipalKind::Service,
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
pub(crate) struct GatewayTaskRoute {
    pub id: CanonicalTaskId,
    pub tenant: TenantId,
    pub owner: PrincipalId,
    pub work_context: WorkContextId,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub source_task_id: String,
    pub authority_digest: Sha256Digest,
    pub ownership: GatewayTaskOwnership,
    pub expires_at: DateTime<Utc>,
}

struct RouteRow {
    id: String,
    tenant: TenantId,
    owner: PrincipalId,
    work_context: WorkContextId,
    profile: String,
    server: String,
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

impl TryFrom<RouteRow> for GatewayTaskRoute {
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
            profile: GatewayProfileId::parse(row.profile)?,
            server: ServerSlug::parse(row.server)?,
            source_task_id: row.source_task_id,
            authority_digest: Sha256Digest::parse(row.authority_digest)?,
            ownership: GatewayTaskOwnership {
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
    fn of(draft: &GatewayTaskRouteDraft) -> Result<Self> {
        let tenant = draft.tenant.as_str();
        Ok(Self {
            tenant: deterministic_tenant_id(tenant)?,
            owner: deterministic_principal_id(tenant, draft.ownership.principal.as_str())?,
            work_context: deterministic_work_context_id(tenant, draft.work_context.as_str())?,
        })
    }
}

impl GatewayState {
    /// Delete the routes that expired at or before `now`, one short transaction per
    /// batch. Replicas that run it at once take different routes.
    pub async fn prune_expired_task_routes(
        &self,
        now: DateTime<Utc>,
        batch: veoveo_platform_store::MaintenanceBatch,
    ) -> Result<veoveo_platform_store::Pruned> {
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
        draft: GatewayTaskRouteDraft,
    ) -> Result<(CanonicalTaskId, GatewayTaskRoute)> {
        ensure!(
            !draft.source_task_id.is_empty()
                && draft.source_task_id.len() <= MAX_SOURCE_TASK_ID_BYTES,
            "source task id must contain 1 to {MAX_SOURCE_TASK_ID_BYTES} bytes"
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
    ) -> Result<Option<GatewayTaskRoute>> {
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
        .map(GatewayTaskRoute::try_from)
        .transpose()
    }

    async fn task_route_for_source(
        &self,
        draft: &GatewayTaskRouteDraft,
        identity: &RouteIdentity,
    ) -> Result<Option<GatewayTaskRoute>> {
        sqlx::query_file_as!(
            RouteRow,
            "src/state/task_routes/for_source.sql",
            draft.server.as_str(),
            draft.source_task_id,
            identity.owner as PrincipalId,
            draft.profile.as_str(),
            identity.tenant as TenantId,
        )
        .fetch_optional(self.store.platform().pool())
        .await
        .context("failed to read the task route of the upstream Task")?
        .map(GatewayTaskRoute::try_from)
        .transpose()
    }
}

fn reusable(
    route: GatewayTaskRoute,
    draft: &GatewayTaskRouteDraft,
    identity: &RouteIdentity,
    now: DateTime<Utc>,
) -> Result<(CanonicalTaskId, GatewayTaskRoute)> {
    ensure!(
        route.tenant == identity.tenant
            && route.owner == identity.owner
            && route.work_context == identity.work_context
            && route.profile == draft.profile
            && route.server == draft.server
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
