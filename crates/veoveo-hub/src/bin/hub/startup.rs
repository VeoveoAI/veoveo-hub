//! Startup: the platform database, the kernel and Hub migrations, the audit class
//! declarations and the installation configuration. Registered servers apply their own
//! migrations.
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
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

/// Make the configuration file the active revision when it differs from it.
pub(super) async fn publish(
    control_store: &HubControlStore,
    catalog: &HubCatalog,
) -> anyhow::Result<()> {
    let sha256 = hex::encode(catalog.configuration_sha256());
    let active = control_store.load_active_revision().await?;
    if let Some(active) = active
        && active.sha256 == sha256
    {
        ensure!(
            &active.control_plane == catalog.control_plane(),
            "the active revision has the configuration's digest but a different document"
        );
        tracing::info!(revision_id = %active.revision_id, "installation configuration is current");
        return Ok(());
    }
    let principal = PrincipalId::parse(STARTUP_PRINCIPAL)?;
    let revision = HubControlPlaneRevision {
        revision_id: new_hub_control_plane_revision_id()?,
        sha256,
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
    match control_store.record_revision(&revision, &context).await {
        Ok(()) => {
            tracing::info!(revision_id = %revision.revision_id, "published the installation configuration");
            Ok(())
        }
        // Another replica starting with the same file published it first.
        Err(error) if veoveo_hub::is_revision_conflict(&error) => {
            let active = control_store.load_active_revision().await?;
            ensure!(
                active.is_some_and(|active| active.sha256 == revision.sha256),
                "another revision became active during startup: {error:#}"
            );
            tracing::info!("another replica published the installation configuration");
            Ok(())
        }
        Err(error) => Err(error),
    }
}
