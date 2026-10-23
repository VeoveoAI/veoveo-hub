//! The installation configuration as published revisions, the pointer to the active
//! one, and the identity rows of the Work Contexts it defines.
use crate::control_plane::{
    HubControlPlane, HubControlPlaneRevision, HubControlPlaneRevisionId,
    HubControlPlaneRevisionSource, WorkContextDefinition,
};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, types::Json};
use veoveo_types::{
    AccessLevel, AccessSubject, ExtensionName, PolicyVersion, PrincipalId, Sha256Digest,
};

use super::{HubStore, HubStoreError};
use super::{TransactionAppend, admitted};
use veoveo_store::{
    EnterpriseId, GrantPermission, GrantSubjectKind, TenantId, WorkContextId,
    WorkContextMembershipLevel, deterministic_enterprise_id, deterministic_tenant_id,
    deterministic_work_context_id,
};

/// One object of a revision: an identity provider, profile, policy rule or any other
/// configuration object, under its kind and identifier.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlObject {
    pub kind: ExtensionName,
    pub id: String,
    /// The tenant the object belongs to, when it belongs to exactly one.
    pub tenant: Option<veoveo_types::TenantId>,
    pub document: serde_json::Map<String, serde_json::Value>,
    /// The policy version a profile evaluates under; only profiles have one.
    pub profile_policy_version: Option<PolicyVersion>,
}

/// A revision to publish and make active.
#[derive(Clone, Copy, Debug)]
pub struct ControlPublication<'a> {
    pub revision: &'a HubControlPlaneRevision,
    /// The active revision the publisher built on; publication fails when another
    /// replaced it first.
    pub replaces: Option<&'a HubControlPlaneRevisionId>,
    pub objects: &'a [ControlObject],
}

/// A change of the active revision that one Hub process claimed: the configuration it
/// replaced, if any, and the one now active.
#[derive(Clone, Debug)]
pub struct ControlActivation {
    pub previous: Option<HubControlPlane>,
    pub revision: HubControlPlaneRevisionId,
    pub active: HubControlPlane,
}

impl HubStore {
    /// Claim the change from the last claimed revision to the active one, once per
    /// installation, or none when no process has a change to claim. A claim waits for
    /// a publication in progress and spans every revision activated since the last
    /// claim.
    pub async fn claim_control_activation(
        &self,
    ) -> Result<Option<ControlActivation>, HubStoreError> {
        let mut transaction = self.pool().begin().await?;
        let pointer = sqlx::query_file!("src/store/control/activation_lock.sql")
            .fetch_one(&mut *transaction)
            .await?;
        let Some(active) = pointer.revision else {
            return Ok(None);
        };
        if pointer.activated_revision.as_ref() == Some(&active) {
            return Ok(None);
        }
        sqlx::query_file!("src/store/control/activation_claim.sql")
            .execute(&mut *transaction)
            .await?;
        let mut plane = async |id: &str| {
            sqlx::query_file_scalar!("src/store/control/revision_plane.sql", id)
                .fetch_one(&mut *transaction)
                .await
                .map(|plane| plane.0)
        };
        let previous = match &pointer.activated_revision {
            Some(previous) => Some(plane(previous).await?),
            None => None,
        };
        let claimed = ControlActivation {
            previous,
            active: plane(&active).await?,
            revision: admitted(
                "control_active.revision",
                HubControlPlaneRevisionId::parse(active),
            )?,
        };
        transaction.commit().await?;
        Ok(Some(claimed))
    }

    /// The active revision, or none before the first publication.
    pub async fn active_control_revision(
        &self,
    ) -> Result<Option<HubControlPlaneRevision>, HubStoreError> {
        let row = sqlx::query_file!("src/store/control/active_revision.sql")
            .fetch_optional(self.pool())
            .await?;
        row.map(|row| {
            Ok(HubControlPlaneRevision {
                revision_id: admitted(
                    "control_revision.id",
                    HubControlPlaneRevisionId::parse(row.id),
                )?,
                sha256: admitted("control_revision.digest", Sha256Digest::parse(row.digest))?
                    .hex()
                    .to_owned(),
                source: source_from_wire(&row.source)?,
                applied_at: row.applied_at,
                applied_by: admitted(
                    "control_revision.applied_by",
                    PrincipalId::parse(row.applied_by),
                )?,
                tenant: row
                    .tenant
                    .map(|tenant| {
                        admitted(
                            "control_revision.tenant",
                            veoveo_types::TenantId::parse(tenant),
                        )
                    })
                    .transpose()?,
                control_plane: row.control_plane.0,
            })
        })
        .transpose()
    }

