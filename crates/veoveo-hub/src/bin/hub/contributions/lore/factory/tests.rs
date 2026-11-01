//! Watching against an in-process fake of Lore and the factory itself, on a fake
//! OpenShell gateway (`tests/factory/gateway.rs`), a fake image builder and a disposable
//! database with the Hub's migrations. The fake Lore admits only the people it is told
//! hold a grant, serves each subscription from a channel the test feeds and ends, and
//! writes a synthetic `openshell/` for each revision that has `run`. The fake builder's
//! digest is the SHA-256 of a `Dockerfile`'s text. Data-environment databases are named
//! by branch across the fixture's PostgreSQL, so these branches have names no other test
//! uses. No Lore server, Lore command line, OpenShell, BuildKit or image is involved.
#[allow(dead_code, reason = "the watch tests use part of the fake gateway")]
#[path = "../../../../../../tests/factory/gateway.rs"]
mod gateway;

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use futures::future::BoxFuture as Building;
use lore_proto::lore::{
    model::v1::Address,
    notification::{BranchPushed, Event, event::Event as Notification},
    thin_client::v1::{NodeType, TreeNode},
};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use veoveo_hub::{
    HubCatalog, HubCatalogAdmission, HubCatalogHandle,
    execution::BranchName,
    factory::{
        self, Factory, Prepared, Push, Revision,
        image::{Build, CacheNamespace, ImageBuildError, ImageName, PinnedImage},
        sandbox_name,
    },
    store::HubStore,
};
use veoveo_types::{PrincipalId, TenantId};

use super::{
    BoxFuture, Branch, Events, LinkNode, Lore, Refused, Watched,
    store::{PrepareFailure, WatchStatus},
    watch::Watches,
};
use crate::contributions::lore::{BranchId, DeploymentId, RepositoryId};
use veoveo_hub::factory::LinkedPrepared;
use veoveo_hub_sdk::hub::LinkName;

const REPOSITORY: &str = "0123456789abcdef0123456789abcdef";
const BOUND: Duration = Duration::from_secs(60);

fn person(name: &str) -> PrincipalId {
    PrincipalId::parse(format!("https://identity.example#{name}")).unwrap()
}

fn revision(digit: char) -> Revision {
    Revision::parse(digit.to_string().repeat(64)).unwrap()
}

fn branch_id(digit: char) -> BranchId {
    BranchId::parse(digit.to_string().repeat(32)).unwrap()
}

fn name(branch: &str) -> BranchName {
    BranchName::parse(branch).unwrap()
}

fn watched() -> Watched {
    Watched {
        deployment: DeploymentId::parse("primary").unwrap(),
        repository: RepositoryId::parse(REPOSITORY).unwrap(),
    }
}

/// What the fake Lore was asked, and as whom.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Subscribe(PrincipalId),
    Branches(PrincipalId),
    Branch(PrincipalId, BranchId),
    Committer(PrincipalId, Revision),
    Read(PrincipalId, Revision),
    RevisionBranch(PrincipalId, Revision),
}

#[derive(Default)]
struct LoreState {
    calls: Vec<Call>,
    /// Who holds a grant on the repository.
    granted: HashSet<PrincipalId>,
    branches: Vec<Branch>,
    committers: HashMap<Revision, PrincipalId>,
    /// The revisions with an executable `openshell/run`, with their `policy.yaml`.
    runs: HashMap<Revision, Option<String>>,
    /// Revisions whose read fails.
    unreadable: HashSet<Revision>,
    /// One sender per subscription, in order.
    subscriptions: Vec<mpsc::UnboundedSender<Result<Event, tonic::Status>>>,
    /// The links at the top level of each revision of the repository.
    links: HashMap<Revision, Vec<LinkNode>>,
    /// The rest of each revision's own tree, as Lore lists it, which its links are read
    /// from.
    trees: HashMap<Revision, Vec<TreeNode>>,
    /// Who read a revision's top-level links, apart from `calls`.
    link_reads: Vec<(PrincipalId, Revision)>,
    /// The linked repositories, each with state of its own.
    linked: HashMap<RepositoryId, Linked>,
}

/// A linked repository of the fake Lore.
#[derive(Default)]
struct Linked {
    granted: HashSet<PrincipalId>,
    branches: Vec<Branch>,
    committers: HashMap<Revision, PrincipalId>,
    /// The branch each revision lives on.
    revision_branches: HashMap<Revision, BranchId>,
    /// Each revision's `openshell/policy.yaml` and `openshell/providers.yaml`.
    openshell: HashMap<Revision, LinkedPrepared>,
    /// Who read what, in order.
    calls: Vec<Call>,
    subscriptions: Vec<mpsc::UnboundedSender<Result<Event, tonic::Status>>>,
}

/// The tree node Lore lists for a link at the top level.
fn link_node(link: &LinkNode) -> TreeNode {
    TreeNode {
        path: link.name.to_string(),
        node_type: NodeType::Link as i32,
        address: Some(Address {
            context: hex::decode(link.repository.as_str()).unwrap().into(),
            hash: hex::decode(link.pinned.as_str()).unwrap().into(),
        }),
        tracking: link.follows,
        ..Default::default()
    }
}

#[derive(Default, Clone)]
struct FakeLore(Arc<Mutex<LoreState>>);

impl FakeLore {
    fn with(&self, change: impl FnOnce(&mut LoreState)) {
        change(&mut self.0.lock().unwrap());
    }

    fn calls(&self) -> Vec<Call> {
        self.0.lock().unwrap().calls.clone()
    }

    fn subscriptions(&self) -> usize {
        self.0.lock().unwrap().subscriptions.len()
    }

    /// Send `event` on the latest subscription.
    fn send(&self, event: Result<Event, tonic::Status>) {
        let state = self.0.lock().unwrap();
        state.subscriptions.last().unwrap().send(event).unwrap();
    }

    /// Send `event` on the first subscription, unless its follower stopped receiving.
    fn send_first(&self, event: Result<Event, tonic::Status>) -> bool {
        let state = self.0.lock().unwrap();
        state.subscriptions[0].send(event).is_ok()
    }

    fn admit(&self, call: Call, person: &PrincipalId) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(call);
        if state.granted.contains(person) {
            Ok(())
        } else {
            Err(Refused(format!("Lore refused {person}: PermissionDenied")).into())
        }
    }

    /// Answer a call on a linked repository from its own state, when `watched` is one.
    fn linked<T>(
        &self,
        watched: &Watched,
        call: Call,
        person: &PrincipalId,
        answer: impl FnOnce(&mut Linked) -> Result<T>,
    ) -> Option<Result<T>> {
        let mut state = self.0.lock().unwrap();
        let linked = state.linked.get_mut(&watched.repository)?;
        linked.calls.push(call);
        if !linked.granted.contains(person) {
            return Some(Err(Refused(format!(
                "Lore refused {person}: PermissionDenied"
            ))
            .into()));
        }
        Some(answer(linked))
    }

    fn linked_calls(&self, repository: &RepositoryId) -> Vec<Call> {
        self.0.lock().unwrap().linked[repository].calls.clone()
    }

    /// Send `event` on the latest subscription to the linked repository.
    fn send_linked(&self, repository: &RepositoryId, event: Result<Event, tonic::Status>) {
        let state = self.0.lock().unwrap();
        state.linked[repository]
            .subscriptions
            .last()
            .unwrap()
            .send(event)
            .unwrap();
    }
}

