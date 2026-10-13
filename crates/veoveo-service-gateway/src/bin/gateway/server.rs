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
use veoveo_sdk_rust::{
    GatewayInternalSigningKey, GatewayInternalTokenIssuer, GatewayProfileId, PublicDeployment,
    ServerSlug, TokenIssuer, public_allowed_hosts,
};
use veoveo_service_gateway::{
    GatewayCatalog, GatewayCatalogHandle, GatewayControlStore, GatewayMcp,
    GatewayRefreshDeliveryWindow, GatewayUpstreamHttpClientPool, RefreshTokenDeliveryCipher,
    registration::{self, RegistrationIdentity, RegistrationService, ServerDatabase},
};

use super::{
    admin::{
        cancel_task, prune_jwt_revocations, read_control_plane, read_server_health, revoke_jwt,
        update_control_plane,
    },
    auth::{
        authenticate_mcp, authorization_server_jwks, authorization_server_metadata,
        protected_resource_metadata,
    },
    host::validate_host,
    oauth::{authorization_callback, authorize_endpoint, revoke_refresh_token, token_endpoint},
    runtime::{
        AdminState, AppState, DynamicMcpState, ProfileAuthState, ProfileMcpService, Readiness,
        build_http_client, current_catalog, spawn_authorization_retention_gc_loop,
        spawn_refresh_delivery_gc_loop,
    },
};

pub(super) struct ServeConfig {
    pub(super) listen_address: SocketAddr,
    pub(super) issuer: String,
    pub(super) catalog: GatewayCatalog,
    pub(super) control_store: GatewayControlStore,
    pub(super) server_database: ServerDatabase,
    pub(super) registration_grace: std::time::Duration,
    pub(super) forwarded_token_key_b64: SecretString,
    pub(super) forwarded_token_verification_key_b64: Option<SecretString>,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: GatewayRefreshDeliveryWindow,
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
    let gateway_state = crate::bindings::gateway_state(control_store.platform_store().clone())?;
    let audit_store = gateway_state.audit_store().clone();
    let instance = Arc::new(
        veoveo_platform_store::Instance::start(
            gateway_state.platform_store(),
            &ServerSlug::parse("gateway")?,
        )
        .await
        .context("start the Gateway's kernel instance")?,
    );
    let catalog = GatewayCatalogHandle::new(initial_catalog.clone());
    let signing_key = GatewayInternalSigningKey::new(
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
            Ok::<_, anyhow::Error>(GatewayInternalSigningKey::new(der)?.verification_key())
        })
        .transpose()?;
    let deployment = PublicDeployment::new(&issuer)?;
    let issuer = TokenIssuer::parse(deployment.base_url())?;
    let internal_token_issuer =
        GatewayInternalTokenIssuer::new(issuer.clone(), signing_key.clone())?;
    let ct = CancellationToken::new();
    let _stop = ct.clone().drop_guard();
    let allowed_hosts = Arc::new(public_allowed_hosts(&deployment, allow_loopback_hosts));
    let http = Arc::new(RwLock::new(build_http_client(&initial_catalog)?));
    let upstream_http = GatewayUpstreamHttpClientPool::new();
    let registrations = RegistrationService::new(
        control_store.platform_store().clone(),
        catalog.clone(),
        gateway_state.clone(),
        RegistrationIdentity {
            issuer,
            signing_key,
            verification_key,
        },
        server_database,
        registration_grace,
    )?;
    registrations.refresh().await?;
    let delegation = veoveo_service_gateway::delegation::DelegationService::new(
        registrations.clone(),
        catalog.clone(),
        gateway_state.clone(),
        internal_token_issuer.clone(),
    )?;
    let registration_watcher = registrations.spawn_watcher(ct.child_token());
    let registration_housekeeping = registrations.spawn_housekeeping(ct.child_token());
    let state = AppState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
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
        .route("/oauth/jwks.json", get(authorization_server_jwks))
        .with_state(state)
        .merge(registration::router(registrations.clone()));

    let auth_state = ProfileAuthState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        deployment: deployment.clone(),
        auth_http: http.clone(),
        delegation,
    };
    let mcp_state = DynamicMcpState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
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
        catalog: catalog.clone(),
        http: http.clone(),
        control_store,
        gateway_state: gateway_state.clone(),
        registrations,
        console_stream: super::admin::ConsoleStreamLimits::new(),
        instance: instance.clone(),
    };
    let admin_router = Router::new()
        .route(
            "/admin/{profile}/control-plane",
            get(read_control_plane).put(update_control_plane),
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
        .route("/admin/{profile}/server-health", get(read_server_health));
    router = router.merge(
        admin_router
            .with_state(admin_state)
            .layer(middleware::from_fn_with_state(auth_state, authenticate_mcp)),
    );
    let router = router
        .layer(middleware::from_fn(
            veoveo_service_gateway::request_observation::observe_request,
        ))
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .merge(probes)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let listener = tokio::net::TcpListener::bind(listen_address)
        .await
        .with_context(|| format!("listen on {listen_address}"))?;
    tracing::info!(
        service = "veoveo-service-gateway",
        address = %listen_address,
        issuer = %deployment.base_url(),
        profile_count = initial_catalog.profile_count(),
        "listening"
    );
    let audit = gateway_state.audit_writer().await.clone();
    let audit_service = veoveo_audit::AuditService::start(
        audit_store,
        audit_signing_key,
        audit_retention_days,
        audit_exports,
    )?;
    gateway_state.set_audit_health(audit_service.health())?;
    let authorization_gc =
        spawn_authorization_retention_gc_loop(gateway_state.clone(), ct.child_token());
    let refresh_gc = spawn_refresh_delivery_gc_loop(gateway_state.clone(), ct.child_token());
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
            .context("gateway HTTP shutdown deadline exceeded")
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
        registration_watcher.await?;
        registration_housekeeping.await?;
        Ok::<(), tokio::task::JoinError>(())
    })
    .await;
    // Deleting the instance's row tells the other replicas at once that this one is gone.
    if let Err(error) = instance.shutdown().await {
        tracing::warn!("the Gateway's kernel instance did not shut down cleanly: {error}");
    }
    result?;
    drained?;
    sealed?;
    cleanup??;
    Ok(())
}

