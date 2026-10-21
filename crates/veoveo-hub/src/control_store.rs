mod audit;
use crate::HubCatalogAdmission;

use crate::control_plane::{HubControlPlane, HubControlPlaneRevision, HubControlPlaneRevisionId};
use anyhow::{Context, Result};
use serde::Serialize;
use veoveo_store::{PlatformStore, StoreConfig};

use crate::store::{ControlObject, ControlPublication, HubStore, HubStoreError};
use veoveo_types::{ExtensionName, TenantId};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HubControlPlaneRevisionHead {
    pub revision_id: HubControlPlaneRevisionId,
    pub sha256: String,
}

pub fn new_hub_control_plane_revision_id() -> Result<HubControlPlaneRevisionId> {
    HubControlPlaneRevisionId::parse(format!("gcp-{}", uuid::Uuid::now_v7()))
        .context("failed to construct UUIDv7 hub control-plane revision id")
}

/// Whether a publication lost to a concurrent one: another revision became active
/// after this one was built.
pub fn is_revision_conflict(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<HubStoreError>(),
        Some(HubStoreError::ControlRevisionChanged | HubStoreError::ControlRevisionExists)
    )
}

#[derive(Debug, Clone)]
pub struct HubControlStore {
    admission: HubCatalogAdmission,
    store: HubStore,
}

impl HubControlStore {
    pub fn from_platform_store(platform: PlatformStore, admission: HubCatalogAdmission) -> Self {
        Self {
            store: HubStore::new(platform),
            admission,
        }
    }

    pub async fn connect(config: StoreConfig, admission: HubCatalogAdmission) -> Result<Self> {
        let platform = PlatformStore::connect(config)
            .await
            .context("failed to connect to the platform database")?;
        Ok(Self {
            store: HubStore::new(platform),
            admission,
        })
    }

    pub fn admission(&self) -> HubCatalogAdmission {
        self.admission.clone()
    }

    pub async fn load_active_revision(&self) -> Result<Option<HubControlPlaneRevision>> {
        self.admission.ensure_bound()?;
        let revision = self
            .store
            .active_control_revision()
            .await
            .context("failed to read the active hub control-plane revision")?;
        if let Some(revision) = &revision {
            self.admission
                .validate(&revision.control_plane)
                .context("stored hub control plane failed validation")?;
        }
        Ok(revision)
    }

    pub async fn load_active_revision_head(&self) -> Result<Option<HubControlPlaneRevisionHead>> {
        Ok(self
            .load_active_revision()
            .await?
            .map(|revision| HubControlPlaneRevisionHead {
                revision_id: revision.revision_id,
                sha256: revision.sha256,
            }))
    }

    /// Publish a revision and make it active, in one transaction with its audit records.
    /// A revision that another publication replaced first fails with
    /// [`HubStoreError::ControlRevisionChanged`].
    pub async fn record_revision(
        &self,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
    ) -> Result<()> {
        let previous = self.load_active_revision().await?;
        self.record_revision_on(previous.as_ref(), revision, context)
            .await
    }

