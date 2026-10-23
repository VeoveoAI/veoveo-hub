//! Collection's decision on synthetic sandboxes, and one sweep against the fake
//! OpenShell gateway and a disposable database holding the owner's ready Computer: an
//! idle Computer's sandbox is deleted keeping its home and the Computer is recorded as
//! collected; a recent browser request keeps it; the next entry creates it at the
//! current version and tells of the update. Every credential and image is synthetic;
//! no Kubernetes or Computer image is involved.
use std::{sync::Arc, time::Duration};

use chrono::{DateTime, TimeDelta, Utc};
use openshell_sdk::raw::proto::{self as p, ProviderCredentialRefreshStrategy as Strategy};
use uuid::Uuid;
use veoveo_types::Sha256Digest;

use super::{BrowserRequests, Collection, Idle, idle};
use crate::{
    computers::{
        ComputerRuntime,
        lifecycle::Entry,
        provisioning::Provisioner,
        test_fixture::{
            catalog, computer, fake::Call, gateway, hub, installation, installation_template,
            ready_computer, runtime,
        },
    },
    store::{Computer, ComputerOperation},
};

const AFTER: TimeDelta = TimeDelta::hours(2);

fn timestamp(at: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: at.timestamp(),
        nanos: i32::try_from(at.timestamp_subsec_nanos()).unwrap(),
    }
}

/// A sandbox in `phase` whose last traffic and creation are the given times.
fn sandbox(
    phase: p::SandboxPhase,
    traffic: Option<DateTime<Utc>>,
    created: Option<DateTime<Utc>>,
) -> p::Sandbox {
    p::Sandbox {
        metadata: Some(p::ObjectMeta {
            created_time: created.map(timestamp),
            ..Default::default()
        }),
        status: Some(p::SandboxStatus {
            phase: phase as i32,
            last_activity_time: traffic.map(timestamp),
            ..Default::default()
        }),
        ..Default::default()
    }
}

