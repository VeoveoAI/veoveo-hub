//! Watching repositories: `PUT /factory/{profile}/watch?repository=<id>` turns a
//! repository on as the caller, and one follower per watched repository subscribes as
//! its watcher, re-checks every branch on each subscription and applies each push as its
//! pusher. A branch whose revision holds links at its top level is served by one runner
//! per link; a push to a linked repository's tracked branch applies to its runners.
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
    factory::{
        Factory, Link, LinkedPrepared, Prepared, Push, Revision, Runner, RunnerPush, TrackedBranch,
    },
    problem::Problem,
};
use veoveo_hub_sdk::hub::LinkName;
use veoveo_store::PlatformStore;
use veoveo_types::PrincipalId;

use super::{LinkNode, Lore, Refused, Watched, store::WatchStore};
use crate::contributions::lore::{BranchId, RepositoryId};

/// How long a follower waits before it subscribes again after a subscription ended.
const RESUBSCRIBE: Duration = Duration::from_secs(5);

/// Held while a push is applied to an instance.
type InstanceLock = Arc<tokio::sync::Mutex<()>>;

/// An instance: a branch's ordinary one, or the runner of one of its links.
type InstanceKey = (RepositoryId, BranchName, Option<LinkName>);

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
    instances: std::sync::Mutex<HashMap<InstanceKey, InstanceLock>>,
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
    /// the branch's latest push could not be prepared; for a link of the branch, also who
    /// watches its linked repository and why the Hub could not run it.
    pub async fn report(
        &self,
        caller: &veoveo_hub_sdk::Principal,
        repository: &RepositoryId,
        branch: &BranchName,
        link: Option<&LinkName>,
    ) -> Result<Report> {
        let Some(watched) = caller
            .tenant
            .as_ref()
            .and_then(|tenant| self.inner.lore.locate(repository, tenant))
        else {
            return Ok(Report::default());
        };
        let runner_failure = match link {
            Some(link) => {
                self.inner
                    .store
                    .runner_failure(&watched, branch, link)
                    .await?
            }
            None => None,
        };
        Ok(Report {
            watch: self.inner.store.status(&watched).await?,
            prepare_failure: self.inner.store.prepare_failure(&watched, branch).await?,
            runner_failure,
        })
    }

    /// The watch of a linked repository of the caller's tenant's Lore deployment.
    pub async fn linked_watch(
        &self,
        caller: &veoveo_hub_sdk::Principal,
        parent: &RepositoryId,
        linked: &RepositoryId,
    ) -> Result<Option<super::store::WatchStatus>> {
        let Some(watched) = caller
            .tenant
            .as_ref()
            .and_then(|tenant| self.inner.lore.locate(parent, tenant))
        else {
            return Ok(None);
        };
        self.inner
            .store
            .status(&Watched {
                deployment: watched.deployment,
                repository: linked.clone(),
            })
            .await
    }

    /// The runner failure of a link of a branch of the caller's tenant's repository,
    /// which names the linked repository a runner that was never created would serve.
    pub async fn runner_failure(
        &self,
        caller: &veoveo_hub_sdk::Principal,
        repository: &RepositoryId,
        branch: &BranchName,
        link: &LinkName,
    ) -> Result<Option<super::store::RunnerFailure>> {
        let Some(watched) = caller
            .tenant
            .as_ref()
            .and_then(|tenant| self.inner.lore.locate(repository, tenant))
        else {
            return Ok(None);
        };
        self.inner
            .store
            .runner_failure(&watched, branch, link)
            .await
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
                self.pushed(watched, &branch, revision.clone(), pusher.clone(), stop)
                    .await;
                self.linked_pushed(watched, &branch, revision, pusher, stop)
                    .await;
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
            self.prepare(watched, &branch.id, &name, revision, pusher, stop)
                .await;
        }
    }

    /// Apply a push to a tracked branch of `linked` to each runner whose link tracks it,
    /// as the push's pusher.
    async fn linked_pushed(
        &self,
        linked: &Watched,
        id: &BranchId,
        revision: Revision,
        pusher: PrincipalId,
        stop: &CancellationToken,
    ) {
        let runners = match self.factory.tracking(linked.repository.as_str()).await {
            Ok(runners) => runners,
            Err(error) => {
                tracing::warn!(repository = %linked.repository, "the linked repository's runners are unknown: {error:#}");
                return;
            }
        };
        for runner in runners
            .into_iter()
            .filter(|runner| runner.link.tracked.id == id.as_str())
        {
            if stop.is_cancelled() {
                return;
            }
            let push = Push {
                revision: revision.clone(),
                pusher: pusher.clone(),
            };
            self.reapply(linked, runner, Some(push), stop).await;
        }
        // A link that could not run on this repository's earlier content may run now.
        self.retry_waiting(linked, stop).await;
    }

    /// Prepare again every branch with a link that could not run on `linked`.
    async fn retry_waiting(&self, linked: &Watched, stop: &CancellationToken) {
        let failed = match self.store.runner_failures_naming(linked).await {
            Ok(failed) => failed,
            Err(error) => {
                tracing::warn!("the links waiting for this repository are unknown: {error:#}");
                return;
            }
        };
        for (parent, branch, revision) in failed {
            if stop.is_cancelled() {
                return;
            }
            self.retry(&parent, &branch, &revision, stop).await;
        }
    }

    /// Apply the runner's branch as it last applied it with the tracked branch at `linked`,
    /// a push to it, or as it last applied it too. The branch's `openshell/` is read again
    /// as its pusher and the tracked branch's as its own.
    async fn reapply(
        &self,
        linked: &Watched,
        runner: Runner,
        pushed: Option<Push>,
        stop: &CancellationToken,
    ) {
        let (Some(revision), Some(pusher)) = (runner.revision.clone(), runner.pusher.clone())
        else {
            return;
        };
        let Some(linked_push) = pushed.or_else(|| {
            runner
                .linked_revision
                .clone()
                .zip(runner.linked_pusher.clone())
                .map(|(revision, pusher)| Push { revision, pusher })
        }) else {
            return;
        };
        let Ok(parent_repository) = RepositoryId::parse(&runner.repository) else {
            return;
        };
        let parent = Watched {
            deployment: linked.deployment.clone(),
            repository: parent_repository,
        };
        let parent_revision = revision.clone();
        let applied = async {
            let directory = tempfile::Builder::new()
                .prefix("veoveo-factory-")
                .tempdir()
                .context("failed to create the push's directory")?;
            let prepared = self
                .lore
                .read(&pusher, &parent, &revision, directory.path())
                .await?
                .context("the branch's revision the runner applied has no openshell/run")?;
            let links = self.lore.links(&pusher, &parent, &revision).await?;
            let linked_directory = tempfile::Builder::new()
                .prefix("veoveo-linked-")
                .tempdir()
                .context("failed to create the linked push's directory")?;
            let linked_prepared = self
                .lore
                .read_linked(
                    &linked_push.pusher,
                    linked,
                    &linked_push.revision,
                    linked_directory.path(),
                )
                .await?;
            if let Some(policy) = &linked_prepared.policy {
                veoveo_hub::factory::check_linked_policy(policy)?;
            }
            let mut link = runner.link.clone();
            if let Ok(id) = BranchId::parse(&link.tracked.id)
                && let Ok(tracked) = self.lore.branch(&linked_push.pusher, linked, &id).await
            {
                link.tracked.name = tracked.name;
            }
            let push = RunnerPush {
                link,
                linked: linked_push,
                prepared: linked_prepared,
                links: links.into_iter().map(|link| link.name).collect(),
            };
            self.change_runner(
                &parent,
                &runner.branch,
                stop,
                Push { revision, pusher },
                prepared,
                push,
            )
            .await;
            drop((directory, linked_directory));
            anyhow::Ok(())
        };
        let outcome = applied.await;
        if outcome.is_ok()
            && let Err(error) = self
                .store
                .runner(&parent, &runner.branch, &runner.link.name, None)
                .await
        {
            tracing::error!("failed to record the runner: {error:#}");
        }
        if let Err(error) = outcome {
            let message = format!("{error:#}");
            tracing::warn!(
                repository = %runner.repository,
                branch = %runner.branch,
                link = %runner.link.name,
                "the tracked branch was not applied to the runner: {message}"
            );
            // The runner keeps serving what it applied; its status says why the push did
            // not reach it.
            let failure = (&linked.repository, &parent_revision, message.as_str());
            if let Err(error) = self
                .store
                .runner(&parent, &runner.branch, &runner.link.name, Some(failure))
                .await
            {
                tracing::error!("failed to record the runner's failure: {error:#}");
            }
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
        for branch in branches.clone() {
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
            match self.applied(watched, &name, &branch.latest).await {
                Ok(true) => continue,
                Ok(false) => {}
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
            self.prepare(watched, &branch.id, &name, branch.latest, pusher, stop)
                .await;
        }
        self.recheck_runners(watched, watcher, &branches, stop)
            .await;
        Ok(())
    }

    /// Whether the branch serves `latest`: its instance applied it, or every runner of the
    /// branch did and no link of it failed.
    async fn applied(
        &self,
        watched: &Watched,
        branch: &BranchName,
        latest: &Revision,
    ) -> Result<bool> {
        let repository = watched.repository.as_str();
        if let Some(status) = self
            .factory
            .status(repository, branch, Duration::ZERO)
            .await?
        {
            return Ok(status.applied_revision.as_ref() == Some(latest));
        }
        let runners = self.factory.runners(repository, branch).await?;
        Ok(!runners.is_empty()
            && runners
                .iter()
                .all(|runner| runner.revision.as_ref() == Some(latest))
            && self
                .store
                .runner_failures(watched, branch)
                .await?
                .is_empty())
    }

    /// Bring each runner whose link tracks a branch of `watched` to that branch's latest
    /// revision, read as its committer, and retry the links that could not run because
    /// nobody watched this repository.
    async fn recheck_runners(
        &self,
        watched: &Watched,
        watcher: &PrincipalId,
        branches: &[super::Branch],
        stop: &CancellationToken,
    ) {
        let runners = match self.factory.tracking(watched.repository.as_str()).await {
            Ok(runners) => runners,
            Err(error) => {
                tracing::warn!(repository = %watched.repository, "the linked repository's runners are unknown: {error:#}");
                return;
            }
        };
        for runner in runners {
            if stop.is_cancelled() {
                return;
            }
            let Some(tracked) = branches
                .iter()
                .find(|branch| branch.id.as_str() == runner.link.tracked.id && !branch.deleted)
            else {
                continue;
            };
            if runner.linked_revision.as_ref() == Some(&tracked.latest) {
                continue;
            }
            let pusher = match self.lore.committer(watcher, watched, &tracked.latest).await {
                Ok(pusher) => pusher,
                Err(error) => {
                    tracing::warn!(repository = %watched.repository, "the tracked branch's committer is unknown: {error:#}");
                    continue;
                }
            };
            let push = Push {
                revision: tracked.latest.clone(),
                pusher,
            };
            self.reapply(watched, runner, Some(push), stop).await;
        }
        self.retry_waiting(watched, stop).await;
    }

    /// Prepare the parent's branch at `revision` again, as its committer read by the
    /// parent's watcher, once a link of it can run.
    async fn retry(
        &self,
        parent: &Watched,
        branch: &BranchName,
        revision: &Revision,
        stop: &CancellationToken,
    ) {
        let retried = async {
            let watcher = self
                .store
                .watcher(parent)
                .await?
                .context("nobody watches the parent repository")?;
            let id = self
                .lore
                .revision_branch(&watcher, parent, revision)
                .await?;
            let pusher = self.lore.committer(&watcher, parent, revision).await?;
            anyhow::Ok((id, pusher))
        };
        match retried.await {
            Ok((id, pusher)) => {
                self.prepare(parent, &id, branch, revision.clone(), pusher, stop)
                    .await;
            }
            Err(error) => {
                tracing::warn!(repository = %parent.repository, %branch, "the branch's links were not retried: {error:#}");
            }
        }
    }

    /// Read `openshell/` at the revision as the pusher, then apply it, or remove the
    /// branch's instance when the revision has no `openshell/run`. A revision whose top
    /// level holds links is applied to one runner per link instead of the instance. A
    /// failed read leaves the instance and the runners as they serve and is recorded for
    /// the branch.
    async fn prepare(
        &self,
        watched: &Watched,
        id: &BranchId,
        branch: &BranchName,
        revision: Revision,
        pusher: PrincipalId,
        stop: &CancellationToken,
    ) {
        let read = async {
            let directory = tempfile::Builder::new()
                .prefix("veoveo-factory-")
                .tempdir()
                .context("failed to create the push's directory")
                .map_err(|error| format!("{error:#}"))?;
            let prepared = self
                .lore
                .read(&pusher, watched, &revision, directory.path())
                .await
                .map_err(|error| format!("reading openshell/ as {pusher} failed: {error:#}"))?;
            // Links are checked whether or not the revision has `openshell/run`: one the
            // factory's checkout cannot leave out is refused either way.
            let links = self
                .lore
                .links(&pusher, watched, &revision)
                .await
                .map_err(|error| {
                    format!("reading the revision's links as {pusher} failed: {error:#}")
                })?;
            Ok::<_, String>((directory, prepared, links))
        };
        let (directory, prepared, links) = match read.await {
            Ok(read) => read,
            Err(message) => {
                self.unprepared(watched, branch, &revision, &pusher, &message)
                    .await;
                return;
            }
        };
        if let Err(error) = self.store.prepared(watched, branch, None).await {
            tracing::error!("failed to record the preparation: {error:#}");
        }
        let push = Push { revision, pusher };
        match prepared {
            Some(prepared) if !links.is_empty() => {
                // A linked branch: its runners serve it, not an instance.
                self.change(watched, branch, stop, None).await;
                self.run_links(watched, id, branch, &push, &prepared, &links, stop)
                    .await;
            }
            prepared => {
                self.retire_runners(watched, branch, &[], stop).await;
                let prepared = prepared.map(|prepared| (push, prepared));
                self.change(watched, branch, stop, prepared).await;
            }
        }
        drop(directory);
    }

    /// Apply the branch's push to the runner of each of its links, creating the runners of
    /// new links and removing those of links it no longer holds.
    #[allow(clippy::too_many_arguments)]
    async fn run_links(
        &self,
        watched: &Watched,
        id: &BranchId,
        branch: &BranchName,
        push: &Push,
        prepared: &Prepared,
        links: &[LinkNode],
        stop: &CancellationToken,
    ) {
        self.retire_runners(watched, branch, links, stop).await;
        let runners = self
            .factory
            .runners(watched.repository.as_str(), branch)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%branch, "the branch's runners are unknown: {error:#}");
                Vec::new()
            });
        let names = links
            .iter()
            .map(|link| link.name.clone())
            .collect::<Vec<_>>();
        for link in links {
            if stop.is_cancelled() {
                return;
            }
            let existing = runners.iter().find(|runner| runner.link.name == link.name);
            let linked = Watched {
                deployment: watched.deployment.clone(),
                repository: link.repository.clone(),
            };
            let resolved = self.resolve(&linked, id, link, existing).await;
            let failure = resolved.as_ref().err().map(|error| format!("{error:#}"));
            let runner = match resolved {
                Ok(runner) => Some(runner),
                // A runner whose tracked branch cannot be read now serves it as it last read
                // it, under the branch's new program.
                Err(_) => match existing {
                    Some(existing) => self.recorded(&linked, existing).await,
                    None => None,
                },
            };
            if let Some((link_record, linked_push, linked_prepared)) = runner {
                let runner_push = RunnerPush {
                    link: link_record,
                    linked: linked_push,
                    prepared: linked_prepared,
                    links: names.clone(),
                };
                self.change_runner(
                    watched,
                    branch,
                    stop,
                    push.clone(),
                    prepared.clone(),
                    runner_push,
                )
                .await;
            }
            let failure = failure
                .as_deref()
                .map(|message| (&link.repository, &push.revision, message));
            if let Some((_, _, message)) = failure {
                tracing::warn!(repository = %watched.repository, %branch, link = %link.name, "the link does not run: {message}");
            }
            if let Err(error) = self
                .store
                .runner(watched, branch, &link.name, failure)
                .await
            {
                tracing::error!("failed to record the runner: {error:#}");
            }
        }
    }

    /// What a runner of `link` serves of its linked repository, read as the linked
    /// repository's watcher: the branch the link tracks, its latest revision and that
    /// revision's committer, and what the revision's `openshell/` adds, read as them. A link
    /// that follows the parent's branch tracks the linked repository's branch of the
    /// parent branch's ID; one with a branch of its own the branch of its pinned revision.
    async fn resolve(
        &self,
        linked: &Watched,
        parent_branch: &BranchId,
        link: &LinkNode,
        existing: Option<&Runner>,
    ) -> Result<(Link, Push, LinkedPrepared)> {
        let watcher = self.store.watcher(linked).await?.with_context(|| {
            format!(
                "nobody watches the linked repository {}: someone with a grant on it turns \
                 it on with watch",
                linked.repository
            )
        })?;
        let tracked = if link.follows {
            parent_branch.clone()
        } else {
            self.lore
                .revision_branch(&watcher, linked, &link.pinned)
                .await
                .context("reading the branch of the link's pinned revision failed")?
        };
        let branch = self
            .lore
            .branch(&watcher, linked, &tracked)
            .await
            .context("reading the tracked branch failed")?;
        anyhow::ensure!(
            !branch.deleted,
            "the tracked branch {} of the linked repository is deleted",
            branch.name
        );
        let pusher = match existing {
            Some(runner) if runner.linked_revision.as_ref() == Some(&branch.latest) => {
                runner.linked_pusher.clone()
            }
            _ => None,
        };
        let pusher = match pusher {
            Some(pusher) => pusher,
            None => self
                .lore
                .committer(&watcher, linked, &branch.latest)
                .await
                .context("reading the tracked branch's committer failed")?,
        };
        let directory = tempfile::Builder::new()
            .prefix("veoveo-linked-")
            .tempdir()
            .context("failed to create the linked push's directory")?;
        let prepared = self
            .lore
            .read_linked(&pusher, linked, &branch.latest, directory.path())
            .await
            .with_context(|| {
                format!("reading the linked repository's openshell/ as {pusher} failed")
            })?;
        // The linked repository is content: its rules name no program of its own.
        if let Some(policy) = &prepared.policy {
            veoveo_hub::factory::check_linked_policy(policy)?;
        }
        Ok((
            Link {
                name: link.name.clone(),
                repository: linked.repository.to_string(),
                tracked: TrackedBranch {
                    id: tracked.to_string(),
                    name: branch.name,
                },
            },
            Push {
                revision: branch.latest,
                pusher,
            },
            prepared,
        ))
    }

    /// The tracked branch as the runner last applied it, read again as its pusher; none
    /// when it never applied one or that read fails.
    async fn recorded(
        &self,
        linked: &Watched,
        runner: &Runner,
    ) -> Option<(Link, Push, LinkedPrepared)> {
        let push = runner
            .linked_revision
            .clone()
            .zip(runner.linked_pusher.clone())
            .map(|(revision, pusher)| Push { revision, pusher })?;
        let directory = tempfile::Builder::new()
            .prefix("veoveo-linked-")
            .tempdir()
            .ok()?;
        let prepared = self
            .lore
            .read_linked(&push.pusher, linked, &push.revision, directory.path())
            .await
            .inspect_err(|error| {
                tracing::warn!(link = %runner.link.name, "the runner's tracked branch cannot be read again: {error:#}");
            })
            .ok()?;
        Some((runner.link.clone(), push, prepared))
    }

    /// Remove the runners of the branch whose links `keep` does not hold, with their
    /// failures.
    async fn retire_runners(
        &self,
        watched: &Watched,
        branch: &BranchName,
        keep: &[LinkNode],
        stop: &CancellationToken,
    ) {
        let kept = |name: &LinkName| keep.iter().any(|link| &link.name == name);
        match self
            .factory
            .runners(watched.repository.as_str(), branch)
            .await
        {
            Ok(runners) => {
                for runner in runners
                    .into_iter()
                    .filter(|runner| !kept(&runner.link.name))
                {
                    self.remove_runner(watched, branch, &runner.link.name, stop)
                        .await;
                }
            }
            Err(error) => {
                tracing::warn!(%branch, "the branch's runners are unknown: {error:#}");
            }
        }
        match self.store.runner_failures(watched, branch).await {
            Ok(failed) => {
                for link in failed.into_iter().filter(|link| !kept(link)) {
                    if let Err(error) = self.store.runner(watched, branch, &link, None).await {
                        tracing::error!("failed to clear the runner's failure: {error:#}");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%branch, "the branch's failed links are unknown: {error:#}");
            }
        }
    }

    async fn remove(&self, watched: &Watched, branch: &BranchName, stop: &CancellationToken) {
        self.change(watched, branch, stop, None).await;
        self.retire_runners(watched, branch, &[], stop).await;
    }

    /// The lock of one instance, which a push or removal holds while it changes it.
    fn lock(&self, key: InstanceKey) -> InstanceLock {
        self.instances
            .lock()
            .expect("the instances lock")
            .entry(key)
            .or_default()
            .clone()
    }

    /// Apply a runner's push once no other push to it is being applied.
    async fn change_runner(
        &self,
        watched: &Watched,
        branch: &BranchName,
        stop: &CancellationToken,
        push: Push,
        prepared: Prepared,
        runner: RunnerPush,
    ) {
        let instance = self.lock((
            watched.repository.clone(),
            branch.clone(),
            Some(runner.link.name.clone()),
        ));
        let _applying = instance.lock().await;
        if stop.is_cancelled() {
            return;
        }
        let link = runner.link.name.clone();
        if let Err(error) = self
            .factory
            .apply_runner(watched.repository.as_str(), branch, push, prepared, runner)
            .await
        {
            tracing::warn!(repository = %watched.repository, %branch, %link, "the push was not applied to the runner: {error:#}");
        }
    }

    async fn remove_runner(
        &self,
        watched: &Watched,
        branch: &BranchName,
        link: &LinkName,
        stop: &CancellationToken,
    ) {
        let instance = self.lock((
            watched.repository.clone(),
            branch.clone(),
            Some(link.clone()),
        ));
        let _applying = instance.lock().await;
        if stop.is_cancelled() {
            return;
        }
        if let Err(error) = self
            .factory
            .remove_runner(watched.repository.as_str(), branch, link)
            .await
        {
            tracing::warn!(repository = %watched.repository, %branch, %link, "the runner was not removed: {error:#}");
        }
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
        let instance = self.lock((watched.repository.clone(), branch.clone(), None));
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
    /// For a link of the branch, why the Hub could not run it: nobody watches its linked
    /// repository, the watcher's grant was refused, or its tracked branch could not be
    /// read. A runner that exists keeps serving as it last read the tracked branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_failure: Option<super::store::RunnerFailure>,
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