impl Lore for FakeLore {
    fn locate(&self, id: &RepositoryId, _: &TenantId) -> Option<Watched> {
        (id.as_str() == REPOSITORY).then(watched)
    }

    fn subscribe<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
    ) -> BoxFuture<'a, Result<Events>> {
        Box::pin(async move {
            let (sender, receiver) = mpsc::unbounded_channel();
            match self.linked(watched, Call::Subscribe(person.clone()), person, |linked| {
                linked.subscriptions.push(sender.clone());
                Ok(())
            }) {
                Some(subscribed) => subscribed?,
                None => {
                    self.admit(Call::Subscribe(person.clone()), person)?;
                    self.0.lock().unwrap().subscriptions.push(sender);
                }
            }
            let events: Events = Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(
                receiver,
            ));
            Ok(events)
        })
    }

    fn branches<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
    ) -> BoxFuture<'a, Result<Vec<Branch>>> {
        Box::pin(async move {
            if let Some(answer) =
                self.linked(watched, Call::Branches(person.clone()), person, |linked| {
                    Ok(linked.branches.clone())
                })
            {
                return answer;
            }
            self.admit(Call::Branches(person.clone()), person)?;
            Ok(self.0.lock().unwrap().branches.clone())
        })
    }

    fn branch<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        id: &'a BranchId,
    ) -> BoxFuture<'a, Result<Branch>> {
        Box::pin(async move {
            if let Some(answer) = self.linked(
                watched,
                Call::Branch(person.clone(), id.clone()),
                person,
                |linked| {
                    linked
                        .branches
                        .iter()
                        .find(|branch| &branch.id == id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Lore refused to read the branch: NotFound"))
                },
            ) {
                return answer;
            }
            self.admit(Call::Branch(person.clone(), id.clone()), person)?;
            let state = self.0.lock().unwrap();
            Ok(state
                .branches
                .iter()
                .find(|branch| &branch.id == id)
                .cloned()
                .expect("a known branch"))
        })
    }

    fn committer<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<PrincipalId>> {
        Box::pin(async move {
            if let Some(answer) = self.linked(
                watched,
                Call::Committer(person.clone(), revision.clone()),
                person,
                |linked| Ok(linked.committers[revision].clone()),
            ) {
                return answer;
            }
            self.admit(Call::Committer(person.clone(), revision.clone()), person)?;
            Ok(self.0.lock().unwrap().committers[revision].clone())
        })
    }

    fn read<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
        directory: &'a Path,
    ) -> BoxFuture<'a, Result<Option<Prepared>>> {
        Box::pin(async move {
            // A linked repository holds no openshell/run of its own here.
            if let Some(answer) = self.linked(
                watched,
                Call::Read(person.clone(), revision.clone()),
                person,
                |_| Ok(None),
            ) {
                return answer;
            }
            self.admit(Call::Read(person.clone(), revision.clone()), person)?;
            let (unreadable, run) = {
                let state = self.0.lock().unwrap();
                (
                    state.unreadable.contains(revision),
                    state.runs.get(revision).cloned(),
                )
            };
            anyhow::ensure!(
                !unreadable,
                "Lore failed to clone the revision: Unavailable"
            );
            let Some(policy) = run else {
                return Ok(None);
            };
            let image = directory.join("openshell/image");
            std::fs::create_dir_all(&image)?;
            std::fs::write(
                image.join("Dockerfile"),
                format!("FROM scratch # {revision}\n"),
            )?;
            if let Some(policy) = policy {
                std::fs::write(directory.join("openshell/policy.yaml"), policy)?;
            }
            Ok(Some(super::read::prepared(directory)?))
        })
    }

    fn links<'a>(
        &'a self,
        person: &'a PrincipalId,
        _: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<super::ParentTree>> {
        Box::pin(async move {
            let mut state = self.0.lock().unwrap();
            state.link_reads.push((person.clone(), revision.clone()));
            anyhow::ensure!(
                state.granted.contains(person),
                Refused(format!("Lore refused {person}: PermissionDenied"))
            );
            let mut nodes = state
                .links
                .get(revision)
                .into_iter()
                .flatten()
                .map(link_node)
                .collect::<Vec<_>>();
            nodes.extend(state.trees.get(revision).into_iter().flatten().cloned());
            super::read::parent_tree(&nodes)
        })
    }

    fn revision_branch<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<BranchId>> {
        Box::pin(async move {
            if let Some(answer) = self.linked(
                watched,
                Call::RevisionBranch(person.clone(), revision.clone()),
                person,
                |linked| Ok(linked.revision_branches[revision].clone()),
            ) {
                return answer;
            }
            self.admit(
                Call::RevisionBranch(person.clone(), revision.clone()),
                person,
            )?;
            let state = self.0.lock().unwrap();
            state
                .branches
                .iter()
                .find(|branch| &branch.latest == revision)
                .map(|branch| branch.id.clone())
                .ok_or_else(|| anyhow::anyhow!("Lore refused to read the revision: NotFound"))
        })
    }

    fn read_linked<'a>(
        &'a self,
        person: &'a PrincipalId,
        watched: &'a Watched,
        revision: &'a Revision,
        _: &'a Path,
    ) -> BoxFuture<'a, Result<LinkedPrepared>> {
        Box::pin(async move {
            self.linked(
                watched,
                Call::Read(person.clone(), revision.clone()),
                person,
                |linked| Ok(linked.openshell.get(revision).cloned().unwrap_or_default()),
            )
            .expect("a linked repository's revision")
        })
    }
}

/// An image builder that builds nothing: an image's digest is the SHA-256 of its
/// context's `Dockerfile`. It records each `Dockerfile` it built.
#[derive(Default)]
struct Builder {
    built: Mutex<Vec<String>>,
}

impl Build for Builder {
    fn build<'a>(
        &'a self,
        context: &'a Path,
        name: &'a ImageName,
        _cache: &'a CacheNamespace,
    ) -> Building<'a, Result<PinnedImage, ImageBuildError>> {
        Box::pin(async move {
            let dockerfile = std::fs::read_to_string(context.join("Dockerfile"))?;
            self.built.lock().unwrap().push(dockerfile.clone());
            let digest = hex::encode(Sha256::digest(dockerfile.as_bytes()));
            Ok(PinnedImage::parse(format!(
                "registry.internal:5000/factory/{}@sha256:{digest}",
                name.as_str()
            ))
            .unwrap())
        })
    }
}

const WORKSPACE: &str = "factory";
const BASE_POLICY: &str = "version: 1\nnetwork_policies:\n  internet:\n    endpoints:\n      - {host: \"**.*.*\", ports: [443]}\n    binaries:\n      - path: /**\n";
const REPOSITORY_POLICY: &str = "version: 1\nnetwork_policies:\n  packages:\n    endpoints:\n      - {host: packages.example, port: 443}\n    binaries:\n      - path: /**\n";

/// The installation's configuration, whose base branch is `main`.
fn catalog(directory: &Path) -> HubCatalogHandle {
    let installation = veoveo_hub::local::generate(directory, "http://127.0.0.1:1").unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&installation.config).unwrap()).unwrap();
    config["execution"]["baseBranch"] = serde_json::json!("main");
    HubCatalogHandle::new(Arc::new(
        HubCatalog::from_control_plane(
            serde_json::from_value(config).unwrap(),
            HubCatalogAdmission::unbound()
                .bind(crate::contributions::bindings().unwrap().catalog.clone())
                .unwrap(),
        )
        .unwrap(),
    ))
}

