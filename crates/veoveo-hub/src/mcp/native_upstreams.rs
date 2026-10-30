//! Byte transport only: reqwest/rmcp retain HTTP, tokens, pooling and streaming.
//! Each accepted private Unix connection opens a new authenticated ForwardTcp. The
//! control plane binds each server endpoint to a sandbox; an instance is bound by its
//! deployment to its factory sandbox, at the port of the endpoint it registered.

use std::{
    collections::BTreeMap,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use openshell_sdk::{AuthConfig, ClientConfig, raw::proto as p};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};
use tokio_stream::wrappers::ReceiverStream;
use veoveo_hub_sdk::registration::ServerEndpoint;
use veoveo_types::DeploymentId;

use crate::{
    control_plane::{NativeUpstreamBinding, NativeUpstreams},
    forward_connections::{Forward, ForwardConnections},
};

const CHUNK: usize = 64 * 1024;
const CONNECTIONS: NonZeroUsize = NonZeroUsize::new(64).unwrap();
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct NativeTransport {
    forwards: ForwardConnections,
    endpoints: BTreeMap<String, Arc<NativeEndpoint>>,
    instances: Option<Arc<Instances>>,
}

/// The factory sandboxes of instances, each bound on first use.
#[derive(Debug)]
struct Instances {
    workspace: String,
    endpoints: parking_lot::Mutex<BTreeMap<DeploymentId, Arc<NativeEndpoint>>>,
}

impl std::fmt::Debug for NativeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeTransport")
            .field("endpoints", &self.endpoints)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct NativeEndpoint {
    binding: NativeUpstreamBinding,
    path: PathBuf,
    listener: JoinHandle<()>,
    // Each endpoint owns its private socket. Unchanged endpoints are shared
    // between revisions; old requests retain changed/removed endpoints to drain.
    _directory: tempfile::TempDir,
}

impl Drop for NativeEndpoint {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

impl NativeTransport {
    /// Connect to the gateway. `instances` is the factory workspace, when the
    /// installation serves instances from factory sandboxes.
    pub(super) async fn start(config: &NativeUpstreams, instances: Option<&str>) -> Result<Self> {
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
        let forwards = ForwardConnections::new(connection, CONNECTIONS);
        // The first connection opens now: a gateway the Hub cannot reach stops startup.
        forwards
            .reserve()
            .context("native upstream forwards")?
            .client()
            .await
            .context("connect native upstream gateway")?;
        Self::with_forwards(config, instances, forwards)
    }

    fn with_forwards(
        config: &NativeUpstreams,
        instances: Option<&str>,
        forwards: ForwardConnections,
    ) -> Result<Self> {
        Self {
            forwards,
            endpoints: BTreeMap::new(),
            instances: None,
        }
        .reconfigured(config, instances)
    }

    #[cfg(test)]
    fn with_client(
        config: &NativeUpstreams,
        instances: Option<&str>,
        client: openshell_sdk::OpenShellClient,
    ) -> Result<Self> {
        Self::with_forwards(
            config,
            instances,
            ForwardConnections::of(client, CONNECTIONS),
        )
    }

    /// The next routing generation. Instance bindings carry over while the factory
    /// workspace stays the same.
    pub(super) fn reconfigured(
        &self,
        config: &NativeUpstreams,
        instances: Option<&str>,
    ) -> Result<Self> {
        let mut endpoints = BTreeMap::new();
        for binding in &config.bindings {
            ensure!(
                binding.port != 0 && !binding.workspace.is_empty() && !binding.sandbox.is_empty(),
                "native upstream binding requires workspace, sandbox and nonzero port"
            );
            let key = binding.endpoint.url().as_str().to_owned();
            ensure!(
                !endpoints.contains_key(&key),
                "duplicate native upstream endpoint {key}"
            );
            let endpoint = match self.endpoints.get(&key).filter(|v| v.binding == *binding) {
                Some(endpoint) => endpoint.clone(),
                None => Arc::new(self.open_endpoint(binding.clone())?),
            };
            endpoints.insert(key, endpoint);
        }
        let instances = instances.map(|workspace| match &self.instances {
            Some(current) if current.workspace == workspace => current.clone(),
            _ => Arc::new(Instances {
                workspace: workspace.to_owned(),
                endpoints: parking_lot::Mutex::default(),
            }),
        });
        Ok(Self {
            forwards: self.forwards.clone(),
            endpoints,
            instances,
        })
    }

