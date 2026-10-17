//! Request-scoped discovery recovery, before any domain request is dispatched.
use std::time::Duration;

use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::ProtocolVersion,
    service::{ClientInitializeError, RunningService},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::{StreamableHttpClientTransportConfig, StreamableHttpError},
    },
};

use super::http_response::RequestError;
use super::upstream_authorized_http::{HubAuthorizedHttpClient, HubAuthorizedHttpError};
use crate::mcp_support::mcp_internal;

/// The whole discovery, both attempts and the pause between them, ends by this deadline.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// The pause before the retry, so a server that is starting has a moment to listen.
const RETRY_PAUSE: Duration = Duration::from_millis(200);

/// Discover the upstream, retrying once after a transport failure. The failure
/// discards the server's pooled client, so the retry and every later request open new
/// connections instead of taking another stale one from a server that restarted
/// behind its port forward.
pub(super) async fn discover<S: ClientHandler + Clone>(
    handler: S,
    http: HubAuthorizedHttpClient,
    config: StreamableHttpClientTransportConfig,
) -> Result<RunningService<RoleClient, S>, RequestError> {
    let deadline = tokio::time::Instant::now() + DISCOVERY_TIMEOUT;
    let mut http = http;
    for attempt in 0..2 {
        let transport = StreamableHttpClientTransport::with_client(http.clone(), config.clone());
        let result = tokio::time::timeout_at(
            deadline,
            handler.clone().serve_with_lifecycle(
                transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            ),
        )
        .await
        .map_err(|_| mcp_internal("upstream discovery exceeded ten seconds"))?;
        match result {
            Ok(running) => return Ok(running),
            Err(error) if attempt == 0 && recoverable(&error) => {
                // Discover is read-only. No tool, Task or domain request has been
                // dispatched, even when the caller intends a mutation afterward.
                tracing::warn!(
                    "retrying upstream discovery once on new connections after a transport failure"
                );
                http = http.reconnected().map_err(RequestError::from)?;
                tokio::time::sleep_until((tokio::time::Instant::now() + RETRY_PAUSE).min(deadline))
                    .await;
            }
            Err(error) => {
                return Err(RequestError::from_initialize(error));
            }
        }
    }
    unreachable!("the final discovery attempt always returns")
}

fn recoverable(error: &ClientInitializeError) -> bool {
    let ClientInitializeError::TransportError { error, .. } = error else {
        return false;
    };
    match error
        .error
        .downcast_ref::<StreamableHttpError<HubAuthorizedHttpError>>()
    {
        Some(StreamableHttpError::Client(HubAuthorizedHttpError::Http(error))) => {
            error.is_connect() || error.is_timeout() || error.is_request() || error.is_body()
        }
        Some(
            StreamableHttpError::UnexpectedEndOfStream
            | StreamableHttpError::TransportChannelClosed,
        ) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