fn configuration(directory: &Path) -> veoveo_hub::control_plane::FactoryConfiguration {
    std::fs::write(directory.join("factory-base.yaml"), BASE_POLICY).unwrap();
    serde_json::from_value(serde_json::json!({
        "gateway": {
            "endpoint": "https://gateway.example/",
            "certificateAuthority": "/unused/ca.pem",
            "bearerEnvironment": "UNUSED"
        },
        "workspace": WORKSPACE,
        "policyFile": directory.join("factory-base.yaml"),
        "template": {"workspace": "installation", "name": "factory-v1"},
        "port": 8800,
        "hubIssuer": "https://hub.example",
        "loreServer": "grpcs://lore.internal:41337",
        "registrationProfile": "vv-factory-registration",
        "loreProfile": "vv-factory-lore",
        "imageBuilder": {
            "buildkit": "tcp://buildkitd.internal:1234",
            "registry": "registry.internal:5000",
        },
    }))
    .unwrap()
}

/// The fixture's database, dropped on a blocking thread. Its drop waits for PostgreSQL's
/// `DROP DATABASE`, which waits until every backend of the shared cluster takes its
/// barrier, and a backend this test's runtime is still connecting, for an apply that is
/// still running, takes it only once the runtime answers it. Dropped on the runtime's
/// thread, the drop would wait for the runtime and stall every test's databases until
/// PostgreSQL's authentication timeout; the runtime waits for the blocking drop instead.
struct Database(Option<veoveo_fixtures::store::TestDb>);

impl std::ops::Deref for Database {
    type Target = veoveo_fixtures::store::TestDb;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("the fixture's database")
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Some(db) = self.0.take() {
            match tokio::runtime::Handle::try_current() {
                Ok(runtime) => drop(runtime.spawn_blocking(move || drop(db))),
                Err(_) => drop(db),
            }
        }
    }
}

struct Fixture {
    db: Database,
    lore: FakeLore,
    gateway: gateway::Gateway,
    builder: Arc<Builder>,
    factory: Factory,
    watches: Watches,
    stop: CancellationToken,
    directory: tempfile::TempDir,
}

impl Fixture {
    async fn start() -> Self {
        let db = Database(Some(veoveo_fixtures::store::TestDb::new().await));
        HubStore::new(db.a.clone()).migrate().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let (gateway, client) = gateway::Gateway::start().await;
        gateway.set_template(
            "installation",
            openshell_sdk::raw::proto::SandboxWorkloadTemplate {
                metadata: Some(openshell_sdk::raw::proto::ObjectMeta {
                    name: "factory-v1".into(),
                    ..Default::default()
                }),
                spec: Some(Default::default()),
            },
        );
        let builder = Arc::new(Builder::default());
        let factory = Factory::new(
            configuration(directory.path()),
            catalog(directory.path()),
            client,
            db.a.clone(),
        )
        .unwrap()
        .with_builder(builder.clone());
        let lore = FakeLore::default();
        let stop = CancellationToken::new();
        let watches = Watches::with_resubscribe(
            Arc::new(lore.clone()),
            factory.clone(),
            db.a.clone(),
            stop.clone(),
            Duration::from_millis(20),
        );
        Self {
            db,
            lore,
            gateway,
            builder,
            factory,
            watches,
            stop,
            directory,
        }
    }

    async fn status(&self) -> Option<WatchStatus> {
        self.watches.store().status(&watched()).await.unwrap()
    }

    /// The revision the branch's instance last applied; none without an instance.
    async fn applied(&self, branch: &str) -> Option<Revision> {
        self.factory
            .status(REPOSITORY, &name(branch), Duration::ZERO)
            .await
            .unwrap()
            .and_then(|status| status.applied_revision)
    }

    /// Whether the branch has an instance.
    async fn serves(&self, branch: &str) -> bool {
        self.factory
            .status(REPOSITORY, &name(branch), Duration::ZERO)
            .await
            .unwrap()
            .is_some()
    }

    /// Apply a push directly, as an earlier watch would have.
    async fn applied_before(&self, branch: &str, revision_digit: char, pusher: &PrincipalId) {
        let context = tempfile::tempdir_in(self.directory.path()).unwrap().keep();
        std::fs::write(
            context.join("Dockerfile"),
            format!("FROM scratch # {}\n", revision(revision_digit)),
        )
        .unwrap();
        self.factory
            .apply(
                REPOSITORY,
                &name(branch),
                Push {
                    revision: revision(revision_digit),
                    pusher: pusher.clone(),
                },
                Prepared {
                    image_context: context,
                    policy: None,
                    providers: None,
                },
            )
            .await
            .unwrap();
    }

    /// The latest pusher the branch's instance records, as its client reads it.
    async fn pusher(&self, branch: &str) -> Option<PrincipalId> {
        let deployment = self
            .factory
            .status(REPOSITORY, &name(branch), Duration::ZERO)
            .await
            .unwrap()?
            .deployment;
        let material = self.gateway.refresh_material(
            WORKSPACE,
            &format!("{}-lore", sandbox_name(deployment)),
            "LORE_ACCESS_TOKEN",
        )?;
        factory::instance_client(
            &self.db.a,
            &material["client_id"],
            &material["client_secret"],
        )
        .await
        .unwrap()?
        .pusher
    }

    /// How many times the builder built the image of `revision_digit`.
    fn built(&self, revision_digit: char) -> usize {
        let dockerfile = format!("FROM scratch # {}\n", revision(revision_digit));
        self.builder
            .built
            .lock()
            .unwrap()
            .iter()
            .filter(|built| **built == dockerfile)
            .count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

fn live(id: char, branch: &str, latest: char) -> Branch {
    Branch {
        id: branch_id(id),
        name: branch.into(),
        latest: revision(latest),
        deleted: false,
    }
}

fn pushed(branch: char, revision: char, pusher: &PrincipalId) -> Event {
    Event {
        event: Some(Notification::BranchPushed(BranchPushed {
            revision: hex::decode(revision.to_string().repeat(64)).unwrap().into(),
            revision_number: 2,
            branch: hex::decode(branch.to_string().repeat(32)).unwrap().into(),
            user_id: pusher.to_string(),
        })),
        ..Default::default()
    }
}

/// Wait until `done` holds.
async fn eventually(what: &str, done: impl Fn() -> bool) {
    tokio::time::timeout(BOUND, async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} did not happen"));
}

/// Wait until `done` answers true.
async fn until<Answer: std::future::Future<Output = bool>>(what: &str, done: impl Fn() -> Answer) {
    tokio::time::timeout(BOUND, async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} did not happen"));
}

async fn bounded(test: impl std::future::Future<Output = ()>) {
    tokio::time::timeout(BOUND, test)
        .await
        .expect("the watch test exceeded its bound");
}

#[tokio::test]
async fn watching_subscribes_as_the_watcher_and_rechecks_every_branch() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, bob, carol) = (person("alice"), person("bob"), person("carol"));
        fixture.lore.with(|state| {
            state.granted.extend([alice.clone(), bob.clone()]);
            state.branches = vec![
                live('1', "main", 'a'),
                live('2', "watch/feature", 'b'),
                live('3', "watch/notes", 'c'),
                Branch {
                    deleted: true,
                    ..live('4', "watch/retired", 'd')
                },
                // The name of a deleted branch that a live branch took again.
                Branch {
                    deleted: true,
                    ..live('5', "main", 'e')
                },
            ];
            state.committers.insert(revision('a'), bob.clone());
            state.committers.insert(revision('c'), bob.clone());
            state.runs.insert(revision('a'), None);
        });
        // Earlier pushes: the feature branch's instance serves its latest revision, and
        // the notes and retired branches have instances.
        fixture.applied_before("watch/feature", 'b', &carol).await;
        fixture.applied_before("watch/notes", '9', &carol).await;
        fixture.applied_before("watch/retired", 'd', &carol).await;

        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        until("the re-check", || async {
            fixture.applied("main").await == Some(revision('a'))
                && !fixture.serves("watch/notes").await
                && !fixture.serves("watch/retired").await
        })
        .await;

