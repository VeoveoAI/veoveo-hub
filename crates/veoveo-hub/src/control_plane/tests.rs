use super::*;
use crate::contract::{CertificateAuthorityFilePath, HubAction, SecretLocator, SecretSource};
use veoveo_hub_sdk::registration::{Access, Risk};
use veoveo_types::ResourceUriTemplate;

fn identity_provider() -> IdentityProvider {
    IdentityProvider {
        id: IdentityProviderId::parse("enterprise").unwrap(),
        issuer: TokenIssuer::parse("https://idp.example.com").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
        },
        claim_mapping: IdentityProviderClaimMapping::default(),
        trusted_certificate_authorities: Vec::new(),
        authorization_endpoint: Some(
            OAuthEndpointUrl::new("https://idp.example.com/oauth2/authorize").unwrap(),
        ),
        token_endpoint: Some(
            OAuthEndpointUrl::new("https://idp.example.com/oauth2/token").unwrap(),
        ),
        enterprise_managed_authorization_endpoint: Some(
            OAuthEndpointUrl::new("https://idp.example.com/oauth2/id-jag").unwrap(),
        ),
        metadata: Value::Null,
    }
}

fn authorization_server() -> ResourceAuthorizationServer {
    ResourceAuthorizationServer {
        id: AuthorizationServerId::parse("veoveo").unwrap(),
        issuer: TokenIssuer::parse("https://veoveo.example/oauth").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://veoveo.example/oauth/jwks.json").unwrap(),
        },
        access_token_key_id: Some(JwtId::parse("test-key").unwrap()),
        access_token_signing_key: Some(
            SecretReferenceId::parse("veoveo_access_token_private_key").unwrap(),
        ),
        identity_provider: Some(IdentityProviderId::parse("enterprise").unwrap()),
        authorization_endpoint: Some(
            OAuthEndpointUrl::new("https://veoveo.example/oauth/authorize").unwrap(),
        ),
        token_endpoint: OAuthEndpointUrl::new("https://veoveo.example/oauth/token").unwrap(),
        metadata: Value::Null,
    }
}

fn signing_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::parse("veoveo_access_token_private_key").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::JwksPrivateKey,
        locator: SecretLocator::parse("VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64").unwrap(),
        owner: SecretOwner::Hub,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn oidc_client_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::parse("enterprise_oidc_client_secret").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::OAuthClientSecret,
        locator: SecretLocator::parse("VEOVEO_IDP_OIDC_CLIENT_SECRET").unwrap(),
        owner: SecretOwner::Hub,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn registration_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::parse("registration_secret").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::RegistrationSecret,
        locator: SecretLocator::parse("VEOVEO_REGISTRATION_SECRET").unwrap(),
        owner: SecretOwner::Hub,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn default_authorization() -> InstallationAuthorization {
    InstallationAuthorization {
        roles: vec![
            InstallationRole {
                name: RoleId::parse("member").unwrap(),
                access: BTreeSet::from([Access::Read, Access::Write]),
                risks: BTreeSet::new(),
            },
            InstallationRole {
                name: RoleId::parse("admin").unwrap(),
                access: BTreeSet::from([Access::Read, Access::Write]),
                risks: BTreeSet::from([Risk::Destructive, Risk::Administer]),
            },
        ],
        grants: vec![
            RoleGrant {
                role: RoleId::parse("member").unwrap(),
                subject: GrantSubject::Group(GroupId::parse("tenant-members").unwrap()),
                tenant: Some(TenantId::parse("tenant-a").unwrap()),
            },
            RoleGrant {
                role: RoleId::parse("admin").unwrap(),
                subject: GrantSubject::Group(GroupId::parse("installation-admins").unwrap()),
                tenant: None,
            },
        ],
        overrides: vec![PermissionOverride::Deny {
            permission: veoveo_types::PermissionName::parse("time.admin").unwrap(),
        }],
    }
}

fn default_secrets() -> Vec<SecretReference> {
    vec![signing_secret(), oidc_client_secret()]
}

fn control_plane_with_secrets(secrets: Vec<SecretReference>) -> HubControlPlane {
    HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        profiles: vec![default_profile()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets,
        metadata: Value::Null,
    }
}

#[test]
fn control_plane_lists_every_public_file_mount() {
    let mut control_plane = control_plane_with_secrets(default_secrets());
    control_plane.identity_providers[0].jwks = JwksSource::File {
        path: JwksFilePath::new("/etc/veoveo/hub/identity-jwks.json").unwrap(),
    };
    control_plane.identity_providers[0]
        .trusted_certificate_authorities
        .push(CertificateAuthoritySource::File {
            path: CertificateAuthorityFilePath::parse("/etc/veoveo/hub/identity-ca.pem").unwrap(),
        });
    control_plane.authorization_servers[0].jwks = JwksSource::File {
        path: JwksFilePath::new("/etc/veoveo/hub/authorization-jwks.json").unwrap(),
    };
    control_plane.oauth_clients[0].jwks = Some(JwksSource::File {
        path: JwksFilePath::new("/etc/veoveo/hub/machine-jwks.json").unwrap(),
    });

    assert_eq!(
        control_plane.jwks_file_paths(),
        BTreeSet::from([
            "/etc/veoveo/hub/authorization-jwks.json",
            "/etc/veoveo/hub/identity-jwks.json",
            "/etc/veoveo/hub/machine-jwks.json",
        ])
    );
    assert_eq!(
        control_plane.certificate_authority_file_paths(),
        BTreeSet::from(["/etc/veoveo/hub/identity-ca.pem"])
    );
}

