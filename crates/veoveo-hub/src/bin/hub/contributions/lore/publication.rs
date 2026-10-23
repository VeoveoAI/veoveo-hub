//! One control-plane revision that changes the Lore section, published as the admin
//! route publishes a revision: built on the active revision, admitted and prepared,
//! recorded only while that revision is still active, then adopted by this process.
//! This process's publications serialize on its publication lock; a revision another
//! process activated meanwhile is a conflict, after which the change is built again on
//! the revision active then.
use std::{future::Future, sync::Arc};

use anyhow::{Context, Result};
use chrono::Utc;
use veoveo_audit_contract::AuditContext;
use veoveo_hub::{
    HubCatalog, HubControlStore,
    control_plane::{
        HubControlPlaneRevision, HubControlPlaneRevisionId, HubControlPlaneRevisionSource,
    },
};
use veoveo_types::{PrincipalId, TenantId};

use super::Lore;
use crate::runtime::{
    AdminState, SharedCatalog, SharedHttpClient, build_http_client, replace_catalog,
    replace_http_client,
};

/// How many times a change is built on the active revision before a publication that
/// keeps conflicting fails.
const ATTEMPTS: usize = 4;

/// What a publication uses of the hosted process.
pub(crate) struct Publisher<'a> {
    pub control_store: &'a HubControlStore,
    pub catalog: &'a SharedCatalog,
    pub http: &'a SharedHttpClient,
    pub publication: &'a tokio::sync::Mutex<()>,
    pub public_base_url: &'a str,
}

/// Who publishes a revision, as it records them.
pub(crate) struct Author<'a> {
    pub source: HubControlPlaneRevisionSource,
    pub principal: &'a PrincipalId,
    pub tenant: Option<&'a TenantId>,
    pub audit: &'a AuditContext,
}

impl<'a> Publisher<'a> {
    pub fn of(state: &'a AdminState) -> Self {
        Self {
            control_store: &state.control_store,
            catalog: &state.catalog,
            http: &state.http,
            publication: &state.publication,
            public_base_url: &state.public_base_url,
        }
    }

