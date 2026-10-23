//! Watching repositories: `PUT /factory/{profile}/watch?repository=<id>` turns a
//! repository on as the caller, and one follower per watched repository subscribes as
//! its watcher, re-checks every branch on each subscription and applies each push as its
//! pusher.
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::{
    Extension, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::put,
};
use futures::StreamExt;
use lore_proto::lore::notification::{Event, event::Event as Notification};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use veoveo_hub::{
    AuthenticatedSubject,
    execution::BranchName,
    factory::{Factory, Prepared, Push, Revision},
    problem::Problem,
};
use veoveo_store::PlatformStore;
use veoveo_types::PrincipalId;

use super::{Lore, Refused, Watched, store::WatchStore};
use crate::contributions::lore::{BranchId, RepositoryId};

/// How long a follower waits before it subscribes again after a subscription ended.
const RESUBSCRIBE: Duration = Duration::from_secs(5);

/// Held while a push is applied to an instance.
type InstanceLock = Arc<tokio::sync::Mutex<()>>;

/// The watched repositories of this Hub and their followers.
#[derive(Clone)]
pub(crate) struct Watches {
    inner: Arc<Inner>,
}

struct Inner {
    lore: Arc<dyn Lore>,
    factory: Factory,
    store: WatchStore,
    resubscribe: Duration,
    stop: CancellationToken,
    followers: std::sync::Mutex<HashMap<Watched, CancellationToken>>,
    /// One lock per instance: the factory applies one push to an instance at a time.
    instances: std::sync::Mutex<HashMap<(RepositoryId, BranchName), InstanceLock>>,
}

impl Watches {
    /// Watches that stop following when `stop` fires.
    pub fn new(
        lore: Arc<dyn Lore>,
        factory: Factory,
        store: PlatformStore,
        stop: CancellationToken,
    ) -> Self {
        Self::with_resubscribe(lore, factory, store, stop, RESUBSCRIBE)
    }

    pub(super) fn with_resubscribe(
        lore: Arc<dyn Lore>,
        factory: Factory,
        store: PlatformStore,
        stop: CancellationToken,
        resubscribe: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                lore,
                factory,
                store: WatchStore::new(store),
                resubscribe,
                stop,
                followers: Default::default(),
                instances: Default::default(),
            }),
        }
    }

    #[cfg(test)]
    pub(super) fn store(&self) -> &WatchStore {
        &self.inner.store
    }

    /// What watching knows about a branch of the repository that the caller's tenant
    /// registers: who watches it and why the last subscription as them failed, and why
    /// the branch's latest push could not be prepared.
    pub async fn report(
        &self,
        caller: &veoveo_hub_sdk::Principal,
        repository: &RepositoryId,
        branch: &BranchName,
    ) -> Result<Report> {
        let Some(watched) = caller
            .tenant
            .as_ref()
            .and_then(|tenant| self.inner.lore.locate(repository, tenant))
        else {
            return Ok(Report::default());
        };
        Ok(Report {
            watch: self.inner.store.status(&watched).await?,
            prepare_failure: self.inner.store.prepare_failure(&watched, branch).await?,
        })
    }

    /// Follow every watched repository as its recorded watcher.
    pub async fn resume(&self) -> Result<()> {
        for (watched, watcher) in self.inner.store.watches().await? {
            self.follow(watched, watcher, None);
        }
        Ok(())
    }

    /// Turn the repository on as `watcher`: subscribe as them now, which fails with
    /// [`Refused`] when they hold no grant on it, then follow it as them.
    pub async fn watch(&self, watched: Watched, watcher: PrincipalId) -> Result<()> {
        let events = self.inner.lore.subscribe(&watcher, &watched).await?;
        self.inner.store.watch(&watched, &watcher).await?;
        self.follow(watched, watcher, Some(events));
        Ok(())
    }

    /// Follow the repository as `watcher`, replacing its earlier follower.
    fn follow(&self, watched: Watched, watcher: PrincipalId, events: Option<super::Events>) {
        let stop = self.inner.stop.child_token();
        if let Some(previous) = self
            .inner
            .followers
            .lock()
            .expect("the followers lock")
            .insert(watched.clone(), stop.clone())
        {
            previous.cancel();
        }
        let inner = self.inner.clone();
        tokio::spawn(async move { inner.follow(&watched, &watcher, events, &stop).await });
    }

    pub fn router(self, auth: veoveo_hub::http::ProfileAuthState) -> Router {
        Router::new()
            .route("/factory/{profile}/watch", put(watch))
            .with_state(self)
            .layer(axum::middleware::from_fn_with_state(
                auth,
                veoveo_hub::http::authenticate_profile,
            ))
    }
}