#[test]
fn external_issuers_need_no_local_signing_authority() {
    let mut config = control_plane_with_secrets(default_secrets());
    config.authorization_servers[0].access_token_key_id = None;
    config.authorization_servers[0].access_token_signing_key = None;
    config
        .secrets
        .retain(|secret| secret.purpose != SecretPurpose::JwksPrivateKey);
    config
        .validate(&crate::catalog_fixture::registry())
        .expect("external verifier configuration");
    config.authorization_servers[0].access_token_key_id =
        Some(JwtId::parse("unpaired-key").unwrap());
    assert!(matches!(
        config.validate(&crate::catalog_fixture::registry()),
        Err(HubControlPlaneError::InvalidAuthorizationServerSigningConfiguration(_))
    ));
}

fn default_policy() -> PolicySet {
    PolicySet {
        version: PolicyVersion::parse("2026-07-02").unwrap(),
        rules: vec![PolicyRule {
            id: PolicyRuleId::parse("allow_admin_read").unwrap(),
            effect: PolicyEffect::Allow,
            actions: BTreeSet::from([HubAction::AdminRead.into()]),
            profiles: BTreeSet::from([HubProfileId::parse("default").unwrap()]),
            protected_resources: BTreeSet::new(),
            principal_ids: BTreeSet::new(),
            tenant_ids: BTreeSet::new(),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            required_scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
            required_data_labels: BTreeSet::new(),
            required_assurances: BTreeSet::new(),
            metadata: Value::Null,
        }],
        metadata: Value::Null,
    }
}

fn default_data_labels() -> Vec<DataLabelDefinition> {
    vec![
        DataLabelDefinition {
            id: DataLabelId::parse("cui").unwrap(),
            title: Some("Controlled Unclassified Information".to_string()),
            description: None,
            regulated: true,
            metadata: Value::Null,
        },
        DataLabelDefinition {
            id: DataLabelId::parse("itar").unwrap(),
            title: Some("ITAR-controlled data".to_string()),
            description: None,
            regulated: true,
            metadata: Value::Null,
        },
        DataLabelDefinition {
            id: DataLabelId::parse("pii").unwrap(),
            title: Some("Personally Identifiable Information".to_string()),
            description: None,
            regulated: true,
            metadata: Value::Null,
        },
    ]
}

fn default_tenants() -> Vec<TenantDefinition> {
    vec![TenantDefinition {
        id: TenantId::parse("tenant-a").unwrap(),
        title: Some("Tenant A".to_string()),
        description: None,
        metadata: Value::Null,
    }]
}

fn default_work_contexts() -> Vec<crate::control_plane::WorkContextDefinition> {
    vec![crate::control_plane::WorkContextDefinition {
        id: WorkContextId::parse("default").unwrap(),
        tenant: TenantId::parse("tenant-a").unwrap(),
        title: "Default work".to_owned(),
        policy_revision: PolicyVersion::parse("2026-07-02").unwrap(),
        output_policy: veoveo_types::WorkContextOutputPolicy {
            owner: veoveo_types::AccessSubject::Group(GroupId::parse("operations").unwrap()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        memberships: vec![crate::control_plane::WorkContextMembershipRule {
            level: veoveo_types::WorkContextMembershipLevel::Owner,
            principals: BTreeSet::new(),
            groups: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::parse("operator").unwrap()]),
            oauth_clients: BTreeSet::from([
                OAuthClientId::parse("operator-local-public").unwrap(),
                OAuthClientId::parse("operator-hosted-public").unwrap(),
                OAuthClientId::parse("operator-service").unwrap(),
            ]),
        }],
    }]
}

fn default_profile() -> HubProfile {
    HubProfile {
        id: HubProfileId::parse("default").unwrap(),
        identity_provider: IdentityProviderId::parse("enterprise").unwrap(),
        authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
        protected_resource: ProtectedResourceId::parse("https://veoveo.example/mcp/operator")
            .unwrap(),
        policy_version: PolicyVersion::parse("2026-07-02").unwrap(),
        auth_modes: BTreeSet::from([
            AuthMode::EnterpriseManagedAuthorization,
            AuthMode::OAuthClientCredentials,
            AuthMode::OidcAuthorizationCodePkce,
        ]),
        discovery_failure_mode: DiscoveryFailureMode::Isolate,
        required_scopes: vec![ScopeName::parse("operator:use").unwrap()],
        metadata: Value::Null,
    }
}