async fn fixture() -> (Computer, Provisioner) {
    let (_fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
    let mut owned = computer(&provisioner);
    owned.ready = true;
    (owned, provisioner)
}

fn hours(value: i64) -> TimeDelta {
    TimeDelta::hours(value)
}

#[tokio::test]
async fn a_computer_idle_in_the_browser_and_the_sandbox_is_collected() {
    let (computer, _) = fixture().await;
    let now = Utc::now();
    let started = now - hours(10);
    let ready = sandbox(p::SandboxPhase::Ready, Some(now - hours(4)), None);
    assert_eq!(
        idle(now, started, AFTER, &computer, Some(now - hours(3)), &ready),
        Some(Idle {
            browser: hours(3),
            sandbox: hours(4)
        })
    );
    // Exactly `after` is idle enough.
    let at_bound = sandbox(p::SandboxPhase::Ready, Some(now - AFTER), None);
    assert!(idle(now, started, AFTER, &computer, Some(now - AFTER), &at_bound).is_some());
}

#[tokio::test]
async fn a_recent_browser_request_keeps_the_computer() {
    let (computer, _) = fixture().await;
    let now = Utc::now();
    let ready = sandbox(p::SandboxPhase::Ready, Some(now - hours(5)), None);
    let browser = Some(now - TimeDelta::minutes(30));
    assert_eq!(
        idle(now, now - hours(10), AFTER, &computer, browser, &ready),
        None
    );
}

#[tokio::test]
async fn recent_sandbox_traffic_keeps_the_computer() {
    let (computer, _) = fixture().await;
    let now = Utc::now();
    // An agent calling its model while nobody has a browser open.
    let working = sandbox(
        p::SandboxPhase::Ready,
        Some(now - TimeDelta::minutes(1)),
        Some(now - hours(9)),
    );
    assert_eq!(
        idle(now, now - hours(10), AFTER, &computer, None, &working),
        None
    );
}

#[tokio::test]
async fn a_running_restart_or_creation_keeps_the_computer() {
    let (mut computer, provisioner) = fixture().await;
    let now = Utc::now();
    let started = now - hours(10);
    let idle_sandbox = sandbox(p::SandboxPhase::Ready, Some(now - hours(5)), None);
    computer.operation = Some(ComputerOperation {
        id: Uuid::now_v7(),
        started_at: now - TimeDelta::minutes(1),
        provisioning: provisioner.spec.clone(),
        notice_shown: false,
    });
    assert_eq!(
        idle(now, started, AFTER, &computer, None, &idle_sandbox),
        None
    );
    // A replacement whose worker ended without recording it no longer runs.
    computer.operation.as_mut().unwrap().started_at = now - hours(1);
    assert!(idle(now, started, AFTER, &computer, None, &idle_sandbox).is_some());

    // A sandbox OpenShell is still creating, starting or removing is not ready.
    computer.operation = None;
    for phase in [
        p::SandboxPhase::Provisioning,
        p::SandboxPhase::Starting,
        p::SandboxPhase::Deleting,
        p::SandboxPhase::Error,
    ] {
        let busy = sandbox(phase, Some(now - hours(5)), None);
        assert_eq!(
            idle(now, started, AFTER, &computer, None, &busy),
            None,
            "{phase:?}"
        );
    }
}

#[tokio::test]
async fn after_a_hub_restart_every_computer_is_active_from_the_restart() {
    let (computer, _) = fixture().await;
    let now = Utc::now();
    // The Hub started an hour ago; the browser and the sandbox were idle long before.
    let started = now - hours(1);
    let ready = sandbox(p::SandboxPhase::Ready, Some(now - hours(6)), None);
    assert_eq!(idle(now, started, AFTER, &computer, None, &ready), None);
    // Once `after` has passed since the restart, both count from it.
    let later = started + AFTER;
    assert_eq!(
        idle(later, started, AFTER, &computer, None, &ready),
        Some(Idle {
            browser: AFTER,
            sandbox: AFTER
        })
    );
}

#[tokio::test]
async fn a_sandbox_without_recorded_traffic_counts_from_its_creation() {
    let (computer, _) = fixture().await;
    let now = Utc::now();
    let started = now - hours(10);
    let quiet = sandbox(p::SandboxPhase::Ready, None, Some(now - hours(3)));
    assert_eq!(
        idle(now, started, AFTER, &computer, None, &quiet),
        Some(Idle {
            browser: hours(10),
            sandbox: hours(3)
        })
    );
    let fresh = sandbox(p::SandboxPhase::Ready, None, Some(now - hours(1)));
    assert_eq!(idle(now, started, AFTER, &computer, None, &fresh), None);
    let unknown = sandbox(p::SandboxPhase::Ready, None, None);
    assert_eq!(idle(now, started, AFTER, &computer, None, &unknown), None);
}

/// The Computer once no operation runs.
async fn settled(runtime: &ComputerRuntime, computer: &Computer) -> Computer {
    loop {
        let current = runtime
            .hub
            .hub_store()
            .computer(&computer.tenant, &computer.principal, &computer.id)
            .await
            .unwrap()
            .unwrap();
        if current.operation.is_none() {
            return current;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_sweep_collects_an_idle_computer_keeping_its_home_and_entry_creates_it_again() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
        let db = crate::test_store::TestDb::new().await;
        let hub = hub(&db);
        let catalog = catalog();
        let computer = ready_computer(
            &hub,
            &catalog,
            &provisioner,
            &Sha256Digest::from_bytes([9; 32]),
        )
        .await;
        let runtime = runtime(hub, catalog, installation(&provisioner));
        let name = computer.native_name();
        let first = computer.provisioning.version().unwrap();
        assert!(matches!(
            runtime.enter(&computer).await.unwrap(),
            Entry::Starting(Some(_))
        ));
        let computer = settled(&runtime, &computer).await;

        // The sandbox's last traffic was three hours ago, as OpenShell records it.
        let now = Utc::now();
        let mut status = fake.sandbox(&name, &name).unwrap().status.unwrap();
        status.last_activity_time = Some(timestamp(now - hours(3)));
        fake.set_status(&name, &name, status);
        let collection = |browser: Arc<BrowserRequests>| Collection {
            catalog: runtime.catalog.clone(),
            hub: runtime.hub.clone(),
            browser,
            started: now - hours(10),
        };

        // A browser request ten minutes ago keeps it.
        let browser = Arc::new(BrowserRequests::default());
        browser.record(&computer.id, now - TimeDelta::minutes(10));
        fake.take_calls();
        collection(browser)
            .sweep(&runtime.installation, AFTER, now)
            .await;
        assert!(fake.sandbox(&name, &name).is_some());
        assert!(
            !fake
                .take_calls()
                .iter()
                .any(|call| matches!(call, Call::DeleteSandbox(_)))
        );

        // With no browser request since the Hub started, the sandbox goes and the home
        // stays; the Computer is recorded as collected, with no failure.
        collection(Arc::default())
            .sweep(&runtime.installation, AFTER, now)
            .await;
        assert!(fake.sandbox(&name, &name).is_none());
        assert!(
            fake.take_calls()
                .iter()
                .any(|call| matches!(call, Call::DeleteSandbox(delete) if delete.keep_workspace)),
            "collection keeps the home"
        );
        let collected = settled(&runtime, &computer).await;
        assert!(collected.collected_at.is_some());
        assert!(collected.failure.is_none());
        assert_eq!(collected.provisioning.version().unwrap(), first);
        let status = runtime.status(&collected, false).await.unwrap();
        assert_eq!(status.sandbox, "none");
        assert_eq!(status.collected, collected.collected_at);

        // A version published meanwhile: the next entry creates the sandbox at it and
        // tells of the update, as any creation does.
        fake.remove_template("installation", "computer");
        fake.set_template(
            "installation",
            installation_template("registry.internal/computer@sha256:1", Some("Newer")),
        );
        let Entry::Starting(Some(operation)) = runtime.enter(&collected).await.unwrap() else {
            panic!("entry of a collected Computer creates its sandbox");
        };
        let current = operation.provisioning.version().unwrap();
        assert_ne!(current, first);
        let created = settled(&runtime, &collected).await;
        assert!(created.collected_at.is_none());
        assert_eq!(created.provisioning.version().unwrap(), current);
        assert!(fake.sandbox(&name, &name).is_some());
        let updated = runtime
            .status(&created, true)
            .await
            .unwrap()
            .updated
            .unwrap();
        assert_eq!((updated.from, updated.to), (first, current));
    })
    .await
    .expect("the collection test exceeded its bound");
}