impl Inner {
    /// Subscribe as the watcher, again whenever a subscription ends, until Lore refuses
    /// them or `stop` fires. Every subscription starts with a re-check of every branch,
    /// which catches up on whatever the gap between subscriptions missed. `stop` ends the
    /// follower between pushes, never while it applies one.
    async fn follow(
        &self,
        watched: &Watched,
        watcher: &PrincipalId,
        events: Option<super::Events>,
        stop: &CancellationToken,
    ) {
        let mut events = events;
        loop {
            let subscribed = match events.take() {
                Some(events) => Ok(events),
                None => tokio::select! {
                    () = stop.cancelled() => return,
                    subscribed = self.lore.subscribe(watcher, watched) => subscribed,
                },
            };
            let ended = match subscribed {
                Ok(events) => self.receive(watched, watcher, events, stop).await,
                Err(error) => Err(error),
            };
            if stop.is_cancelled() {
                return;
            }
            if let Err(error) = ended {
                let message = format!("{error:#}");
                tracing::warn!(repository = %watched.repository, %watcher, "watching failed: {message}");
                if let Err(error) = self
                    .store
                    .subscribed(watched, watcher, Some(&message))
                    .await
                {
                    tracing::error!("failed to record the watch's failure: {error:#}");
                }
                // Lore admitted the watcher no longer; anyone with access watches again.
                if error.is::<Refused>() {
                    return;
                }
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.resubscribe) => {}
            }
        }
    }

    /// Re-check every branch, then handle each event until the stream ends. Lore ends a
    /// lagging subscriber's stream with `ABORTED`; the next subscription re-checks.
    async fn receive(
        &self,
        watched: &Watched,
        watcher: &PrincipalId,
        mut events: super::Events,
        stop: &CancellationToken,
    ) -> Result<()> {
        self.store.subscribed(watched, watcher, None).await?;
        self.recheck(watched, watcher, stop).await?;
        loop {
            let event = tokio::select! {
                () = stop.cancelled() => return Ok(()),
                event = events.next() => event,
            };
            let Some(event) = event else {
                return Ok(());
            };
            match event {
                Ok(event) => self.handle(watched, watcher, event, stop).await?,
                Err(status) => {
                    tracing::info!(
                        repository = %watched.repository,
                        "Lore ended the subscription: {:?}: {}",
                        status.code(),
                        status.message()
                    );
                    return Ok(());
                }
            }
        }
    }

    async fn handle(
        &self,
        watched: &Watched,
        watcher: &PrincipalId,
        event: Event,
        stop: &CancellationToken,
    ) -> Result<()> {
        match event.event {
            Some(Notification::BranchPushed(pushed)) => {
                let (Ok(branch), Ok(revision), Ok(pusher)) = (
                    BranchId::parse(hex::encode(&pushed.branch)),
                    Revision::parse(hex::encode(&pushed.revision)),
                    PrincipalId::parse(pushed.user_id),
                ) else {
                    tracing::warn!(repository = %watched.repository, "Lore sent a malformed push");
                    return Ok(());
                };
                self.pushed(watched, &branch, revision, pusher, stop).await;
                Ok(())
            }
            Some(Notification::BranchCreated(_) | Notification::BranchDeleted(_)) => {
                self.recheck(watched, watcher, stop).await
            }
            _ => Ok(()),
        }
    }

    /// Apply a push as its pusher, who also names its branch.
    async fn pushed(
        &self,
        watched: &Watched,
        id: &BranchId,
        revision: Revision,
        pusher: PrincipalId,
        stop: &CancellationToken,
    ) {
        let branch = match self.lore.branch(&pusher, watched, id).await {
            Ok(branch) => branch,
            Err(error) => {
                tracing::warn!(repository = %watched.repository, branch = %id, "the pushed branch is unknown: {error:#}");
                return;
            }
        };
        let Ok(name) = BranchName::parse(&branch.name) else {
            tracing::warn!(repository = %watched.repository, branch = %branch.name, "the branch's name cannot name an instance");
            return;
        };
        if branch.deleted {
            self.remove(watched, &name, stop).await;
        } else {
            self.prepare(watched, &name, revision, pusher, stop).await;
        }
    }

    /// Serve each live branch's latest revision that its instance has not applied, read
    /// as the revision's committer, and remove the instances of deleted branches whose
    /// name no live branch took.
    async fn recheck(
        &self,
        watched: &Watched,
        watcher: &PrincipalId,
        stop: &CancellationToken,
    ) -> Result<()> {
        let branches = self
            .lore
            .branches(watcher, watched)
            .await
            .context("re-checking the repository's branches failed")?;
        let live = branches
            .iter()
            .filter(|branch| !branch.deleted)
            .map(|branch| branch.name.clone())
            .collect::<std::collections::HashSet<_>>();
        for branch in branches {
            if stop.is_cancelled() {
                break;
            }
            let Ok(name) = BranchName::parse(&branch.name) else {
                continue;
            };
            if branch.deleted {
                if !live.contains(&branch.name) {
                    self.remove(watched, &name, stop).await;
                }
                continue;
            }
            let applied = self
                .factory
                .status(watched.repository.as_str(), &name, Duration::ZERO)
                .await
                .map(|status| status.and_then(|status| status.applied_revision));
            match applied {
                Ok(Some(applied)) if applied == branch.latest => continue,
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(branch = %name, "the branch's instance is unknown: {error:#}");
                    continue;
                }
            }
            let pusher = match self.lore.committer(watcher, watched, &branch.latest).await {
                Ok(pusher) => pusher,
                Err(error) => {
                    let message = format!("the latest revision's committer is unknown: {error:#}");
                    self.unprepared(watched, &name, &branch.latest, watcher, &message)
                        .await;
                    continue;
                }
            };
            self.prepare(watched, &name, branch.latest, pusher, stop)
                .await;
        }
        Ok(())
    }

    /// Read `openshell/` at the revision as the pusher, then apply it, or remove the
    /// branch's instance when the revision has no `openshell/run`. A failed read leaves
    /// the instance as it serves and is recorded for the branch.
    async fn prepare(
        &self,
        watched: &Watched,
        branch: &BranchName,
        revision: Revision,
        pusher: PrincipalId,
        stop: &CancellationToken,
    ) {
        let read = async {
            let directory = tempfile::Builder::new()
                .prefix("veoveo-factory-")
                .tempdir()
                .context("failed to create the push's directory")?;
            let prepared = self
                .lore
                .read(&pusher, watched, &revision, directory.path())
                .await?;
            anyhow::Ok((directory, prepared))
        };
        let (directory, prepared) = match read.await {
            Ok(read) => read,
            Err(error) => {
                let message = format!("reading openshell/ as {pusher} failed: {error:#}");
                self.unprepared(watched, branch, &revision, &pusher, &message)
                    .await;
                return;
            }
        };
        if let Err(error) = self.store.prepared(watched, branch, None).await {
            tracing::error!("failed to record the preparation: {error:#}");
        }
        let prepared = prepared.map(|prepared| (Push { revision, pusher }, prepared));
        self.change(watched, branch, stop, prepared).await;
        drop(directory);
    }

    async fn remove(&self, watched: &Watched, branch: &BranchName, stop: &CancellationToken) {
        self.change(watched, branch, stop, None).await;
    }

    /// Apply a prepared push to the branch's instance, or remove it, once no other push
    /// to it is being applied. A follower replaced meanwhile changes nothing: its
    /// successor re-checks the branch, so the latest revision wins.
    async fn change(
        &self,
        watched: &Watched,
        branch: &BranchName,
        stop: &CancellationToken,
        push: Option<(Push, Prepared)>,
    ) {
        let instance = self
            .instances
            .lock()
            .expect("the instances lock")
            .entry((watched.repository.clone(), branch.clone()))
            .or_default()
            .clone();
        let _applying = instance.lock().await;
        if stop.is_cancelled() {
            return;
        }
        let repository = watched.repository.as_str();
        let changed = match push {
            Some((push, prepared)) => self.factory.apply(repository, branch, push, prepared).await,
            None => self.factory.remove(repository, branch).await,
        };
        // The factory records why a step failed on the instance.
        if let Err(error) = changed {
            tracing::warn!(repository = %watched.repository, %branch, "the push was not applied: {error:#}");
        }
    }

    async fn unprepared(
        &self,
        watched: &Watched,
        branch: &BranchName,
        revision: &Revision,
        pusher: &PrincipalId,
        message: &str,
    ) {
        tracing::warn!(repository = %watched.repository, %branch, "{message}");
        if let Err(error) = self
            .store
            .prepared(watched, branch, Some((revision, pusher, message)))
            .await
        {
            tracing::error!("failed to record the failed preparation: {error:#}");
        }
    }
}