fn default_oauth_clients() -> Vec<OAuthClientRegistration> {
    vec![
        OAuthClientRegistration {
            id: OAuthClientId::parse("operator-local-public").unwrap(),
            authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
            default_work_context: WorkContextId::parse("default").unwrap(),
            invocation_mode: veoveo_types::InvocationMode::Direct,
            display_name: Some("Veoveo Operator Local Client".to_string()),
            client_surface: OAuthClientSurface::FullMcp,
            direct_task_call_adapter: false,
            allowed_resources: BTreeSet::from([ProtectedResourceId::parse(
                "https://veoveo.example/mcp/operator",
            )
            .unwrap()]),
            grant_types: BTreeSet::from([
                OAuthGrantType::AuthorizationCodePkce,
                OAuthGrantType::RefreshToken,
                OAuthGrantType::EnterpriseManagedAuthorization,
            ]),
            auth_methods: BTreeSet::from([OAuthClientAuthMethod::None]),
            redirect_uris: vec![
                OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
                OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").unwrap(),
            ],
            allowed_scopes: BTreeSet::from([
                ScopeName::parse("operator:use").unwrap(),
                ScopeName::parse("admin:manage").unwrap(),
            ]),
            credential_secret: None,
            jwks: None,
            tenant: None,
            metadata: Value::Null,
        },
        OAuthClientRegistration {
            id: OAuthClientId::parse("operator-service").unwrap(),
            authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
            default_work_context: WorkContextId::parse("default").unwrap(),
            invocation_mode: veoveo_types::InvocationMode::Automated,
            display_name: Some("Veoveo Operator Service".to_string()),
            client_surface: OAuthClientSurface::FullMcp,
            direct_task_call_adapter: false,
            allowed_resources: BTreeSet::from([ProtectedResourceId::parse(
                "https://veoveo.example/mcp/operator",
            )
            .unwrap()]),
            grant_types: BTreeSet::from([OAuthGrantType::ClientCredentials]),
            auth_methods: BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt]),
            redirect_uris: vec![],
            allowed_scopes: BTreeSet::from([
                ScopeName::parse("operator:use").unwrap(),
                ScopeName::parse("admin:manage").unwrap(),
            ]),
            credential_secret: None,
            jwks: Some(JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/clients/jwks.json")
                    .unwrap(),
            }),
            tenant: None,
            metadata: Value::Null,
        },
    ]
}

fn default_oidc_clients() -> Vec<IdentityProviderOidcClientRegistration> {
    vec![IdentityProviderOidcClientRegistration {
        id: OidcClientRegistrationId::parse("enterprise").unwrap(),
        identity_provider: IdentityProviderId::parse("enterprise").unwrap(),
        authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
        allowed_resources: BTreeSet::from([ProtectedResourceId::parse(
            "https://veoveo.example/mcp/operator",
        )
        .unwrap()]),
        client_id: OidcClientId::parse("veoveo").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
        auth_method: OidcClientAuthMethod::ClientSecretPost,
        credential_secret: SecretReferenceId::parse("enterprise_oidc_client_secret").unwrap(),
        scopes: BTreeSet::from([
            ScopeName::parse("openid").unwrap(),
            ScopeName::parse("profile").unwrap(),
            ScopeName::parse("email").unwrap(),
        ]),
        metadata: Value::Null,
    }]
}

#[test]
fn identifiers_reject_invalid_wire_values() {
    assert!(ServerSlug::parse("Media").is_err());
    assert!(veoveo_types::ResourceScheme::parse("1media").is_err());
    assert!(HubProfileId::parse("default/profile").is_err());
    assert!(OAuthRedirectUri::new("https://veoveo.example/oauth/callback").is_ok());
    assert!(OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").is_ok());
    assert!(OAuthRedirectUri::new("http://[::1]:8789/oauth/callback").is_ok());
    assert!(OAuthRedirectUri::new("http://example.com/oauth/callback").is_err());
    assert!(OAuthRedirectUri::new("http://127.0.0.1/oauth/callback").is_err());
    assert!(OAuthRedirectUri::new("http://127.0.0.1:0/oauth/callback").is_err());
    assert!(OAuthStateValue::parse("oauth-state-1").is_ok());
    assert!(OAuthStateValue::parse("oauth state").is_err());
    assert!(OidcNonce::parse("nonce-1").is_ok());
    assert!(OAuthAuthorizationCode::parse("a".repeat(43)).is_ok());
    assert!(OAuthAuthorizationCode::parse("short").is_err());
    assert!(PrincipalDisplayName::new("Mara Chen").is_ok());
    assert!(PrincipalDisplayName::new("mara.chen@example.com").is_ok());
    assert!(PrincipalDisplayName::new("").is_err());
    assert!(PrincipalDisplayName::new(" Mara Chen ").is_err());
    assert!(PrincipalDisplayName::new("Mara\nChen").is_err());
    assert!(PrincipalDisplayName::new("x".repeat(257)).is_err());
    assert!(PkceCodeChallenge::parse("A".repeat(43)).is_ok());
    assert!(PkceCodeVerifier::parse("a".repeat(129)).is_err());
    assert!(ResourceUri::new("media://artifact/abc").is_ok());
    assert!(ResourceUriTemplate::new("media://model").is_err());
}

#[test]
fn resource_uri_templates_match_simple_resource_uris() {
    let model_template = ResourceUriTemplate::new("media://model/{model_id}").unwrap();
    assert!(model_template.matches_uri(&ResourceUri::new("media://model/provider/model").unwrap()));
    assert!(model_template.matches_uri(&ResourceUri::new("media://model/simple").unwrap()));
    assert!(!model_template.matches_uri(&ResourceUri::new("media://models").unwrap()));
    assert!(
        !model_template
            .matches_uri(&ResourceUri::new("simulation://model/provider/model").unwrap())
    );

    let usage_template = ResourceUriTemplate::new("media://usage/task/{task_id}").unwrap();
    assert!(usage_template.matches_uri(&ResourceUri::new("media://usage/task/task-1").unwrap()));
    assert!(!usage_template.matches_uri(&ResourceUri::new("media://usage/task/").unwrap()));
    assert!(!usage_template.matches_uri(&ResourceUri::new("media://usage").unwrap()));
}

#[test]
fn resource_uri_templates_reject_unsupported_expressions() {
    assert!(ResourceUriTemplate::new("media://model/{}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{+model_id}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{model id}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{model_id").is_err());
    assert!(ResourceUriTemplate::new("media://model/model_id}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{left}{right}").is_err());
}

#[test]
fn auth_modes_expose_mcp_extension_ids() {
    assert_eq!(
        AuthMode::EnterpriseManagedAuthorization.mcp_extension_id(),
        Some(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
    );
    assert_eq!(
        AuthMode::OAuthClientCredentials.mcp_extension_id(),
        Some(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION)
    );
    assert_eq!(AuthMode::OidcAuthorizationCodePkce.mcp_extension_id(), None);
}

#[test]
fn hub_actions_expose_final_subscription_and_task_methods() {
    assert_eq!(
        HubAction::SubscriptionsListen.mcp_method(),
        Some("subscriptions/listen")
    );
    assert_eq!(HubAction::TasksUpdate.mcp_method(), Some("tasks/update"));
    assert_eq!(HubAction::TasksCancel.mcp_method(), Some("tasks/cancel"));
}

#[test]
fn control_plane_validates_cross_references() {
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: default_authorization(),
        registration: Some(RegistrationSettings {
            secret: SecretReferenceId::parse("registration_secret").unwrap(),
        }),
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: vec![
            signing_secret(),
            oidc_client_secret(),
            registration_secret(),
        ],
        metadata: Value::Null,
    };

    config
        .validate(&crate::catalog_fixture::registry())
        .expect("valid hub control plane");
}

#[test]
fn control_plane_rejects_unknown_identity_provider_reference() {
    let mut profile = default_profile();
    profile.identity_provider = IdentityProviderId::parse("missing").unwrap();
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown identity provider must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownIdentityProvider { .. }
    ));
}