        let calls = fixture.lore.calls();
        assert_eq!(
            calls,
            [
                Call::Subscribe(alice.clone()),
                Call::Branches(alice.clone()),
                Call::Committer(alice.clone(), revision('a')),
                Call::Read(bob.clone(), revision('a')),
                Call::Committer(alice.clone(), revision('c')),
                Call::Read(bob.clone(), revision('c')),
            ]
        );
        // The unapplied revision was applied as its committer; the applied one was not
        // built again.
        assert_eq!(fixture.pusher("main").await, Some(bob));
        assert_eq!(fixture.applied("watch/feature").await, Some(revision('b')));
        assert_eq!((fixture.built('a'), fixture.built('b')), (1, 1));
        assert_eq!(
            fixture.status().await,
            Some(WatchStatus {
                watcher: alice,
                failed: None
            })
        );
    })
    .await;
}

#[tokio::test]
async fn a_push_is_read_as_its_pusher_and_applied_or_removed() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, carol) = (person("alice"), person("carol"));
        fixture.lore.with(|state| {
            state.granted.extend([alice.clone(), carol.clone()]);
            state.branches = vec![live('1', "watch/carol", 'a')];
            state
                .runs
                .insert(revision('b'), Some(REPOSITORY_POLICY.into()));
        });
        fixture.applied_before("watch/carol", 'a', &alice).await;
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        eventually("the re-check", || fixture.lore.calls().len() == 2).await;

        fixture.lore.send(Ok(pushed('1', 'b', &carol)));
        until("the push", || async {
            fixture.applied("watch/carol").await == Some(revision('b'))
        })
        .await;
        assert_eq!(
            fixture.lore.calls()[2..],
            [
                Call::Branch(carol.clone(), branch_id('1')),
                Call::Read(carol.clone(), revision('b')),
            ]
        );
        // The factory built the revision's image context and added its policy.
        assert_eq!(fixture.built('b'), 1);
        assert_eq!(fixture.pusher("watch/carol").await, Some(carol.clone()));
        let deployment = fixture
            .factory
            .status(REPOSITORY, &name("watch/carol"), Duration::ZERO)
            .await
            .unwrap()
            .unwrap()
            .deployment;
        let sandbox = fixture
            .gateway
            .sandbox(WORKSPACE, &sandbox_name(deployment))
            .unwrap();
        assert!(
            sandbox
                .spec
                .unwrap()
                .policy
                .unwrap()
                .network_policies
                .contains_key("packages")
        );

        // A push whose revision has no openshell/run stops serving the branch.
        fixture.lore.send(Ok(pushed('1', 'c', &carol)));
        until("the removal", || async {
            !fixture.serves("watch/carol").await
        })
        .await;
        assert_eq!(
            fixture.lore.calls().last(),
            Some(&Call::Read(carol, revision('c')))
        );
    })
    .await;
}

#[tokio::test]
async fn a_resubscription_after_aborted_rechecks_and_applies_an_unapplied_branch() {
    bounded(async {
        let fixture = Fixture::start().await;
        let alice = person("alice");
        fixture.lore.with(|state| {
            state.granted.insert(alice.clone());
            state.branches = vec![live('1', "main", 'a')];
            state.committers.insert(revision('a'), alice.clone());
            state.committers.insert(revision('b'), alice.clone());
            state.runs.insert(revision('a'), None);
            state.runs.insert(revision('b'), None);
        });
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        until("the first re-check", || async {
            fixture.applied("main").await == Some(revision('a'))
        })
        .await;

        // The push of b went to a subscriber that lagged; Lore ends its stream.
        fixture
            .lore
            .with(|state| state.branches = vec![live('1', "main", 'b')]);
        fixture
            .lore
            .send(Err(tonic::Status::aborted("subscriber lagged")));
        until("the second re-check", || async {
            fixture.applied("main").await == Some(revision('b'))
        })
        .await;
        assert_eq!(fixture.lore.subscriptions(), 2);
        // A third subscription finds nothing left to apply.
        fixture
            .lore
            .send(Err(tonic::Status::aborted("subscriber lagged")));
        eventually("the third re-check", || {
            fixture
                .lore
                .calls()
                .iter()
                .filter(|call| matches!(call, Call::Branches(_)))
                .count()
                == 3
        })
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!((fixture.built('a'), fixture.built('b')), (1, 1));
    })
    .await;
}

fn caller(id: &PrincipalId) -> veoveo_hub_sdk::Principal {
    veoveo_hub_sdk::Principal {
        id: id.clone(),
        kind: veoveo_hub_sdk::PrincipalKind::User,
        issuer: veoveo_hub_sdk::TokenIssuer::parse("https://identity.example").unwrap(),
        subject: veoveo_hub_sdk::TokenSubject::parse("caller").unwrap(),
        tenant: Some(TenantId::parse("local").unwrap()),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: Default::default(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    }
}

#[tokio::test]
async fn a_watcher_who_lost_access_shows_as_failed_at_resubscription() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, mallory) = (person("alice"), person("mallory"));
        fixture.lore.with(|state| {
            state.granted.insert(alice.clone());
        });
        // The route answers 403 for a caller Lore refuses, 400 for no repository ID and
        // 204 once it watches as the caller.
        let refused = fixture
            .watches
            .answer(&caller(&mallory), REPOSITORY.into())
            .await;
        assert_eq!(refused.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(fixture.status().await, None);
        let invalid = fixture.watches.answer(&caller(&alice), "main".into()).await;
        assert_eq!(invalid.status(), axum::http::StatusCode::BAD_REQUEST);
        let watching = fixture
            .watches
            .answer(&caller(&alice), REPOSITORY.into())
            .await;
        assert_eq!(watching.status(), axum::http::StatusCode::NO_CONTENT);
        eventually("the re-check", || fixture.lore.calls().len() == 3).await;
        // Lore checks access only when a subscription starts.
        fixture.lore.with(|state| state.granted.clear());
        fixture
            .lore
            .send(Err(tonic::Status::aborted("subscriber lagged")));
        eventually("the failed resubscription", || {
            fixture.lore.calls().len() == 4
        })
        .await;
        until("the recorded failure", || async {
            fixture
                .status()
                .await
                .is_some_and(|status| status.failed.is_some())
        })
        .await;
        let status = fixture.status().await.unwrap();
        assert_eq!(status.watcher, alice);
        assert!(
            status
                .failed
                .as_deref()
                .unwrap()
                .contains("Lore refused https://identity.example#alice"),
            "{status:?}"
        );
        // The follower stops: someone with access watches again.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(fixture.lore.calls().len(), 4);
        assert_eq!(fixture.lore.subscriptions(), 1);
    })
    .await;
}