    /// Publish `revision` as the successor of `previous`, the revision its author based
    /// it on. The publication fails with [`HubStoreError::ControlRevisionChanged`] when
    /// `previous` is no longer active.
    pub async fn record_revision_on(
        &self,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
    ) -> Result<()> {
        self.record_revision_on_inner(
            None,
            previous,
            revision,
            context,
            None,
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
    }

    /// Source activation repeats its generation/authority checks inside the
    /// existing control publication transaction, never after committing routing.
    pub async fn record_activation_on(
        &self,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        activation: &crate::activation::ActivationPublication,
    ) -> Result<()> {
        self.record_revision_on_inner(
            None,
            previous,
            revision,
            context,
            Some(crate::activation::PublicationGuard::Select(activation)),
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
    }

    pub async fn record_environment_on(
        &self,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        environment: &crate::activation::EnvironmentPublication,
    ) -> Result<()> {
        self.record_revision_on_inner(
            None,
            previous,
            revision,
            context,
            Some(crate::activation::PublicationGuard::Environment(
                environment,
            )),
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
    }

    pub async fn record_activation_placement_on(
        &self,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        placement: &crate::activation::ActivationPlacement,
    ) -> Result<()> {
        self.record_revision_on_inner(
            None,
            previous,
            revision,
            context,
            Some(crate::activation::PublicationGuard::Place(placement)),
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
    }

    pub async fn record_activation_on_connection(
        &self,
        connection: &mut sqlx::PgConnection,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        activation: &crate::activation::ActivationPublication,
    ) -> Result<()> {
        self.record_revision_on_inner(
            Some(connection),
            previous,
            revision,
            context,
            Some(crate::activation::PublicationGuard::Select(activation)),
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
    }
    pub async fn record_activation_placement_on_connection(
        &self,
        connection: &mut sqlx::PgConnection,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        placement: &crate::activation::ActivationPlacement,
    ) -> Result<()> {
        self.record_revision_on_inner(
            Some(connection),
            previous,
            revision,
            context,
            Some(crate::activation::PublicationGuard::Place(placement)),
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
    }
    /// Native source definitions and their reservations append on the SAME existing
    /// publication connection alongside the ordinary audit records.
    pub async fn record_source_environment_on(
        &self,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        environment: &crate::activation::EnvironmentPublication,
        source: &impl crate::store::TransactionAppend,
    ) -> Result<()> {
        self.record_revision_on_inner(
            None,
            previous,
            revision,
            context,
            Some(crate::activation::PublicationGuard::Environment(
                environment,
            )),
            source,
        )
        .await
    }

    async fn record_revision_on_inner(
        &self,
        connection: Option<&mut sqlx::PgConnection>,
        previous: Option<&HubControlPlaneRevision>,
        revision: &HubControlPlaneRevision,
        context: &veoveo_audit_contract::AuditContext,
        activation: Option<crate::activation::PublicationGuard<'_>>,
        additional: &impl crate::store::TransactionAppend,
    ) -> Result<()> {
        let sections = self
            .admission
            .validate(&revision.control_plane)
            .context("refusing to persist invalid hub control plane")?;
        self.store
            .check_execution_configuration(revision.control_plane.execution.as_ref())
            .await?;
        let audit = crate::audit::AuditAppend::new(audit::changes(
            &crate::audit_registry(),
            previous.as_ref().map_or(&[], |revision| {
                revision.control_plane.work_contexts.as_slice()
            }),
            &revision.control_plane.work_contexts,
            context,
            revision,
        )?);
        let records = PublicationRecords { audit, additional };
        let objects = control_plane_objects(&revision.control_plane, &sections)?;
        let publication = ControlPublication {
            revision,
            replaces: previous.as_ref().map(|previous| &previous.revision_id),
            objects: &objects,
        };
        if let Some(connection) = connection {
            return HubStore::publish_control_revision_on(
                connection,
                publication,
                &records,
                activation,
            )
            .await
            .context("failed to publish owned activation revision");
        }
        match activation {
            Some(crate::activation::PublicationGuard::Place(placement)) => {
                self.store
                    .publish_activation_placement_revision(publication, &records, placement)
                    .await
            }
            Some(crate::activation::PublicationGuard::Select(activation)) => {
                self.store
                    .publish_activation_revision(publication, &records, activation)
                    .await
            }
            Some(crate::activation::PublicationGuard::Environment(environment)) => {
                self.store
                    .publish_environment_revision(publication, &records, environment)
                    .await
            }
            None => {
                self.store
                    .publish_control_revision(publication, &records)
                    .await
            }
        }
        .context("failed to publish hub control-plane revision")
    }

    pub fn platform_store(&self) -> &PlatformStore {
        self.store.platform()
    }
}

fn control_plane_objects(
    control_plane: &HubControlPlane,
    sections: &crate::contract::AdmittedCatalogSections,
) -> Result<Vec<ControlObject>> {
    let mut objects = Vec::new();
    for identity_provider in &control_plane.identity_providers {
        objects.push(object(
            None,
            "identity_provider",
            identity_provider.id.as_str(),
            identity_provider,
        )?);
    }
    for authorization_server in &control_plane.authorization_servers {
        objects.push(object(
            None,
            "authorization_server",
            authorization_server.id.as_str(),
            authorization_server,
        )?);
    }
    for profile in &control_plane.profiles {
        let mut row = object(None, "profile", profile.id.as_str(), profile)?;
        row.profile_policy_version = Some(profile.policy_version.clone());
        objects.push(row);
    }
    for admitted in sections.objects() {
        objects.push(object(
            admitted.tenant.clone(),
            admitted.kind.as_str(),
            &admitted.id,
            &admitted.value,
        )?);
    }
    for tenant in &control_plane.tenants {
        objects.push(object(
            Some(tenant.id.clone()),
            "tenant",
            tenant.id.as_str(),
            tenant,
        )?);
    }
    for context in &control_plane.work_contexts {
        objects.push(object(
            Some(context.tenant.clone()),
            "work_context",
            context.id.as_str(),
            context,
        )?);
    }
    for policy in &control_plane.policies {
        objects.push(object(None, "policy", policy.version.as_str(), policy)?);
        for rule in &policy.rules {
            objects.push(object(
                single_tenant(&rule.tenant_ids),
                "policy_rule",
                format!("{}/{}", policy.version, rule.id),
                rule,
            )?);
        }
    }
    for data_label in &control_plane.data_labels {
        objects.push(object(
            None,
            "data_label",
            data_label.id.as_str(),
            data_label,
        )?);
    }
    for client in &control_plane.oauth_clients {
        objects.push(object(None, "oauth_client", client.id.as_str(), client)?);
    }
    for client in &control_plane.oidc_clients {
        objects.push(object(None, "oidc_client", client.id.as_str(), client)?);
    }
    for secret in &control_plane.secrets {
        objects.push(object(None, "secret", secret.id.as_str(), secret)?);
    }
    Ok(objects)
}

fn object(
    tenant: Option<TenantId>,
    kind: impl Into<String>,
    id: impl Into<String>,
    value: impl Serialize,
) -> Result<ControlObject> {
    let serde_json::Value::Object(document) = serde_json::to_value(value)? else {
        anyhow::bail!("hub control-plane object did not serialize as an object");
    };
    Ok(ControlObject {
        kind: ExtensionName::parse(kind.into())?,
        id: id.into(),
        tenant,
        document,
        profile_policy_version: None,
    })
}

fn single_tenant<'a>(tenants: impl IntoIterator<Item = &'a TenantId>) -> Option<TenantId> {
    let mut tenants = tenants.into_iter();
    let tenant = tenants.next()?;
    tenants.next().is_none().then(|| tenant.clone())
}

struct PublicationRecords<'a, A> {
    audit: crate::audit::AuditAppend,
    additional: &'a A,
}
impl<A: crate::store::TransactionAppend> crate::store::TransactionAppend
    for PublicationRecords<'_, A>
{
    async fn append(
        &self,
        connection: &mut sqlx::PgConnection,
    ) -> Result<(), crate::store::HubStoreError> {
        self.audit.append(connection).await?;
        self.additional.append(connection).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_control_kinds_use_the_shared_extension_name_admission() {
        let row = object(
            None,
            "vendor/fleet-v2",
            "record",
            serde_json::json!({"value":true}),
        )
        .unwrap();
        assert_eq!(row.kind.as_str(), "vendor/fleet-v2");
        for kind in ["", "bad name", "bad?name", "nonascii-é"] {
            assert!(
                object(None, kind, "record", serde_json::json!({"value":true})).is_err(),
                "accepted invalid control kind {kind:?}"
            );
        }
    }

    #[test]
    fn generated_revision_ids_use_uuid_v7() {
        let id = new_hub_control_plane_revision_id().unwrap();
        let uuid = uuid::Uuid::parse_str(id.as_str().strip_prefix("gcp-").unwrap()).unwrap();
        assert_eq!(uuid.get_version_num(), 7);
    }

    #[test]
    fn a_rule_belongs_to_a_tenant_only_when_it_names_exactly_one() {
        let a = TenantId::parse("tenant-a").unwrap();
        let b = TenantId::parse("tenant-b").unwrap();
        assert_eq!(single_tenant([&a]), Some(a.clone()));
        assert_eq!(single_tenant([&a, &b]), None);
        assert_eq!(single_tenant(std::iter::empty()), None);
    }
}