#[test]
fn control_plane_rejects_unknown_authorization_server_reference() {
    let mut profile = default_profile();
    profile.authorization_server = AuthorizationServerId::parse("missing").unwrap();
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown authorization server must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownAuthorizationServer { .. }
    ));
}

#[test]
fn control_plane_rejects_authorization_server_unknown_identity_provider() {
    let mut authorization_server = authorization_server();
    authorization_server.identity_provider = Some(IdentityProviderId::parse("missing").unwrap());
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("authorization server IdP reference must be known");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownAuthorizationServerIdentityProvider { .. }
    ));
}

#[test]
fn control_plane_rejects_profile_without_auth_modes() {
    let mut profile = default_profile();
    profile.auth_modes.clear();
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("empty auth modes must fail");

    assert!(matches!(err, HubControlPlaneError::MissingAuthModes { .. }));
}

#[test]
fn control_plane_rejects_oidc_profile_without_browser_endpoints() {
    let mut auth_server = authorization_server();
    auth_server.authorization_endpoint = None;
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![auth_server],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("OIDC browser auth requires authorization endpoint");

    assert!(matches!(
        err,
        HubControlPlaneError::MissingAuthorizationServerEndpoint {
            endpoint: AuthorizationServerEndpoint::Authorization,
            ..
        }
    ));

    let mut idp = identity_provider();
    idp.token_endpoint = None;
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![idp],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("OIDC browser auth requires token endpoint");

    assert!(matches!(
        err,
        HubControlPlaneError::MissingIdentityProviderEndpoint {
            endpoint: IdentityProviderEndpoint::Token,
            ..
        }
    ));
}

#[test]
fn control_plane_rejects_extension_auth_modes_without_matching_endpoints() {
    let mut idp = identity_provider();
    idp.enterprise_managed_authorization_endpoint = None;
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::EnterpriseManagedAuthorization]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![idp],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("enterprise-managed auth requires endpoint");

    assert!(matches!(
        err,
        HubControlPlaneError::MissingIdentityProviderEndpoint {
            endpoint: IdentityProviderEndpoint::EnterpriseManagedAuthorization,
            ..
        }
    ));
}

#[test]
fn control_plane_rejects_unknown_secret_owner_references() {
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: vec![
            signing_secret(),
            SecretReference {
                id: SecretReferenceId::parse("profile_secret").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::OAuthClientSecret,
                locator: SecretLocator::parse("PROFILE_SECRET").unwrap(),
                owner: SecretOwner::Profile {
                    profile: HubProfileId::parse("missing").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            },
        ],
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown profile secret owner must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownSecretOwnerProfile { .. }
    ));

    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: vec![
            signing_secret(),
            SecretReference {
                id: SecretReferenceId::parse("tenant_secret").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::TokenExchangeCredential,
                locator: SecretLocator::parse("TENANT_SECRET").unwrap(),
                owner: SecretOwner::Tenant {
                    tenant: TenantId::parse("missing").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            },
        ],
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown tenant secret owner must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownSecretOwnerTenant { .. }
    ));
}

#[test]
fn control_plane_rejects_missing_oauth_client_for_auth_mode() {
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: vec![],
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("profile auth modes require OAuth clients");

    assert!(matches!(
        err,
        HubControlPlaneError::MissingOAuthClientForAuthMode { .. }
    ));
}

#[test]
fn control_plane_allows_service_profile_before_durable_client_admission() {
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OAuthClientCredentials]);
    let mut contexts = default_work_contexts();
    for context in &mut contexts {
        for rule in &mut context.memberships {
            rule.oauth_clients.clear();
        }
    }
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: contexts,
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: vec![],
        oidc_clients: vec![],
        secrets: default_secrets(),
        metadata: Value::Null,
    };
    config
        .validate(&crate::catalog_fixture::registry())
        .expect("durable service registration follows installation");
}

