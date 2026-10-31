//! The gateway connections ForwardTcp streams ride. A forward holds one HTTP/2 stream
//! for the whole life of the connection it carries, and the Hub sandbox's supervisor
//! admits at most [`STREAMS`] concurrent streams on one connection to the gateway: a
//! stream beyond them waits, unannounced, until another ends. Forwards therefore ride
//! connections of their own, at most that many on each, each opened at its first
//! forward and kept.
use std::{num::NonZeroUsize, sync::Arc, time::Duration};

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
    /// One place per forward, taken before a connection's stream, so forwards waiting
    /// for one are served in turn.
    places: Arc<Semaphore>,
}

struct Connection {
    client: OnceCell<OpenShellClient>,
    streams: Arc<Semaphore>,
}

/// One forward's stream on one of the connections; dropping it frees the stream.
pub(crate) struct Forward {
    connections: ForwardConnections,
    index: usize,
    // Fields drop in order: the stream before the place, so whoever takes the freed
    // place finds a free stream.
    _stream: OwnedSemaphorePermit,
    _place: OwnedSemaphorePermit,
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
            places: Arc::new(Semaphore::new(forwards)),
        }
    }

    /// A free stream on one of the connections, or none while every forward is in use.
    pub(crate) fn reserve(&self) -> Option<Forward> {
        let place = self.places.clone().try_acquire_owned().ok()?;
        self.stream(place)
    }

    /// A free stream, waiting up to `wait` for a forward to end while every one is in
    /// use.
    pub(crate) async fn reserve_within(&self, wait: Duration) -> Option<Forward> {
        let place = tokio::time::timeout(wait, self.places.clone().acquire_owned())
            .await
            .ok()?
            .ok()?;
        self.stream(place)
    }

    fn stream(&self, place: OwnedSemaphorePermit) -> Option<Forward> {
        let (index, stream) =
            self.connections
                .iter()
                .enumerate()
                .find_map(|(index, connection)| {
                    Some((index, connection.streams.clone().try_acquire_owned().ok()?))
                })?;
        Some(Forward {
            connections: self.clone(),
            index,
            _stream: stream,
            _place: place,
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

    #[tokio::test]
    async fn a_waiting_forward_takes_the_next_one_that_ends() {
        let connections = ForwardConnections::new(
            ClientConfig::new("https://gateway.invalid".to_owned()),
            NonZeroUsize::new(2).unwrap(),
        );
        let first = connections.reserve().unwrap();
        let _second = connections.reserve().unwrap();
        assert!(
            connections
                .reserve_within(Duration::from_millis(10))
                .await
                .is_none(),
            "no forward ended within the wait"
        );
        let waiting = tokio::spawn({
            let connections = connections.clone();
            async move {
                connections
                    .reserve_within(Duration::from_secs(5))
                    .await
                    .map(|forward| forward.index)
            }
        });
        tokio::task::yield_now().await;
        let index = first.index;
        drop(first);
        assert_eq!(waiting.await.unwrap(), Some(index));
    }
}
