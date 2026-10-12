//! Startup: the platform database, kernel migrations and the installation
//! configuration. Server modules migrate when their servers register.
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use veoveo_mcp_contract::{
    GatewayControlPlaneRevision, GatewayControlPlaneRevisionSource,
    audit::{AuditActor, AuditContext, AuditPrincipalKind, AuditRequest},
};
use veoveo_platform_store::{PlatformStore, StoreAuthLevel, StoreConfig};
use veoveo_service_gateway::{
    GatewayCatalog, GatewayControlStore, new_gateway_control_plane_revision_id,
};
use veoveo_types::PrincipalId;

/// The principal startup records as the publisher of the configuration file.
const STARTUP_PRINCIPAL: &str = "veoveo-service-gateway";

/// Connect to the platform database, waiting up to `wait` for it to accept
/// connections. With root credentials the Gateway first creates its namespace and
/// database.
pub(super) async fn connect(
    config: StoreConfig,
    password: &SecretString,
    wait: Duration,
) -> anyhow::Result<PlatformStore> {
    let deadline = Instant::now() + wait;
    if config.auth_level() == StoreAuthLevel::Root {
        retry(deadline, "create the namespace and database", || {
            provision(&config, password)
        })
        .await?;
    }
    retry(deadline, "connect to the platform database", || {
        PlatformStore::connect(config.clone())
    })
    .await
}

async fn retry<T, E, F, Fut>(deadline: Instant, what: &str, mut attempt: F) -> anyhow::Result<T>
where
    E: Into<anyhow::Error>,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    loop {
        let error = match tokio::time::timeout(Duration::from_secs(30), attempt()).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => error.into(),
            Err(_) => anyhow::anyhow!("timed out"),
        };
        if Instant::now() >= deadline {
            return Err(error.context(format!("{what} before the startup deadline")));
        }
        tracing::warn!("cannot {what} yet: {error:#}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn provision(config: &StoreConfig, password: &SecretString) -> anyhow::Result<()> {
    use surrealdb::{
        Surreal,
        engine::remote::ws::{Ws, Wss},
        opt::{Config, auth::Root},
    };
    let address = config
        .endpoint()
        .as_str()
        .strip_prefix(&format!("{}://", config.endpoint().scheme()))
        .context("validated database endpoint")?;
    let options = Config::new()
        .query_timeout(config.query_timeout())
        .transaction_timeout(config.transaction_timeout());
    let db = match config.endpoint().scheme() {
        "ws" => Surreal::new::<Ws>((address, options)).await?,
        "wss" => Surreal::new::<Wss>((address, options)).await?,
        _ => anyhow::bail!("the database endpoint must use ws or wss"),
    };
    db.signin(Root {
        username: config.username().into(),
        password: password.expose_secret().into(),
    })
    .await?;
    db.query(include_str!(
        "../../queries/bin/gateway/startup/define_namespace.surql"
    ))
    .bind(("namespace", config.namespace().to_owned()))
    .await?
    .check()?;
    db.use_ns(config.namespace()).await?;
    db.query(include_str!(
        "../../queries/bin/gateway/startup/define_database.surql"
    ))
    .bind(("database", config.database().to_owned()))
    .await?
    .check()?;
    Ok(())
}

/// Apply pending kernel migrations. The runner also knows every server module a
/// namespace claim recorded, because the database holds their history. A fresh
/// database has no claims table until the first kernel migration creates it.
pub(super) async fn migrate(store: &PlatformStore) -> anyhow::Result<()> {
    use veoveo_service_gateway::registration::{apply_modules, claimed_modules};
    let claimed = match claimed_modules(store).await {
        Ok(claimed) => claimed,
        Err(error) => {
            tracing::info!("no namespace claims yet ({error:#}); applying the kernel schema first");
            apply_modules(store.client(), [], Vec::new())
                .await
                .context("apply the kernel migrations")?;
            claimed_modules(store)
                .await
                .context("read the claimed server modules")?
        }
    };
    apply_modules(store.client(), claimed.iter(), Vec::new())
        .await
        .context("apply the kernel migrations")?;
    tracing::info!(server_modules = claimed.len(), "kernel schema is current");
    Ok(())
}

/// Make the configuration file the active revision when it differs from it.
pub(super) async fn publish(
    control_store: &GatewayControlStore,
    catalog: &GatewayCatalog,
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
    let revision = GatewayControlPlaneRevision {
        revision_id: new_gateway_control_plane_revision_id()?,
        sha256,
        source: GatewayControlPlaneRevisionSource::SeedFile,
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
            oauth_client: None,
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        },
        authority: Default::default(),
        request: AuditRequest::background(),
    };
    control_store.record_revision(&revision, &context).await?;
    tracing::info!(revision_id = %revision.revision_id, "published the installation configuration");
    Ok(())
}