#[test]
fn control_plane_rejects_missing_oidc_client_for_browser_auth() {
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![profile],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: vec![],
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("browser OIDC auth requires an OIDC client registration");

    assert!(matches!(
        err,
        HubControlPlaneError::MissingOidcClientForProfile { .. }
    ));
}

#[test]
fn control_plane_rejects_oidc_client_without_openid_scope() {
    let mut clients = default_oidc_clients();
    clients[0]
        .scopes
        .remove(&ScopeName::parse("openid").unwrap());
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: clients,
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("OIDC clients must request the openid scope");

    assert!(matches!(
        err,
        HubControlPlaneError::OidcClientMissingOpenIdScope(_)
    ));
}

#[test]
fn control_plane_rejects_public_client_credentials() {
    let mut clients = default_oauth_clients();
    clients[1].auth_methods = BTreeSet::from([OAuthClientAuthMethod::None]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("client credentials must not be public");

    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientPublicClientCredentials(_)
    ));
}

#[test]
fn control_plane_rejects_private_key_jwt_without_jwks() {
    let mut clients = default_oauth_clients();
    clients[1].jwks = None;
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("private-key JWT clients require a JWKS source");

    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientMissingJwks { .. }
    ));
}

#[test]
fn control_plane_rejects_unsupported_oauth_client_auth_combinations() {
    let mut clients = default_oauth_clients();
    clients[0].auth_methods = BTreeSet::from([
        OAuthClientAuthMethod::None,
        OAuthClientAuthMethod::PrivateKeyJwt,
    ]);
    clients[0].jwks = Some(JwksSource::Remote {
        jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/browser/jwks.json").unwrap(),
    });
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("browser OAuth client must remain public none-auth");

    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));

    let mut clients = default_oauth_clients();
    clients[0].grant_types = BTreeSet::from([OAuthGrantType::RefreshToken]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("refresh tokens require an authorization-code grant on the same client");
    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));

    let mut clients = default_oauth_clients();
    clients[1].auth_methods = BTreeSet::from([
        OAuthClientAuthMethod::PrivateKeyJwt,
        OAuthClientAuthMethod::ClientSecretPost,
    ]);
    clients[1].credential_secret =
        Some(SecretReferenceId::parse("enterprise_oidc_client_secret").unwrap());
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("hosted client credentials must select one authentication method");

    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));

    let mut clients = default_oauth_clients();
    clients[1].grant_types = BTreeSet::from([
        OAuthGrantType::AuthorizationCodePkce,
        OAuthGrantType::ClientCredentials,
    ]);
    clients[1].redirect_uris =
        vec![OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap()];
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("one OAuth client must not mix browser and client credentials grants");

    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));
}

#[test]
fn control_plane_rejects_oauth_client_missing_required_scope() {
    let mut clients = default_oauth_clients();
    clients[0]
        .allowed_scopes
        .remove(&ScopeName::parse("operator:use").unwrap());
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("client allowed scopes must cover profile and policy scopes");

    assert!(matches!(
        err,
        HubControlPlaneError::OAuthClientMissingAllowedScope { .. }
    ));
}

#[test]
fn control_plane_rejects_duplicate_tenants() {
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: vec![
            TenantDefinition {
                id: TenantId::parse("tenant-a").unwrap(),
                title: Some("Tenant A".to_string()),
                description: None,
                metadata: Value::Null,
            },
            TenantDefinition {
                id: TenantId::parse("tenant-a").unwrap(),
                title: Some("Tenant A duplicate".to_string()),
                description: None,
                metadata: Value::Null,
            },
        ],
        work_contexts: default_work_contexts(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("duplicate tenants must fail");

    assert!(matches!(err, HubControlPlaneError::DuplicateTenant(_)));
}

#[test]
fn control_plane_rejects_duplicate_policy_rule_ids() {
    let mut policy = default_policy();
    policy.rules.push(policy.rules[0].clone());
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("duplicate policy rule ids must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::DuplicatePolicyRule { .. }
    ));
}

