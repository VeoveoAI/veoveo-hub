use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{any, get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use parking_lot::RwLock;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_hub::{
    HubCatalog, HubCatalogHandle, HubControlStore, HubMcp, HubRefreshDeliveryWindow,
    RefreshTokenDeliveryCipher,
    registration::{self, RegistrationIdentity, RegistrationService, ServerDatabase},
};
use veoveo_hub_sdk::{
    HubInternalSigningKey, HubInternalTokenIssuer, PublicDeployment, ServerSlug, TokenIssuer,
    public_allowed_hosts,
};

use super::{
    admin::{
        cancel_task, prune_jwt_revocations, read_control_plane, read_server_health, revoke_jwt,
        update_control_plane, validate_control_plane,
    },
    auth::{
        authenticate_mcp, authorization_server_jwks, authorization_server_metadata,
        protected_resource_metadata,
    },
    host::validate_host,
    oauth::{authorization_callback, authorize_endpoint, revoke_refresh_token, token_endpoint},
    runtime::{
        AdminState, AppState, DynamicMcpState, ProfileAuthState, Readiness, build_http_client,
        current_catalog, spawn_authorization_retention_gc_loop, spawn_refresh_delivery_gc_loop,
        spawn_usage_partition_loop,
    },
};

pub(super) struct ServeConfig {
    pub(super) listen_address: SocketAddr,
    pub(super) issuer: String,
    pub(super) catalog: HubCatalog,
    pub(super) control_store: HubControlStore,
    pub(super) server_database: ServerDatabase,
    pub(super) registration_grace: std::time::Duration,
    pub(super) forwarded_token_key_b64: SecretString,
    pub(super) forwarded_token_verification_key_b64: Option<SecretString>,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: HubRefreshDeliveryWindow,
    pub(super) allow_loopback_hosts: bool,
    pub(super) offline_mode: bool,
    pub(super) audit_retention_days: std::num::NonZeroU32,
    pub(super) audit_exports: veoveo_audit::export::AuditExportConfig,
    pub(super) audit_signing_key: Arc<veoveo_audit::integrity::AuditSigningKey>,
}

pub(super) async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    let ServeConfig {
        listen_address,
        issuer,
        catalog: initial_catalog,
        control_store,
        server_database,
        registration_grace,
        forwarded_token_key_b64,
        forwarded_token_verification_key_b64,
        refresh_delivery_cipher,
        refresh_delivery_window,
        allow_loopback_hosts,
        offline_mode,
        audit_retention_days,
        audit_signing_key,
        audit_exports,
    } = config;
    let initial_catalog = Arc::new(initial_catalog);
    let hub_state = crate::bindings::hub_state(control_store.platform_store().clone())?;
    let audit_store = hub_state.audit_store().clone();
    let instance = Arc::new(
        veoveo_store::Instance::start(hub_state.platform_store(), &ServerSlug::parse("hub")?)
            .await
            .context("start the Hub's kernel instance")?,
    );
    let catalog = HubCatalogHandle::new(initial_catalog.clone());
    let signing_key = HubInternalSigningKey::new(
        BASE64_STANDARD
            .decode(forwarded_token_key_b64.expose_secret().trim())
            .context("the forwarded token key must be base64-encoded Ed25519 PKCS#8 DER")?,
    )?;
    // Only the public half of the verification key outlives this statement.
    let verification_key = forwarded_token_verification_key_b64
        .map(|key| {
            let der = BASE64_STANDARD.decode(key.expose_secret().trim()).context(
                "the forwarded token verification key must be base64-encoded Ed25519 PKCS#8 DER",
            )?;
            Ok::<_, anyhow::Error>(HubInternalSigningKey::new(der)?.verification_key())
        })
        .transpose()?;
    let deployment = PublicDeployment::new(&issuer)?;
    let issuer = TokenIssuer::parse(deployment.base_url())?;
    let internal_token_issuer = HubInternalTokenIssuer::new(issuer.clone(), signing_key.clone())?;
    let ct = CancellationToken::new();
    let _stop = ct.clone().drop_guard();
    let allowed_hosts = Arc::new(public_allowed_hosts(&deployment, allow_loopback_hosts));
    let http = Arc::new(RwLock::new(build_http_client(&initial_catalog)?));
    let initial_catalog = Arc::new(
        initial_catalog
            .as_ref()
            .clone()
            .prepare_runtime(
                None,
                deployment.base_url(),
                super::runtime::current_http_client(&http),
            )
            .await?,
    );
    catalog.replace(initial_catalog.clone())?;
    // Instances run in factory sandboxes when the installation configures them.
    let factory = match &initial_catalog.control_plane().factory {
        Some(config) => Some(
            veoveo_hub::factory::Factory::connect(
                config.clone(),
                catalog.clone(),
                control_store.platform_store().clone(),
            )
            .await?,
        ),
        None => None,
    };
    // The Hub follows every watched repository as its watcher and applies its pushes.
    let watches = match &factory {
        Some(factory) => {
            let watches = crate::contributions::lore::factory::watch::Watches::new(
                Arc::new(
                    crate::contributions::lore::factory::native::NativeLore::new(catalog.clone()),
                ),
                factory.clone(),
                control_store.platform_store().clone(),
                ct.child_token(),
            );
            watches.resume().await?;
            Some(watches)
        }
        None => None,
    };
    let upstream_http = veoveo_hub::HubUpstreamHttpClientPool::new();
    let registrations = RegistrationService::new(
        control_store.platform_store().clone(),
        catalog.clone(),
        hub_state.clone(),
        RegistrationIdentity {
            issuer,
            signing_key,
            verification_key,
        },
        server_database,
        registration_grace,
    )?;
    registrations.refresh().await?;
    let delegation = veoveo_hub::delegation::DelegationService::new(
        registrations.clone(),
        catalog.clone(),
        hub_state.clone(),
        internal_token_issuer.clone(),
    )?;
    let registration_watcher = registrations.spawn_watcher(ct.child_token());
    let selection_watcher =
        veoveo_hub::selection::spawn_watcher(hub_state.clone(), ct.child_token());
    let registration_housekeeping = registrations.spawn_housekeeping(ct.child_token());
    let state = AppState {
        token_profiles: crate::contributions::bindings()?.profiles.clone(),
        catalog: catalog.clone(),
        hub_state: hub_state.clone(),
        http: http.clone(),
        public_base_url: deployment.base_url().to_string(),
        refresh_delivery_cipher,
        refresh_delivery_window,
        delegation: delegation.clone(),
    };

    // Probes reach the process by any address, so they skip Host validation.
    let probes = Router::new()
        .route("/livez", get(|| async { StatusCode::NO_CONTENT }))
        .route("/readyz", get(readyz))
        .with_state(state.clone());
    let mut router = Router::new()
        .route(
            "/oauth/device_authorization",
            post(super::oauth::device_authorization),
        )
        .route(
            "/oauth/device",
            get(super::oauth::device_verification).post(super::oauth::device_approval),
        )
        .route("/oauth/authorize", get(authorize_endpoint))
        .route("/oauth/callback", get(authorization_callback))
        .route("/oauth/token", post(token_endpoint))
        .route("/oauth/revoke", post(revoke_refresh_token))
        .route(
            "/.well-known/oauth-protected-resource/mcp/{profile}",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/oauth",
            get(authorization_server_metadata),
        )
        // RFC 8414 §5: the same metadata where OpenID Connect Discovery clients read it,
        // under the issuer.
        .route(
            "/oauth/.well-known/openid-configuration",
            get(authorization_server_metadata),
        )
        .route("/oauth/jwks.json", get(authorization_server_jwks))
        .with_state(state.clone())
        .merge(registration::router(registrations.clone()))
        .merge(crate::contributions::lore::catalog::router(state));

    let auth_state = ProfileAuthState {
        catalog: catalog.clone(),
        hub_state: hub_state.clone(),
        deployment: deployment.clone(),
        auth_http: http.clone(),
        delegation,
    };
    let mcp_state = DynamicMcpState {
        catalog: catalog.clone(),
        hub_state: hub_state.clone(),
        internal_token_issuer: internal_token_issuer.clone(),
        upstream_http: upstream_http.clone(),
        allowed_hosts: allowed_hosts.clone(),
        cancellation_token: ct.child_token(),
        services: Arc::new(RwLock::new(BTreeMap::new())),
    };
    let mcp_router = Router::new()
        .route("/mcp/{profile}", any(dynamic_mcp_profile))
        .route("/mcp/{profile}/{*path}", any(dynamic_mcp_profile))
        .with_state(mcp_state)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(mcp_router);

    router = router.merge(
        Router::new()
            .route(
                "/console-api/{profile}/session",
                get(crate::console::bootstrap),
            )
            .with_state(crate::console::ConsoleState {
                catalog: catalog.clone(),
                offline_mode,
            })
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                authenticate_mcp,
            )),
    );

    let admin_state = AdminState {
        public_base_url: deployment.base_url().to_string(),
        publication: Arc::new(tokio::sync::Mutex::new(())),
        catalog: catalog.clone(),
        http: http.clone(),
        control_store,
        hub_state: hub_state.clone(),
        registrations,
        console_stream: super::admin::ConsoleStreamLimits::new(),
        internal_token_issuer: internal_token_issuer.clone(),
        upstream_http: upstream_http.clone(),
    };
    let control_watcher = super::control_reload::spawn(admin_state.clone(), ct.clone());
    let activation_store = admin_state.control_store.clone();
    let computers = veoveo_hub::computers::ComputerService::new(
        catalog.clone(),
        hub_state.clone(),
        deployment.base_url(),
    )?;
    computers.collect(ct.child_token());
    router = router
        .merge(computers.router(auth_state.clone()))
        .merge(crate::contributions::lore::repositories::router(
            admin_state.clone(),
            auth_state.clone(),
        ))
        .merge(veoveo_hub::selection::router(
            hub_state.clone(),
            auth_state.clone(),
        ));
    if let (Some(factory), Some(watches)) = (&factory, &watches) {
        router = router
            .merge(crate::contributions::lore::instance::router(
                factory.clone(),
                watches.clone(),
                catalog.clone(),
                auth_state.clone(),
            ))
            .merge(watches.clone().router(auth_state.clone()));
    }
    let admin_router = Router::new()
        .route(
            "/admin/{profile}/control-plane",
            get(read_control_plane).put(update_control_plane),
        )
        .route(
            "/admin/{profile}/control-plane/validate",
            post(validate_control_plane),
        )
        .route(
            "/admin/{profile}/console/audit/views",
            post(super::admin::console_audit::open_view),
        )
        .route(
            "/admin/{profile}/console/audit/partitions",
            get(super::admin::console_audit::partitions),
        )
        .route(
            "/admin/{profile}/console/audit/records",
            get(super::admin::console_audit::records),
        )
        .route(
            "/admin/{profile}/console/audit/summary",
            get(super::admin::console_audit::summary),
        )
        .route(
            "/admin/{profile}/console/audit/stream",
            get(super::admin::console_audit::stream_audit),
        )
        .route(
            "/admin/{profile}/console/audit/export",
            get(super::admin::console_audit::export_audit),
        )
        .route("/admin/{profile}/jwt-revocations", post(revoke_jwt))
        .route(
            "/admin/{profile}/jwt-revocations/prune",
            post(prune_jwt_revocations),
        )
        .route(
            "/admin/{profile}/tasks/{server}/{task_id}/cancel",
            post(cancel_task),
        )
        .route("/admin/{profile}/server-health", get(read_server_health))
        .route(
            "/admin/{profile}/usage/daily",
            get(super::admin::daily_usage),
        );
    router = router.merge(
        admin_router
            .with_state(admin_state)
            .layer(middleware::from_fn_with_state(auth_state, authenticate_mcp)),
    );
    let router = router
        .layer(middleware::from_fn(
            veoveo_hub::request_observation::observe_request,
        ))
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .merge(probes)
        .layer(
            // A server error the Hub answers on purpose, such as Computers being
            // unavailable, is logged where its cause is known, not as a failure here.
            TraceLayer::new(tower_http::classify::SharedClassifier::new(
                veoveo_hub::http::UnexpectedServerErrors,
            ))
            .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let listener = match passed_listener()? {
        Some(listener) => listener,
        None => tokio::net::TcpListener::bind(listen_address)
            .await
            .with_context(|| format!("listen on {listen_address}"))?,
    };
    // A revision imported or published while no Hub ran is applied once the Hub listens,
    // since the gateway's renewals call its token endpoint.
    crate::contributions::after_activation(&activation_store, hub_state.hub_store());
    tracing::info!(
        service = "veoveo-hub",
        address = %listener.local_addr()?,
        issuer = %deployment.base_url(),
        profile_count = initial_catalog.profile_count(),
        "listening"
    );
    let audit = hub_state.audit_writer().await.clone();
    let audit_service = veoveo_audit::AuditService::start(
        audit_store,
        audit_signing_key,
        audit_retention_days,
        audit_exports,
    )?;
    hub_state.set_audit_health(audit_service.health())?;
    let authorization_gc =
        spawn_authorization_retention_gc_loop(hub_state.clone(), ct.child_token());
    let refresh_gc = spawn_refresh_delivery_gc_loop(hub_state.clone(), ct.child_token());
    let usage_partitions = spawn_usage_partition_loop(hub_state.clone(), ct.child_token());
    let serving = std::future::IntoFuture::into_future(
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown({
            let audit = audit.clone();
            let ct = ct.clone();
            async move {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("install SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                    _ = audit.closed() => {},
                }
                ct.cancel();
            }
        }),
    );
    tokio::pin!(serving);
    let result: anyhow::Result<()> = tokio::select! {
        result = &mut serving => result.map_err(Into::into),
        _ = ct.cancelled() => tokio::time::timeout(std::time::Duration::from_secs(30), &mut serving)
            .await
            .context("hub HTTP shutdown deadline exceeded")
            .and_then(|result| result.map_err(Into::into)),
    };
    ct.cancel();
    let drained = audit.shutdown(std::time::Duration::from_secs(30)).await;
    let sealed = audit_service
        .shutdown(std::time::Duration::from_secs(30))
        .await;
    let cleanup = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        authorization_gc.await?;
        refresh_gc.await?;
        usage_partitions.await?;
        registration_watcher.await?;
        selection_watcher.await?;
        control_watcher.await?;
        registration_housekeeping.await?;
        Ok::<(), tokio::task::JoinError>(())
    })
    .await;
    // Deleting the instance's row tells the other replicas at once that this one is gone.
    if let Err(error) = instance.shutdown().await {
        tracing::warn!("the Hub's kernel instance did not shut down cleanly: {error}");
    }
    result?;
    drained?;
    sealed?;
    cleanup??;
    Ok(())
}

