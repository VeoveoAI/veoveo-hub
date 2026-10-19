//! Startup: the platform database, the kernel and Hub migrations, the audit class
//! declarations and the installation configuration. Registered servers apply their own
//! migrations.
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::Utc;
use veoveo_audit::AuditStore;
use veoveo_audit_contract::{AuditActor, AuditContext, AuditPrincipalKind, AuditRequest};
use veoveo_hub::control_plane::{HubControlPlaneRevision, HubControlPlaneRevisionSource};
use veoveo_hub::store::HubStore;
use veoveo_hub::{HubCatalog, HubControlStore, new_hub_control_plane_revision_id};
use veoveo_store::{PlatformStore, StoreConfig};
use veoveo_types::PrincipalId;

/// The principal startup records as the publisher of the configuration file.
const STARTUP_PRINCIPAL: &str = "veoveo-hub";

/// Connect to the platform database, waiting up to `wait` for it to accept the login.
pub(super) async fn connect(config: StoreConfig, wait: Duration) -> anyhow::Result<PlatformStore> {
    let deadline = Instant::now() + wait;
    loop {
        let error = match tokio::time::timeout(
            Duration::from_secs(30),
            PlatformStore::connect(config.clone()),
        )
        .await
        {
            Ok(Ok(store)) => return Ok(store),
            Ok(Err(error)) => anyhow::Error::from(error),
            Err(_) => anyhow::anyhow!("timed out"),
        };
        if Instant::now() >= deadline {
            return Err(
                error.context("connect to the platform database before the startup deadline")
            );
        }
        tracing::warn!("cannot connect to the platform database yet: {error:#}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Apply pending kernel and Hub migrations, then record the audit classes the
/// Hub records, before its first audit record. Concurrent replicas wait on the
/// migrators' locks, and repeating a declaration changes nothing.
pub(super) async fn migrate(store: &PlatformStore) -> anyhow::Result<()> {
    HubStore::new(store.clone())
        .migrate()
        .await
        .context("apply the kernel and Hub migrations")?;
    AuditStore::new(store.clone(), veoveo_hub::audit_registry())
        .declare()
        .await
        .context("declare the Hub's audit classes")?;
    tracing::info!("kernel and Hub schemas are current");
    Ok(())
}

/// The database is authoritative after its first publication. The file seeds only
/// an empty installation; a restart never rolls live assignments back to that file.
pub(super) async fn load_catalog(
    control_store: &HubControlStore,
    seed: &std::path::Path,
) -> anyhow::Result<HubCatalog> {
    if let Some(active) = control_store.load_active_revision().await? {
        return HubCatalog::from_control_plane(active.control_plane, control_store.admission());
    }
    let catalog = HubCatalog::load_json(seed, control_store.admission())?;
    match publish(control_store, &catalog, None).await {
        Ok(()) => Ok(catalog),
        Err(error) if veoveo_hub::is_revision_conflict(&error) => {
            let active = control_store
                .load_active_revision()
                .await?
                .context("concurrent bootstrap published no revision")?;
            HubCatalog::from_control_plane(active.control_plane, control_store.admission())
        }
        Err(error) => Err(error),
    }
}

/// Explicit trusted operator import, using the normal revision/audit transaction.
/// Run while the hosted process is stopped; live administration uses its HTTP API.
pub(super) async fn import(
    control_store: &HubControlStore,
    catalog: &HubCatalog,
) -> anyhow::Result<()> {
    let previous = control_store.load_active_revision().await?;
    publish(control_store, catalog, previous.as_ref()).await
}
async fn publish(
    control_store: &HubControlStore,
    catalog: &HubCatalog,
    previous: Option<&HubControlPlaneRevision>,
) -> anyhow::Result<()> {
    let principal = PrincipalId::parse(STARTUP_PRINCIPAL)?;
    let revision = HubControlPlaneRevision {
        revision_id: new_hub_control_plane_revision_id()?,
        sha256: hex::encode(catalog.configuration_sha256()),
        source: HubControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now(),
        applied_by: principal.clone(),
        tenant: None,
        control_plane: catalog.control_plane().clone(),
    };
    let context = AuditContext {
        actor: AuditActor {
            principal,
            kind: AuditPrincipalKind::Service,
            tenant: None,
            work_context: None,
        },
        via: None,
        request: AuditRequest::background(),
    };
    control_store
        .record_revision_on(previous, &revision, &context)
        .await?;
    tracing::info!(revision_id = %revision.revision_id, "published installation configuration");
    Ok(())
}