    /// Publish a revision with its objects, replace the Work Contexts with the ones it
    /// defines, make it active and append `records`, in one transaction.
    pub async fn publish_control_revision(
        &self,
        publication: ControlPublication<'_>,
        records: &impl TransactionAppend,
    ) -> Result<(), HubStoreError> {
        let revision = publication.revision;
        let digest = admitted(
            "control revision digest",
            Sha256Digest::from_hex(&revision.sha256),
        )?;
        let mut transaction = self.pool().begin().await?;
        let active = sqlx::query_file_scalar!("src/store/control/lock_active.sql")
            .fetch_one(&mut *transaction)
            .await?;
        if active.as_deref() != publication.replaces.map(HubControlPlaneRevisionId::as_str) {
            return Err(HubStoreError::ControlRevisionChanged);
        }
        sqlx::query_file!(
            "src/store/control/create_revision.sql",
            revision.revision_id.as_str(),
            digest.as_str(),
            source_wire(revision.source),
            revision.applied_at,
            revision.applied_by.as_str(),
            revision.tenant.as_ref().map(|tenant| tenant.as_str()),
            Json(&revision.control_plane) as _,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| match error.as_database_error() {
            Some(database) if database.is_unique_violation() => {
                HubStoreError::ControlRevisionExists
            }
            _ => error.into(),
        })?;
        insert_objects(&mut transaction, revision, publication.objects).await?;
        replace_work_contexts(
            &mut transaction,
            &revision.control_plane.work_contexts,
            revision.applied_at,
        )
        .await?;
        sqlx::query_file!(
            "src/store/control/activate.sql",
            revision.revision_id.as_str(),
            revision.applied_at,
        )
        .execute(&mut *transaction)
        .await?;
        records.append(&mut transaction).await?;
        transaction.commit().await?;
        Ok(())
    }
}

