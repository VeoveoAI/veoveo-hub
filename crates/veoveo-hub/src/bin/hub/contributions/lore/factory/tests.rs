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
use lore_proto::lore::notification::{BranchPushed, Event, event::Event as Notification};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use veoveo_hub::{
    HubCatalog, HubCatalogAdmission, HubCatalogHandle,
    execution::BranchName,
    factory::{
        self, Factory, Prepared, Push, Revision,
        image::{Build, ImageBuildError, ImageName, PinnedImage},
        sandbox_name,
    },
    store::HubStore,
};
use veoveo_types::{PrincipalId, TenantId};

use super::{
    BoxFuture, Branch, Events, Lore, Refused, Watched,
    store::{PrepareFailure, WatchStatus},
    watch::Watches,
};
use crate::contributions::lore::{DeploymentId, RepositoryId, workspace::BranchId};

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
}

impl Lore for FakeLore {
    fn locate(&self, id: &RepositoryId, _: &TenantId) -> Option<Watched> {
        (id.as_str() == REPOSITORY).then(watched)
    }

    fn subscribe<'a>(
        &'a self,
        person: &'a PrincipalId,
        _: &'a Watched,
    ) -> BoxFuture<'a, Result<Events>> {
        Box::pin(async move {
            self.admit(Call::Subscribe(person.clone()), person)?;
            let (sender, receiver) = mpsc::unbounded_channel();
            self.0.lock().unwrap().subscriptions.push(sender);
            let events: Events = Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(
                receiver,
            ));
            Ok(events)
        })
    }

    fn branches<'a>(
        &'a self,
        person: &'a PrincipalId,
        _: &'a Watched,
    ) -> BoxFuture<'a, Result<Vec<Branch>>> {
        Box::pin(async move {
            self.admit(Call::Branches(person.clone()), person)?;
            Ok(self.0.lock().unwrap().branches.clone())
        })
    }

    fn branch<'a>(
        &'a self,
        person: &'a PrincipalId,
        _: &'a Watched,
        id: &'a BranchId,
    ) -> BoxFuture<'a, Result<Branch>> {
        Box::pin(async move {
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
        _: &'a Watched,
        revision: &'a Revision,
    ) -> BoxFuture<'a, Result<PrincipalId>> {
        Box::pin(async move {
            self.admit(Call::Committer(person.clone(), revision.clone()), person)?;
            Ok(self.0.lock().unwrap().committers[revision].clone())
        })
    }

    fn read<'a>(
        &'a self,
        person: &'a PrincipalId,
        _: &'a Watched,
        revision: &'a Revision,
        directory: &'a Path,
    ) -> BoxFuture<'a, Result<Option<Prepared>>> {
        Box::pin(async move {
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
        "cpu": "8",
        "memory": "16Gi",
        "port": 8800,
        "hubIssuer": "https://hub.example",
        "loreServer": "grpcs://lore.internal:41337",
        "registrationProfile": "vv-factory-registration",
        "loreProfile": "vv-factory-lore",
        "imageBuilder": {
            "buildkit": "tcp://buildkitd.internal:1234",
            "registry": "registry.internal:5000",
            "lore": format!("registry.internal:5000/lore@sha256:{}", "a".repeat(64)),
        },
    }))
    .unwrap()
}

struct Fixture {
    db: veoveo_fixtures::store::TestDb,
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
        let db = veoveo_fixtures::store::TestDb::new().await;
        HubStore::new(db.a.clone()).migrate().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let (gateway, client) = gateway::Gateway::start().await;
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
            "LORE_AUTH_TOKEN",
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
