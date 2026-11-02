//! Live publication fences cache watches opened from immutable admitted views.
use super::*;
use crate::mcp::discovery::AdmittedCatalog;
use crate::{HubCatalog, HubCatalogHandle, HubState};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, ServerHandler, ServiceExt,
    model::{ProtocolVersion, ServerCapabilities, ServerConfig, Tool},
    service::{RoleClient, RunningService, SubscriptionContext},
    transport::streamable_http_server::StreamableHttpService,
};
use tokio::sync::mpsc;
use veoveo_hub_sdk::{ResolvedComponent, ResolvedComposition};

const DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone)]
struct Source {
    opened: mpsc::UnboundedSender<()>,
    ended: mpsc::UnboundedSender<()>,
    infos: mpsc::UnboundedSender<()>,
    changes: tokio::sync::broadcast::Sender<()>,
    list_changed: bool,
}
struct Ended(mpsc::UnboundedSender<()>);
impl Drop for Ended {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        let _ = self.infos.send(());
        let mut capabilities = ServerCapabilities::builder().enable_tools().build();
        if self.list_changed {
            capabilities.tools.as_mut().unwrap().list_changed = Some(true);
        }
        ServerConfig::new(capabilities)
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        // Observe both normal cancellation and disposal of this actual listen future.
        let _ended = Ended(self.ended.clone());
        let mut changes = self.changes.subscribe();
        self.opened.send(()).unwrap();
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                changed = changes.recv() => {
                    changed.expect("fixture change channel closed or lagged");
                    context.sink().notify_tool_list_changed().await
                        .map_err(|_| mcp_internal("fixture notification failed"))?;
                }
            }
        }
    }
}

#[derive(Clone)]
struct Downstream;
impl ServerHandler for Downstream {}

struct Fixture {
    root: HubMcp,
    server: crate::registration::RegisteredServer,
    opened: mpsc::UnboundedReceiver<()>,
    ended: mpsc::UnboundedReceiver<()>,
    infos: mpsc::UnboundedReceiver<()>,
    changes: tokio::sync::broadcast::Sender<()>,
    downstream: RunningService<RoleServer, Downstream>,
    _client: RunningService<RoleClient, ()>,
    serving: tokio::task::JoinHandle<()>,
    _database: crate::test_store::TestDb,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.serving.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let database = crate::test_store::TestDb::new().await;
        let (opened, receiver) = mpsc::unbounded_channel();
        let (ended, ended_receiver) = mpsc::unbounded_channel();
        let (infos, info_receiver) = mpsc::unbounded_channel();
        let (changes, _) = tokio::sync::broadcast::channel(8);
        let source = Source {
            opened,
            ended,
            infos,
            changes: changes.clone(),
            list_changed: true,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let service = StreamableHttpService::new(
            move || Ok(source.clone()),
            veoveo_hub_sdk::stateless_session_manager(),
            veoveo_hub_sdk::canonical_streamable_http_server_config(),
        );
        let router = axum::Router::new().nest_service("/mcp", service);
        let serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut server = crate::test_catalog::registered_server("watch-fixture", &endpoint);
        let mut declaration = (*server.authorization).clone();
        declaration.requirements.tools.insert(
            "sample".parse().unwrap(),
            veoveo_hub_sdk::registration::RouteAccess::Authenticated,
        );
        server.authorization = Arc::new(declaration);
        server.capabilities =
            crate::registration::ServerCapabilities::from_declaration(&server.authorization);
        let plane = serde_json::from_str(include_str!("../../../testdata/hub.json")).unwrap();
        let catalog = HubCatalog::from_control_plane(plane, crate::test_catalog::binding())
            .unwrap()
            .with_registrations(crate::registration::RegistrationSnapshot::new([
                server.clone()
            ]));
        let root = HubMcp::new(
            HubCatalogHandle::new(Arc::new(catalog)),
            "workspace".parse().unwrap(),
            HubState::new(database.a.clone()),
            crate::test_catalog::token_issuer(),
            crate::mcp::upstream_http::HubUpstreamHttpClientPool::new(),
        );
        let (server_io, client_io) = tokio::io::duplex(8192);
        let downstream = tokio::spawn(async move { Downstream.serve(server_io).await.unwrap() });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .unwrap();
        // `server/discover` alone selects no lifecycle: the downstream session starts
        // with the client's first request that carries its own metadata.
        let (downstream, listed) = tokio::join!(downstream, client.list_tools(None));
        let downstream = downstream.unwrap();
        listed.unwrap();
        Self {
            root,
            server,
            opened: receiver,
            ended: ended_receiver,
            infos: info_receiver,
            changes,
            downstream,
            _client: client,
            serving,
            _database: database,
        }
    }