#[tokio::test]
async fn a_failed_read_is_recorded_for_the_branch_until_a_push_is_prepared() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, carol) = (person("alice"), person("carol"));
        fixture.lore.with(|state| {
            state.granted.extend([alice.clone(), carol.clone()]);
            state.branches = vec![live('1', "main", 'a')];
            state.committers.insert(revision('a'), carol.clone());
            state.unreadable.insert(revision('a'));
            state.runs.insert(revision('b'), None);
        });
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        let store = fixture.watches.store().clone();
        let failure = || async {
            store
                .prepare_failure(&watched(), &name("main"))
                .await
                .unwrap()
        };
        until("the recorded failure", || async {
            failure().await.is_some()
        })
        .await;
        let PrepareFailure {
            revision: failed,
            pusher,
            message,
        } = failure().await.unwrap();
        assert_eq!((failed, pusher), (revision('a'), carol.clone()));
        assert!(
            message.contains("reading openshell/ as https://identity.example#carol failed")
                && message.contains("Lore failed to clone the revision"),
            "{message}"
        );
        assert!(!fixture.serves("main").await);

        fixture.lore.send(Ok(pushed('1', 'b', &carol)));
        until("the push", || async {
            fixture.applied("main").await == Some(revision('b'))
        })
        .await;
        assert_eq!(failure().await, None);
    })
    .await;
}

#[tokio::test]
async fn resuming_follows_every_recorded_watch_as_its_watcher() {
    bounded(async {
        let fixture = Fixture::start().await;
        let alice = person("alice");
        fixture.lore.with(|state| {
            state.granted.insert(alice.clone());
        });
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        eventually("the re-check", || fixture.lore.calls().len() == 2).await;
        // Another Hub process on the same database.
        let lore = FakeLore::default();
        lore.with(|state| {
            state.granted.insert(alice.clone());
        });
        let stop = CancellationToken::new();
        let resumed = Watches::with_resubscribe(
            Arc::new(lore.clone()),
            fixture.factory.clone(),
            fixture.db.a.clone(),
            stop.clone(),
            Duration::from_millis(20),
        );
        resumed.resume().await.unwrap();
        eventually("the resumed subscription", || lore.calls().len() == 2).await;
        assert_eq!(
            lore.calls(),
            [Call::Subscribe(alice.clone()), Call::Branches(alice)]
        );
        stop.cancel();
    })
    .await;
}

#[tokio::test]
async fn watching_again_replaces_the_watcher_and_its_follower() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, bob, carol) = (person("alice"), person("bob"), person("carol"));
        fixture.lore.with(|state| {
            state
                .granted
                .extend([alice.clone(), bob.clone(), carol.clone()]);
            state.branches = vec![live('1', "main", 'a')];
            state.committers.insert(revision('a'), carol.clone());
            state.runs.insert(revision('b'), None);
        });
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        fixture.watches.watch(watched(), bob.clone()).await.unwrap();
        assert_eq!(fixture.status().await.unwrap().watcher, bob);
        eventually("Bob's re-check", || {
            fixture.lore.calls().contains(&Call::Branches(bob.clone()))
        })
        .await;
        // Alice's follower stopped; only Bob's applies the push.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = fixture.lore.send_first(Ok(pushed('1', 'b', &carol)));
        fixture.lore.send(Ok(pushed('1', 'b', &carol)));
        until("the push", || async {
            fixture.applied("main").await == Some(revision('b'))
        })
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(fixture.built('b'), 1);
    })
    .await;
}

const LINKED_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const LINKED_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TEAM_POLICY: &str = "version: 1\nnetwork_policies:\n  team-api:\n    endpoints:\n      - {host: api.team.example, port: 443}\n    binaries:\n      - path: /sandbox/src/**\n";

fn linked_watched(repository: &str) -> Watched {
    Watched {
        deployment: DeploymentId::parse("primary").unwrap(),
        repository: RepositoryId::parse(repository).unwrap(),
    }
}

fn link(name: &str, repository: &str, pinned: char, follows: bool) -> LinkNode {
    LinkNode {
        name: LinkName::parse(name).unwrap(),
        repository: RepositoryId::parse(repository).unwrap(),
        pinned: revision(pinned),
        follows,
    }
}

impl Fixture {
    /// The runners of the branch, by link.
    async fn runners(&self, branch: &str) -> Vec<veoveo_hub::factory::Runner> {
        self.factory
            .runners(REPOSITORY, &name(branch))
            .await
            .unwrap()
    }

    /// The runner's sandbox, as OpenShell holds it.
    async fn runner_sandbox(&self, branch: &str, link: &str) -> openshell_sdk::raw::proto::Sandbox {
        let deployment = self
            .factory
            .runner_status(
                REPOSITORY,
                &name(branch),
                &LinkName::parse(link).unwrap(),
                Duration::ZERO,
            )
            .await
            .unwrap()
            .unwrap()
            .deployment;
        self.gateway
            .sandbox(WORKSPACE, &sandbox_name(deployment))
            .unwrap()
    }
}

/// A linked branch, whose links follow the parent's branch and pin one of their own: Alice
/// watches the parent, Bob the repository of `team-a`, and Dave that of `team-b`.
fn linked_branch(
    fixture: &Fixture,
    branch: &str,
) -> (PrincipalId, PrincipalId, PrincipalId, PrincipalId) {
    let (alice, bob, carol, dave) = (
        person("alice"),
        person("bob"),
        person("carol"),
        person("dave"),
    );
    fixture.lore.with(|state| {
        state.granted.insert(alice.clone());
        state.branches = vec![live('1', branch, '1')];
        state.committers.insert(revision('1'), alice.clone());
        state.runs.insert(revision('1'), None);
        state.links.insert(
            revision('1'),
            vec![
                link("team-a", LINKED_A, 'a', true),
                link("team-b", LINKED_B, 'b', false),
            ],
        );
        state.trees.insert(revision('1'), own_tree('1'));
        let a = state
            .linked
            .entry(RepositoryId::parse(LINKED_A).unwrap())
            .or_default();
        a.granted.extend([bob.clone(), carol.clone()]);
        // A following link tracks the linked repository's branch of the parent branch's
        // ID, which Lore named with the ID appended because the name was taken.
        a.branches = vec![Branch {
            name: format!("{branch}-{}", branch_id('1')),
            ..live('1', "", 'c')
        }];
        a.committers.insert(revision('c'), carol.clone());
        a.openshell.insert(
            revision('c'),
            LinkedPrepared {
                policy: Some(TEAM_POLICY.into()),
                providers: None,
            },
        );
        let b = state
            .linked
            .entry(RepositoryId::parse(LINKED_B).unwrap())
            .or_default();
        b.granted.insert(dave.clone());
        // A pinned link tracks the branch its pinned revision lives on.
        b.revision_branches.insert(revision('b'), branch_id('2'));
        b.branches = vec![live('2', "release", 'd')];
        b.committers.insert(revision('d'), dave.clone());
    });
    (alice, bob, carol, dave)
}

