use super::*;
use crate::mcp::upstream_http::{GatewayUpstreamHttpClientPool, upstream_client_builder};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Router, http::StatusCode, response::IntoResponse};
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ClientConfig,
        ErrorData as McpError, ServerCapabilities, ServerConfig,
    },
    service::{RequestContext, RoleServer},
    transport::streamable_http_server::StreamableHttpService,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[derive(Clone)]
struct Source(Arc<AtomicUsize>);
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn call_tool(
        &self,
        _: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(CallToolResult::success(vec![]).into())
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Disconnect(usize),
    Unauthorized,
    ProtocolError,
    InvalidJson,
}

struct Fixture {
    endpoint: String,
    attempts: Arc<AtomicUsize>,
    mutations: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn start(fault: Fault) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let source = Source(mutations.clone());
        let service = StreamableHttpService::new(
            move || Ok(source.clone()),
            veoveo_sdk_rust::stateless_session_manager(),
            veoveo_sdk_rust::canonical_streamable_http_server_config(),
        );
        let count = attempts.clone();
        let router = Router::new().nest_service("/mcp", service).layer(
            axum::middleware::from_fn(move |request: axum::extract::Request, next: axum::middleware::Next| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    match fault {
                        Fault::Disconnect(_) => next.run(request).await,
                        Fault::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
                        Fault::InvalidJson => ([("content-type", "application/json")], "invalid").into_response(),
                        Fault::ProtocolError => {
                            let bytes = axum::body::to_bytes(request.into_body(), 65536).await.unwrap();
                            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                            axum::Json(serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "error":{"code":-32603,"message":"source failure"}})).into_response()
                        }
                    }
                }
            }),
        );
        let disconnected = attempts.clone();
        let server = tokio::spawn(async move {
            if let Fault::Disconnect(times) = fault {
                for _ in 0..times {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut bytes = [0; 4096];
                    assert!(socket.read(&mut bytes).await.unwrap() > 0);
                    disconnected.fetch_add(1, Ordering::SeqCst);
                    // The discovery POST reached the old connection, which dies
                    // without an HTTP response during upstream replacement.
                    drop(socket);
                }
            }
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            endpoint,
            attempts,
            mutations,
            server,
        }
    }

    async fn connect(&self) -> Result<RunningService<RoleClient, ClientConfig>, RequestError> {
        connect(&pool(), "upstream-a", &self.endpoint).await
    }
}

/// The Gateway's per-server pool, with clients that bypass any proxy to reach loopback
/// upstreams.
fn pool() -> GatewayUpstreamHttpClientPool {
    GatewayUpstreamHttpClientPool::with_builder(|| {
        upstream_client_builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
    })
}

async fn connect(
    pool: &GatewayUpstreamHttpClientPool,
    slug: &str,
    endpoint: &str,
) -> Result<RunningService<RoleClient, ClientConfig>, RequestError> {
    let server = crate::test_catalog::registered_server(slug, endpoint);
    let http = GatewayAuthorizedHttpClient::new(
        pool.transport(&server.slug).unwrap(),
        crate::test_catalog::token_issuer(),
        "workspace".parse().unwrap(),
        &server,
        &super::super::task_ownership_tests::subject(),
        Default::default(),
    );
    discover(
        ClientConfig::default(),
        http,
        StreamableHttpClientTransportConfig::with_uri(endpoint.to_owned()),
    )
    .await
}

#[tokio::test]
async fn discovery_recovers_one_disconnect_without_repeating_domain_mutations() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(30), async {
        let fixture = Fixture::start(Fault::Disconnect(1)).await;
        let client = fixture
            .connect()
            .await
            .expect("discovery should reconnect once");
        assert_eq!(fixture.attempts.load(Ordering::SeqCst), 2);
        client
            .call_tool(CallToolRequestParams::new("mutate"))
            .await
            .unwrap();
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
        client.cancel().await.unwrap();

        for (fault, attempts) in [
            (Fault::Disconnect(2), 2),
            (Fault::Unauthorized, 1),
            (Fault::ProtocolError, 1),
            (Fault::InvalidJson, 1),
        ] {
            let fixture = Fixture::start(fault).await;
            assert!(fixture.connect().await.is_err());
            assert_eq!(fixture.attempts.load(Ordering::SeqCst), attempts);
            assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
        }
    })
    .await
    .expect("discovery recovery matrix exceeded thirty seconds");
}

