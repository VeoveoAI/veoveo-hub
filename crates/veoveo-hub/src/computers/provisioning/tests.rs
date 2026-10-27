//! A Computer's custody handoff and sandbox replacement against the fake OpenShell
//! gateway (`tests/factory/gateway.rs`). The handoff creates the owner's providers, hands
//! the user's refresh grant and the Computer's own Lore client to the gateway and rotates
//! both credentials; it creates no sandbox. A replacement clones the version's
//! template with the owner as `LORE_IDENTITY` and the version as `COMPUTER_VERSION`,
//! deletes an existing sandbox keeping its workspace and creates the sandbox, with the
//! Lore provider attached. A later handoff keeps the Lore provider, replaces only its
//! client's secret and replaces the running sandbox. The fake reports synthetic
//! rotation expiries, makes sandboxes ready at once and accepts forwarding without
//! relaying bytes; no OpenShell, Lore, Hub token endpoint or model is involved, and every
//! credential is a synthetic string.
use std::time::Duration;

use openshell_sdk::raw::proto as p;

use super::super::test_fixture::{
    LORE_KEY, LORE_PROFILE, OWNER, catalog, computer, fake::Call, gateway, next_login, scope,
};
use super::{HUB_PROVIDER, LORE_PROVIDER, MODEL_PROVIDER, Strategy};
use crate::registration;

/// The Lore refresh a handoff configured: the Computer's own client, identified by its
/// native name, with the secret whose digest the handoff's beginning recorded.
fn assert_lore_refresh(
    refresh: &p::ConfigureProviderRefreshRequest,
    name: &str,
    client: &veoveo_types::Sha256Digest,
) {
    assert_eq!(
        (
            refresh.workspace_scope.clone(),
            refresh.provider.as_str(),
            refresh.credential_key.as_str(),
            refresh.strategy,
            refresh.secret_material_keys.as_slice(),
        ),
        (
            scope(name),
            LORE_PROVIDER,
            LORE_KEY,
            Strategy::Oauth2ClientCredentials as i32,
            ["client_secret".to_owned()].as_slice(),
        )
    );
    assert_eq!(refresh.material["client_id"], name);
    assert_eq!(
        &registration::digest(&refresh.material["client_secret"]),
        client
    );
}