#[tokio::test]
async fn a_linked_branch_runs_one_runner_per_link_serving_the_branch_each_link_tracks() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, bob, carol, dave) = linked_branch(&fixture, "links/main");
        fixture
            .watches
            .store()
            .watch(&linked_watched(LINKED_A), &bob)
            .await
            .unwrap();
        fixture
            .watches
            .store()
            .watch(&linked_watched(LINKED_B), &dave)
            .await
            .unwrap();
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        until("both runners", || async {
            let runners = fixture.runners("links/main").await;
            runners.len() == 2
                && runners
                    .iter()
                    .all(|runner| runner.linked_revision.is_some())
        })
        .await;

        // The branch has no ordinary instance; each link has a runner.
        assert!(!fixture.serves("links/main").await);
        let runners = fixture.runners("links/main").await;
        let [a, b] = [&runners[0], &runners[1]];
        assert_eq!(
            (a.link.name.as_str(), a.link.repository.as_str()),
            ("team-a", LINKED_A)
        );
        assert_eq!(a.link.tracked.id, branch_id('1').as_str());
        assert_eq!(
            a.link.tracked.name,
            format!("links/main-{}", branch_id('1'))
        );
        assert_eq!(
            (a.linked_revision.clone(), a.linked_pusher.clone()),
            (Some(revision('c')), Some(carol.clone()))
        );
        assert_eq!(
            (
                b.link.name.as_str(),
                b.link.tracked.id.as_str(),
                b.link.tracked.name.as_str()
            ),
            ("team-b", branch_id('2').as_str(), "release")
        );
        assert_eq!(
            (b.linked_revision.clone(), b.linked_pusher.clone()),
            (Some(revision('d')), Some(dave.clone()))
        );
        for runner in &runners {
            assert_eq!(
                (runner.revision.clone(), runner.pusher.clone()),
                (Some(revision('1')), Some(alice.clone()))
            );
        }
        // The linked repositories were read as their watchers, and each tracked head's
        // openshell/ as its committer.
        assert_eq!(
            fixture
                .lore
                .linked_calls(&RepositoryId::parse(LINKED_A).unwrap()),
            [
                Call::Branch(bob.clone(), branch_id('1')),
                Call::Committer(bob.clone(), revision('c')),
                Call::Read(carol.clone(), revision('c')),
            ]
        );
        assert_eq!(
            fixture
                .lore
                .linked_calls(&RepositoryId::parse(LINKED_B).unwrap()),
            [
                Call::RevisionBranch(dave.clone(), revision('b')),
                Call::Branch(dave.clone(), branch_id('2')),
                Call::Committer(dave.clone(), revision('d')),
                Call::Read(dave.clone(), revision('d')),
            ]
        );

        // Each runner checks the parent out without its links and its linked repository
        // at the tracked branch, under the linked repository's own network rules.
        let sandbox = fixture.runner_sandbox("links/main", "team-a").await;
        let spec = sandbox.spec.unwrap();
        assert_eq!(
            spec.command[3..],
            [
                "runner",
                &format!("grpcs://lore.internal:41337/{REPOSITORY}"),
                "links/main",
                &format!("grpcs://lore.internal:41337/{LINKED_A}"),
                branch_id('1').as_str(),
                "crates",
                "openshell",
            ]
        );
        assert!(
            spec.policy
                .unwrap()
                .network_policies
                .contains_key("team-api")
        );
        let other = fixture.runner_sandbox("links/main", "team-b").await;
        assert!(
            !other
                .spec
                .unwrap()
                .policy
                .unwrap()
                .network_policies
                .contains_key("team-api")
        );
        // The image is the parent's, built once for the branch's revision.
        assert_eq!(fixture.built('1'), 2);
        let status = fixture
            .factory
            .runner_status(
                REPOSITORY,
                &name("links/main"),
                &LinkName::parse("team-b").unwrap(),
                Duration::ZERO,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&status.runner).unwrap(),
            serde_json::json!({
                "link": "team-b",
                "linkedRepository": LINKED_B,
                "trackedBranch": {"id": branch_id('2').as_str(), "name": "release"},
                "linkedRevision": revision('d').as_str(),
            })
        );

        // A push to the tracked branch of team-a's repository applies to its runner alone,
        // as its pusher.
        let erin = person("erin");
        fixture.lore.with(|state| {
            let a = state
                .linked
                .get_mut(&RepositoryId::parse(LINKED_A).unwrap())
                .unwrap();
            a.granted.insert(erin.clone());
            a.branches[0].latest = revision('e');
            a.committers.insert(revision('e'), erin.clone());
        });
        fixture
            .watches
            .watch(linked_watched(LINKED_A), bob.clone())
            .await
            .unwrap();
        fixture.lore.send_linked(
            &RepositoryId::parse(LINKED_A).unwrap(),
            Ok(pushed('1', 'e', &erin)),
        );
        until("the tracked push", || async {
            fixture.runners("links/main").await[0].linked_revision == Some(revision('e'))
        })
        .await;
        let runners = fixture.runners("links/main").await;
        assert_eq!(runners[0].linked_pusher, Some(erin));
        assert_eq!(runners[1].linked_revision, Some(revision('d')));
    })
    .await;
}

#[tokio::test]
async fn a_link_runs_once_its_linked_repository_is_watched_and_goes_with_the_link() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, bob, _, dave) = linked_branch(&fixture, "links/second");
        fixture
            .watches
            .store()
            .watch(&linked_watched(LINKED_A), &bob)
            .await
            .unwrap();
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        let caller = veoveo_hub_sdk::Principal {
            id: alice.clone(),
            kind: veoveo_hub_sdk::PrincipalKind::User,
            issuer: veoveo_hub_sdk::TokenIssuer::parse("https://identity.example").unwrap(),
            subject: veoveo_hub_sdk::TokenSubject::parse("alice").unwrap(),
            tenant: Some(TenantId::parse("tenant").unwrap()),
            groups: Default::default(),
            group_roles: Default::default(),
            roles: Default::default(),
            scopes: Default::default(),
            data_labels: Default::default(),
            assurances: Default::default(),
            authenticated_at: None,
        };
        let team_b = LinkName::parse("team-b").unwrap();
        let failure = || async {
            fixture
                .watches
                .report(
                    &caller,
                    &RepositoryId::parse(REPOSITORY).unwrap(),
                    &name("links/second"),
                    Some(&team_b),
                )
                .await
                .unwrap()
                .runner_failure
        };
        until("team-a's runner and team-b's failure", || async {
            let runners = fixture.runners("links/second").await;
            runners.len() == 1 && runners[0].linked_revision.is_some() && failure().await.is_some()
        })
        .await;
        // Nobody watches team-b's repository: its runner waits, and says why.
        let report = fixture
            .watches
            .report(
                &caller,
                &RepositoryId::parse(REPOSITORY).unwrap(),
                &name("links/second"),
                Some(&team_b),
            )
            .await
            .unwrap();
        let failed = report.runner_failure.unwrap();
        assert_eq!(failed.linked_repository.as_str(), LINKED_B);
        assert!(
            failed
                .message
                .contains("nobody watches the linked repository"),
            "{}",
            failed.message
        );

        // Once Dave watches it, the link runs.
        fixture
            .watches
            .watch(linked_watched(LINKED_B), dave.clone())
            .await
            .unwrap();
        until("team-b's runner", || async {
            let runners = fixture.runners("links/second").await;
            runners.len() == 2
                && runners
                    .iter()
                    .all(|runner| runner.linked_revision.is_some())
                && failure().await.is_none()
        })
        .await;

        // A push that drops team-b's link removes its runner; one without links serves the
        // branch from an ordinary instance again.
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('2');
            state.committers.insert(revision('2'), alice.clone());
            state.runs.insert(revision('2'), None);
            state
                .links
                .insert(revision('2'), vec![link("team-a", LINKED_A, 'a', true)]);
        });
        fixture.lore.send(Ok(pushed('1', '2', &alice)));
        until("team-b's runner removed", || async {
            let runners = fixture.runners("links/second").await;
            runners.len() == 1 && runners[0].revision == Some(revision('2'))
        })
        .await;
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('3');
            state.committers.insert(revision('3'), alice.clone());
            state.runs.insert(revision('3'), None);
        });
        fixture.lore.send(Ok(pushed('1', '3', &alice)));
        until("the ordinary instance", || async {
            fixture.applied("links/second").await == Some(revision('3'))
        })
        .await;
        assert!(fixture.runners("links/second").await.is_empty());
    })
    .await;
}