#[test]
fn control_plane_rejects_unknown_policy_rule_references() {
    let mut policy = default_policy();
    policy.rules[0].profiles = BTreeSet::from([HubProfileId::parse("missing").unwrap()]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown policy profile must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownPolicyRuleProfile { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].required_data_labels =
        BTreeSet::from([DataLabelId::parse("unknown_label").unwrap()]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown policy data label must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownPolicyRuleDataLabel { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].tenant_ids = BTreeSet::from([TenantId::parse("missing").unwrap()]);
    let config = HubControlPlane {
        execution: None,
        branding: None,
        native_upstreams: None,
        computers: None,
        factory: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        authorization: InstallationAuthorization::default(),
        registration: None,
        profiles: vec![default_profile()],
        extensions: Default::default(),
        tenants: default_tenants(),
        work_contexts: default_work_contexts(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("unknown policy tenant must fail");

    assert!(matches!(
        err,
        HubControlPlaneError::UnknownPolicyRuleTenant { .. }
    ));
}

#[test]
fn policy_decision_defaults_to_explicit_deny() {
    let decision = PolicyDecision::deny(
        HubProfileId::parse("default").unwrap(),
        HubAction::ToolsCall,
        PolicyTarget::Tool {
            server: ServerSlug::parse("media").unwrap(),
            tool: LocalToolName::parse("run").unwrap(),
        },
        PolicyReasonCode::MissingScope,
        TraceId::parse("trace-1").unwrap(),
    );

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
}

#[test]
fn refresh_tokens_are_redacted_from_diagnostics_but_serialize_on_the_wire() {
    let raw = "R".repeat(43);
    let token = OAuthRefreshToken::parse(raw.clone()).unwrap();

    assert_eq!(token.to_string(), "[REDACTED]");
    assert!(!format!("{token:?}").contains(&raw));
    assert_eq!(serde_json::to_value(token).unwrap(), serde_json::json!(raw));
}

#[test]
fn normalized_identity_and_decision_reject_unknown_fields() {
    let principal = serde_json::json!({
        "id": "alice", "kind": "user", "issuer": "https://idp.example.com",
        "subject": "alice"
    });
    let admitted: Principal = serde_json::from_value(principal.clone()).unwrap();
    let mut unknown = principal;
    unknown["external_claim"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Principal>(unknown).is_err());
    assert!(
        serde_json::from_value::<veoveo_hub_sdk::GroupMembership>(serde_json::json!({
            "group": "engineering", "role": "read", "unexpected": true
        }))
        .is_err()
    );
    let decision = PolicyDecision::deny(
        HubProfileId::parse("default").unwrap(),
        crate::contract::HubAction::ToolsCall,
        PolicyTarget::Hub,
        PolicyReasonCode::PolicyDeny,
        TraceId::parse("trace-normalized").unwrap(),
    );
    let mut wire = serde_json::to_value(&decision).unwrap();
    assert!(serde_json::from_value::<PolicyDecision>(wire.clone()).is_ok());
    wire["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<PolicyDecision>(wire).is_err());
    assert_eq!(
        serde_json::from_value::<Principal>(serde_json::to_value(&admitted).unwrap()).unwrap(),
        admitted
    );
}

#[test]
fn control_plane_refuses_retired_core_keys_in_extension_space() {
    let control = control_plane_with_secrets(default_secrets());
    let wire = serde_json::to_value(control).unwrap();
    assert!(serde_json::from_value::<HubControlPlane>(wire.clone()).is_ok());
    for key in [
        "identity_providers",
        "authorization_servers",
        "work_contexts",
        "data_labels",
        "oauth_clients",
        "oidc_clients",
    ] {
        let mut mixed = wire.clone();
        mixed[key] = serde_json::json!([]);
        assert!(
            serde_json::from_value::<HubControlPlane>(mixed).is_err(),
            "retired core key {key}"
        );
    }
}

#[test]
fn revision_wire_refuses_retired_names_instead_of_dropping_them() {
    let value = HubControlPlaneRevision {
        revision_id: HubControlPlaneRevisionId::parse("gcp-fixture").unwrap(),
        sha256: "a".repeat(64),
        source: HubControlPlaneRevisionSource::SeedFile,
        applied_at: chrono::DateTime::parse_from_rfc3339("2026-10-06T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        applied_by: PrincipalId::parse("author").unwrap(),
        tenant: None,
        control_plane: control_plane_with_secrets(default_secrets()),
    };
    let current = serde_json::to_value(value).unwrap();
    let schema = serde_json::to_value(schemars::schema_for!(HubControlPlaneRevision)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&current));
    assert!(serde_json::from_value::<HubControlPlaneRevision>(current.clone()).is_ok());
    for (canonical, retired) in [
        ("revisionId", "revision_id"),
        ("appliedAt", "applied_at"),
        ("appliedBy", "applied_by"),
        ("controlPlane", "control_plane"),
    ] {
        for mixed in [false, true] {
            let mut invalid = current.clone();
            invalid[retired] = invalid[canonical].clone();
            if !mixed {
                invalid.as_object_mut().unwrap().remove(canonical);
            }
            assert!(serde_json::from_value::<HubControlPlaneRevision>(invalid.clone()).is_err());
            assert!(!validator.is_valid(&invalid));
        }
    }
}

fn registering_control_plane() -> HubControlPlane {
    let mut secrets = default_secrets();
    secrets.push(registration_secret());
    let mut control_plane = control_plane_with_secrets(secrets);
    control_plane.authorization = default_authorization();
    control_plane.registration = Some(RegistrationSettings {
        secret: SecretReferenceId::parse("registration_secret").unwrap(),
    });
    control_plane
}

#[test]
fn a_control_plane_names_no_server() {
    registering_control_plane()
        .validate(&crate::catalog_fixture::registry())
        .unwrap();
    let mut wire = serde_json::to_value(registering_control_plane()).unwrap();
    assert!(wire.get("servers").is_none());
    assert_eq!(
        wire["registration"],
        serde_json::json!({ "secret": "registration_secret" })
    );
    assert_eq!(
        wire["authorization"]["grants"][0]["subject"]["group"],
        "tenant-members"
    );
    assert_eq!(wire["authorization"]["overrides"][0]["kind"], "deny");
    let round_trip: HubControlPlane = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(round_trip, registering_control_plane());
    wire["servers"] = serde_json::json!([]);
    assert!(serde_json::from_value::<HubControlPlane>(wire).is_err());
}

#[test]
fn installation_roles_and_grants_are_checked() {
    let registry = crate::catalog_fixture::registry();
    let mut config = registering_control_plane();
    config
        .authorization
        .roles
        .push(config.authorization.roles[0].clone());
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::DuplicateRole(_))
    ));

    let mut config = registering_control_plane();
    config.authorization.grants[0].role = RoleId::parse("missing").unwrap();
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::UnknownGrantRole(_))
    ));

    let mut config = registering_control_plane();
    config.authorization.grants[0].tenant = Some(TenantId::parse("missing").unwrap());
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::UnknownGrantTenant { .. })
    ));

    let mut config = registering_control_plane();
    config
        .authorization
        .overrides
        .push(PermissionOverride::AddRisks {
            permission: veoveo_types::PermissionName::parse("time.admin").unwrap(),
            risks: BTreeSet::from([Risk::Billed]),
        });
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::ConflictingPermissionOverride(_))
    ));
}

