//! Computer runtime fixtures: the fake OpenShell gateway (`tests/factory/gateway.rs`), a
//! provisioner on it with the test catalog's browser client, profile and Work Context,
//! an owner's Computer and the handoff of its next sign-in. Every credential is a
//! synthetic string.
#[allow(dead_code, reason = "the Computer tests use part of the fake gateway")]
#[path = "../../tests/factory/gateway.rs"]
pub(super) mod fake;

use std::{collections::HashMap, num::NonZeroUsize, sync::Arc};

use chrono::{TimeDelta, Utc};
use openshell_sdk::{
    OpenShellClient,
    raw::proto::{self as p, ProviderCredentialRefreshStrategy as Strategy},
};
use secrecy::SecretString;
use uuid::Uuid;
use veoveo_hub_sdk::OAuthClientId;
use veoveo_types::{PrincipalId, TenantId};

use super::{
    ComputerId, ComputerProvisionSpec,
    gateway::Gateway,
    provisioning::{LoginGrant, Provisioner, metadata},
    spec::PersistenceBackend,
};
use crate::{
    HubCatalog,
    contract::{SecretLocator, SecretOwner, SecretPurpose, SecretReference, SecretSource},
    control_plane::Computers,
    custody_client::CustodyClient,
    store::Computer,
};

pub(super) const PUBLIC: &str = "https://hub.example";
pub(super) const LORE_PROFILE: &str = "vv-lore";
pub(super) const LORE_KEY: &str = "LORE_ACCESS_TOKEN";
pub(super) const OWNER: &str = "https://identity.example#alice";

pub(super) fn config() -> Computers {
    serde_json::from_value(serde_json::json!({
        "gateway": {"endpoint": "https://gateway.example/", "certificateAuthority": "/unused/ca", "bearerEnvironment": "UNUSED"},
        "template": {"workspace": "installation", "name": "computer"},
        "policyFile": "/unused/policy", "command": ["/usr/local/bin/opencode", "serve"],
        "oauthClient": "operator-local-public", "profile": "operator", "workContext": "operations", "port": 4096,
        "hubProvider": {"profile": "vv-computer-hub", "credentialKey": "COMPUTER_HUB_REFERENCE"},
        "modelProvider": {"profile": "vv-computer-model", "credentialKey": "DEEPINFRA_API_KEY", "secret": "computer-model-api-key"},
        "loreProvider": {"profile": LORE_PROFILE, "credentialKey": LORE_KEY}
    }))
    .unwrap()
}

/// A global profile with one bearer credential in `key`, refreshed at this Hub's token
/// endpoint with `refresh`.
pub(super) fn profile(id: &str, key: &str, refresh: Option<Strategy>) -> p::ProviderProfile {
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

pub(super) async fn gateway(lore: Strategy) -> (fake::Gateway, Provisioner) {
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

pub(super) fn provisioner(client: OpenShellClient, config: Computers) -> Provisioner {
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
    Provisioner::of(
        Gateway::of(client, NonZeroUsize::new(4).unwrap()),
        Arc::new(config),
        policy,
        PUBLIC,
        spec,
    )
}

pub(super) fn computer(provisioner: &Provisioner) -> Computer {
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
pub(super) fn next_login(computer: &Computer) -> (Computer, LoginGrant) {
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

pub(super) fn catalog() -> HubCatalog {
    HubCatalog::from_control_plane(
        serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap(),
        crate::catalog_fixture::binding(),
    )
    .unwrap()
}

pub(super) fn scope(name: &str) -> Option<p::WorkspaceSelector> {
    Some(p::workspace_selector(name.to_owned()))
}
