//! Byte transport only: reqwest/rmcp retain HTTP, tokens, pooling and streaming.
//! Each accepted private Unix connection opens a new authenticated ForwardTcp.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use openshell_sdk::{AuthConfig, ClientConfig, OpenShellClient, raw::proto as p};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Semaphore, mpsc},
    task::{JoinHandle, JoinSet},
};
use tokio_stream::wrappers::ReceiverStream;
use veoveo_hub_sdk::registration::ServerEndpoint;

use crate::control_plane::{NativeUpstreamBinding, NativeUpstreams};

const CHUNK: usize = 64 * 1024;
const CONNECTIONS: usize = 64;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(super) struct NativeTransport {
    sockets: BTreeMap<String, PathBuf>,
    listeners: Vec<JoinHandle<()>>,
    // TempDir is mode 0700; nobody outside this process user can open its sockets.
    _directory: tempfile::TempDir,
}

impl Drop for NativeTransport {
    fn drop(&mut self) {
        for listener in &self.listeners {
            listener.abort();
        }
    }
}

impl NativeTransport {
    pub(super) async fn start(config: &NativeUpstreams) -> Result<Self> {
        ensure!(
            config.gateway.scheme() == "https",
            "native upstream gateway requires verified HTTPS"
        );
        let bearer = std::env::var(&config.bearer_environment)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .context("native upstream bearer environment variable is missing or empty")?;
        let mut connection = ClientConfig::new(config.gateway.to_string());
        connection.ca_cert = Some(tokio::fs::read(&config.certificate_authority).await?);
        connection.auth = Some(AuthConfig::oidc(bearer));
        let client = OpenShellClient::connect(connection)
            .await
            .context("connect native upstream gateway")?;
        Self::with_client(config, client).await
    }

    async fn with_client(config: &NativeUpstreams, client: OpenShellClient) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("vv-hub-")
            .tempdir_in("/tmp")?;
        let mut transport = Self {
            sockets: BTreeMap::new(),
            listeners: Vec::new(),
            _directory: directory,
        };
        let permits = Arc::new(Semaphore::new(CONNECTIONS));
        for (index, binding) in config.bindings.iter().enumerate() {
            ensure!(
                binding.port != 0 && !binding.workspace.is_empty() && !binding.sandbox.is_empty(),
                "native upstream binding requires workspace, sandbox and nonzero port"
            );
            let endpoint = binding.endpoint.url().as_str().to_owned();
            ensure!(
                !transport.sockets.contains_key(&endpoint),
                "duplicate native upstream endpoint {endpoint}"
            );
            let path = transport._directory.path().join(index.to_string());
            let listener = UnixListener::bind(&path)?;
            transport.sockets.insert(endpoint, path);
            let client = client.clone();
            let binding = binding.clone();
            let permits = permits.clone();
            transport.listeners.push(tokio::spawn(async move {
                let mut connections = JoinSet::new();
                loop {
                    tokio::select! {
                        result = connections.join_next(), if !connections.is_empty() => {
                            if let Some(Err(error)) = result { tracing::debug!(%error, "native upstream task ended"); }
                        }
                        accepted = listener.accept() => {
                            let Ok((socket, _)) = accepted else { break };
                            let Ok(permit) = permits.clone().try_acquire_owned() else { drop(socket); continue };
                            let client = client.clone();
                            let binding = binding.clone();
                            connections.spawn(async move {
                                let _permit = permit;
                                if let Err(error) = forward(socket, client, binding).await {
                                    tracing::debug!(%error, "native upstream connection ended");
                                }
                            });
                        }
                    }
                }
                // Dropping JoinSet cancels all owned streams, including subscriptions.
            }));
        }
        Ok(transport)
    }

    pub(super) fn socket(&self, endpoint: &ServerEndpoint) -> Result<&Path> {
        self.sockets
            .get(endpoint.url().as_str())
            .map(PathBuf::as_path)
            .with_context(|| format!("no native upstream binding for {}", endpoint.url()))
    }
}