/// The listening socket the process that started the Hub passed it, by the socket
/// activation protocol of sd_listen_fds(3): `LISTEN_FDS=1` and the socket at file
/// descriptor 3, for this process when `LISTEN_PID` is set and names it.
fn passed_listener() -> anyhow::Result<Option<tokio::net::TcpListener>> {
    use std::os::fd::FromRawFd as _;
    let Ok(count) = std::env::var("LISTEN_FDS") else {
        return Ok(None);
    };
    if std::env::var("LISTEN_PID").is_ok_and(|pid| pid != std::process::id().to_string()) {
        return Ok(None);
    }
    anyhow::ensure!(
        count == "1",
        "LISTEN_FDS passes {count} sockets; the Hub listens on one"
    );
    // SAFETY: by the protocol, file descriptor 3 is the listening socket passed to this
    // process, and nothing else in it owns that descriptor.
    let passed = unsafe { std::net::TcpListener::from_raw_fd(3) };
    // The duplicate closes on exec, so the Hub's own child processes never inherit it.
    let listener = passed
        .try_clone()
        .context("duplicate the passed socket at file descriptor 3")?;
    drop(passed);
    listener.set_nonblocking(true)?;
    Ok(Some(tokio::net::TcpListener::from_std(listener)?))
}

async fn dynamic_mcp_profile(
    State(state): State<DynamicMcpState>,
    mut request: Request,
) -> axum::response::Response {
    let Some(profile_id) = veoveo_hub::http::profile_from_request(&request) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let snapshot = state.catalog.snapshot();
    if snapshot.catalog().profile(&profile_id).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(mut subject) = request
        .extensions()
        .get::<veoveo_hub::AuthenticatedSubject>()
        .cloned()
    else {
        return veoveo_hub::problem::Problem::internal().into_response();
    };
    let admitted = match veoveo_hub::execution::admit_execution(
        &state.hub_state,
        snapshot.catalog(),
        &mut subject,
    )
    .await
    {
        Ok(admitted) => admitted,
        Err(error) => {
            tracing::error!(%error, "execution routing unavailable");
            return veoveo_hub::problem::Problem::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "execution-unavailable",
                "Execution unavailable",
            )
            .into_response();
        }
    };
    let client = subject.access_token.oauth_client_id.clone();
    request.extensions_mut().insert(subject);
    let handler = {
        let mut services = state.services.write();
        services
            .entry(profile_id.clone())
            .or_insert_with(|| {
                HubMcp::new(
                    state.catalog.clone(),
                    profile_id,
                    state.hub_state.clone(),
                    state.internal_token_issuer.clone(),
                    state.upstream_http.clone(),
                )
            })
            .clone()
    }
    .for_admission(&snapshot, admitted, client);
    build_profile_mcp_service(&state, handler)
        .oneshot(request)
        .await
        .into_response()
}