async fn dynamic_mcp_profile(
    State(state): State<DynamicMcpState>,
    request: Request,
) -> axum::response::Response {
    let Some(profile_id) = veoveo_service_gateway::http::profile_from_request(&request) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    if catalog.profile(&profile_id).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    drop(catalog);

    let service = {
        let mut services = state.services.write();
        services
            .entry(profile_id.clone())
            .or_insert_with(|| build_profile_mcp_service(&state, profile_id))
            .clone()
    };
    service.oneshot(request).await.into_response()
}

fn build_profile_mcp_service(
    state: &DynamicMcpState,
    profile_id: GatewayProfileId,
) -> ProfileMcpService {
    // Every stateless request gets its own handler clone while the profile's
    // discovery cache and change broadcaster remain process-wide.
    let gateway_mcp = GatewayMcp::new(
        state.catalog.clone(),
        profile_id.clone(),
        state.gateway_state.clone(),
        state.internal_token_issuer.clone(),
        state.upstream_http.clone(),
    );
    let mcp_service = StreamableHttpService::new(
        move || Ok(gateway_mcp.clone()),
        veoveo_sdk_rust::stateless_session_manager(),
        veoveo_sdk_rust::canonical_streamable_http_server_config()
            .with_allowed_hosts(state.allowed_hosts.iter().cloned())
            .with_cancellation_token(state.cancellation_token.child_token()),
    );
    Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_sdk_rust::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn(
            veoveo_service_gateway::mcp::http_response::preserve_upstream_http_rejection,
        ))
}

async fn readyz(State(state): State<AppState>) -> axum::response::Response {
    let catalog = current_catalog(&state.catalog);
    if !state.gateway_state.audit_ready() {
        return veoveo_service_gateway::problem::Problem::new(
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
