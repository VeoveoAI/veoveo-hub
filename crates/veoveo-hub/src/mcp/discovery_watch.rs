//! Cache validity follows an authenticated catalog stream and the authority epoch.
use super::{HubMcp, discovery::DiscoveryCacheKey};
use crate::{
    AuthenticatedSubject,
    mcp_support::{mcp_internal, upstream_error},
};
use rmcp::{
    model::{ErrorData as McpError, ServerNotification, SubscriptionFilter},
    service::{Peer, RoleClient, RoleServer, Subscription},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, OnceCell};
use tokio_util::sync::CancellationToken;
use veoveo_hub_sdk::ServerSlug;

const MAX_WATCHES: usize = 256;
const OPEN_DEADLINE: Duration = Duration::from_secs(10);
#[derive(Debug, Default)]
pub(super) struct DiscoveryWatches {
    watches: Mutex<BTreeMap<DiscoveryCacheKey, Arc<OnceCell<Watch>>>>,
    /// Servers whose discovered capabilities advertise no catalog change while they
    /// run, by catalog generation. Their discovery needs no watch.
    unchanging: std::sync::Mutex<BTreeSet<(u64, ServerSlug)>>,
}
impl DiscoveryWatches {
    pub(super) fn unchanging(&self, generation: u64, server: &ServerSlug) -> bool {
        self.unchanging
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&(generation, server.clone()))
    }
    /// Remember that `server` advertises no catalog change in `generation`, forgetting
    /// earlier generations.
    pub(super) fn learned_unchanging(&self, generation: u64, server: &ServerSlug) {
        let mut unchanging = self
            .unchanging
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unchanging.retain(|(known, _)| *known >= generation);
        unchanging.insert((generation, server.clone()));
    }
}
#[derive(Debug)]
struct Watch {
    cancelled: CancellationToken,
    /// The server advertises no change its filter asks for: nothing is watched.
    unchanging: bool,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.cancelled.cancel();
    }
}
/// Registered servers change their tools, resources and prompts by registering again,
/// which starts a new catalog generation and empties the discovery cache. A server whose
/// tool descriptions change while it runs advertises `tools.listChanged` and tells its
/// listeners, so every server with tools is asked for its tool-list changes; one whose
/// discovered capabilities do not advertise them is not watched.
pub(super) fn catalog_change_filter(
    capabilities: &crate::registration::ServerCapabilities,
) -> SubscriptionFilter {
    let mut filter = SubscriptionFilter::new();
    if capabilities.tools {
        filter.tools_list_changed = Some(true);
    }
    filter
}

