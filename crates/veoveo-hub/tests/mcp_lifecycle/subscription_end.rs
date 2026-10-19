//! A source SSE response can end without completing its MCP listen request. The
//! Hub completes its downstream stream, permitting a fresh listen and state read.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use axum::{Router, body::Body};
use futures::StreamExt;
use rmcp::{
    ServerHandler,
    model::{
        ErrorCode, ErrorData, ListResourcesResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, ServerCapabilities, ServerConfig, ServerNotification, SubscriptionFilter,
    },
    service::{RequestContext, RoleServer, ServiceError, SubscriptionContext, SubscriptionEnd},
    transport::streamable_http_server::StreamableHttpService,
};
use secrecy::SecretString;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::registration::{
    ClientMetadata, RegistrationDeclaration, ServerEndpoint, ServerVersion,
    client::RegistrationClient, statement::ServerKeyPair,
};

use super::{
    ClientLifecycleMode, ClientServiceExt, Hub, ProtocolVersion, StreamableHttpClientTransport,
    StreamableHttpClientTransportConfig,
};

const URI: &str = "streamfixture://value";
const REFUSAL: &str = "fixture refused the established listener";

#[derive(Debug, Clone, Copy)]
enum Change {
    Update,
    Refuse,
}

/// This fixture declares one resource; its owner changes it independently of a
/// listener and can end only the HTTP response body, without an MCP final result.
#[derive(Clone)]
struct Source {
    value: Arc<AtomicUsize>,
    changes: broadcast::Sender<Change>,
    opened: mpsc::UnboundedSender<SubscriptionFilter>,
}

impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_resources()
                .enable_resources_subscribe()
                .build(),
        )
    }

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![Resource::new(URI, "value")],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        assert_eq!(request.uri, URI);
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            self.value.load(Ordering::SeqCst).to_string(),
            URI,
        )])
        .into())
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(
            if requested
                .resource_subscriptions
                .as_ref()
                .is_some_and(|uris| uris.iter().any(|uri| uri == URI))
            {
                source_filter()
            } else {
                SubscriptionFilter::new()
            },
        )
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        let mut changes = self.changes.subscribe();
        assert_eq!(context.accepted(), &source_filter());
        self.opened.send(context.accepted().clone()).unwrap();
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                change = changes.recv() => match change.unwrap() {
                    Change::Update => context.sink().notify_resource_updated(URI).await.unwrap(),
                    Change::Refuse => return Err(ErrorData::invalid_params(REFUSAL, None)),
                }
            }
        }
    }
}

fn source_filter() -> SubscriptionFilter {
    SubscriptionFilter::builder()
        .resource_subscription(URI)
        .build()
}