    fn open_endpoint(&self, binding: NativeUpstreamBinding) -> Result<NativeEndpoint> {
        let directory = tempfile::Builder::new()
            .prefix("vv-hub-")
            .tempdir_in("/tmp")?;
        let path = directory.path().join("socket");
        let listener = UnixListener::bind(&path)?;
        let forwards = self.forwards.clone();
        let target = binding.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    result = connections.join_next(), if !connections.is_empty() => {
                        if let Some(Err(error)) = result { tracing::debug!(%error, "native upstream task ended"); }
                    }
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { break };
                        let (forwards, binding) = (forwards.clone(), target.clone());
                        connections.spawn(async move {
                            let Some(stream) = forwards.reserve_within(CONNECT_TIMEOUT).await else {
                                tracing::warn!(sandbox = %binding.sandbox, "every native upstream forward stayed in use for ten seconds, so a connection was refused");
                                return;
                            };
                            if let Err(error) = forward(socket, stream, binding).await {
                                tracing::debug!(%error, "native upstream connection ended");
                            }
                        });
                    }
                }
            }
        });
        Ok(NativeEndpoint {
            binding,
            path,
            listener: task,
            _directory: directory,
        })
    }

    pub(super) fn same_endpoint(&self, other: &Self, endpoint: &ServerEndpoint) -> bool {
        match (
            self.endpoints.get(endpoint.url().as_str()),
            other.endpoints.get(endpoint.url().as_str()),
        ) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Whether both generations bind instances to the same factory sandboxes.
    pub(super) fn same_instances(&self, other: &Self) -> bool {
        match (&self.instances, &other.instances) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    pub(super) fn socket(&self, endpoint: &ServerEndpoint) -> Result<&Path> {
        self.endpoints
            .get(endpoint.url().as_str())
            .map(|v| v.path.as_path())
            .with_context(|| format!("no native upstream binding for {}", endpoint.url()))
    }

    /// Close the listener of every instance whose deployment `registered` no longer
    /// names: its instance was removed with its registrations.
    pub(super) fn retain_instances(&self, registered: impl Fn(DeploymentId) -> bool) {
        if let Some(instances) = &self.instances {
            instances
                .endpoints
                .lock()
                .retain(|deployment, _| registered(*deployment));
        }
    }

    /// The socket of an instance's factory sandbox, at the port of the endpoint the
    /// instance registered.
    pub(super) fn instance_socket(
        &self,
        deployment: DeploymentId,
        endpoint: &ServerEndpoint,
    ) -> Result<PathBuf> {
        let instances = self.instances.as_ref().context(
            "instances run in factory sandboxes; the installation configures no factory",
        )?;
        let binding = NativeUpstreamBinding {
            endpoint: endpoint.clone(),
            workspace: instances.workspace.clone(),
            sandbox: crate::factory::sandbox_name(deployment),
            port: endpoint
                .url()
                .port_or_known_default()
                .with_context(|| format!("{} names no port", endpoint.url()))?,
        };
        let mut endpoints = instances.endpoints.lock();
        if let Some(bound) = endpoints.get(&deployment).filter(|v| v.binding == binding) {
            return Ok(bound.path.clone());
        }
        let bound = Arc::new(self.open_endpoint(binding)?);
        let path = bound.path.clone();
        endpoints.insert(deployment, bound);
        Ok(path)
    }
}

async fn forward(
    socket: UnixStream,
    stream: Forward,
    binding: NativeUpstreamBinding,
) -> Result<()> {
    let sandbox = binding.sandbox.clone();
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
        let mut grpc = stream.client().await?.raw_grpc_fresh().await?;
        let opened =
            tokio::time::timeout(CONNECT_TIMEOUT, grpc.forward_tcp(ReceiverStream::new(rx))).await;
        let mut upstream = match opened {
            Ok(Ok(response)) => response.into_inner(),
            Ok(Err(status)) => {
                tracing::warn!(%sandbox, code = ?status.code(), message = status.message(), "the gateway refused a native upstream forward");
                return Err(status.into());
            }
            Err(elapsed) => {
                tracing::warn!(%sandbox, "the gateway opened no native upstream forward within ten seconds");
                return Err(elapsed.into());
            }
        };
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
    use openshell_sdk::OpenShellClient;
    use veoveo_hub_sdk::DeploymentId;

    #[tokio::test]
    async fn native_destination_replacement_retains_only_inflight_and_unaffected_listeners()
    -> Result<()> {
        let client = OpenShellClient::from_parts(
            tonic::transport::Endpoint::from_static("http://127.0.0.1:9").connect_lazy(),
            openshell_sdk::EdgeAuthInterceptor::noop(),
        );
        let mut config: NativeUpstreams = serde_json::from_value(serde_json::json!({
            "gateway":"https://gateway.example/", "certificateAuthority":"/unused/ca", "bearerEnvironment":"UNUSED",
            "bindings":[
                {"endpoint":"http://fixture-a.internal/mcp", "workspace":"fixture", "sandbox":"a", "port":8800},
                {"endpoint":"http://fixture-b.internal/mcp", "workspace":"fixture", "sandbox":"b", "port":8800}
            ]
        }))?;
        let old = NativeTransport::with_client(&config, None, client)?;
        let a = config.bindings[0].endpoint.clone();
        let b = config.bindings[1].endpoint.clone();
        let inflight = old.endpoints[a.url().as_str()].clone();
        let old_path = inflight.path.clone();
        let mut invalid = config.clone();
        invalid.bindings.push(invalid.bindings[0].clone());
        assert!(old.reconfigured(&invalid, None).is_err());
        assert!(
            old.socket(&a)?.exists(),
            "rejected preparation cannot disturb active listeners"
        );
        config.bindings[0].sandbox = "replacement".into();
        let mut added = config.bindings[0].clone();
        added.endpoint = ServerEndpoint::new(url::Url::parse("http://fixture-c.internal/mcp")?)?;
        added.sandbox = "added".into();
        let c = added.endpoint.clone();
        config.bindings.push(added);
        let next = old.reconfigured(&config, None)?;
        assert!(old.socket(&c).is_err());
        assert!(next.socket(&c)?.exists());
        assert!(!old.same_endpoint(&next, &a));
        assert!(old.same_endpoint(&next, &b));
        assert_ne!(old.socket(&a)?, next.socket(&a)?);
        drop(old);
        assert!(
            old_path.exists(),
            "inflight snapshot retains old native listener"
        );
        drop(inflight);
        assert!(
            !old_path.exists(),
            "drained endpoint no longer owns a socket"
        );
        config.bindings.remove(0);
        let removed_path = next.socket(&a)?.to_owned();
        let removed = next.reconfigured(&config, None)?;
        assert!(removed.socket(&a).is_err());
        assert!(next.same_endpoint(&removed, &b));
        assert!(
            removed_path.exists(),
            "old generation is still usable while retained"
        );
        drop(next);
        assert!(!removed_path.exists());
        assert!(removed.socket(&b)?.exists());
        Ok(())
    }

    #[tokio::test]
    async fn an_instance_is_bound_by_its_deployment_to_its_factory_sandbox() -> Result<()> {
        let client = OpenShellClient::from_parts(
            tonic::transport::Endpoint::from_static("http://127.0.0.1:9").connect_lazy(),
            openshell_sdk::EdgeAuthInterceptor::noop(),
        );
        let config: NativeUpstreams = serde_json::from_value(serde_json::json!({
            "gateway":"https://gateway.example/", "certificateAuthority":"/unused/ca",
            "bearerEnvironment":"UNUSED", "bindings":[]
        }))?;
        let deployment = DeploymentId::new();
        let endpoint = ServerEndpoint::new(url::Url::parse(
            "http://instance.internal:8800/fixture/mcp",
        )?)?;
        assert!(
            NativeTransport::with_client(&config, None, client.clone())?
                .instance_socket(deployment, &endpoint)
                .is_err(),
            "without a factory no instance is bound"
        );
        let native = NativeTransport::with_client(&config, Some("factory"), client)?;
        let path = native.instance_socket(deployment, &endpoint)?;
        assert!(path.exists());
        assert_eq!(native.instance_socket(deployment, &endpoint)?, path);
        assert_eq!(
            native.instances.as_ref().unwrap().endpoints.lock()[&deployment].binding,
            NativeUpstreamBinding {
                endpoint: endpoint.clone(),
                workspace: "factory".into(),
                sandbox: crate::factory::sandbox_name(deployment),
                port: 8800,
            }
        );
        let reloaded = native.reconfigured(&config, Some("factory"))?;
        assert!(native.same_instances(&reloaded));
        assert_eq!(reloaded.instance_socket(deployment, &endpoint)?, path);
        let moved = native.reconfigured(&config, Some("elsewhere"))?;
        assert!(!native.same_instances(&moved));
        assert_ne!(moved.instance_socket(deployment, &endpoint)?, path);
        let reregistered = ServerEndpoint::new(url::Url::parse(
            "http://instance.internal:8810/fixture/mcp",
        )?)?;
        assert_ne!(native.instance_socket(deployment, &reregistered)?, path);
        assert!(!path.exists(), "a replaced binding releases its socket");
        Ok(())
    }

    #[tokio::test]
    async fn a_removed_instance_closes_its_listener() -> Result<()> {
        let client = OpenShellClient::from_parts(
            tonic::transport::Endpoint::from_static("http://127.0.0.1:9").connect_lazy(),
            openshell_sdk::EdgeAuthInterceptor::noop(),
        );
        let config: NativeUpstreams = serde_json::from_value(serde_json::json!({
            "gateway":"https://gateway.example/", "certificateAuthority":"/unused/ca",
            "bearerEnvironment":"UNUSED", "bindings":[]
        }))?;
        let native = NativeTransport::with_client(&config, Some("factory"), client)?;
        let endpoint = ServerEndpoint::new(url::Url::parse(
            "http://instance.internal:8800/fixture/mcp",
        )?)?;
        let (removed, kept) = (DeploymentId::new(), DeploymentId::new());
        let removed_path = native.instance_socket(removed, &endpoint)?;
        let kept_path = native.instance_socket(kept, &endpoint)?;
        let listener = native.instances.as_ref().unwrap().endpoints.lock()[&removed]
            .listener
            .abort_handle();
        let reloaded = native.reconfigured(&config, Some("factory"))?;

        native.retain_instances(|deployment| deployment == kept);
        assert!(
            !removed_path.exists(),
            "the removed instance's socket is gone"
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            while !listener.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .context("the removed instance's listener task ended")?;
        assert!(kept_path.exists(), "another instance keeps its listener");
        assert_eq!(reloaded.instance_socket(kept, &endpoint)?, kept_path);
        Ok(())
    }

    /// The external fixture serves /healthz, /mcp and /sse, as declared by the
    /// qualification repository. This exercises real native forwarding, not a mock.
    #[tokio::test]
    #[ignore = "requires an approved native fixture and private operator configuration"]
    async fn native_requests_streaming_cancellation_and_new_connections() -> Result<()> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let path = std::env::var("VEOVEO_NATIVE_TRANSPORT_TEST_CONFIG")?;
        let config: NativeUpstreams = serde_json::from_slice(&std::fs::read(path)?)?;
        let endpoint = &config.bindings.first().context("fixture binding")?.endpoint;
        let pool = HubUpstreamHttpClientPool::native(&config, None).await?;
        let deployment = DeploymentId::new();
        let transport = pool
            .transport(deployment, endpoint, false)
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
            pool.transport(DeploymentId::new(), &missing, false)
                .is_err(),
            "unbound endpoint rejected"
        );
        drop(transport);
        drop(replacement);
        drop(pool);
        let native = NativeTransport::start(&config, None).await?;
        let directories = native
            .endpoints
            .values()
            .map(|endpoint| endpoint._directory.path().to_owned())
            .collect::<Vec<_>>();
        drop(native);
        ensure!(
            directories.iter().all(|directory| !directory.exists()),
            "private socket directory removed with owner"
        );
        Ok(())
    }
}