impl HubMcp {
    pub(super) async fn discovery_watch_active(&self, key: &DiscoveryCacheKey) -> bool {
        let snapshot = self.catalog.snapshot();
        if snapshot.generation() != key.catalog_generation {
            return false;
        }
        let Some((_, manifest)) = snapshot
            .catalog()
            .profile_server(&self.profile_id, &key.server)
        else {
            return false;
        };
        let filter = catalog_change_filter(&manifest.capabilities);
        if filter.tools_list_changed.is_none()
            && filter.resources_list_changed.is_none()
            && filter.prompts_list_changed.is_none()
        {
            return true;
        }
        if self
            .discovery_watches
            .unchanging(key.catalog_generation, &key.server)
        {
            return true;
        }
        self.discovery_watches
            .watches
            .lock()
            .await
            .get(key)
            .and_then(|cell| cell.get())
            .is_some_and(|watch| !watch.cancelled.is_cancelled())
    }
    pub(super) async fn ensure_discovery_watch(
        &self,
        key: &DiscoveryCacheKey,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<(), McpError> {
        let snapshot = self.catalog.snapshot();
        if snapshot.generation() != key.catalog_generation {
            return Err(mcp_internal("catalog changed during discovery; retry"));
        }
        let (_, manifest) = snapshot
            .catalog()
            .profile_server(&self.profile_id, &key.server)
            .ok_or_else(|| mcp_internal("unknown catalog server"))?;
        let filter = catalog_change_filter(&manifest.capabilities);
        if filter.tools_list_changed.is_none()
            && filter.resources_list_changed.is_none()
            && filter.prompts_list_changed.is_none()
        {
            return Ok(());
        }
        if self
            .discovery_watches
            .unchanging(key.catalog_generation, &key.server)
        {
            return Ok(());
        }
        let cell = {
            let mut watches = self.discovery_watches.watches.lock().await;
            watches.retain(|watch_key, cell| {
                watch_key.catalog_generation == key.catalog_generation
                    && cell
                        .get()
                        .is_none_or(|watch| !watch.cancelled.is_cancelled())
            });
            if let Some(cell) = watches.get(key) {
                cell.clone()
            } else {
                if watches.len() >= MAX_WATCHES {
                    return Err(mcp_internal("catalog subscription capacity reached"));
                }
                let cell = Arc::new(OnceCell::new());
                watches.insert(key.clone(), cell.clone());
                cell
            }
        };
        let initialized = cell.get_or_try_init(|| async {
            let remaining = (subject.access_token.expires_at - chrono::Utc::now()).to_std()
                .map_err(|_| mcp_internal("catalog authority expired"))?;
            let deadline = tokio::time::Instant::now() + remaining;
            let mut generation = self.catalog.subscribe();
            let (connection, subscription) = tokio::time::timeout(OPEN_DEADLINE, async {
                let connection = self.upstream(&key.server, downstream, subject).await.map_err(super::http_response::RequestError::into_protocol)?;
                // Ask only for the changes the server advertises.
                let effective = connection
                    .peer
                    .peer_info()
                    .map(|info| filter.supported_by(&info.capabilities))
                    .unwrap_or_default();
                if effective == SubscriptionFilter::new() {
                    return Ok::<_, McpError>((connection, None));
                }
                let subscription = open_catalog_subscription(&connection.peer, &effective).await?;
                Ok((connection, Some(subscription)))
            }).await.map_err(|_| mcp_internal("catalog subscription open deadline exceeded"))??;
            let Some(mut subscription) = subscription else {
                return Ok(Watch { cancelled: CancellationToken::new(), unchanging: true });
            };
            if *generation.borrow_and_update() != key.catalog_generation { return Err(mcp_internal("catalog changed while opening subscription")); }
            // A new stream cannot recover notifications from before registration.
            // Rebuild this key after the stream is listening, closing that gap.
            self.discovery.invalidate_cached_key(key).await;
            let cancelled = CancellationToken::new();
            let stop = cancelled.clone();
            let cache = Arc::downgrade(&self.discovery);
            let key = key.clone();
            tokio::spawn(async move {
                let _connection = connection;
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => break,
                        _ = tokio::time::sleep_until(deadline) => break,
                        _ = generation.changed() => break,
                        next = subscription.next() => {
                            let Some(cache) = cache.upgrade() else { break; };
                            match next {
                                Ok(Some(ServerNotification::ToolListChangedNotification(_))) => cache.invalidate_tools(&key.server).await,
                                Ok(Some(ServerNotification::ResourceListChangedNotification(_))) => cache.invalidate_resource_surfaces(&key.server).await,
                                Ok(Some(ServerNotification::PromptListChangedNotification(_))) => cache.invalidate_prompts(&key.server).await,
                                Ok(Some(_)) => {},
                                Ok(None) | Err(_) => break,
                            }
                        }
                    }
                }
                if let Some(cache) = cache.upgrade() { cache.invalidate_key(&key).await; }
                stop.cancel();
            });
            Ok::<_,McpError>(Watch { cancelled, unchanging: false })
        }).await;
        if let Ok(watch) = &initialized
            && watch.unchanging
        {
            self.discovery_watches
                .learned_unchanging(key.catalog_generation, &key.server);
            let mut watches = self.discovery_watches.watches.lock().await;
            if watches
                .get(key)
                .is_some_and(|current| Arc::ptr_eq(current, &cell))
            {
                watches.remove(key);
            }
            return Ok(());
        }
        if let Err(error) = initialized {
            let mut watches = self.discovery_watches.watches.lock().await;
            if watches
                .get(key)
                .is_some_and(|current| Arc::ptr_eq(current, &cell))
            {
                watches.remove(key);
            }
            return Err(error);
        }
        if initialized.unwrap().cancelled.is_cancelled() {
            return Err(mcp_internal("catalog subscription ended; retry"));
        }
        Ok(())
    }
}

pub(super) async fn open_catalog_subscription(
    peer: &Peer<RoleClient>,
    filter: &SubscriptionFilter,
) -> Result<Subscription, McpError> {
    let subscription = peer.listen(filter.clone()).await.map_err(upstream_error)?;
    if subscription.acknowledged() != filter {
        return Err(mcp_internal("upstream narrowed its catalog subscription"));
    }
    Ok(subscription)
}
