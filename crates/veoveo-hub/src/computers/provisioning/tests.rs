//! A Computer's custody handoff against the fake OpenShell gateway
//! (`tests/factory/gateway.rs`). The Hub clones the owner's template with the owner as
//! `LORE_IDENTITY`, creates the owner's providers, hands the user's refresh grant and
//! the Computer's own Lore client to the gateway, rotates both credentials and only then
//! creates or starts the sandbox, with the Lore provider attached. A later handoff keeps
//! the Lore provider and replaces only its client's secret. The fake reports synthetic
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
async fn a_handoff_rotates_the_owners_lore_access_before_the_sandbox_starts() {
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

        let calls = fake.take_calls();
        let [
            Call::CreateWorkspace(_),
            Call::CreateSandboxTemplate(template),
            Call::CreateProvider(_),
            Call::CreateProvider(_),
            Call::CreateProvider(lore),
            Call::ConfigureProviderRefresh(hub),
            Call::RotateProviderCredential(hub_rotation),
            Call::ConfigureProviderRefresh(refresh),
            Call::RotateProviderCredential(rotation),
            Call::CreateSandbox(create),
        ] = calls.as_slice()
        else {
            panic!("the first handoff sent {calls:#?}");
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
        assert_eq!(
            create.spec.as_ref().unwrap().providers,
            [HUB_PROVIDER, MODEL_PROVIDER, LORE_PROVIDER]
        );
        let sandbox = fake.sandbox(&name, &name).unwrap();
        provisioner.validate_sandbox(&computer, &sandbox).unwrap();
        let mut without_lore = sandbox.clone();
        without_lore.spec.as_mut().unwrap().providers.pop();
        assert!(
            provisioner
                .validate_sandbox(&computer, &without_lore)
                .is_err(),
            "a sandbox created without the Lore provider differs from the installation"
        );

        // The next login keeps the provider, and its placeholder, and replaces only the
        // client's secret, refreshing the token before the sandbox starts again.
        let (computer, grant) = next_login(&computer);
        let second = grant.lore.digest();
        assert_ne!(second, first);
        provisioner
            .handoff(&catalog, &computer, grant)
            .await
            .unwrap();
        let calls = fake.take_calls();
        let [
            Call::StopSandbox(_),
            Call::ConfigureProviderRefresh(_),
            Call::RotateProviderCredential(_),
            Call::ConfigureProviderRefresh(refresh),
            Call::RotateProviderCredential(rotation),
            Call::StartSandbox(_),
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
    })
    .await
    .expect("the handoff test exceeded its bound");
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
