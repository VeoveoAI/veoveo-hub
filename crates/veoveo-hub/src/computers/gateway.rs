//! Fixed native control-plane connection. No browser input selects a destination.
use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, ensure};
use openshell_sdk::{AuthConfig, ClientConfig, OpenShellClient, SdkError, raw::proto as p};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    sync::{Semaphore, mpsc},
};
use tokio_stream::wrappers::ReceiverStream;

use crate::control_plane::ComputerGateway;

pub(super) const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECTIONS: usize = 64;
const CHUNK: usize = 64 * 1024;

/// Preserve native status for control flow without retaining provider text.
#[derive(Debug, thiserror::Error)]
pub(super) enum ForwardError {
    #[error("native Computer forwarding refused: {0}")]
    Refused(tonic::Code),
    #[error("native Computer forwarding transport unavailable")]
    Transport,
    #[error("native Computer forwarding deadline")]
    Deadline,
    #[error("Computer forwarding capacity exhausted")]
    Capacity,
    #[error("Computer forwarding authentication refused")]
    Authentication,
    #[error("Computer forwarding configuration refused")]
    Configuration,
}
impl ForwardError {
    fn from_sdk(error: &SdkError) -> Self {
        if let Some(status) = error.grpc_status() {
            return Self::Refused(status.code());
        }
        match error {
            SdkError::Auth { .. } => Self::Authentication,
            SdkError::Connect { .. } | SdkError::Io { .. } => Self::Transport,
            _ => Self::Configuration,
        }
    }
    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Refused(tonic::Code::Unavailable | tonic::Code::DeadlineExceeded)
                | Self::Transport
                | Self::Deadline
        )
    }
}

#[derive(Clone)]
pub(super) struct Gateway {
    pub client: OpenShellClient,
    permits: Arc<Semaphore>,
}
impl Gateway {
    pub async fn connect(config: &ComputerGateway) -> Result<Self> {
        let bearer = std::env::var(&config.bearer_environment)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .context("Computer provisioning authority missing")?;
        let mut connection = ClientConfig::new(config.endpoint.to_string());
        connection.ca_cert = Some(tokio::fs::read(&config.certificate_authority).await?);
        connection.auth = Some(AuthConfig::oidc(bearer));
        let client = tokio::time::timeout(RPC_TIMEOUT, OpenShellClient::connect(connection))
            .await?
            .context("connect Computer gateway")?;
        Ok(Self {
            client,
            permits: Arc::new(Semaphore::new(CONNECTIONS)),
        })
    }

    /// Retry only transient native connection failures under one startup bound.
    /// The successful connection is returned for actual forwarding or closed by
    /// the provisioning caller; no HTTP/application health request is sent.
    pub async fn stream_when_ready(
        &self,
        workspace: String,
        port: u16,
        timeout: Duration,
    ) -> std::result::Result<(DuplexStream, tokio::task::JoinHandle<()>), ForwardError> {
        tokio::time::timeout(timeout, async {
            loop {
                match self.stream(workspace.clone(), port).await {
                    Ok(stream) => return Ok(stream),
                    Err(error) if error.retryable() => {
                        tokio::time::sleep(Duration::from_millis(250)).await
                    }
                    Err(error) => return Err(error),
                }
            }
        })
        .await
        .map_err(|_| ForwardError::Deadline)?
    }

    /// A single native authenticated stream to one already-admitted workspace.
    /// The returned task is cancelled when the owning HTTP/WS connection ends.
    pub async fn stream(
        &self,
        workspace: String,
        port: u16,
    ) -> std::result::Result<(DuplexStream, tokio::task::JoinHandle<()>), ForwardError> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ForwardError::Capacity)?;
        let (caller, socket) = tokio::io::duplex(CHUNK);
        let client = self.client.clone();
        let (tx, rx) = mpsc::channel(4);
        tx.send(p::TcpForwardFrame {
            payload: Some(p::tcp_forward_frame::Payload::Init(p::TcpForwardInit {
                workspace: workspace.clone(),
                sandbox: workspace,
                target: Some(p::tcp_forward_init::Target::Tcp(p::TcpRelayTarget {
                    host: "127.0.0.1".into(),
                    port: u32::from(port),
                })),
                ..Default::default()
            })),
        })
        .await
        .map_err(|_| ForwardError::Transport)?;
        let mut grpc = client
            .raw_grpc_fresh()
            .await
            .map_err(|error| ForwardError::from_sdk(&error))?;
        let mut upstream =
            tokio::time::timeout(RPC_TIMEOUT, grpc.forward_tcp(ReceiverStream::new(rx)))
                .await
                .map_err(|_| ForwardError::Deadline)?
                .map_err(|status| ForwardError::Refused(status.code()))?
                .into_inner();
        let task = tokio::spawn(async move {
            let _permit = permit;
            let (mut read, mut write) = tokio::io::split(socket);
            let upload = async {
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
            let download = async {
                while let Some(frame) = upstream.message().await? {
                    let Some(p::tcp_forward_frame::Payload::Data(bytes)) = frame.payload else {
                        anyhow::bail!("unexpected native Computer frame");
                    };
                    ensure!(bytes.len() <= CHUNK, "native Computer frame exceeds bound");
                    write.write_all(&bytes).await?;
                }
                Ok::<_, anyhow::Error>(())
            };
            // Dropping either half cancels the native stream and releases capacity.
            let _ = tokio::select! { result = upload => result, result = download => result };
        });
        Ok((caller, task))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listener_retry_uses_native_codes_and_refuses_auth_configuration_and_capacity() {
        for code in [tonic::Code::Unavailable, tonic::Code::DeadlineExceeded] {
            assert!(ForwardError::Refused(code).retryable());
        }
        for code in [
            tonic::Code::Unauthenticated,
            tonic::Code::PermissionDenied,
            tonic::Code::InvalidArgument,
            tonic::Code::NotFound,
            tonic::Code::FailedPrecondition,
            tonic::Code::ResourceExhausted,
        ] {
            assert!(!ForwardError::Refused(code).retryable());
        }
        let sentinel = "synthetic-secret-provider-description";
        let refused = ForwardError::from_sdk(&SdkError::auth(sentinel));
        assert!(!refused.retryable());
        assert!(!refused.to_string().contains(sentinel));
        assert!(!ForwardError::from_sdk(&SdkError::invalid_config(sentinel)).retryable());
        assert!(!ForwardError::Capacity.retryable());
    }
}