/// A branch's own tree beside its links: `crates/` and `openshell/` with one file each,
/// whose content `content` names.
fn own_tree(content: char) -> Vec<TreeNode> {
    let file = |path: &str| TreeNode {
        path: path.into(),
        node_type: NodeType::File as i32,
        address: Some(Address {
            context: hex::decode(REPOSITORY).unwrap().into(),
            hash: hex::decode(content.to_string().repeat(64)).unwrap().into(),
        }),
        size: 10,
        ..Default::default()
    };
    vec![
        tree_node("crates", NodeType::Directory),
        file("crates/server.rs"),
        tree_node("openshell", NodeType::Directory),
        file("openshell/run"),
    ]
}

/// The sandbox calls that name the sandbox `name`, by kind.
fn calls_naming(calls: &[gateway::Call], name: &str) -> Vec<&'static str> {
    calls
        .iter()
        .filter_map(|call| match call {
            gateway::Call::CreateSandbox(request) if request.name == name => Some("create"),
            gateway::Call::DeleteSandbox(request) if request.name == name => Some("delete"),
            gateway::Call::StopSandbox(request) if request.name == name => Some("stop"),
            gateway::Call::StartSandbox(request) if request.name == name => Some("start"),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_link_added_or_removed_leaves_the_other_runners_serving() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, bob, _, dave) = linked_branch(&fixture, "links/many");
        for (linked, watcher) in [(LINKED_A, &bob), (LINKED_B, &dave)] {
            fixture
                .watches
                .store()
                .watch(&linked_watched(linked), watcher)
                .await
                .unwrap();
        }
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        until("both runners", || async {
            let runners = fixture.runners("links/many").await;
            runners.len() == 2
                && runners
                    .iter()
                    .all(|runner| runner.linked_revision.is_some())
        })
        .await;
        let sandbox = |link: &'static str| {
            let fixture = &fixture;
            async move {
                sandbox_name(
                    fixture
                        .factory
                        .runner_status(
                            REPOSITORY,
                            &name("links/many"),
                            &LinkName::parse(link).unwrap(),
                            Duration::ZERO,
                        )
                        .await
                        .unwrap()
                        .unwrap()
                        .deployment,
                )
            }
        };
        let (a, b) = (sandbox("team-a").await, sandbox("team-b").await);
        // The runner's checkout includes the branch's own top-level entries; its links
        // are left out by the view's first rule.
        let spec = fixture
            .runner_sandbox("links/many", "team-a")
            .await
            .spec
            .unwrap();
        assert_eq!(spec.command[8..], ["crates", "openshell"]);
        fixture.gateway.take_calls();

        // A push that adds a link, its own tree unchanged: the new link's runner is created
        // and the others' sandboxes are neither recreated nor restarted.
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('2');
            state.committers.insert(revision('2'), alice.clone());
            state.runs.insert(revision('2'), None);
            state.links.insert(
                revision('2'),
                vec![
                    link("team-a", LINKED_A, 'a', true),
                    link("team-b", LINKED_B, 'b', false),
                    link("team-c", LINKED_A, 'a', true),
                ],
            );
            state.trees.insert(revision('2'), own_tree('1'));
        });
        fixture.lore.send(Ok(pushed('1', '2', &alice)));
        until("team-c's runner", || async {
            let runners = fixture.runners("links/many").await;
            runners.len() == 3
                && runners
                    .iter()
                    .all(|runner| runner.revision == Some(revision('2')))
        })
        .await;
        let calls = fixture.gateway.take_calls();
        let c = sandbox("team-c").await;
        assert_eq!(calls_naming(&calls, &c), ["create"]);
        for unchanged in [&a, &b] {
            assert!(
                calls_naming(&calls, unchanged).is_empty(),
                "{unchanged}: {calls:?}"
            );
        }
        assert_eq!(
            fixture
                .runner_sandbox("links/many", "team-a")
                .await
                .spec
                .unwrap()
                .command,
            spec.command
        );

        // Removing the link deletes its runner alone.
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('3');
            state.committers.insert(revision('3'), alice.clone());
            state.runs.insert(revision('3'), None);
            state.links.insert(
                revision('3'),
                vec![
                    link("team-a", LINKED_A, 'a', true),
                    link("team-b", LINKED_B, 'b', false),
                ],
            );
            state.trees.insert(revision('3'), own_tree('1'));
        });
        fixture.lore.send(Ok(pushed('1', '3', &alice)));
        until("team-c's runner removed", || async {
            let runners = fixture.runners("links/many").await;
            runners.len() == 2
                && runners
                    .iter()
                    .all(|runner| runner.revision == Some(revision('3')))
        })
        .await;
        let calls = fixture.gateway.take_calls();
        assert_eq!(calls_naming(&calls, &c), ["delete"]);
        for unchanged in [&a, &b] {
            assert!(
                calls_naming(&calls, unchanged).is_empty(),
                "{unchanged}: {calls:?}"
            );
        }

        // A change of the branch's own content applies the new program to every runner.
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('4');
            state.committers.insert(revision('4'), alice.clone());
            state.runs.insert(revision('4'), None);
            state.links.insert(
                revision('4'),
                vec![
                    link("team-a", LINKED_A, 'a', true),
                    link("team-b", LINKED_B, 'b', false),
                ],
            );
            state.trees.insert(revision('4'), own_tree('9'));
        });
        fixture.lore.send(Ok(pushed('1', '4', &alice)));
        until("the new program", || async {
            let runners = fixture.runners("links/many").await;
            runners
                .iter()
                .all(|runner| runner.revision == Some(revision('4')))
        })
        .await;
        let calls = fixture.gateway.take_calls();
        for changed in [&a, &b] {
            assert!(
                !calls_naming(&calls, changed).is_empty(),
                "{changed}: {calls:?}"
            );
        }
    })
    .await;
}

/// A node of a revision's own tree: a directory, or a link to `LINKED_A`.
fn tree_node(path: &str, node_type: NodeType) -> TreeNode {
    let address = (node_type == NodeType::Link).then(|| Address {
        context: hex::decode(LINKED_A).unwrap().into(),
        hash: hex::decode(revision('a').as_str()).unwrap().into(),
    });
    TreeNode {
        path: path.into(),
        node_type: node_type as i32,
        address,
        ..Default::default()
    }
}