#[tokio::test]
async fn a_handoff_rotates_the_owners_lore_access_and_a_replacement_keeps_the_home() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
        let catalog = catalog();
        let (computer, grant) = next_login(&computer(&provisioner));
        let name = computer.native_name();
        let first = grant.lore.digest();
        provisioner
            .handoff(&catalog, &computer, grant)
            .await
            .unwrap();

        // Custody only: the workspace, its providers and their refresh, rotated.
        let calls = fake.take_calls();
        let [
            Call::CreateWorkspace(_),
            Call::CreateProvider(_),
            Call::CreateProvider(_),
            Call::CreateProvider(lore),
            Call::ConfigureProviderRefresh(hub),
            Call::RotateProviderCredential(hub_rotation),
            Call::ConfigureProviderRefresh(refresh),
            Call::RotateProviderCredential(rotation),
        ] = calls.as_slice()
        else {
            panic!("the first handoff sent {calls:#?}");
        };
        // The Lore provider has no credential of its own: the gateway mints it.
        assert_eq!(lore.workspace_scope, scope(&name));
        let lore = lore.provider.as_ref().unwrap();
        assert_eq!(
            (
                lore.metadata.as_ref().unwrap().name.as_str(),
                lore.r#type.as_str()
            ),
            (LORE_PROVIDER, LORE_PROFILE)
        );
        assert!(lore.credentials.is_empty());
        assert_eq!(hub.provider, HUB_PROVIDER);
        assert_eq!(hub_rotation.provider, HUB_PROVIDER);
        assert_lore_refresh(refresh, &name, &first);
        assert_eq!(
            (
                rotation.workspace_scope.clone(),
                rotation.provider.as_str(),
                rotation.credential_key.as_str()
            ),
            (scope(&name), LORE_PROVIDER, LORE_KEY)
        );
        assert!(
            fake.sandbox(&name, &name).is_none(),
            "custody creates no sandbox"
        );

        // The first replacement clones the version's template for the owner and creates
        // the sandbox from it, with the Lore provider attached.
        let operation = uuid::Uuid::now_v7();
        provisioner.replace(&computer, operation).await.unwrap();
        let calls = fake.take_calls();
        let [
            Call::DeleteSandboxTemplate(_),
            Call::CreateSandboxTemplate(template),
            Call::CreateSandbox(create),
        ] = calls.as_slice()
        else {
            panic!("the first replacement sent {calls:#?}");
        };
        let environment = &template
            .template
            .as_ref()
            .unwrap()
            .spec
            .as_ref()
            .unwrap()
            .workload
            .as_ref()
            .unwrap()
            .environment;
        assert_eq!(environment["LORE_IDENTITY"], OWNER);
        assert_eq!(environment["PUBLIC_BASE_PATH"], computer.base_path());
        assert_eq!(environment["SSL_CERT_DIR"], "/etc/ssl/certs");
        assert_eq!(
            environment[super::COMPUTER_VERSION],
            provisioner.spec.version().unwrap().as_str()
        );
        assert_eq!(
            create.spec.as_ref().unwrap().providers,
            [HUB_PROVIDER, MODEL_PROVIDER, LORE_PROVIDER]
        );
        let sandbox = fake.sandbox(&name, &name).unwrap();
        assert_eq!(
            sandbox.spec.as_ref().unwrap().environment[super::COMPUTER_VERSION],
            provisioner.spec.version().unwrap().as_str(),
            "the sandbox's environment names its version"
        );
        provisioner.validate_sandbox(&computer, &sandbox).unwrap();
        let mut without_lore = sandbox.clone();
        without_lore.spec.as_mut().unwrap().providers.pop();
        assert!(
            provisioner
                .validate_sandbox(&computer, &without_lore)
                .is_err(),
            "a sandbox created without the Lore provider differs from the installation"
        );

        // The next handoff keeps the providers and replaces only the client's secret. Its
        // refresh configuration begins a new authorization epoch, whose placeholders the
        // running sandbox's processes never received, so it replaces that sandbox,
        // keeping the home, with request IDs of the handoff.
        let (computer, grant) = next_login(&computer);
        let second = grant.lore.digest();
        assert_ne!(second, first);
        provisioner
            .handoff(&catalog, &computer, grant)
            .await
            .unwrap();
        let calls = fake.take_calls();
        let [
            Call::ConfigureProviderRefresh(_),
            Call::RotateProviderCredential(_),
            Call::ConfigureProviderRefresh(refresh),
            Call::RotateProviderCredential(rotation),
            Call::DeleteSandbox(delete),
            Call::DeleteSandboxTemplate(_),
            Call::CreateSandboxTemplate(_),
            Call::CreateSandbox(create),
        ] = calls.as_slice()
        else {
            panic!("the next handoff sent {calls:#?}");
        };
        assert_lore_refresh(refresh, &name, &second);
        assert_eq!(rotation.provider, LORE_PROVIDER);
        assert_eq!(
            fake.refresh_material(&name, LORE_PROVIDER, LORE_KEY)
                .unwrap()["client_id"],
            name
        );
        assert!(delete.keep_workspace, "the restart keeps the home");
        let handoff = computer.handoff.unwrap();
        assert_eq!(
            create.request_id,
            uuid::Uuid::new_v5(&handoff, b"create").to_string()
        );
        assert!(fake.sandbox(&name, &name).is_some());

        // A later replacement deletes the sandbox keeping its workspace volume, and
        // replaces the template clone, each with a request ID of its operation.
        let operation = uuid::Uuid::now_v7();
        provisioner.replace(&computer, operation).await.unwrap();
        let calls = fake.take_calls();
        let [
            Call::DeleteSandbox(delete),
            Call::DeleteSandboxTemplate(_),
            Call::CreateSandboxTemplate(_),
            Call::CreateSandbox(create),
        ] = calls.as_slice()
        else {
            panic!("the later replacement sent {calls:#?}");
        };
        assert!(delete.keep_workspace, "a replacement keeps the home");
        assert_eq!(
            delete.request_id,
            uuid::Uuid::new_v5(&operation, b"delete").to_string()
        );
        assert_eq!(
            create.request_id,
            uuid::Uuid::new_v5(&operation, b"create").to_string()
        );
    })
    .await
    .expect("the handoff test exceeded its bound");
}

