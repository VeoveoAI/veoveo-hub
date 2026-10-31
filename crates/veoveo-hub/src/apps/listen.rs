//! Live resources: a view's `subscriptions/listen`, forwarded as the person through the
//! Hub's own MCP endpoint, whose event stream the bridge relays to the host page with
//! each message's JSON text unchanged. A listen holds one place of its browser session's
//! ceiling while it lives, and ends when the session no longer admits or its bearer
//! expires.
use std::{
    collections::BTreeMap, convert::Infallible, num::NonZeroUsize, sync::Arc, time::Duration,
};

use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use chrono::{DateTime, Utc};
use futures::{StreamExt as _, future::BoxFuture};
use parking_lot::Mutex;
use serde_json::{Value, json};
use veoveo_hub_sdk::HubRefreshFamilyId;

use super::admission::INTERNAL_ERROR;

/// How often a comment keeps an open stream alive through proxies.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// The bounds of live resources.
#[derive(Debug, Clone, Copy)]
pub struct ListenLimits {
    /// The most open listens of one browser session on this replica.
    pub per_session: NonZeroUsize,
    /// How often an open listen reads its browser session again.
    pub recheck: Duration,
}

impl Default for ListenLimits {
    fn default() -> Self {
        Self {
            per_session: NonZeroUsize::new(8).expect("nonzero"),
            recheck: crate::computers::STREAM_ADMISSION_INTERVAL,
        }
    }
}

/// The open listens of each browser session on this replica.
#[derive(Debug, Default)]
pub(super) struct Listens {
    open: Mutex<BTreeMap<HubRefreshFamilyId, usize>>,
}

impl Listens {
    /// A place for one more listen of `family`'s session, or None at `limit`.
    pub(super) fn acquire(
        self: &Arc<Self>,
        family: &HubRefreshFamilyId,
        limit: NonZeroUsize,
    ) -> Option<ListenPermit> {
        let mut open = self.open.lock();
        let count = open.entry(family.clone()).or_default();
        if *count >= limit.get() {
            return None;
        }
        *count += 1;
        Some(ListenPermit {
            listens: self.clone(),
            family: family.clone(),
        })
    }

    #[cfg(test)]
    pub(super) fn open(&self, family: &HubRefreshFamilyId) -> usize {
        self.open.lock().get(family).copied().unwrap_or_default()
    }
}

/// One listen's place in its session's ceiling, given back when the listen ends.
#[derive(Debug)]
pub(super) struct ListenPermit {
    listens: Arc<Listens>,
    family: HubRefreshFamilyId,
}

impl Drop for ListenPermit {
    fn drop(&mut self) {
        let mut open = self.listens.open.lock();
        if let Some(count) = open.get_mut(&self.family) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.remove(&self.family);
            }
        }
    }
}

/// Why the bridge ended a listen the endpoint had not ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ended {
    /// The browser session no longer admits.
    Session,
    /// The listen's bearer expired.
    Expired,
    /// The endpoint's stream broke.
    Broken(String),
}

impl Ended {
    fn message(&self) -> &'static str {
        match self {
            Self::Session => "The browser session ended; sign in to listen again.",
            Self::Expired => "The listen's authorization expired; listen again to resume.",
            Self::Broken(_) => "The listen's stream broke; listen again to resume.",
        }
    }
}

/// One open listen to relay.
pub(super) struct Relay {
    /// The view's request `id`, which every message of the stream names.
    pub(super) id: Value,
    /// The App, for logs.
    pub(super) app: String,
    /// The endpoint's event stream.
    pub(super) body: Body,
    pub(super) permit: ListenPermit,
    /// When the listen's bearer expires.
    pub(super) expires_at: DateTime<Utc>,
    pub(super) recheck: Duration,
    /// Whether the browser session still admits, read again under its own bound.
    pub(super) admitted: Arc<dyn Fn() -> BoxFuture<'static, bool> + Send + Sync>,
}

/// The listen's stream for the host page: each message of the endpoint's stream as one
/// event whose data is its JSON text, then, when the bridge ends the listen first, one
/// final JSON-RPC error that names the listen and why. Dropping the stream, as when the
/// page aborts its fetch, drops the endpoint's stream, which ends the listen and its
/// upstream listens.
pub(super) fn relay(relay: Relay) -> Response {
    let Relay {
        id,
        app,
        body,
        permit,
        expires_at,
        recheck,
        admitted,
    } = relay;
    let lifetime = (expires_at - Utc::now()).to_std().unwrap_or_default();
    let events = async_stream::stream! {
        let _permit = permit;
        let mut messages = sse_stream::SseStream::new(body);
        let deadline = tokio::time::sleep(lifetime);
        tokio::pin!(deadline);
        let mut checks = tokio::time::interval_at(tokio::time::Instant::now() + recheck, recheck);
        checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let ended = loop {
            tokio::select! {
                message = messages.next() => match message {
                    Some(Ok(message)) => {
                        if let Some(data) = message.data {
                            yield Ok::<Event, Infallible>(Event::default().data(data));
                        }
                    }
                    Some(Err(error)) => break Some(Ended::Broken(error.to_string())),
                    None => break None,
                },
                () = &mut deadline => break Some(Ended::Expired),
                _ = checks.tick() => {
                    if !admitted().await {
                        break Some(Ended::Session);
                    }
                }
            }
        };
        // The endpoint's stream goes first: dropping it ends the listen upstream.
        drop(messages);
        match &ended {
            None => tracing::debug!(app, "App listen completed"),
            Some(Ended::Broken(cause)) => tracing::warn!(app, %cause, "App listen stream broke"),
            Some(reason) => tracing::info!(app, ?reason, "App listen ended by the bridge"),
        }
        if let Some(ended) = ended {
            let error = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": INTERNAL_ERROR, "message": ended.message()}
            });
            yield Ok(Event::default().data(error.to_string()));
        }
    };
    let mut response = Sse::new(events)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_holds_at_most_its_ceiling_of_listens() {
        let listens = Arc::new(Listens::default());
        let family = HubRefreshFamilyId::parse(uuid::Uuid::now_v7().to_string()).unwrap();
        let other = HubRefreshFamilyId::parse(uuid::Uuid::now_v7().to_string()).unwrap();
        let limit = NonZeroUsize::new(2).unwrap();
        let first = listens.acquire(&family, limit).unwrap();
        let second = listens.acquire(&family, limit).unwrap();
        assert!(
            listens.acquire(&family, limit).is_none(),
            "the ceiling refuses a third"
        );
        let elsewhere = listens.acquire(&other, limit).unwrap();
        assert_eq!(listens.open(&family), 2);
        drop(first);
        assert_eq!(listens.open(&family), 1);
        let third = listens.acquire(&family, limit).unwrap();
        drop((second, third, elsewhere));
        assert_eq!(listens.open(&family), 0);
        assert!(
            listens.open.lock().is_empty(),
            "an ended session leaves no entry"
        );
    }
}
