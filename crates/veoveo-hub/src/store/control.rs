//! The installation configuration as published revisions, the pointer to the active
//! one, and the identity rows of the tenants it defines.
use crate::control_plane::{
    HubControlPlane, HubControlPlaneRevision, HubControlPlaneRevisionId,
    HubControlPlaneRevisionSource, TenantDefinition,
};
use sqlx::{PgConnection, types::Json};
use veoveo_types::{ExtensionName, PolicyVersion, PrincipalId, Sha256Digest};

use super::{HubStore, HubStoreError};
use super::{TransactionAppend, admitted};
use veoveo_store::{EnterpriseId, TenantId, deterministic_enterprise_id, deterministic_tenant_id};

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

impl HubStore {
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

    /// Publish a revision with its objects, ensure the identity row of every tenant it
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
        ensure_tenants(&mut transaction, &revision.control_plane.tenants).await?;
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

/// Ensure the identity row of every tenant `tenants` defines, and of the installation's
/// enterprise they belong to. A tenant a later revision omits keeps its row, as the
/// principals and Task routes that name it do.
async fn ensure_tenants(
    connection: &mut PgConnection,
    tenants: &[TenantDefinition],
) -> Result<(), HubStoreError> {
    if tenants.is_empty() {
        return Ok(());
    }
    let enterprise = deterministic_enterprise_id();
    sqlx::query_file!(
        "src/store/control/ensure_enterprise.sql",
        enterprise as EnterpriseId,
    )
    .execute(&mut *connection)
    .await?;
    for tenant in tenants {
        sqlx::query_file!(
            "src/store/control/ensure_tenant.sql",
            deterministic_tenant_id(tenant.id.as_str())? as TenantId,
            enterprise as EnterpriseId,
            tenant.id.as_str(),
        )
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
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