#[tokio::test]
async fn upstream_http_eof_completes_the_hub_listener_and_allows_a_fresh_snapshot_and_push() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("the source closure lifecycle exceeded two minutes");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start(directory.path(), &db, Duration::from_secs(60)).await;
    let http = reqwest::Client::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let key = ServerKeyPair::generate().unwrap();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            slug: "streamfixture".parse().unwrap(),
            uri_scheme: "streamfixture".parse().unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(endpoint.parse().unwrap()).unwrap(),
        },
        serde_json::from_value(serde_json::json!({
            "permissions": [{
                "permission": "streamfixture.values.read",
                "resource": "streamfixture.value", "access": "read"
            }],
            "requirements": {"resources": {"streamfixture://value": "streamfixture.values.read"}}
        }))
        .unwrap(),
    )
    .unwrap();
    let registration = RegistrationClient::new(
        http.clone(),
        RegistrationClient::discover(&http, &hub.base.parse().unwrap())
            .await
            .unwrap(),
        SecretString::from(hub.secret("VEOVEO_REGISTRATION_SECRET").to_owned()),
        key,
        metadata,
    )
    .unwrap();
    let _registered = registration.register().await.unwrap();

    let (opened, mut openings) = mpsc::unbounded_channel();
    let (cuts, mut cut_streams) = mpsc::unbounded_channel();
    let source = Source {
        value: Arc::new(AtomicUsize::new(1)),
        changes: broadcast::channel(4).0,
        opened,
    };
    let served = source.clone();
    let service = StreamableHttpService::new(
        move || Ok(served.clone()),
        veoveo_hub_sdk::stateless_session_manager(),
        veoveo_hub_sdk::canonical_streamable_http_server_config(),
    );
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let cuts = cuts.clone();
                async move {
                    let listen = request
                        .headers()
                        .get("mcp-method")
                        .is_some_and(|method| method == "subscriptions/listen");
                    let response = next.run(request).await;
                    if !listen {
                        return response;
                    }
                    let cut = CancellationToken::new();
                    cuts.send(cut.clone()).unwrap();
                    let (parts, body) = response.into_parts();
                    // HTTP EOF without SubscriptionsListenResult, unlike a handler
                    // returning Ok(()). Pinned RMCP reports this as Abrupt + Ok(None).
                    let stream = body.into_data_stream().take_until(cut.cancelled_owned());
                    axum::response::Response::from_parts(parts, Body::from_stream(stream))
                }
            },
        ));
    let _server = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let token = hub.access_token(&http, hub.caller("member")).await;
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(hub.installation.resource.clone())
                    .auth_header(token),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    // Hub discovery is asynchronous and its first-list settlement is bounded.
    // Subscribe before listing so a late catalog is observed without a sleep or
    // a first-snapshot readiness assumption. The test's outer deadline bounds it.
    let mut catalog = client
        .listen(
            SubscriptionFilter::builder()
                .resources_list_changed()
                .build(),
        )
        .await
        .unwrap();
    let uri = loop {
        let resources = client.list_all_resources().await.unwrap();
        if let Some(resource) = resources.iter().find(|resource| resource.name == "value") {
            break resource.uri.clone();
        }
        assert!(matches!(
            catalog.next().await.unwrap(),
            Some(ServerNotification::ResourceListChangedNotification(_))
        ));
    };
    catalog.cancel().await.unwrap();
    // Catalog reconciliation is Hub-owned, not a source resource listener.
    // Both queues below must refer only to the explicitly requested URI.
    assert!(matches!(
        openings.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    assert!(matches!(
        cut_streams.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    let filter = SubscriptionFilter::builder()
        .resource_subscription(uri.clone())
        .build();
    let mut first = client.listen(filter.clone()).await.unwrap();
    assert_eq!(first.acknowledged(), &filter);
    assert_eq!(openings.recv().await.unwrap(), source_filter());
    cut_streams.recv().await.unwrap().cancel();
    assert!(first.next().await.unwrap().is_none());
    assert!(matches!(first.end(), Some(SubscriptionEnd::Graceful(_))));

    // A change in the disconnected interval is recovered by reading current
    // state after re-listening. A later change is separately proved as pushed.
    source.value.store(2, Ordering::SeqCst);
    let mut reopened = client.listen(filter.clone()).await.unwrap();
    assert_eq!(reopened.acknowledged(), &filter);
    assert_eq!(openings.recv().await.unwrap(), source_filter());
    let snapshot = client
        .read_resource(ReadResourceRequestParams::new(uri.clone()))
        .await
        .unwrap();
    assert!(
        matches!(&snapshot.contents[0], ResourceContents::TextResourceContents { text, .. } if text == "2")
    );
    source.value.store(3, Ordering::SeqCst);
    source.changes.send(Change::Update).unwrap();
    let Some(ServerNotification::ResourceUpdatedNotification(update)) =
        reopened.next().await.unwrap()
    else {
        panic!("the reopened listener did not push the resource update");
    };
    assert_eq!(update.params.uri, uri);
    let snapshot = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .unwrap();
    assert!(
        matches!(&snapshot.contents[0], ResourceContents::TextResourceContents { text, .. } if text == "3")
    );

    // A real upstream protocol refusal must remain a refusal, not be converted
    // to a normal end or retried as another listener.
    source.changes.send(Change::Refuse).unwrap();
    let Err(ServiceError::McpError(error)) = reopened.next().await else {
        panic!("an upstream protocol error was hidden as normal closure");
    };
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(error.message, REFUSAL);
    assert!(matches!(
        openings.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    client.cancel().await.unwrap();
}
