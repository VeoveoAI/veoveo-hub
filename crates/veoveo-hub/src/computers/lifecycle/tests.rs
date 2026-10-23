//! The one lifecycle path against the fake OpenShell gateway, which makes a created
//! sandbox ready at once, and a disposable database holding the owner's ready Computer.
//! Entry creates a missing sandbox at the current version; a ready one is served
//! whatever its version; a restart deletes it keeping the home, creates it at the
//! current version and records that version with an update notice told once; a failed
//! restart records its cause and keeps the recorded version, which the previous version
//! starts again; a missing template leaves Computers unavailable while a running one is
//! served. Every credential and image is synthetic; no Kubernetes or Computer image is
//! involved.
use std::time::Duration;

use openshell_sdk::raw::proto::ProviderCredentialRefreshStrategy as Strategy;
use veoveo_types::Sha256Digest;

use super::{Begun, Entry, Target};
use crate::{
    computers::{
        ComputerRuntime,
        test_fixture::{
            catalog, fake::Call, gateway, hub, installation, installation_template, ready_computer,
            runtime,
        },
    },
    store::Computer,
};

/// The Computer once no replacement runs.
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

fn environment(runtime_sandbox: &openshell_sdk::raw::proto::Sandbox) -> String {
    runtime_sandbox.spec.as_ref().unwrap().environment["COMPUTER_VERSION"].clone()
}

#[tokio::test]
async fn entry_creates_a_restart_updates_and_a_failure_keeps_the_version() {
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
        fake.take_calls();

        // Entry with no sandbox creates one at the current version.
        let Entry::Starting(Some(operation)) = runtime.enter(&computer).await.unwrap() else {
            panic!("entry without a sandbox creates one");
        };
        assert_eq!(operation.provisioning.version().unwrap(), first);
        let computer = settled(&runtime, &computer).await;
        assert!(computer.failure.is_none());
        let sandbox = fake.sandbox(&name, &name).unwrap();
        assert_eq!(environment(&sandbox), first.as_str());
        assert!(matches!(
            runtime.enter(&computer).await.unwrap(),
            Entry::Ready
        ));
        let status = runtime.status(&computer, true).await.unwrap();
        assert_eq!(
            (
                status.version.version.clone(),
                status.sandbox,
                status.update_available
            ),
            (first.clone(), "ready", false)
        );
        assert!(status.updated.is_none(), "the first creation is no update");

        // The installation publishes another template: a new image and a release note.
        fake.remove_template("installation", "computer");
        fake.set_template(
            "installation",
            installation_template(
                "registry.internal/computer@sha256:1",
                Some("Faster builds\nThe image adds a compiler."),
            ),
        );
        let status = runtime.status(&computer, true).await.unwrap();
        let current = status.current.clone().unwrap();
        assert!(status.update_available);
        assert_ne!(current.version, first);
        assert_eq!(current.summary.as_deref(), Some("Faster builds"));
        let changes = status.whats_new.clone().unwrap();
        assert_eq!(
            changes.image.unwrap().to,
            "registry.internal/computer@sha256:1"
        );
        assert_eq!(
            changes.note.as_deref(),
            Some("Faster builds\nThe image adds a compiler.")
        );
        // A ready sandbox is served whatever its version.
        assert!(matches!(
            runtime.enter(&computer).await.unwrap(),
            Entry::Ready
        ));

        // A restart deletes the sandbox keeping its home and records the new version
        // once its sandbox is ready.
        fake.take_calls();
        assert_eq!(
            runtime
                .begin(&computer, Target::Current, false)
                .await
                .unwrap(),
            Begun::Started
        );
        let computer = settled(&runtime, &computer).await;
        assert_eq!(computer.provisioning.version().unwrap(), current.version);
        assert!(
            fake.take_calls()
                .iter()
                .any(|call| matches!(call, Call::DeleteSandbox(delete) if delete.keep_workspace)),
            "the restart keeps the home"
        );
        assert_eq!(
            environment(&fake.sandbox(&name, &name).unwrap()),
            current.version.as_str()
        );
        let status = runtime.status(&computer, true).await.unwrap();
        assert!(!status.update_available);
        let updated = status.updated.unwrap();
        assert_eq!(
            (updated.from, updated.to),
            (first.clone(), current.version.clone())
        );
        assert_eq!(
            updated.whats_new.note.as_deref(),
            Some("Faster builds\nThe image adds a compiler.")
        );
        assert!(
            runtime
                .status(&computer, true)
                .await
                .unwrap()
                .updated
                .is_none(),
            "an update is told once"
        );

        // A failed restart records its cause and keeps the version.
        fake.remove_template("installation", "computer");
        fake.set_template(
            "installation",
            installation_template("registry.internal/computer@sha256:2", None),
        );
        fake.refuse_next(
            "CreateSandbox",
            tonic::Status::resource_exhausted("synthetic capacity"),
        );
        runtime
            .begin(&computer, Target::Current, false)
            .await
            .unwrap();
        let computer = settled(&runtime, &computer).await;
        let failure = computer.failure.clone().unwrap();
        assert!(
            failure.cause.contains("creating the sandbox"),
            "{}",
            failure.cause
        );
        assert!(
            !failure.cause.contains("synthetic capacity"),
            "native descriptions stay out of the cause"
        );
        assert_ne!(failure.version, current.version);
        assert_eq!(computer.provisioning.version().unwrap(), current.version);
        assert!(fake.sandbox(&name, &name).is_none());
        assert!(matches!(
            runtime.enter(&computer).await.unwrap(),
            Entry::Attention
        ));
        let status = runtime.status(&computer, true).await.unwrap();
        assert_eq!(status.sandbox, "none");
        assert_eq!(status.failure.unwrap().cause, failure.cause);

        // The previous version starts again from the recorded spec.
        runtime
            .begin(&computer, Target::Previous, false)
            .await
            .unwrap();
        let computer = settled(&runtime, &computer).await;
        assert!(computer.failure.is_none());
        assert_eq!(computer.provisioning.version().unwrap(), current.version);
        assert_eq!(
            environment(&fake.sandbox(&name, &name).unwrap()),
            current.version.as_str()
        );

        // Without the template no Computer can start, and the running one is served.
        fake.remove_template("installation", "computer");
        let unavailable = runtime.installation.current().await.err().unwrap();
        assert!(unavailable.0.contains("does not exist"), "{unavailable}");
        assert!(matches!(
            runtime.enter(&computer).await.unwrap(),
            Entry::Ready
        ));
        let status = runtime.status(&computer, false).await.unwrap();
        assert!(status.current.is_none() && !status.update_available);
        assert!(status.current_unavailable.is_some());
    })
    .await
    .expect("the lifecycle test exceeded its bound");
}