/// What watching knows about a branch, beside its instance's status.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    /// The repository's watcher and why the last subscription as them failed; none
    /// while nobody watches the repository.
    pub watch: Option<super::store::WatchStatus>,
    /// Why the Hub could not read the branch's latest push as its pusher.
    pub prepare_failure: Option<super::store::PrepareFailure>,
}

/// The query of `PUT /factory/{profile}/watch`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchQuery {
    repository: String,
}

/// Turn a repository on as the caller, who becomes its watcher: 204 once the Hub
/// watches it as them, 403 when they hold no grant on it.
async fn watch(
    State(watches): State<Watches>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(query): Query<WatchQuery>,
) -> Response {
    watches.answer(&subject.principal, query.repository).await
}

impl Watches {
    pub(super) async fn answer(
        &self,
        caller: &veoveo_hub_sdk::Principal,
        repository: String,
    ) -> Response {
        let Ok(repository) = RepositoryId::parse(repository) else {
            return Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid-repository",
                "Invalid repository",
            )
            .detail("the repository is not a Lore repository ID")
            .into_response();
        };
        let forbidden = |detail: String| {
            Problem::new(StatusCode::FORBIDDEN, "watch-forbidden", "Watch forbidden")
                .detail(detail)
                .into_response()
        };
        let Some(watched) = caller
            .tenant
            .as_ref()
            .and_then(|tenant| self.inner.lore.locate(&repository, tenant))
        else {
            return forbidden(format!(
                "the caller's tenant has no Lore repository {repository}"
            ));
        };
        match self.watch(watched, caller.id.clone()).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => match error.downcast::<Refused>() {
                Ok(Refused(detail)) => forbidden(detail),
                Err(error) => {
                    tracing::warn!("watching failed: {error:#}");
                    Problem::new(
                        StatusCode::BAD_GATEWAY,
                        "lore-unavailable",
                        "Lore unavailable",
                    )
                    .detail(format!("{error:#}"))
                    .into_response()
                }
            },
        }
    }
}