    fn admitted(&self) -> (HubMcp, AuthenticatedSubject, DiscoveryCacheKey) {
        let snapshot = self.root.live_catalog.snapshot();
        let composition = ResolvedComposition::new(BTreeMap::from([(
            self.server.slug.clone(),
            ResolvedComponent::Deployment {
                deployment: self.server.deployment,
            },
        )]));
        let mut subject = crate::mcp::task_ownership_tests::subject();
        subject.execution = Some(composition.context());
        let view = self.root.for_admission(
            &snapshot,
            crate::execution::Admitted {
                composition,
                unavailable: BTreeMap::new(),
            },
            subject.access_token.oauth_client_id.clone(),
        );
        let key = DiscoveryCacheKey {
            catalog_generation: snapshot.generation(),
            principal: subject.actor.id.clone(),
            authorization_fingerprint: crate::mcp::discovery_authorization_fingerprint(&subject)
                .unwrap(),
            server: self.server.slug.clone(),
            link: None,
        };
        (view, subject, key)
    }

    async fn open(
        &mut self,
        view: &HubMcp,
        subject: &AuthenticatedSubject,
        key: &DiscoveryCacheKey,
    ) -> CancellationToken {
        tokio::time::timeout(
            DEADLINE,
            view.ensure_discovery_watch(key, self.downstream.peer().clone(), subject),
        )
        .await
        .unwrap()
        .unwrap();
        tokio::time::timeout(DEADLINE, self.opened.recv())
            .await
            .unwrap()
            .unwrap();
        let cell = view
            .discovery_watches
            .watches
            .lock()
            .await
            .get(key)
            .unwrap()
            .clone();
        let cancelled = cell.get().unwrap().cancelled.clone();
        // Healthy reuse exercises the actual ensure method, not just a map lookup.
        view.ensure_discovery_watch(key, self.downstream.peer().clone(), subject)
            .await
            .unwrap();
        let reused = view
            .discovery_watches
            .watches
            .lock()
            .await
            .get(key)
            .unwrap()
            .clone();
        assert!(Arc::ptr_eq(&cell, &reused));
        assert!(!cancelled.is_cancelled());
        assert!(view.discovery_watch_active(key).await);
        assert!(matches!(
            self.opened.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        cancelled
    }

    fn publish(&self) {
        self.root.live_catalog.replace_registrations(
            crate::registration::RegistrationSnapshot::new([self.server.clone()]),
        );
    }
}

fn payload(description: &'static str, denied: u32) -> AdmittedCatalog<Tool> {
    AdmittedCatalog {
        items: vec![Tool::new("sample", description, Arc::default())],
        denied,
    }
}

async fn cache(view: &HubMcp, key: &DiscoveryCacheKey, value: &AdmittedCatalog<Tool>) {
    let fetch = view.discovery.start_tools(key.clone()).await.unwrap();
    view.discovery
        .finish_tools(fetch.with_denied(value.denied), value.items.clone())
        .await;
    assert_eq!(view.discovery.tools(key).await, Some(value.clone()));
}

#[tokio::test]
async fn live_publication_fences_a_selected_discovery_watch() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut fixture = Fixture::new().await;
        let (view, subject, key) = fixture.admitted();
        let cancelled = fixture.open(&view, &subject, &key).await;
        cache(&view, &key, &payload("before publication", 7)).await;
        let late = view
            .discovery
            .start_tools(key.clone())
            .await
            .unwrap()
            .with_denied(11);
        fixture.publish();
        // Collect every observation even when the pre-fix cancellation wait expires.
        let ended = tokio::time::timeout(DEADLINE, cancelled.cancelled())
            .await
            .is_ok();
        let upstream_ended = tokio::time::timeout(DEADLINE, fixture.ended.recv())
            .await
            .ok()
            .flatten()
            .is_some();
        let active = view.discovery_watch_active(&key).await;
        let after_publication = view.discovery.tools(&key).await;
        view.discovery
            .finish_tools(late, payload("late old response", 11).items)
            .await;
        let after_late = view.discovery.tools(&key).await;
        assert_eq!(
            (ended, upstream_ended, active, after_publication, after_late),
            (true, true, false, None, None)
        );
    })
    .await
    .expect("selected watch publication scenario exceeded its bound");
}

