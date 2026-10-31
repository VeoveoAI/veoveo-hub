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
}
struct Ended(mpsc::UnboundedSender<()>);
impl Drop for Ended {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
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
        self.opened.send(()).unwrap();
        context.cancelled().await;
        Ok(())
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
        let source = Source { opened, ended };
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
        let downstream = downstream.await.unwrap();
        Self {
            root,
            server,
            opened: receiver,
            ended: ended_receiver,
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
