//! A Computer's custody handoff against the fake OpenShell gateway
//! (`tests/factory/gateway.rs`). The Hub clones the owner's template with the owner as
//! `LORE_IDENTITY`, creates the owner's providers, hands the user's refresh grant and
//! the Computer's own Lore client to the gateway, rotates both credentials and only then
//! creates or starts the sandbox, with the Lore provider attached. A later handoff keeps
//! the Lore provider and replaces only its client's secret. The fake reports synthetic
//! rotation expiries, makes sandboxes ready at once and accepts forwarding without
//! relaying bytes; no OpenShell, Lore, Hub token endpoint or model is involved, and every
//! credential is a synthetic string.
#[allow(dead_code, reason = "the handoff tests use part of the fake gateway")]
#[path = "../../../tests/factory/gateway.rs"]
mod fake;

use std::{collections::HashMap, num::NonZeroUsize, sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use fake::Call;
use openshell_sdk::{OpenShellClient, raw::proto as p};
use secrecy::SecretString;
use uuid::Uuid;
use veoveo_hub_sdk::OAuthClientId;
use veoveo_types::{PrincipalId, TenantId};

use super::{
    HUB_PROVIDER, LORE_PROVIDER, LoginGrant, MODEL_PROVIDER, Provisioner, Strategy, metadata,
};
use crate::{
    HubCatalog,
    computers::{ComputerId, ComputerProvisionSpec, gateway::Gateway, spec::PersistenceBackend},
    contract::{SecretLocator, SecretOwner, SecretPurpose, SecretReference, SecretSource},
    control_plane::Computers,
    custody_client::CustodyClient,
    registration,
    store::Computer,
};

const PUBLIC: &str = "https://hub.example";
const LORE_PROFILE: &str = "vv-lore";
const LORE_KEY: &str = "LORE_ACCESS_TOKEN";
const OWNER: &str = "https://identity.example#alice";

fn config() -> Computers {
    serde_json::from_value(serde_json::json!({
        "gateway": {"endpoint": "https://gateway.example/", "certificateAuthority": "/unused/ca", "bearerEnvironment": "UNUSED"},
        "template": {"workspace": "installation", "name": "computer"},
        "policyFile": "/unused/policy", "command": ["/usr/local/bin/opencode", "serve"],
        "oauthClient": "computer-browser", "profile": "computer", "workContext": "default", "port": 4096,
        "hubProvider": {"profile": "vv-computer-hub", "credentialKey": "COMPUTER_HUB_REFERENCE"},
        "modelProvider": {"profile": "vv-computer-model", "credentialKey": "DEEPINFRA_API_KEY", "secret": "computer-model-api-key"},
        "loreProvider": {"profile": LORE_PROFILE, "credentialKey": LORE_KEY}
    }))
    .unwrap()
}

/// A global profile with one bearer credential in `key`, refreshed at this Hub's token
/// endpoint with `refresh`.
fn profile(id: &str, key: &str, refresh: Option<Strategy>) -> p::ProviderProfile {
    p::ProviderProfile {
        id: id.into(),
        scope: "platform".into(),
        credentials: vec![p::ProviderProfileCredential {
            name: "access_token".into(),
            env_vars: vec![key.into()],
            required: true,
            auth_style: "bearer".into(),
            header_name: "authorization".into(),
            refresh: refresh.map(|strategy| p::ProviderCredentialRefresh {
                strategy: strategy as i32,
                token_url: format!("{PUBLIC}/oauth/token"),
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    }
}

async fn gateway(lore: Strategy) -> (fake::Gateway, Provisioner) {
    let (fake, client) = fake::Gateway::start().await;
    let config = config();
    fake.set_profile(profile(
        &config.hub_provider.profile,
        &config.hub_provider.credential_key,
        Some(Strategy::Oauth2RefreshToken),
    ));
    fake.set_profile(profile(
        &config.model_provider.profile,
        &config.model_provider.credential_key,
        None,
    ));
    fake.set_profile(profile(LORE_PROFILE, LORE_KEY, Some(lore)));
    fake.ready_on_start();
    (fake, provisioner(client, config))
}

fn provisioner(client: OpenShellClient, config: Computers) -> Provisioner {
    let policy = openshell_policy::validate_and_canonicalize_sandbox_policy(
        openshell_policy::parse_sandbox_policy("version: 1\nnetwork_policies: {}\n").unwrap(),
    )
    .unwrap();
    let template = p::SandboxWorkloadTemplate {
        metadata: Some(metadata("computer", "installation")),
        spec: Some(p::SandboxWorkloadTemplateSpec {
            workload: Some(p::SandboxWorkloadConfig {
                image: "registry.internal/computer@sha256:0".into(),
                environment: HashMap::from([("SSL_CERT_DIR".into(), "/etc/ssl/certs".into())]),
                ..Default::default()
            }),
            ..Default::default()
        }),
    };
    // The model seed resolves through its trusted Env locator. The variable Cargo and
    // nextest set for every test process stands in as a synthetic, nonsecret value.
    let model_secret = SecretReference {
        id: config.model_provider.secret.clone(),
        source: SecretSource::Env,
        purpose: SecretPurpose::ProviderApiKey,
        locator: SecretLocator::parse("CARGO_PKG_NAME").unwrap(),
        owner: SecretOwner::Hub,
        rotation_hint: None,
        metadata: serde_json::Value::Null,
    };
    let spec = ComputerProvisionSpec::new(
        config.clone(),
        model_secret,
        format!("{PUBLIC}/mcp/computer"),
        PersistenceBackend::Kubernetes,
        &policy,
        &template,
    );
    Provisioner {
        gateway: Gateway::of(client, NonZeroUsize::new(4).unwrap()),
        config: Arc::new(config),
        policy,
        public: PUBLIC.into(),
        spec,
    }
}

fn computer(provisioner: &Provisioner) -> Computer {
    Computer {
        id: ComputerId::new(),
        tenant: TenantId::parse("tenant-a").unwrap(),
        principal: PrincipalId::parse(OWNER).unwrap(),
        workspace_request: Uuid::now_v7(),
        template_request: Uuid::now_v7(),
        model_provider_request: Uuid::now_v7(),
        hub_provider_request: Uuid::now_v7(),
        sandbox_request: Uuid::now_v7(),
        handoff: Some(Uuid::now_v7()),
        configure_request: Some(Uuid::now_v7()),
        rotate_request: Some(Uuid::now_v7()),
        family: None,
        ready: false,
        provisioning: provisioner.spec.clone(),
    }
}

/// The handoff the next login begins: new request IDs and a new Lore client secret.
fn next_login(computer: &Computer) -> (Computer, LoginGrant) {
    let mut next = computer.clone();
    next.handoff = Some(Uuid::now_v7());
    next.configure_request = Some(Uuid::now_v7());
    next.rotate_request = Some(Uuid::now_v7());
    let grant = LoginGrant {
        access: SecretString::from("synthetic-access"),
        client: OAuthClientId::parse("computer-browser").unwrap(),
        refresh: SecretString::from("synthetic-refresh"),
        access_expiry: Utc::now() + TimeDelta::seconds(300),
        lore: CustodyClient::new(computer.native_name()).unwrap(),
    };
    (next, grant)
}

fn catalog() -> HubCatalog {
    HubCatalog::from_control_plane(
        serde_json::from_str(include_str!("../../../testdata/hub.json")).unwrap(),
        crate::catalog_fixture::binding(),
    )
    .unwrap()
}

fn scope(name: &str) -> Option<p::WorkspaceSelector> {
    Some(p::workspace_selector(name.to_owned()))
}

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