#[tokio::test]
async fn a_revision_with_a_link_its_checkout_cannot_leave_out_is_refused_at_prepare() {
    bounded(async {
        let fixture = Fixture::start().await;
        let alice = person("alice");
        fixture.lore.with(|state| {
            state.granted.insert(alice.clone());
            state.branches = vec![live('1', "links/deep", '1')];
            state.committers.insert(revision('1'), alice.clone());
            state.runs.insert(revision('1'), None);
        });
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        until("the instance", || async {
            fixture.applied("links/deep").await == Some(revision('1'))
        })
        .await;
        let store = fixture.watches.store().clone();
        let refused = |digit: char| {
            let store = store.clone();
            async move {
                store
                    .prepare_failure(&watched(), &name("links/deep"))
                    .await
                    .unwrap()
                    .filter(|failure| failure.revision == revision(digit))
            }
        };
        let push = |digit: char, links: Vec<LinkNode>, tree: Vec<TreeNode>| {
            fixture.lore.with(|state| {
                state.branches[0].latest = revision(digit);
                state.committers.insert(revision(digit), alice.clone());
                state.runs.insert(revision(digit), None);
                state.links.insert(revision(digit), links);
                state.trees.insert(revision(digit), tree);
            });
            fixture.lore.send(Ok(pushed('1', digit, &alice)));
        };

        // A link below the top level of a branch without top-level links: its clone would
        // reach the linked repository, so the push is refused and the instance keeps
        // serving.
        push(
            '2',
            Vec::new(),
            vec![
                tree_node("assets", NodeType::Directory),
                tree_node("assets/lib", NodeType::Link),
            ],
        );
        until("the refusal", || async { refused('2').await.is_some() }).await;
        let message = refused('2').await.unwrap().message;
        assert!(
            message
                .contains("reading the revision's links as https://identity.example#alice failed")
                && message.contains("the revision holds the link assets/lib below its top level"),
            "{message}"
        );
        assert_eq!(fixture.applied("links/deep").await, Some(revision('1')));

        // The same for a linked branch: none of its links runs.
        push(
            '3',
            vec![link("team-a", LINKED_A, 'a', true)],
            vec![
                tree_node("docs", NodeType::Directory),
                tree_node("docs/lib", NodeType::Link),
            ],
        );
        until("the linked refusal", || async {
            refused('3').await.is_some()
        })
        .await;
        assert!(
            refused('3')
                .await
                .unwrap()
                .message
                .contains("the revision holds the link docs/lib below its top level")
        );
        assert_eq!(fixture.applied("links/deep").await, Some(revision('1')));
        assert!(fixture.runners("links/deep").await.is_empty());

        // A top-level link whose name is no DNS label is not left out either.
        push('4', Vec::new(), vec![tree_node("Team_A", NodeType::Link)]);
        until("the name's refusal", || async {
            refused('4').await.is_some()
        })
        .await;
        assert!(
            refused('4')
                .await
                .unwrap()
                .message
                .contains("the revision's link Team_A is not named by a lowercase DNS label")
        );

        // A push without such links is prepared and clears the refusal.
        push(
            '5',
            Vec::new(),
            vec![tree_node("assets", NodeType::Directory)],
        );
        until("the next push", || async {
            fixture.applied("links/deep").await == Some(revision('5'))
        })
        .await;
        assert_eq!(
            store
                .prepare_failure(&watched(), &name("links/deep"))
                .await
                .unwrap(),
            None
        );

        // A revision without `openshell/` is still refused for a deep link.
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('6');
            state.committers.insert(revision('6'), alice.clone());
            state.trees.insert(
                revision('6'),
                vec![
                    tree_node("docs", NodeType::Directory),
                    tree_node("docs/lib", NodeType::Link),
                ],
            );
        });
        fixture.lore.send(Ok(pushed('1', '6', &alice)));
        until("the refusal without openshell/", || async {
            refused('6').await.is_some()
        })
        .await;
        assert_eq!(fixture.applied("links/deep").await, Some(revision('5')));

        // Without `openshell/` and without such links, nothing serves the branch and
        // nothing failed.
        fixture.lore.with(|state| {
            state.branches[0].latest = revision('7');
            state.committers.insert(revision('7'), alice.clone());
        });
        fixture.lore.send(Ok(pushed('1', '7', &alice)));
        until("the instance's removal", || async {
            !fixture.serves("links/deep").await
        })
        .await;
        assert_eq!(
            store
                .prepare_failure(&watched(), &name("links/deep"))
                .await
                .unwrap(),
            None
        );
    })
    .await;
}

#[tokio::test]
async fn a_linked_repositorys_rule_for_a_program_of_its_own_is_refused() {
    bounded(async {
        let fixture = Fixture::start().await;
        let (alice, bob, _, dave) = linked_branch(&fixture, "links/content");
        let content_rule = |binary: &str| LinkedPrepared {
            policy: Some(format!(
                "version: 1\nnetwork_policies:\n  team-api:\n    endpoints:\n      - {{host: api.team.example, port: 443}}\n    binaries:\n      - path: \"{binary}\"\n"
            )),
            providers: None,
        };
        let team_b = || RepositoryId::parse(LINKED_B).unwrap();
        fixture.lore.with(|state| {
            state
                .linked
                .get_mut(&team_b())
                .unwrap()
                .openshell
                .insert(revision('d'), content_rule("/sandbox/link/bin/sync"));
        });
        for (linked, watcher) in [(LINKED_A, &bob), (LINKED_B, &dave)] {
            fixture
                .watches
                .watch(linked_watched(linked), watcher.clone())
                .await
                .unwrap();
        }
        fixture
            .watches
            .watch(watched(), alice.clone())
            .await
            .unwrap();
        let caller = veoveo_hub_sdk::Principal {
            tenant: Some(TenantId::parse("tenant").unwrap()),
            ..caller(&alice)
        };
        let link = LinkName::parse("team-b").unwrap();
        let failure = || async {
            fixture
                .watches
                .report(
                    &caller,
                    &RepositoryId::parse(REPOSITORY).unwrap(),
                    &name("links/content"),
                    Some(&link),
                )
                .await
                .unwrap()
                .runner_failure
        };
        // Team-b's rule names a program in its own checkout: its link gets no runner.
        until("team-a's runner and team-b's refusal", || async {
            let runners = fixture.runners("links/content").await;
            runners.len() == 1 && runners[0].linked_revision.is_some() && failure().await.is_some()
        })
        .await;
        let refused = failure().await.unwrap().message;
        assert!(
            refused.contains("names the program `/sandbox/link/bin/sync`, which can lie under /sandbox/link"),
            "{refused}"
        );

        // A push that names the parent's program instead runs the link.
        let push = |digit: char, binary: &str| {
            fixture.lore.with(|state| {
                let b = state.linked.get_mut(&team_b()).unwrap();
                b.branches[0].latest = revision(digit);
                b.committers.insert(revision(digit), dave.clone());
                b.openshell.insert(revision(digit), content_rule(binary));
            });
            fixture
                .lore
                .send_linked(&team_b(), Ok(pushed('2', digit, &dave)));
        };
        push('f', "/sandbox/src/**");
        until("team-b's runner", || async {
            let runners = fixture.runners("links/content").await;
            runners.len() == 2
                && runners[1].linked_revision == Some(revision('f'))
                && failure().await.is_none()
        })
        .await;

        // A later push that names one again leaves the runner as it serves, and says why.
        push('9', "/**");
        until("the tracked push's refusal", || async { failure().await.is_some() }).await;
        assert!(failure().await.unwrap().message.contains("names the program `/**`"));
        assert_eq!(
            fixture.runners("links/content").await[1].linked_revision,
            Some(revision('f'))
        );
    })
    .await;
}