#[test]
fn registration_settings_require_a_hub_registration_secret() {
    let registry = crate::catalog_fixture::registry();
    let mut config = registering_control_plane();
    config.registration.as_mut().unwrap().secret = SecretReferenceId::parse("missing").unwrap();
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::UnknownRegistrationSecret(_))
    ));

    let mut config = registering_control_plane();
    config.registration.as_mut().unwrap().secret = signing_secret().id;
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::RegistrationSecretMismatch(_))
    ));
}

#[test]
fn policy_rules_cannot_grant_registered_server_actions() {
    let mut config = registering_control_plane();
    config.policies[0].rules[0].actions = BTreeSet::from([HubAction::ToolsCall.into()]);
    assert!(matches!(
        config.validate(&crate::catalog_fixture::registry()),
        Err(HubControlPlaneError::PolicyRuleServerAction { .. })
    ));
    let _ = (Access::Read, Risk::Physical);
}

fn external_client_configuration(method: OAuthClientAuthMethod) -> HubControlPlane {
    let mut config = control_plane_with_secrets(default_secrets());
    config.authorization_servers[0].access_token_key_id = None;
    config.authorization_servers[0].access_token_signing_key = None;
    let caller = &mut config.oauth_clients[1];
    caller.auth_methods = BTreeSet::from([method]);
    caller.credential_secret = None;
    caller.jwks = None;
    config
}

#[test]
fn external_confidential_clients_record_actual_auth_without_local_custody() {
    for method in [
        OAuthClientAuthMethod::ClientSecretPost,
        OAuthClientAuthMethod::ClientSecretBasic,
        OAuthClientAuthMethod::PrivateKeyJwt,
        OAuthClientAuthMethod::TlsClientAuth,
    ] {
        external_client_configuration(method)
            .validate(&crate::catalog_fixture::registry())
            .expect("external issuer owns client authentication material");
    }
}

#[test]
fn external_confidential_client_metadata_preserves_admission_constraints() {
    let registry = crate::catalog_fixture::registry();
    let mut config = external_client_configuration(OAuthClientAuthMethod::None);
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::OAuthClientPublicClientCredentials(_))
    ));
    config = external_client_configuration(OAuthClientAuthMethod::ClientSecretPost);
    config.oauth_clients[1]
        .grant_types
        .insert(OAuthGrantType::RefreshToken);
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. })
    ));
    config = external_client_configuration(OAuthClientAuthMethod::ClientSecretPost);
    config.oauth_clients[1].allowed_scopes.clear();
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::OAuthClientMissingAllowedScope { .. })
    ));
    config = external_client_configuration(OAuthClientAuthMethod::ClientSecretPost);
    config.oauth_clients[1].allowed_resources =
        BTreeSet::from([ProtectedResourceId::parse("https://unadmitted.example/mcp").unwrap()]);
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::UnknownOAuthClientResource { .. })
    ));
    config = external_client_configuration(OAuthClientAuthMethod::ClientSecretPost);
    config.oauth_clients[1].credential_secret = Some(SecretReferenceId::parse("absent").unwrap());
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::UnknownOAuthClientSecret { .. })
    ));
}

#[test]
fn hosted_client_credentials_require_the_selected_methods_authentication_material() {
    let registry = crate::catalog_fixture::registry();
    let mut config = control_plane_with_secrets(default_secrets());
    config.oauth_clients[1].auth_methods =
        BTreeSet::from([OAuthClientAuthMethod::ClientSecretPost]);
    config.oauth_clients[1].jwks = None;
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::OAuthClientMissingCredentialSecret { .. })
    ));
    config.oauth_clients[1].credential_secret = Some(SecretReferenceId::parse("absent").unwrap());
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::UnknownOAuthClientSecret { .. })
    ));
    config.oauth_clients[1].credential_secret =
        Some(SecretReferenceId::parse("enterprise_oidc_client_secret").unwrap());
    config
        .validate(&registry)
        .expect("explicit secret-post clients need their bound secret, not caller JWKS");

    config.oauth_clients[1].auth_methods = BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt]);
    config.oauth_clients[1].credential_secret = None;
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::OAuthClientMissingJwks { .. })
    ));
}

#[test]
fn hosted_secret_post_requires_the_runtime_client_secret_purpose() {
    let registry = crate::catalog_fixture::registry();
    let mut config = control_plane_with_secrets(default_secrets());
    let mut secret = oidc_client_secret();
    secret.id = SecretReferenceId::parse("service_client_secret").unwrap();
    secret.purpose = SecretPurpose::TokenExchangeCredential;
    config.oauth_clients[1].auth_methods =
        BTreeSet::from([OAuthClientAuthMethod::ClientSecretPost]);
    config.oauth_clients[1].credential_secret = Some(secret.id.clone());
    config.secrets.push(secret);
    assert!(matches!(
        config.validate(&registry),
        Err(HubControlPlaneError::OAuthClientSecretPurposeMismatch { .. })
    ));

    let server = &mut config.authorization_servers[0];
    server.access_token_signing_key = None;
    server.access_token_key_id = None;
    config
        .validate(&registry)
        .expect("external issuer secret metadata retains its existing purpose contract");
}