/// A port forward in front of an upstream, as an installation reaches its servers. The
/// forward keeps the Gateway's side of each connection open when the server behind it
/// stops: a connection opened before a restart reads its next request and closes
/// without an answer, because the server it reached is gone.
struct Forward {
    endpoint: String,
    accepted: Arc<AtomicUsize>,
    stale: Arc<AtomicUsize>,
    restarts: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Forward {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Forward {
    async fn start(backend: std::net::SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let accepted = Arc::new(AtomicUsize::new(0));
        let stale = Arc::new(AtomicUsize::new(0));
        let restarts = Arc::new(AtomicUsize::new(0));
        let task = tokio::spawn({
            let (accepted, stale, restarts) = (accepted.clone(), stale.clone(), restarts.clone());
            async move {
                loop {
                    let (client, _) = listener.accept().await.unwrap();
                    accepted.fetch_add(1, Ordering::SeqCst);
                    tokio::spawn(relay(client, backend, restarts.clone(), stale.clone()));
                }
            }
        });
        Self {
            endpoint,
            accepted,
            stale,
            restarts,
            task,
        }
    }
}

async fn relay(
    client: tokio::net::TcpStream,
    backend: std::net::SocketAddr,
    restarts: Arc<AtomicUsize>,
    stale: Arc<AtomicUsize>,
) {
    let Ok(upstream) = tokio::net::TcpStream::connect(backend).await else {
        return;
    };
    let opened = restarts.load(Ordering::SeqCst);
    let (mut client_read, mut client_write) = client.into_split();
    let (mut upstream_read, mut upstream_write) = upstream.into_split();
    // The answers stop when the server goes, but the Gateway's side stays open: the
    // task hands back its write half, which closes only when the relay ends.
    let answers = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut upstream_read, &mut client_write).await;
        client_write
    });
    let mut bytes = [0; 8192];
    while let Ok(read) = client_read.read(&mut bytes).await {
        if read == 0 {
            break;
        }
        if restarts.load(Ordering::SeqCst) != opened {
            // The server this connection reached is gone: the request goes unanswered
            // and the connection closes.
            stale.fetch_add(1, Ordering::SeqCst);
            break;
        }
        if upstream_write.write_all(&bytes[..read]).await.is_err() {
            break;
        }
    }
    answers.abort();
}

/// An upstream serving on a listener, counting the HTTP requests and the tool calls it
/// receives. Dropping it stops the server.
struct Upstream {
    requests: Arc<AtomicUsize>,
    mutations: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Upstream {
    fn serve(listener: TcpListener) -> Self {
        let requests = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let source = Source(mutations.clone());
        let service = StreamableHttpService::new(
            move || Ok(source.clone()),
            veoveo_sdk_rust::stateless_session_manager(),
            veoveo_sdk_rust::canonical_streamable_http_server_config(),
        );
        let counted = requests.clone();
        let router = Router::new()
            .nest_service("/mcp", service)
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let counted = counted.clone();
                    async move {
                        counted.fetch_add(1, Ordering::SeqCst);
                        next.run(request).await
                    }
                },
            ));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            requests,
            mutations,
            task,
        }
    }
}

/// Bind `address` again once the stopped server has released it.
async fn bind_again(address: std::net::SocketAddr) -> TcpListener {
    for _ in 0..100 {
        if let Ok(listener) = TcpListener::bind(address).await {
            return listener;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{address} stayed bound after its server stopped");
}

/// Leave `count` idle connections in `slug`'s pool through `endpoint`: concurrent requests
/// answered in full.
async fn pool_connections(
    pool: &GatewayUpstreamHttpClientPool,
    slug: &str,
    endpoint: &str,
    count: usize,
) {
    let transport = pool
        .transport(&crate::test_catalog::registered_server(slug, endpoint).slug)
        .unwrap();
    futures::future::join_all((0..count).map(|_| async {
        let response = transport.client().get(endpoint).send().await.unwrap();
        response.bytes().await.unwrap();
    }))
    .await;
}

#[tokio::test]
async fn after_an_upstream_restart_discovery_and_calls_succeed_and_other_pools_stay() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(30), async {
        let pool = pool();

        // `upstream-a` restarts behind its forward; `upstream-b` keeps running.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a_backend = listener.local_addr().unwrap();
        let a_first = Upstream::serve(listener);
        let a = Forward::start(a_backend).await;
        let b_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let b_backend = b_listener.local_addr().unwrap();
        let b_upstream = Upstream::serve(b_listener);
        let b = Forward::start(b_backend).await;

        pool_connections(&pool, "upstream-a", &a.endpoint, 4).await;
        pool_connections(&pool, "upstream-b", &b.endpoint, 4).await;
        let a_pooled = a.accepted.load(Ordering::SeqCst);
        let b_pooled = b.accepted.load(Ordering::SeqCst);
        assert!(
            a_pooled >= 2,
            "upstream-a's pool holds {a_pooled} connection(s)"
        );

        drop(a_first);
        let a_second = Upstream::serve(bind_again(a_backend).await);
        a.restarts.fetch_add(1, Ordering::SeqCst);

        // Every discovery and every tool call after the restart succeeds.
        const CALLS: usize = 4;
        for call in 0..CALLS {
            let client = connect(&pool, "upstream-a", &a.endpoint)
                .await
                .unwrap_or_else(|error| panic!("discovery {call} after the restart: {error:?}"));
            client
                .call_tool(CallToolRequestParams::new("mutate"))
                .await
                .unwrap_or_else(|error| panic!("tool call {call} after the restart: {error:?}"));
            client.cancel().await.unwrap();
        }
        assert_eq!(a_second.mutations.load(Ordering::SeqCst), CALLS);
        // Only the first discovery's first attempt reached a stale connection: its
        // failure discarded the pool, so no later request took another.
        assert_eq!(a.stale.load(Ordering::SeqCst), 1);
        assert!(a_second.requests.load(Ordering::SeqCst) >= 2 * CALLS);

        // upstream-b's pool is untouched: its next requests reuse its idle connections.
        let client = connect(&pool, "upstream-b", &b.endpoint).await.unwrap();
        client
            .call_tool(CallToolRequestParams::new("mutate"))
            .await
            .unwrap();
        client.cancel().await.unwrap();
        assert_eq!(b_upstream.mutations.load(Ordering::SeqCst), 1);
        assert_eq!(b.accepted.load(Ordering::SeqCst), b_pooled);
    })
    .await
    .expect("restart recovery exceeded thirty seconds");
}
