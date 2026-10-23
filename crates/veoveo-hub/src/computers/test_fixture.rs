//! Computer runtime fixtures: the fake OpenShell gateway (`tests/factory/gateway.rs`)
//! serving the installation's template, a provisioner on it with the test catalog's
//! browser client, profile and Work Context, an owner's Computer and the handoff of its
//! next sign-in. Every credential is a synthetic string.
pub(super) use crate::test_gateway as fake;

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
    provisioning::{Installation, LoginGrant, Provisioner, metadata},
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
    let provisioner = provisioner(client, config);
    fake.set_template("installation", provisioner.spec.template().unwrap());
    (fake, provisioner)
}

/// The installation of `provisioner`'s parts, whose current version the fake's
/// installation template names.
pub(super) fn installation(provisioner: &Provisioner) -> Installation {
    Installation::of(provisioner)
}

/// The installation template, with `image` and a release note when one is given.
pub(super) fn installation_template(image: &str, note: Option<&str>) -> p::SandboxWorkloadTemplate {
    p::SandboxWorkloadTemplate {
        metadata: Some(p::ObjectMeta {
            name: "computer".into(),
            workspace: "installation".into(),
            annotations: note
                .map(|note| HashMap::from([(super::RELEASE_NOTE.into(), note.into())]))
                .unwrap_or_default(),
            ..Default::default()
        }),
        spec: Some(p::SandboxWorkloadTemplateSpec {
            workload: Some(p::SandboxWorkloadConfig {
                image: image.into(),
                environment: HashMap::from([("SSL_CERT_DIR".into(), "/etc/ssl/certs".into())]),
                ..Default::default()
            }),
            ..Default::default()
        }),
    }
}

/// The model seed's trusted Env locator. The variable Cargo and nextest set for every
/// test process stands in as a synthetic, nonsecret value.
pub(super) fn model_secret(config: &Computers) -> SecretReference {
    SecretReference {
        id: config.model_provider.secret.clone(),
        source: SecretSource::Env,
        purpose: SecretPurpose::ProviderApiKey,
        locator: SecretLocator::parse("CARGO_PKG_NAME").unwrap(),
        owner: SecretOwner::Hub,
        rotation_hint: None,
        metadata: serde_json::Value::Null,
    }
}

pub(super) fn provisioner(client: OpenShellClient, config: Computers) -> Provisioner {
    let policy = openshell_policy::validate_and_canonicalize_sandbox_policy(
        openshell_policy::parse_sandbox_policy("version: 1\nnetwork_policies: {}\n").unwrap(),
    )
    .unwrap();
    let mut template = installation_template("registry.internal/computer@sha256:0", None);
    template.metadata = Some(metadata("computer", "installation"));
    let spec = ComputerProvisionSpec::new(
        config.clone(),
        model_secret(&config),
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
        model_provider_request: Uuid::now_v7(),
        hub_provider_request: Uuid::now_v7(),
        handoff: Some(Uuid::now_v7()),
        configure_request: Some(Uuid::now_v7()),
        rotate_request: Some(Uuid::now_v7()),
        family: None,
        ready: false,
        provisioning: provisioner.spec.clone(),
        operation: None,
        failure: None,
        update_notice: None,
        collected_at: None,
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

/// The owner's ready Computer in `hub`'s store: its row with `provisioner`'s version, a
/// completed sign-in whose custody handoff ran on the fake gateway, and a browser
/// session bound to `cookie`. Creates no sandbox.
pub(super) async fn ready_computer(
    hub: &crate::HubState,
    catalog: &HubCatalog,
    provisioner: &Provisioner,
    cookie: &veoveo_types::Sha256Digest,
) -> Computer {
    use std::collections::BTreeSet;
    use veoveo_hub_sdk::{Principal, PrincipalKind, TokenIssuer, TokenSubject};
    let config = provisioner.config.as_ref();
    let profile = catalog.profile(&config.profile).unwrap();
    let tenant = TenantId::parse("tenant-a").unwrap();
    let principal = Principal {
        id: PrincipalId::parse(OWNER).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://identity.example").unwrap(),
        subject: TokenSubject::parse("alice").unwrap(),
        tenant: Some(tenant.clone()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: profile.required_scopes.iter().cloned().collect(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    };
    let issued = hub
        .issue_refresh_token(crate::HubRefreshIssueRequest {
            authorization_server: &profile.authorization_server,
            protected_resource: &profile.protected_resource,
            oauth_client_id: &config.oauth_client,
            work_context: &config.work_context,
            principal: &principal,
            principal_display_name: &crate::contract::PrincipalDisplayName::new("Owner").unwrap(),
            scopes: &principal.scopes,
            now: Utc::now(),
        })
        .await
        .unwrap();
    let store = hub.hub_store();
    let computer = store
        .ensure_computer(&tenant, &principal.id, &provisioner.spec)
        .await
        .unwrap();
    let (_, grant) = next_login(&computer);
    let handoff = store
        .begin_computer_handoff(&computer, &grant.lore.digest())
        .await
        .unwrap();
    let pending = store
        .computer(&tenant, &principal.id, &computer.id)
        .await
        .unwrap()
        .unwrap();
    provisioner.handoff(catalog, &pending, grant).await.unwrap();
    store
        .lock_computer_handoff(&pending, handoff)
        .await
        .unwrap()
        .unwrap()
        .complete(&issued.grant.family_id, cookie)
        .await
        .unwrap();
    store
        .computer(&tenant, &principal.id, &computer.id)
        .await
        .unwrap()
        .unwrap()
}

/// A Hub over a disposable database, admitting the test catalog's browser client.
pub(super) fn hub(db: &crate::test_store::TestDb) -> crate::HubState {
    crate::HubState::new(db.a.clone())
        .bind_token_extensions(Default::default())
        .unwrap()
        .bind_oauth_client_resolver(Arc::new(crate::oauth_clients::CatalogOAuthClientResolver))
        .unwrap()
}

/// The runtime of the Computer routes on `installation`.
pub(super) fn runtime(
    hub: crate::HubState,
    catalog: HubCatalog,
    installation: Installation,
) -> super::ComputerRuntime {
    super::ComputerRuntime {
        catalog: crate::HubCatalogHandle::new(Arc::new(catalog)),
        hub,
        config: installation.config.clone(),
        public: url::Url::parse(&format!("{PUBLIC}/")).unwrap(),
        http: reqwest::Client::new(),
        installation,
        browser: Default::default(),
    }
}