    /// Publish what `change` makes of the active revision's Lore section, under the
    /// revision ID `id` gives the attempt. Each attempt first awaits `ready`, which
    /// answers whether there is still something to publish; `change` then sees the
    /// active revision's catalog and answers whether it changed the section. Both run
    /// again after a conflict, and nothing is published when either answers false.
    /// Answers the published revision, which this process adopted.
    pub async fn publish<Ready, Checked>(
        &self,
        author: &Author<'_>,
        mut id: impl FnMut() -> Result<HubControlPlaneRevisionId>,
        mut ready: Ready,
        mut change: impl FnMut(&HubCatalog, &mut Lore) -> Result<bool>,
    ) -> Result<Option<HubControlPlaneRevisionId>>
    where
        Ready: FnMut() -> Checked,
        Checked: Future<Output = Result<bool>>,
    {
        let _publication = self.publication.lock().await;
        for _ in 0..ATTEMPTS {
            if !ready().await? {
                return Ok(None);
            }
            let previous = self
                .control_store
                .load_active_revision()
                .await?
                .context("active control plane missing")?;
            let current = HubCatalog::from_control_plane(
                previous.control_plane.clone(),
                self.catalog.current().admission(),
            )?;
            let mut section = Lore::of(&current)?;
            if !change(&current, &mut section)? {
                return Ok(None);
            }
            let mut plane = previous.control_plane.clone();
            plane
                .extensions
                .insert("lore".into(), serde_json::to_value(section)?);
            let catalog = HubCatalog::from_control_plane(plane.clone(), current.admission())?;
            self.catalog.validate_replacement(&catalog)?;
            let http = build_http_client(&catalog)?;
            let prepared = catalog
                .prepare_runtime(
                    Some(&self.catalog.current()),
                    self.public_base_url,
                    http.clone(),
                )
                .await?;
            let revision = HubControlPlaneRevision {
                revision_id: id()?,
                sha256: hex::encode(prepared.configuration_sha256()),
                source: author.source,
                applied_at: Utc::now(),
                applied_by: author.principal.clone(),
                tenant: author.tenant.cloned(),
                control_plane: plane,
            };
            match self
                .control_store
                .record_revision_on(Some(&previous), &revision, author.audit)
                .await
            {
                Ok(()) => {
                    replace_http_client(self.http, http);
                    replace_catalog(self.catalog, Arc::new(prepared))?;
                    return Ok(Some(revision.revision_id));
                }
                Err(error) if veoveo_hub::is_revision_conflict(&error) => continue,
                Err(error) => return Err(error),
            }
        }
        anyhow::bail!("the publication conflicted with other publications {ATTEMPTS} times")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::lore::{
        DeploymentId, NativeAction, Owned, RepositoryId, tests::plane,
    };
    use std::time::Duration;
    use veoveo_hub::{HubCatalogHandle, store::HubStore};

    const CREATED: &str = "22222222222222222222222222222222";

    fn audit() -> AuditContext {
        AuditContext {
            actor: veoveo_audit_contract::AuditActor {
                principal: PrincipalId::parse("native-alice").unwrap(),
                kind: veoveo_audit_contract::AuditPrincipalKind::User,
                tenant: Some(TenantId::parse("tenant-a").unwrap()),
                work_context: None,
            },
            via: None,
            request: veoveo_audit_contract::AuditRequest::background(),
        }
    }

    /// Another process publishes a revision while this one builds its change, so the
    /// first attempt conflicts; the change is built again on that revision and both
    /// changes stay.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_conflict_builds_the_change_again_on_the_revision_active_then() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = veoveo_fixtures::store::TestDb::new().await;
            HubStore::new(db.a.clone()).migrate().await.unwrap();
            veoveo_audit::AuditStore::new(db.a.clone(), veoveo_hub::audit_registry())
                .declare()
                .await
                .unwrap();
            let control_store = HubControlStore::from_platform_store(
                db.a.clone(),
                crate::catalog_admission().unwrap(),
            );
            let initial =
                HubCatalog::from_control_plane(plane(), control_store.admission()).unwrap();
            crate::startup::import(&control_store, &initial)
                .await
                .unwrap();
            let catalog = HubCatalogHandle::new(Arc::new(initial));
            let http = Arc::new(parking_lot::RwLock::new(
                build_http_client(&catalog.current()).unwrap(),
            ));
            let publication = tokio::sync::Mutex::new(());
            let publisher = Publisher {
                control_store: &control_store,
                catalog: &catalog,
                http: &http,
                publication: &publication,
                public_base_url: "https://hub.example",
            };
            let alice = PrincipalId::parse("native-alice").unwrap();
            let tenant = TenantId::parse("tenant-a").unwrap();
            let audit = audit();
            let author = Author {
                source: HubControlPlaneRevisionSource::Contribution,
                principal: &alice,
                tenant: Some(&tenant),
                audit: &audit,
            };
            let deployment = DeploymentId::parse("source").unwrap();
            let repository = RepositoryId::parse(CREATED).unwrap();
            let mut attempts = 0;
            let published = publisher
                .publish(
                    &author,
                    veoveo_hub::new_hub_control_plane_revision_id,
                    || async { Ok(true) },
                    |_, section| {
                        attempts += 1;
                        if attempts == 1 {
                            tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    let active =
                                        control_store.load_active_revision().await?.unwrap();
                                    let mut competing = active.clone();
                                    competing.revision_id =
                                        veoveo_hub::new_hub_control_plane_revision_id()?;
                                    competing.control_plane.metadata =
                                        serde_json::json!({"published": "elsewhere"});
                                    competing.sha256 = hex::encode(
                                        HubCatalog::from_control_plane(
                                            competing.control_plane.clone(),
                                            control_store.admission(),
                                        )?
                                        .configuration_sha256(),
                                    );
                                    control_store
                                        .record_revision_on(Some(&active), &competing, &audit)
                                        .await
                                })
                            })?;
                        }
                        Ok(section.register_owned(&Owned {
                            deployment: &deployment,
                            repository: &repository,
                            tenant: &tenant,
                            principal: &alice,
                            title: "Created",
                        })?)
                    },
                )
                .await
                .unwrap();
            assert_eq!(attempts, 2, "the conflicting attempt is built again");
            let active = control_store.load_active_revision().await.unwrap().unwrap();
            assert_eq!(Some(active.revision_id.clone()), published);
            assert_eq!(
                active.control_plane.metadata,
                serde_json::json!({"published": "elsewhere"}),
                "the other process's revision stays"
            );
            let section = Lore::of(&catalog.current()).unwrap();
            assert!(
                section
                    .assignments
                    .iter()
                    .any(|a| a.repository == repository
                        && a.principal == alice
                        && a.actions.contains(&NativeAction::Owner)),
                "this process adopted its revision"
            );
        })
        .await
        .expect("the publication test exceeded its bound");
    }
}
