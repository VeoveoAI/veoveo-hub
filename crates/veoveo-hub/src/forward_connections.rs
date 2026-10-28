//! The gateway connections ForwardTcp streams ride. A forward holds one HTTP/2 stream
//! for the whole life of the connection it carries, and the Hub sandbox's supervisor
//! admits at most [`STREAMS`] concurrent streams on one connection to the gateway: a
//! stream beyond them waits, unannounced, until another ends. Forwards therefore ride
//! connections of their own, at most that many on each, each opened at its first
//! forward and kept.
use std::{num::NonZeroUsize, sync::Arc};

use openshell_sdk::{ClientConfig, OpenShellClient, SdkError};
use tokio::sync::{OnceCell, OwnedSemaphorePermit, Semaphore};

/// The streams OpenShell's supervisor admits on one HTTP/2 connection
/// (`openshell-supervisor-network`, `l7::http2::MAX_STREAMS`).
const STREAMS: usize = 32;

/// Gateway connections for a fixed number of concurrent forwards.
#[derive(Clone)]
pub(crate) struct ForwardConnections {
    /// What each connection opens with; none when every connection is a given client.
    config: Option<ClientConfig>,
    connections: Arc<[Connection]>,
}

struct Connection {
    client: OnceCell<OpenShellClient>,
    streams: Arc<Semaphore>,
}

/// One forward's stream on one of the connections; dropping it frees the stream.
pub(crate) struct Forward {
    connections: ForwardConnections,
    index: usize,
    _stream: OwnedSemaphorePermit,
}

impl ForwardConnections {
    /// Connections for `forwards` concurrent forwards, each opened with `config`.
    pub(crate) fn new(config: ClientConfig, forwards: NonZeroUsize) -> Self {
        Self::build(Some(config), None, forwards)
    }

    /// Connections that are all `client`, such as a test's fake.
    #[cfg(test)]
    pub(crate) fn of(client: OpenShellClient, forwards: NonZeroUsize) -> Self {
        Self::build(None, Some(client), forwards)
    }

    /// Connections for another number of forwards, opened the same way. Forwards in
    /// flight keep the connections they ride.
    pub(crate) fn with_limit(&self, forwards: NonZeroUsize) -> Self {
        let client = match &self.config {
            Some(_) => None,
            None => self.connections[0].client.get().cloned(),
        };
        Self::build(self.config.clone(), client, forwards)
    }

    fn build(
        config: Option<ClientConfig>,
        client: Option<OpenShellClient>,
        forwards: NonZeroUsize,
    ) -> Self {
        let forwards = forwards.get();
        let connections = (0..forwards.div_ceil(STREAMS))
            .map(|index| Connection {
                client: OnceCell::new_with(client.clone()),
                streams: Arc::new(Semaphore::new(STREAMS.min(forwards - index * STREAMS))),
            })
            .collect();
        Self {
            config,
            connections,
        }
    }

    /// A free stream on one of the connections, or none while every forward is in use.
    pub(crate) fn reserve(&self) -> Option<Forward> {
        self.connections
            .iter()
            .enumerate()
            .find_map(|(index, connection)| {
                let stream = connection.streams.clone().try_acquire_owned().ok()?;
                Some(Forward {
                    connections: self.clone(),
                    index,
                    _stream: stream,
                })
            })
    }
}

impl Forward {
    /// The client of the connection this forward rides, opened on its first forward.
    pub(crate) async fn client(&self) -> Result<OpenShellClient, SdkError> {
        let config = &self.connections.config;
        self.connections.connections[self.index]
            .client
            .get_or_try_init(|| async {
                match config {
                    Some(config) => OpenShellClient::connect(config.clone()).await,
                    None => Err(SdkError::invalid_config("no gateway to connect to")),
                }
            })
            .await
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_fill_connections_of_at_most_the_supervisors_streams() {
        let connections = ForwardConnections::new(
            ClientConfig::new("https://gateway.invalid".to_owned()),
            NonZeroUsize::new(70).unwrap(),
        );
        let mut forwards: Vec<Forward> = std::iter::from_fn(|| connections.reserve()).collect();
        let mut per_connection = [0; 3];
        for forward in &forwards {
            per_connection[forward.index] += 1;
        }
        assert_eq!(per_connection, [32, 32, 6]);
        assert!(
            connections.reserve().is_none(),
            "the limit refuses the next forward"
        );

        let freed = forwards
            .iter()
            .position(|forward| forward.index == 1)
            .unwrap();
        forwards.swap_remove(freed);
        assert_eq!(
            connections.reserve().unwrap().index,
            1,
            "a freed stream is reused"
        );
    }
}
