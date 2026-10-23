//! Factory sandboxes from Lore. A person turns a repository on with `watch`; the Hub
//! then subscribes to the repository's Lore notifications as that person. Each push of a
//! branch is prepared as its pusher: the Hub reads `openshell/` at the pushed revision and
//! applies it to the branch's instance, or removes the instance when the revision has no
//! `openshell/run`. Lore checks access only when a subscription starts and drops nothing
//! silently, ending a lagging stream instead, so on every subscription the Hub re-checks
//! every branch: it applies each latest revision it has not applied and removes the
//! instances of deleted branches. Hub acts in Lore only as a person, under that person's
//! current grant.
pub(crate) mod native;
mod read;
mod store;
pub(crate) mod watch;

use std::{future::Future, path::Path, pin::Pin};

use anyhow::Result;
use futures::stream::BoxStream;
use lore_proto::lore::notification::Event;
use veoveo_hub::factory::{Prepared, Revision};
use veoveo_types::PrincipalId;

use super::{DeploymentId, RepositoryId};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A repository of a Lore deployment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Watched {
    pub deployment: DeploymentId,
    pub repository: RepositoryId,
}

/// A branch of a repository as Lore reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Branch {
    pub id: super::BranchId,
    pub name: String,
    pub latest: Revision,
    /// Deleted or archived; Lore keeps the record of either.
    pub deleted: bool,
}

/// Lore refused the person, or the Hub issues the person no Lore token for the
/// repository: they hold no grant on it.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct Refused(pub String);

/// A repository's notifications, as one subscription receives them.
pub(crate) type Events = BoxStream<'static, Result<Event, tonic::Status>>;

/// What the Hub does in Lore, each time as `person` under their current grant.
pub(crate) trait Lore: Send + Sync + 'static {
    /// The repository of `id` that the deployment registers for the tenant.
    fn locate(&self, id: &RepositoryId, tenant: &veoveo_types::TenantId) -> Option<Watched>;
    /// Subscribe to the repository's notifications; the stream is open once this
    /// returns.
    fn subscribe<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
    ) -> BoxFuture<'a, Result<Events>>;
    /// Every branch of the repository, deleted ones included.
    fn branches<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
    ) -> BoxFuture<'a, Result<Vec<Branch>>>;
    fn branch<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        id: &'a super::BranchId,
    ) -> BoxFuture<'a, Result<Branch>>;
    /// Who committed the revision.
    fn committer<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<PrincipalId>>;
    /// Read `openshell/` at the revision into `directory`; none when the revision has no
    /// executable `openshell/run`.
    fn read<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
        directory: &'a Path,
    ) -> BoxFuture<'a, Result<Option<Prepared>>>;
}

#[cfg(test)]
mod tests;