#[tokio::test]
async fn stale_admission_cannot_remove_a_newer_discovery_watch() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut fixture = Fixture::new().await;
        let (old, subject, old_key) = fixture.admitted();
        fixture.publish();
        let (current, current_subject, key) = fixture.admitted();
        let cancelled = fixture.open(&current, &current_subject, &key).await;
        let expected = payload("current complete descriptor", 13);
        cache(&current, &key, &expected).await;
        let result = old
            .ensure_discovery_watch(&old_key, fixture.downstream.peer().clone(), &subject)
            .await;
        let refused = result.err().map(|error| error.message.to_string());
        let retained = current
            .discovery_watches
            .watches
            .lock()
            .await
            .contains_key(&key);
        let active = !cancelled.is_cancelled();
        let contents = current.discovery.tools(&key).await;
        assert_eq!(
            (refused, retained, active, contents),
            (
                Some("catalog changed during discovery; retry".to_owned()),
                true,
                true,
                Some(expected)
            ),
        );
    })
    .await
    .expect("stale admission/new watch scenario exceeded its bound");
}

#[tokio::test]
async fn publication_while_ensure_waits_for_watch_lock_is_rechecked() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut fixture = Fixture::new().await;
        let (view, subject, key) = fixture.admitted();
        let cancelled = fixture.open(&view, &subject, &key).await;
        let guard = view.discovery_watches.watches.lock().await;
        let pending =
            view.ensure_discovery_watch(&key, fixture.downstream.peer().clone(), &subject);
        tokio::pin!(pending);
        // The first await in ensure is this held mutex: no sleep or test hook.
        assert!(futures::poll!(pending.as_mut()).is_pending());
        fixture.publish();
        drop(guard);
        let result = pending.await;
        let refused = result.err().map(|error| error.message.to_string());
        let ended = tokio::time::timeout(DEADLINE, cancelled.cancelled())
            .await
            .is_ok();
        let active = view.discovery_watch_active(&key).await;
        assert_eq!(
            (refused, ended, active),
            (
                Some("catalog changed during discovery; retry".to_owned()),
                true,
                false
            )
        );
    })
    .await
    .expect("watch-lock publication scenario exceeded its bound");
}

/// Real catalog contribution: the caller holds a read grant on the runners' source.
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct RunnerGrantFixture {
    tenant: String,
    principal: String,
    repository: String,
}
impl crate::contract::CatalogSection for RunnerGrantFixture {
    fn validate(
        &self,
        facts: &crate::contract::CatalogFacts,
    ) -> Result<(), veoveo_types::ExtensionError> {
        if !facts
            .tenants
            .contains(&veoveo_types::TenantId::parse(&self.tenant).unwrap())
        {
            return Err(veoveo_types::ExtensionError::new("unknown fixture tenant"));
        }
        Ok(())
    }

    fn protected_resources(&self) -> Vec<crate::contract::ProtectedResourceDescriptor> {
        Vec::new()
    }

    fn repository_grants(
        &self,
    ) -> Result<Vec<crate::contract::RepositoryGrantDescriptor>, veoveo_types::ExtensionError> {
        Ok(vec![crate::contract::RepositoryGrantDescriptor {
            tenant: veoveo_types::TenantId::parse(&self.tenant).unwrap(),
            repository: veoveo_hub_sdk::registration::RepositoryId::parse(&self.repository)
                .unwrap(),
            grantee: crate::contract::RepositoryGrantee::Principal(
                veoveo_types::PrincipalId::parse(&self.principal).unwrap(),
            ),
            actions: BTreeSet::new(),
        }])
    }
}