async fn insert_objects(
    connection: &mut PgConnection,
    revision: &HubControlPlaneRevision,
    objects: &[ControlObject],
) -> Result<(), HubStoreError> {
    let kinds = objects
        .iter()
        .map(|object| object.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    let ids = objects
        .iter()
        .map(|object| object.id.clone())
        .collect::<Vec<_>>();
    let tenants = objects
        .iter()
        .map(|object| object.tenant.as_ref().map(ToString::to_string))
        .collect::<Vec<_>>();
    let documents = objects
        .iter()
        .map(|object| serde_json::Value::Object(object.document.clone()))
        .collect::<Vec<_>>();
    let versions = objects
        .iter()
        .map(|object| {
            object
                .profile_policy_version
                .as_ref()
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();
    sqlx::query_file!(
        "src/store/control/create_objects.sql",
        revision.revision_id.as_str(),
        &kinds,
        &ids,
        &tenants as &[Option<String>],
        &documents,
        &versions as &[Option<String>],
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Make the identity schema's Work Contexts exactly the ones `contexts` defines.
async fn replace_work_contexts(
    connection: &mut PgConnection,
    contexts: &[WorkContextDefinition],
    now: DateTime<Utc>,
) -> Result<(), HubStoreError> {
    let enterprise = deterministic_enterprise_id();
    if !contexts.is_empty() {
        sqlx::query_file!(
            "src/store/control/ensure_enterprise.sql",
            enterprise as EnterpriseId,
        )
        .execute(&mut *connection)
        .await?;
    }
    let mut retained = Vec::with_capacity(contexts.len());
    for context in contexts {
        let tenant = deterministic_tenant_id(context.tenant.as_str())?;
        let id = deterministic_work_context_id(context.tenant.as_str(), context.id.as_str())?;
        retained.push(id);
        sqlx::query_file!(
            "src/store/control/ensure_tenant.sql",
            tenant as TenantId,
            enterprise as EnterpriseId,
            context.tenant.as_str(),
        )
        .execute(&mut *connection)
        .await?;
        let policy = &context.output_policy;
        let (owner_kind, owner_key) = subject(&policy.owner);
        let data_labels = policy
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        sqlx::query_file!(
            "src/store/control/upsert_work_context.sql",
            id as WorkContextId,
            tenant as TenantId,
            context.id.as_str(),
            context.title,
            context.policy_revision.as_str(),
            owner_kind as GrantSubjectKind,
            owner_key,
            policy.classification.as_ref().map(ToString::to_string),
            &data_labels,
            now,
        )
        .execute(&mut *connection)
        .await?;
        sqlx::query_file!("src/store/control/delete_grants.sql", id as WorkContextId)
            .execute(&mut *connection)
            .await?;
        sqlx::query_file!(
            "src/store/control/delete_memberships.sql",
            id as WorkContextId
        )
        .execute(&mut *connection)
        .await?;
        for (ordinal, grant) in (0_i32..).zip(&policy.initial_grants) {
            let (subject_kind, subject_key) = subject(&grant.subject);
            sqlx::query_file!(
                "src/store/control/create_grant.sql",
                id as WorkContextId,
                ordinal,
                subject_kind as GrantSubjectKind,
                subject_key,
                permission(grant.level) as GrantPermission,
            )
            .execute(&mut *connection)
            .await?;
        }
        for (ordinal, rule) in (0_i32..).zip(&context.memberships) {
            let principals = texts(&rule.principals);
            let groups = texts(&rule.groups);
            let roles = texts(&rule.roles);
            let oauth_clients = texts(&rule.oauth_clients);
            sqlx::query_file!(
                "src/store/control/create_membership.sql",
                id as WorkContextId,
                ordinal,
                level(rule.level) as WorkContextMembershipLevel,
                &principals,
                &groups,
                &roles,
                &oauth_clients,
            )
            .execute(&mut *connection)
            .await?;
        }
    }
    sqlx::query_file!(
        "src/store/control/delete_stale_work_contexts.sql",
        &retained as &[WorkContextId],
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn texts<T: ToString>(values: &std::collections::BTreeSet<T>) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn subject(subject: &AccessSubject) -> (GrantSubjectKind, String) {
    match subject {
        AccessSubject::Principal(principal) => (GrantSubjectKind::Principal, principal.to_string()),
        AccessSubject::Group(group) => (GrantSubjectKind::Group, group.to_string()),
    }
}

fn permission(level: AccessLevel) -> GrantPermission {
    match level {
        AccessLevel::Read => GrantPermission::Read,
        AccessLevel::Write => GrantPermission::Write,
        AccessLevel::Admin => GrantPermission::Admin,
    }
}

fn level(level: veoveo_types::WorkContextMembershipLevel) -> WorkContextMembershipLevel {
    match level {
        veoveo_types::WorkContextMembershipLevel::Viewer => WorkContextMembershipLevel::Viewer,
        veoveo_types::WorkContextMembershipLevel::Contributor => {
            WorkContextMembershipLevel::Contributor
        }
        veoveo_types::WorkContextMembershipLevel::Custodian => {
            WorkContextMembershipLevel::Custodian
        }
        veoveo_types::WorkContextMembershipLevel::Owner => WorkContextMembershipLevel::Owner,
    }
}

fn source_wire(source: HubControlPlaneRevisionSource) -> &'static str {
    match source {
        HubControlPlaneRevisionSource::AdminApi => "admin_api",
        HubControlPlaneRevisionSource::SeedFile => "seed_file",
        HubControlPlaneRevisionSource::Contribution => "contribution",
    }
}

fn source_from_wire(source: &str) -> Result<HubControlPlaneRevisionSource, HubStoreError> {
    match source {
        "admin_api" => Ok(HubControlPlaneRevisionSource::AdminApi),
        "seed_file" => Ok(HubControlPlaneRevisionSource::SeedFile),
        "contribution" => Ok(HubControlPlaneRevisionSource::Contribution),
        _ => Err(HubStoreError::InvalidStoredValue {
            column: "control_revision.source",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_sources_round_trip_through_their_column_spelling() {
        for source in [
            HubControlPlaneRevisionSource::AdminApi,
            HubControlPlaneRevisionSource::SeedFile,
            HubControlPlaneRevisionSource::Contribution,
        ] {
            assert_eq!(source_from_wire(source_wire(source)).unwrap(), source);
            assert_eq!(
                serde_json::to_value(source).unwrap(),
                serde_json::Value::String(source_wire(source).to_owned())
            );
        }
        assert!(source_from_wire("unknown").is_err());
    }
}