#[test]
fn computer_bootstrap_admits_only_fixed_direct_clients_and_env_provider_seeds() {
    let mut plane = control_plane_with_secrets(default_secrets());
    let mut seed = signing_secret();
    seed.id = SecretReferenceId::parse("computer-model-seed").unwrap();
    seed.purpose = SecretPurpose::ProviderApiKey;
    plane.secrets.push(seed);
    let computers: Computers = serde_json::from_value(serde_json::json!({
        "gateway": {"endpoint":"https://gateway.test", "certificateAuthority":"/etc/native/ca.pem", "bearerEnvironment":"COMPUTER_PROVISION_AUTHORITY"},
        "template": {"workspace":"installation", "name":"computer"},
        "policyFile":"/etc/native/computer-policy.yaml", "command":["/usr/local/bin/computer"],
        "oauthClient":"operator-local-public", "profile":"default", "workContext":"default", "port":4096,
        "hubProvider":{"profile":"computer-user", "credentialKey":"access_token"},
        "modelProvider":{"profile":"computer-model", "credentialKey":"api_key", "secret":"computer-model-seed"}
    })).unwrap();
    assert_eq!(computers.forwarding_connection_limit.get(), 512);
    let mut zero = serde_json::to_value(&computers).unwrap();
    zero["forwardingConnectionLimit"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Computers>(zero).is_err());
    let mut explicit = serde_json::to_value(&computers).unwrap();
    explicit["forwardingConnectionLimit"] = serde_json::json!(768);
    assert_eq!(
        serde_json::from_value::<Computers>(explicit)
            .unwrap()
            .forwarding_connection_limit
            .get(),
        768
    );
    plane.computers = Some(computers);
    plane.validate(&crate::catalog_fixture::registry()).unwrap();
    let mut bad = plane.clone();
    bad.secrets.last_mut().unwrap().source = SecretSource::Vault;
    assert!(matches!(
        bad.validate(&crate::catalog_fixture::registry()),
        Err(HubControlPlaneError::InvalidComputers(_))
    ));
    let mut bad = plane.clone();
    bad.oauth_clients[0]
        .grant_types
        .remove(&OAuthGrantType::RefreshToken);
    assert!(matches!(
        bad.validate(&crate::catalog_fixture::registry()),
        Err(HubControlPlaneError::InvalidComputers(_))
    ));
    let mut bad = plane.clone();
    bad.computers.as_mut().unwrap().command.clear();
    assert!(matches!(
        bad.validate(&crate::catalog_fixture::registry()),
        Err(HubControlPlaneError::InvalidComputers(_))
    ));
    let mut bad = plane;
    bad.computers.as_mut().unwrap().port = 0;
    assert!(matches!(
        bad.validate(&crate::catalog_fixture::registry()),
        Err(HubControlPlaneError::InvalidComputers(_))
    ));
}

#[test]
fn a_factory_needs_native_forwarding_on_its_gateway_and_valid_defaults() {
    let mut plane = control_plane_with_secrets(default_secrets());
    let factory: FactoryConfiguration = serde_json::from_value(serde_json::json!({
        "gateway": {"endpoint":"https://gateway.test", "certificateAuthority":"/etc/native/ca.pem", "bearerEnvironment":"FACTORY_AUTHORITY"},
        "workspace":"factory", "policyFile":"/etc/native/factory-base.yaml",
        "cpu":"500m", "memory":"16Gi", "port":8800, "hubIssuer":"https://hub.test",
        "registrationProfile":"factory-registration", "caps":{"instancesPerUser":3},
        "imageBuilder":{
            "buildkit":"tcp://buildkitd.test:1234", "registry":"registry.test:5000",
            "lore":"registry.test:5000/lore@sha256:8dad0d15745459ae06213192eba0680a68cbb02e71757d16a739f3f9e34c71b4"
        }
    }))
    .unwrap();
    assert!(!factory.caps.as_ref().unwrap().enforce);
    plane.factory = Some(factory);
    let invalid = |plane: &HubControlPlane| {
        matches!(
            plane.validate(&crate::catalog_fixture::registry()),
            Err(HubControlPlaneError::InvalidFactory(_))
        )
    };
    assert!(
        invalid(&plane),
        "instances are reached through native forwarding"
    );
    plane.native_upstreams = Some(
        serde_json::from_value(serde_json::json!({
            "gateway":"https://gateway.test", "certificateAuthority":"/etc/native/ca.pem",
            "bearerEnvironment":"NATIVE_AUTHORITY", "bindings":[]
        }))
        .unwrap(),
    );
    plane.validate(&crate::catalog_fixture::registry()).unwrap();
    for change in [
        (|f: &mut FactoryConfiguration| f.cpu = "0".into()) as fn(&mut FactoryConfiguration),
        |f| f.memory = "16GB".into(),
        |f| f.workspace = "Factory".into(),
        |f| f.port = 0,
        |f| f.caps.as_mut().unwrap().instances_per_user = 0,
        |f| f.gateway.endpoint = url::Url::parse("https://other.test").unwrap(),
    ] {
        let mut bad = plane.clone();
        change(bad.factory.as_mut().unwrap());
        assert!(invalid(&bad), "{:?}", bad.factory);
    }
}