/// Observe the same production path for either two links of one selection, or two
/// selections of one link. Only the initialized endpoint's capability differs.
async fn unchanging_runner_isolation(same_link: bool) {
    let mut fixture = Fixture::new().await;
    let (opened, mut static_opened) = mpsc::unbounded_channel();
    let (ended, _) = mpsc::unbounded_channel();
    let (infos, mut static_infos) = mpsc::unbounded_channel();
    let static_source = Source {
        opened,
        ended,
        infos,
        changes: tokio::sync::broadcast::channel(8).0,
        list_changed: false,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let service = StreamableHttpService::new(
        move || Ok(static_source.clone()),
        veoveo_hub_sdk::stateless_session_manager(),
        veoveo_hub_sdk::canonical_streamable_http_server_config(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let static_serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let repository = veoveo_hub_sdk::registration::RepositoryId::parse("a".repeat(32)).unwrap();
    let mut static_server = fixture.server.clone();
    static_server.deployment = veoveo_hub_sdk::DeploymentId::new();
    static_server.endpoint =
        veoveo_hub_sdk::registration::ServerEndpoint::new(endpoint.parse().unwrap()).unwrap();
    static_server.branch = Some("static".parse().unwrap());
    static_server.runner = Some(crate::registration::RunnerOf {
        link: "first".parse().unwrap(),
        repository: repository.clone(),
    });
    fixture.server.branch = Some(
        if same_link { "dynamic" } else { "static" }
            .parse()
            .unwrap(),
    );
    fixture.server.runner = Some(crate::registration::RunnerOf {
        link: if same_link { "first" } else { "second" }.parse().unwrap(),
        repository: repository.clone(),
    });
    assert_eq!(static_server.authorization, fixture.server.authorization);

    let mut plane: crate::control_plane::HubControlPlane =
        serde_json::from_str(include_str!("../../../testdata/hub.json")).unwrap();
    let tenant = plane.tenants[0].id.clone();
    let policy_revision = plane.profiles[0].policy_version.clone();
    let mut subject = crate::mcp::task_ownership_tests::subject();
    // This fixture admits the configured tenant, not the Task fixture's enterprise.
    subject.actor.tenant = Some(tenant.clone());
    subject.principal.tenant = Some(tenant.clone());
    subject.authority.tenant = tenant;
    subject.authority.policy_revision = policy_revision;
    let grant = RunnerGrantFixture {
        tenant: subject.principal.tenant.as_ref().unwrap().to_string(),
        principal: subject.principal.id.to_string(),
        repository: repository.to_string(),
    };
    let mut builder = crate::kernel_catalog_registry_builder().unwrap();
    builder
        .register_section::<RunnerGrantFixture>("watchfixture".parse().unwrap())
        .unwrap();
    let binding = crate::HubCatalogAdmission::unbound()
        .bind(builder.build().unwrap())
        .unwrap();
    plane.extensions.insert(
        "watchfixture".to_owned(),
        serde_json::to_value(grant).unwrap(),
    );
    let catalog = HubCatalog::from_control_plane(plane, binding)
        .unwrap()
        .with_registrations(crate::registration::RegistrationSnapshot::new([
            static_server.clone(),
            fixture.server.clone(),
        ]));
    // The grant is admitted through the same contribution that production uses.
    for server in [&static_server, &fixture.server] {
        assert!(catalog.reaches(&subject.principal, server));
        assert_eq!(
            catalog
                .repository_grant(&subject.principal, &repository)
                .unwrap()
                .repository,
            repository,
        );
    }
    fixture.root = HubMcp::new(
        HubCatalogHandle::new(Arc::new(catalog)),
        "workspace".parse().unwrap(),
        HubState::new(fixture._database.a.clone()),
        crate::test_catalog::token_issuer(),
        crate::mcp::upstream_http::HubUpstreamHttpClientPool::new(),
    );
    let snapshot = fixture.root.live_catalog.snapshot();
    let admitted = |server: &crate::registration::RegisteredServer| {
        let runners = if same_link {
            BTreeMap::from([(server.link().unwrap().clone(), server.deployment)])
        } else {
            BTreeMap::from([
                (
                    static_server.link().unwrap().clone(),
                    static_server.deployment,
                ),
                (
                    fixture.server.link().unwrap().clone(),
                    fixture.server.deployment,
                ),
            ])
        };
        let composition = ResolvedComposition::new(BTreeMap::from([(
            server.slug.clone(),
            ResolvedComponent::Runners { runners },
        )]));
        let mut subject = subject.clone();
        subject.execution = Some(composition.context());
        let view = fixture.root.for_admission(
            &snapshot,
            crate::execution::Admitted {
                composition,
                unavailable: BTreeMap::new(),
            },
            subject.access_token.oauth_client_id.clone(),
        );
        let selected = view.catalog.current();
        assert_eq!(
            selected
                .keyed_server(&view.profile_id, &server.slug, server.link())
                .unwrap()
                .deployment,
            server.deployment,
        );
        let key = DiscoveryCacheKey {
            catalog_generation: snapshot.generation(),
            principal: subject.actor.id.clone(),
            authorization_fingerprint: crate::mcp::discovery_authorization_fingerprint(&subject)
                .unwrap(),
            server: server.slug.clone(),
            link: server.link().cloned(),
        };
        (view, subject, key)
    };
    let (static_view, static_subject, static_key) = admitted(&static_server);
    let (dynamic_view, dynamic_subject, dynamic_key) = admitted(&fixture.server);
    assert_ne!(static_key, dynamic_key);
    assert_eq!(
        static_key.catalog_generation,
        dynamic_key.catalog_generation
    );
    assert_eq!(static_key.link == dynamic_key.link, same_link);
    assert_eq!(
        static_key.authorization_fingerprint == dynamic_key.authorization_fingerprint,
        !same_link,
    );

    static_view
        .ensure_discovery_watch(
            &static_key,
            fixture.downstream.peer().clone(),
            &static_subject,
        )
        .await
        .unwrap();
    assert!(
        static_infos.try_recv().is_ok(),
        "static runner was actually discovered"
    );
    while static_infos.try_recv().is_ok() {}
    static_view
        .ensure_discovery_watch(
            &static_key,
            fixture.downstream.peer().clone(),
            &static_subject,
        )
        .await
        .unwrap();
    let static_reused = matches!(
        static_infos.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    );
    let static_not_watched = matches!(
        static_opened.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Record the pre-ensure result instead of aborting before the other observations.
    let active_before = dynamic_view.discovery_watch_active(&dynamic_key).await;
    dynamic_view
        .ensure_discovery_watch(
            &dynamic_key,
            fixture.downstream.peer().clone(),
            &dynamic_subject,
        )
        .await
        .unwrap();
    let opened = tokio::time::timeout(DEADLINE, fixture.opened.recv())
        .await
        .ok()
        .flatten()
        .is_some();
    let discovered = fixture.infos.try_recv().is_ok();
    while fixture.infos.try_recv().is_ok() {}
    let first_cell = dynamic_view
        .discovery_watches
        .watches
        .lock()
        .await
        .get(&dynamic_key)
        .cloned();
    dynamic_view
        .ensure_discovery_watch(
            &dynamic_key,
            fixture.downstream.peer().clone(),
            &dynamic_subject,
        )
        .await
        .unwrap();
    let next_cell = dynamic_view
        .discovery_watches
        .watches
        .lock()
        .await
        .get(&dynamic_key)
        .cloned();
    let reused = match (first_cell, next_cell) {
        (Some(first), Some(next)) => Arc::ptr_eq(&first, &next),
        _ => false,
    };
    let no_extra_discovery = matches!(
        fixture.infos.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    );
    let no_extra_listen = matches!(
        fixture.opened.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    );
    let expected = payload("dynamic runner complete descriptor", 17);
    cache(&dynamic_view, &dynamic_key, &expected).await;
    let notified = fixture.changes.send(()).is_ok();
    let evicted = tokio::time::timeout(DEADLINE, async {
        loop {
            if dynamic_view.discovery.tools(&dynamic_key).await.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    let contents = dynamic_view.discovery.tools(&dynamic_key).await;
    static_serving.abort();
    assert_eq!(
        (
            static_reused,
            static_not_watched,
            active_before,
            opened,
            discovered,
            reused,
            no_extra_discovery,
            no_extra_listen,
            notified,
            evicted,
            contents
        ),
        (
            true, true, false, true, true, true, true, true, true, true, None
        ),
    );
}

#[tokio::test]
async fn an_unchanging_link_does_not_suppress_another_runners_watch() {
    tokio::time::timeout(Duration::from_secs(30), unchanging_runner_isolation(false))
        .await
        .expect("different-link watch isolation exceeded its bound");
}

#[tokio::test]
async fn an_unchanging_deployment_does_not_suppress_the_same_link_in_another_selection() {
    tokio::time::timeout(Duration::from_secs(30), unchanging_runner_isolation(true))
        .await
        .expect("selected-deployment watch isolation exceeded its bound");
}