/// The owner's Computer with custody handed off and a ready sandbox of its version, on
/// the fake gateway, whose recorded calls are cleared.
async fn computer_with_sandbox() -> (
    super::super::test_fixture::fake::Gateway,
    super::Provisioner,
    crate::store::Computer,
) {
    let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
    let (computer, grant) = next_login(&computer(&provisioner));
    provisioner
        .handoff(&catalog(), &computer, grant)
        .await
        .unwrap();
    provisioner
        .replace(&computer, uuid::Uuid::now_v7())
        .await
        .unwrap();
    fake.take_calls();
    (fake, provisioner, computer)
}

#[tokio::test]
async fn a_replacement_waits_for_the_sandbox_to_go_when_the_delete_answers_after_its_deadline() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (fake, provisioner, computer) = computer_with_sandbox().await;
        let name = computer.native_name();
        // Like the real gateway while the Pod stops, the delete answers once the sandbox
        // is gone, three times later than the replacement awaits its answer.
        fake.delete_slowly(Some(Duration::from_millis(1500)));
        let started = std::time::Instant::now();
        provisioner
            .replace(&computer, uuid::Uuid::now_v7())
            .await
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(1500));
        let calls = fake.take_calls();
        let [
            Call::DeleteSandbox(delete),
            Call::DeleteSandboxTemplate(_),
            Call::CreateSandboxTemplate(_),
            Call::CreateSandbox(_),
        ] = calls.as_slice()
        else {
            panic!("the replacement sent {calls:#?}");
        };
        assert!(delete.keep_workspace);
        assert_eq!(
            super::phase(&fake.sandbox(&name, &name).unwrap()),
            p::SandboxPhase::Ready,
            "the new sandbox runs"
        );
    })
    .await
    .expect("the replacement test exceeded its bound");
}

#[tokio::test]
async fn a_sandbox_that_does_not_go_fails_the_replacement_with_what_still_runs() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (fake, provisioner, computer) = computer_with_sandbox().await;
        let name = computer.native_name();
        fake.set_status(
            &name,
            &name,
            p::SandboxStatus {
                phase: p::SandboxPhase::Ready as i32,
                agent_pod: "computer-pod".into(),
                conditions: vec![p::SandboxCondition {
                    r#type: "Ready".into(),
                    status: "False".into(),
                    reason: "Terminating".into(),
                    message: "the workload has not exited".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        fake.delete_slowly(None);
        let error = provisioner
            .replace(&computer, uuid::Uuid::now_v7())
            .await
            .unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "the old sandbox was not gone after 3 s: it is still deleting with Pod \
             computer-pod; Ready False (Terminating): the workload has not exited"
        );
        let calls = fake.take_calls();
        assert!(
            matches!(calls.as_slice(), [Call::DeleteSandbox(_)]),
            "nothing replaces a sandbox that has not gone: {calls:#?}"
        );
    })
    .await
    .expect("the replacement test exceeded its bound");
}

#[tokio::test]
async fn a_lore_profile_that_cannot_own_the_clients_refresh_is_refused_before_custody() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (fake, provisioner) = gateway(Strategy::Oauth2RefreshToken).await;
        let (computer, grant) = next_login(&computer(&provisioner));
        assert!(
            provisioner
                .handoff(&catalog(), &computer, grant)
                .await
                .is_err()
        );
        assert_eq!(fake.take_calls(), []);
    })
    .await
    .expect("the handoff test exceeded its bound");
}

#[tokio::test]
async fn availability_is_logged_on_each_change_and_not_on_each_read() {
    use super::{Availability, Unavailable, availability};
    let (_fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
    let available = Ok(provisioner);
    let missing = Err(Unavailable("the template does not exist".into()));
    let invalid = Err(Unavailable("the template is not usable".into()));
    assert_eq!(availability(None, &available), Availability::Unchanged);
    assert_eq!(
        availability(None, &missing),
        Availability::Unavailable("the template does not exist")
    );
    assert_eq!(
        availability(Some(&available), &missing),
        Availability::Unavailable("the template does not exist")
    );
    assert_eq!(
        availability(Some(&missing), &missing),
        Availability::Unchanged,
        "the same cause is told once"
    );
    assert_eq!(
        availability(Some(&missing), &invalid),
        Availability::Unavailable("the template is not usable"),
        "a new cause is told"
    );
    assert_eq!(
        availability(Some(&invalid), &available),
        Availability::Restored
    );
    assert_eq!(
        availability(Some(&available), &available),
        Availability::Unchanged
    );
}