fn build_profile_mcp_service(state: &DynamicMcpState, hub_mcp: HubMcp) -> Router {
    // Every stateless HTTP operation retains its exact admitted composition. The
    // profile handler shares discovery/progress resources with other selections.
    let mcp_service = StreamableHttpService::new(
        move || Ok(hub_mcp.clone()),
        veoveo_hub_sdk::stateless_session_manager(),
        veoveo_hub_sdk::canonical_streamable_http_server_config()
            .with_allowed_hosts(state.allowed_hosts.iter().cloned())
            .with_cancellation_token(state.cancellation_token.child_token()),
    );
    Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_hub_sdk::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn(
            veoveo_hub::mcp::http_response::preserve_upstream_http_rejection,
        ))
}

async fn readyz(State(state): State<AppState>) -> axum::response::Response {
    let catalog = current_catalog(&state.catalog);
    if !state.hub_state.audit_ready() {
        return veoveo_hub::problem::Problem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not-ready",
            "Not ready",
        )
        .detail("the audit service is unavailable")
        .into_response();
    }
    Json(Readiness {
        status: "ready",
        servers: catalog.server_count(),
        profiles: catalog.profile_count(),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_allowed_hosts_use_public_authority_only() {
        let deployment = PublicDeployment::new("https://veoveo.example").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, false),
            vec!["veoveo.example"]
        );
    }

    #[test]
    fn local_allowed_hosts_are_explicit() {
        let deployment = PublicDeployment::new("https://veoveo.example").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, true),
            vec!["veoveo.example", "localhost", "127.0.0.1", "::1"]
        );
    }
}