async fn forward(
    socket: UnixStream,
    client: OpenShellClient,
    binding: NativeUpstreamBinding,
) -> Result<()> {
    let (tx, rx) = mpsc::channel(4);
    tx.send(p::TcpForwardFrame {
        payload: Some(p::tcp_forward_frame::Payload::Init(p::TcpForwardInit {
            workspace: binding.workspace,
            sandbox: binding.sandbox,
            target: Some(p::tcp_forward_init::Target::Tcp(p::TcpRelayTarget {
                host: "127.0.0.1".into(),
                port: u32::from(binding.port),
            })),
            ..Default::default()
        })),
    })
    .await?;
    let (mut read, mut write) = socket.into_split();
    let upload = async move {
        let mut buffer = vec![0; CHUNK];
        loop {
            let count = read.read(&mut buffer).await?;
            if count == 0 {
                return Ok::<_, anyhow::Error>(());
            }
            tx.send(p::TcpForwardFrame {
                payload: Some(p::tcp_forward_frame::Payload::Data(
                    buffer[..count].to_vec(),
                )),
            })
            .await?;
        }
    };
    // Upload remains polled while opening/waiting for the response. A client that
    // disconnects cancels the RPC even after its HTTP request body has been sent.
    let download = async move {
        let mut grpc = client.raw_grpc_fresh().await?;
        let mut upstream =
            tokio::time::timeout(CONNECT_TIMEOUT, grpc.forward_tcp(ReceiverStream::new(rx)))
                .await??
                .into_inner();
        while let Some(frame) = upstream.message().await? {
            match frame.payload {
                Some(p::tcp_forward_frame::Payload::Data(bytes)) => write.write_all(&bytes).await?,
                _ => anyhow::bail!("unexpected native upstream frame"),
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    tokio::select! { result = upload => result, result = download => result }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::HubUpstreamHttpClientPool;
    use futures::StreamExt;
    use veoveo_hub_sdk::ServerSlug;

    /// The external fixture serves /healthz, /mcp and /sse, as declared by the
    /// qualification repository. This exercises real native forwarding, not a mock.
    #[tokio::test]
    #[ignore = "requires an approved native fixture and private operator configuration"]
    async fn native_requests_streaming_cancellation_and_new_connections() -> Result<()> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let path = std::env::var("VEOVEO_NATIVE_TRANSPORT_TEST_CONFIG")?;
        let config: NativeUpstreams = serde_json::from_slice(&std::fs::read(path)?)?;
        let endpoint = &config.bindings.first().context("fixture binding")?.endpoint;
        let pool = HubUpstreamHttpClientPool::native(&config).await?;
        let slug = ServerSlug::parse("native-fixture")?;
        let transport = pool
            .transport(&slug, endpoint)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut url = endpoint.url().clone();
        url.set_path("/healthz");
        ensure!(
            transport
                .client()
                .get(url.clone())
                .send()
                .await?
                .text()
                .await?
                == "ok",
            "ordinary request"
        );
        url.set_path("/mcp");
        let reply: serde_json::Value = transport
            .client()
            .post(url.clone())
            .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
            .send()
            .await?
            .json()
            .await?;
        ensure!(reply["result"]["method"] == "ping", "ordinary MCP request");
        url.set_path("/sse");
        url.set_query(Some("interval_ms=200&count=100"));
        let mut stream = transport
            .client()
            .get(url.clone())
            .send()
            .await?
            .bytes_stream();
        for _ in 0..3 {
            let bytes = tokio::time::timeout(Duration::from_secs(3), stream.next())
                .await?
                .context("live stream")??;
            ensure!(!bytes.is_empty(), "streamed update");
        }
        // Cancelling a subscription must release its native stream without waiting
        // for the remaining events. Discard the client as discovery recovery does.
        drop(stream);
        transport.discard();
        let replacement = transport.current().map_err(|e| anyhow::anyhow!("{e}"))?;
        url.set_path("/healthz");
        url.set_query(None);
        ensure!(
            tokio::time::timeout(Duration::from_secs(3), replacement.client().get(url).send())
                .await??
                .text()
                .await?
                == "ok",
            "fresh connection after cancellation"
        );
        let missing = ServerEndpoint::new(url::Url::parse("http://unbound.internal/mcp")?)?;
        ensure!(
            pool.transport(&ServerSlug::parse("unbound")?, &missing)
                .is_err(),
            "unbound endpoint rejected"
        );
        drop(transport);
        drop(replacement);
        drop(pool);
        let native = NativeTransport::start(&config).await?;
        let directory = native._directory.path().to_path_buf();
        drop(native);
        ensure!(
            !directory.exists(),
            "private socket directory removed with owner"
        );
        Ok(())
    }
}
