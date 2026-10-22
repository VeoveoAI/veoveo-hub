use crate::contract::{
    AuthorizationServerId, HubAction, SecretLocator, SecretOwner, SecretPurpose, SecretReference,
    SecretReferenceId, SecretSource,
};
use std::collections::BTreeSet;
use veoveo_hub_sdk::ProtectedResourceId;

use crate::contract::{
    OAuthClientAuthMethod, OAuthGrantType, OAuthRedirectUri, OidcClientRegistrationId,
};
use crate::control_plane::{
    AuthMode, DataLabelDefinition, DiscoveryFailureMode, HttpsUrl, IdentityProvider,
    IdentityProviderId, IdentityProviderOidcClientRegistration, JwksSource,
    MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION, MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION,
    OAuthClientRegistration, OAuthClientSurface, OAuthEndpointUrl, OidcClientAuthMethod,
    OidcClientId, PolicyEffect, PolicyReasonCode, PolicyRule, PolicyRuleId, PolicyTarget,
    ResourceAuthorizationServer, TenantDefinition, TraceId, WorkContextDefinition,
    WorkContextMembershipRule,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use veoveo_hub_sdk::{
    CanonicalTaskId, JwtId, OAuthClientId, Principal, PrincipalKind, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    AccessSubject, DataLabelId, GroupId, InvocationMode, PrincipalId, RoleId, ScopeName, TenantId,
    WorkContextId,
};
use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

use super::*;
use crate::{BearerError, PolicyRequest, www_authenticate_challenge};

fn identity_provider() -> IdentityProvider {
    IdentityProvider {
        id: IdentityProviderId::parse("enterprise").unwrap(),
        issuer: TokenIssuer::parse("https://idp.example.com").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
        },
        claim_mapping: crate::control_plane::IdentityProviderClaimMapping::default(),
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

fn policy() -> PolicySet {
    PolicySet {
        version: PolicyVersion::parse("2026-07-02").unwrap(),
        rules: vec![PolicyRule {
            id: crate::control_plane::PolicyRuleId::parse("allow_admin_read").unwrap(),
            effect: PolicyEffect::Allow,
            actions: BTreeSet::from([HubAction::AdminRead.into()]),
            profiles: BTreeSet::from([HubProfileId::parse("default").unwrap()]),
            protected_resources: BTreeSet::new(),
            principal_ids: BTreeSet::new(),
            tenant_ids: BTreeSet::from([TenantId::parse("tenant-a").unwrap()]),
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

fn data_labels() -> Vec<DataLabelDefinition> {
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

fn tenants() -> Vec<TenantDefinition> {
    vec![TenantDefinition {
        id: TenantId::parse("tenant-a").unwrap(),
        title: Some("Tenant A".to_string()),
        description: None,
        metadata: Value::Null,
    }]
}

fn work_contexts() -> Vec<WorkContextDefinition> {
    vec![WorkContextDefinition {
        id: WorkContextId::parse("mission").unwrap(),
        tenant: TenantId::parse("tenant-a").unwrap(),
        title: "Mission".into(),
        policy_revision: PolicyVersion::parse("2026-07-02").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Group(GroupId::parse("operations").unwrap()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        memberships: vec![
            WorkContextMembershipRule {
                level: WorkContextMembershipLevel::Owner,
                principals: BTreeSet::new(),
                groups: BTreeSet::new(),
                roles: BTreeSet::from([RoleId::parse("operator").unwrap()]),
                oauth_clients: BTreeSet::new(),
            },
            WorkContextMembershipRule {
                level: WorkContextMembershipLevel::Owner,
                principals: BTreeSet::new(),
                groups: BTreeSet::new(),
                roles: BTreeSet::new(),
                oauth_clients: BTreeSet::from([OAuthClientId::parse("operator-service").unwrap()]),
            },
        ],
    }]
}

fn profile() -> HubProfile {
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

fn oauth_clients() -> Vec<OAuthClientRegistration> {
    vec![
        OAuthClientRegistration {
            id: OAuthClientId::parse("operator-local-public").unwrap(),
            authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
            default_work_context: WorkContextId::parse("mission").unwrap(),
            invocation_mode: InvocationMode::Direct,
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
                ScopeName::parse("operator:admin").unwrap(),
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
            default_work_context: WorkContextId::parse("mission").unwrap(),
            invocation_mode: InvocationMode::Automated,
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
                ScopeName::parse("operator:admin").unwrap(),
                ScopeName::parse("admin:manage").unwrap(),
            ]),
            credential_secret: None,
            jwks: Some(JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/clients/jwks.json")
                    .unwrap(),
            }),
            tenant: Some(TenantId::parse("tenant-a").unwrap()),
            metadata: Value::Null,
        },
    ]
}

fn oidc_clients() -> Vec<IdentityProviderOidcClientRegistration> {
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

fn catalog() -> HubCatalog {
    catalog_with_policy(policy())
}

fn catalog_with_policy(policy: PolicySet) -> HubCatalog {
    catalog_with_profile_and_policy(profile(), policy)
}

fn catalog_with_profile_and_policy(profile: HubProfile, policy: PolicySet) -> HubCatalog {
    HubCatalog::from_control_plane(
        HubControlPlane {
            execution: None,
            branding: None,
            native_upstreams: None,
            computers: None,
            factory: None,
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            profiles: vec![profile],
            authorization: Default::default(),
            registration: None,
            extensions: Default::default(),
            tenants: tenants(),
            work_contexts: work_contexts(),
            policies: vec![policy],
            data_labels: data_labels(),
            oauth_clients: oauth_clients(),
            oidc_clients: oidc_clients(),
            secrets: vec![signing_secret(), oidc_client_secret()],
            metadata: Value::Null,
        },
        crate::test_catalog::binding(),
    )
    .unwrap()
}

fn principal(scopes: &[&str]) -> Principal {
    Principal {
        id: PrincipalId::parse("user@example.com").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://idp.example.com").unwrap(),
        subject: TokenSubject::parse("00u123").unwrap(),
        tenant: Some(TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: scopes
            .iter()
            .map(|scope| ScopeName::parse(*scope).unwrap())
            .collect(),
        data_labels: BTreeSet::<DataLabelId>::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(
            DateTime::parse_from_rfc3339("2026-07-02T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ),
    }
}

#[test]
fn json_config_round_trips_through_contract_validation() {
    let text = serde_json::to_string(&catalog().control_plane().clone()).unwrap();
    let parsed: HubControlPlane = serde_json::from_str(&text).unwrap();
    let catalog = HubCatalog::from_control_plane(parsed, crate::test_catalog::binding()).unwrap();

    assert_eq!(
        catalog.server_count(),
        0,
        "servers join only through registration"
    );
    assert_eq!(catalog.profile_count(), 1);
}

#[test]
fn builds_protected_resource_metadata_for_profile() {
    let catalog = catalog();
    let metadata = catalog
        .protected_resource_metadata(&HubProfileId::parse("default").unwrap())
        .unwrap();

    assert_eq!(metadata.resource, "https://veoveo.example/mcp/operator");
    assert_eq!(
        metadata.authorization_servers,
        vec!["https://veoveo.example/oauth".to_string()]
    );
    assert_eq!(
        metadata.scopes_supported,
        vec!["audit:read".to_string(), "operator:use".to_string()]
    );
    assert_eq!(
        metadata.bearer_methods_supported,
        vec!["header".to_string()]
    );
    assert!(
        metadata
            .extensions
            .contains_key(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
    );
    assert!(
        metadata
            .extensions
            .contains_key(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION)
    );
}

#[test]
fn builds_authorization_server_metadata_for_profile() {
    let catalog = catalog();
    let metadata = catalog
        .authorization_server_metadata(&HubProfileId::parse("default").unwrap())
        .unwrap();

    assert_eq!(metadata.issuer, "https://veoveo.example/oauth");
    assert_eq!(
        metadata.authorization_endpoint.as_deref(),
        Some("https://veoveo.example/oauth/authorize")
    );
    assert_eq!(
        metadata.token_endpoint,
        "https://veoveo.example/oauth/token"
    );
    assert_eq!(
        metadata.revocation_endpoint.as_deref(),
        Some("https://veoveo.example/oauth/revoke")
    );
    assert_eq!(
        metadata.jwks_uri.as_deref(),
        Some("https://veoveo.example/oauth/jwks.json")
    );
    assert_eq!(
        metadata.scopes_supported,
        vec!["audit:read".to_string(), "operator:use".to_string()]
    );
    assert_eq!(metadata.response_types_supported, vec!["code".to_string()]);
    assert!(
        metadata
            .grant_types_supported
            .contains(&"authorization_code".to_string())
    );
    assert!(
        metadata
            .grant_types_supported
            .contains(&"refresh_token".to_string())
    );
    assert!(
        metadata
            .grant_types_supported
            .contains(&"client_credentials".to_string())
    );
    assert!(
        metadata
            .grant_types_supported
            .contains(&"urn:ietf:params:oauth:grant-type:jwt-bearer".to_string())
    );
    assert_eq!(
        metadata.code_challenge_methods_supported,
        vec!["S256".to_string()]
    );
    assert!(
        metadata
            .token_endpoint_auth_methods_supported
            .contains(&"private_key_jwt".to_string())
    );
    assert!(
        metadata
            .token_endpoint_auth_methods_supported
            .contains(&"none".to_string())
    );
    assert!(
        metadata
            .token_endpoint_auth_signing_alg_values_supported
            .contains(&"RS256".to_string())
    );
    assert_eq!(
        metadata.revocation_endpoint_auth_methods_supported,
        vec!["none".to_string()]
    );
    assert_eq!(
        metadata.authorization_grant_profiles_supported,
        vec!["urn:ietf:params:oauth:grant-profile:id-jag".to_string()]
    );
}

#[test]
fn authorization_server_metadata_states_every_member_with_a_default() {
    let metadata = crate::AuthorizationServerMetadata {
        issuer: "https://veoveo.example/oauth".to_owned(),
        authorization_endpoint: None,
        device_authorization_endpoint: None,
        token_endpoint: "https://veoveo.example/oauth/token".to_owned(),
        revocation_endpoint: None,
        jwks_uri: None,
        scopes_supported: Vec::new(),
        response_types_supported: Vec::new(),
        grant_types_supported: Vec::new(),
        code_challenge_methods_supported: Vec::new(),
        token_endpoint_auth_methods_supported: Vec::new(),
        token_endpoint_auth_signing_alg_values_supported: Vec::new(),
        revocation_endpoint_auth_methods_supported: Vec::new(),
        authorization_grant_profiles_supported: Vec::new(),
    };
    // RFC 8414 §2: `response_types_supported` is required, and an absent grant type or
    // token endpoint authentication method list means its default.
    assert_eq!(
        serde_json::to_value(metadata).unwrap(),
        serde_json::json!({
            "issuer": "https://veoveo.example/oauth",
            "token_endpoint": "https://veoveo.example/oauth/token",
            "response_types_supported": [],
            "grant_types_supported": [],
            "token_endpoint_auth_methods_supported": []
        })
    );
}

#[test]
fn protected_resource_metadata_includes_policy_required_scopes() {
    let mut policy = policy();
    policy.rules.push(PolicyRule {
        id: PolicyRuleId::parse("allow_admin_write").unwrap(),
        effect: PolicyEffect::Allow,
        actions: BTreeSet::from([HubAction::AdminWrite.into()]),
        profiles: BTreeSet::from([HubProfileId::parse("default").unwrap()]),
        protected_resources: BTreeSet::new(),
        principal_ids: BTreeSet::new(),
        tenant_ids: BTreeSet::new(),
        groups: BTreeSet::new(),
        roles: BTreeSet::new(),
        required_scopes: BTreeSet::from([ScopeName::parse("admin:manage").unwrap()]),
        required_data_labels: BTreeSet::new(),
        required_assurances: BTreeSet::new(),
        metadata: Value::Null,
    });
    let catalog = catalog_with_policy(policy);
    let metadata = catalog
        .protected_resource_metadata(&HubProfileId::parse("default").unwrap())
        .unwrap();

    assert!(
        metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "operator:use")
    );
    assert!(
        metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "admin:manage")
    );
}

#[test]
fn hub_policy_rules_require_their_groups() {
    let mut policy = policy();
    policy.rules[0].actions = BTreeSet::from([HubAction::AdminWrite.into()]);
    policy.rules[0].groups = BTreeSet::from([GroupId::parse("auditors").unwrap()]);
    let catalog = catalog_with_policy(policy);
    let principal = principal(&["operator:use"]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &HubProfileId::parse("default").unwrap(),
        action: HubAction::AdminWrite.into(),
        target: &PolicyTarget::Hub,
        trace_id: &TraceId::parse("trace-admin-filtered").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingGroup);
}

#[test]
fn builds_www_authenticate_challenge_with_scope() {
    let metadata = "https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator";
    let scopes = [ScopeName::parse("operator:use").unwrap()];

    assert_eq!(
        www_authenticate_challenge(metadata, &scopes, None),
        "Bearer resource_metadata=\"https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator\", scope=\"operator:use\""
    );
    assert_eq!(
        www_authenticate_challenge(metadata, &scopes, Some(BearerError::InvalidToken)),
        "Bearer error=\"invalid_token\", resource_metadata=\"https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator\", scope=\"operator:use\""
    );
    assert_eq!(
        www_authenticate_challenge(metadata, &[], Some(BearerError::InsufficientScope)),
        "Bearer error=\"insufficient_scope\", resource_metadata=\"https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator\""
    );
}

#[test]
fn canonical_task_id_requires_uuid_v7() {
    let value = uuid::Uuid::now_v7().to_string();
    let task = CanonicalTaskId::parse(value.clone()).unwrap();
    assert_eq!(task.as_str(), value);
}

fn native_upstream_configuration() -> crate::control_plane::NativeUpstreams {
    use crate::control_plane::{NativeUpstreamBinding, NativeUpstreams};
    NativeUpstreams {
        gateway: url::Url::parse("https://host.openshell.internal:17670").unwrap(),
        certificate_authority: "/trusted/gateway-ca.pem".into(),
        bearer_environment: "GATEWAY_REFERENCE".into(),
        bindings: vec![NativeUpstreamBinding {
            endpoint: veoveo_hub_sdk::registration::ServerEndpoint::new(
                url::Url::parse("http://fixture.internal:8800/fixture/mcp").unwrap(),
            )
            .unwrap(),
            workspace: "services".into(),
            sandbox: "fixture".into(),
            port: 8800,
        }],
    }
}

fn native_catalog(native: Option<crate::control_plane::NativeUpstreams>) -> HubCatalog {
    let mut plane = catalog().control_plane().clone();
    plane.native_upstreams = native;
    HubCatalog::from_control_plane(plane, crate::test_catalog::binding()).unwrap()
}

#[test]
fn native_upstream_reload_rejects_bootstrap_but_admits_live_destinations() {
    let initial = native_upstream_configuration();
    let original = Arc::new(native_catalog(Some(initial.clone())));
    let handle = HubCatalogHandle::new(original.clone());
    let changes = handle.subscribe();
    for field in 0..4 {
        let mut next = initial.clone();
        match field {
            0 => {
                next.gateway.set_port(Some(17671)).unwrap();
            }
            1 => next.certificate_authority = "/trusted/other-ca.pem".into(),
            2 => next.bearer_environment = "OTHER_REFERENCE".into(),
            _ => {}
        }
        let candidate = native_catalog(if field == 3 { None } else { Some(next) });
        assert!(
            handle
                .validate_replacement(&candidate)
                .unwrap_err()
                .to_string()
                .contains("connection/authentication")
        );
        assert!(handle.replace(Arc::new(candidate)).is_err());
        assert!(Arc::ptr_eq(&handle.current(), &original));
        assert_eq!(handle.snapshot().generation(), 0);
        assert!(!changes.has_changed().unwrap());
    }
    for field in 0..5 {
        let mut next = initial.clone();
        match field {
            0 => next.bindings[0].workspace = "other".into(),
            1 => next.bindings[0].sandbox = "replacement".into(),
            2 => next.bindings[0].port = 8801,
            3 => {
                next.bindings[0].endpoint = veoveo_hub_sdk::registration::ServerEndpoint::new(
                    url::Url::parse("http://other.internal:8800/fixture/mcp").unwrap(),
                )
                .unwrap()
            }
            _ => next.bindings.clear(),
        }
        handle
            .validate_replacement(&native_catalog(Some(next)))
            .unwrap();
    }
}

#[test]
fn native_upstream_reload_accepts_unchanged_placement_and_keeps_registrations() {
    let initial = Arc::new(native_catalog(Some(native_upstream_configuration())));
    let handle = HubCatalogHandle::new(initial.clone());
    handle.replace_registrations(crate::registration::RegistrationSnapshot::new([
        crate::test_catalog::registered_server(
            "fixture",
            "http://fixture.internal:8800/fixture/mcp",
        ),
    ]));
    let mut changes = handle.subscribe();
    let before = handle.snapshot();
    let mut plane = initial.control_plane().clone();
    plane.metadata = serde_json::json!({"revision": "changed"});
    let candidate =
        Arc::new(HubCatalog::from_control_plane(plane, crate::test_catalog::binding()).unwrap());
    handle.validate_replacement(&candidate).unwrap();
    handle.replace(candidate.clone()).unwrap();
    let after = handle.snapshot();
    assert_eq!(after.generation(), before.generation() + 1);
    assert!(changes.has_changed().unwrap());
    assert_eq!(*changes.borrow_and_update(), after.generation());
    assert_eq!(
        after.catalog().configuration_sha256(),
        candidate.configuration_sha256()
    );
    assert_eq!(after.catalog().server_count(), 1);
    assert_eq!(
        after.catalog().control_plane().metadata["revision"],
        "changed"
    );
}

#[test]
fn registered_authenticated_routes_keep_current_admission_and_task_boundaries() {
    use veoveo_hub_sdk::{
        LocalToolName, calls::ProviderContract, definition::CollectionTool, hosting::DomainServer,
    };
    use veoveo_hub_sdk_fixture::{
        contract::{Observatory, ReadingsTool},
        mcp::ObservatoryMcp,
    };
    let base = catalog();
    let declaration = ObservatoryMcp::routes().authorization();
    let catalog = crate::policy::PolicyCatalog::new(
        base.control_plane().clone(),
        crate::test_catalog::registry(),
    )
    .unwrap()
    .with_server(Observatory::slug(), declaration);
    let profile = HubProfileId::parse("default").unwrap();
    let trace = TraceId::parse("authenticated-route").unwrap();
    let caller = principal(&["operator:use"]);
    let target = PolicyTarget::Tool {
        server: Observatory::slug(),
        tool: LocalToolName::parse(ReadingsTool::TOOL).unwrap(),
    };
    let request = |principal, target| PolicyRequest {
        principal,
        profile: &profile,
        action: HubAction::ToolsCall.into(),
        target,
        trace_id: &trace,
    };
    let admitted = crate::policy::decide(&catalog, request(&caller, &target));
    assert_eq!(admitted.effect, PolicyEffect::Allow);
    assert!(admitted.granted.is_empty());
    assert!(admitted.permission.is_none());
    let missing_scope = principal(&[]);
    assert_eq!(
        crate::policy::decide(&catalog, request(&missing_scope, &target)).reason,
        PolicyReasonCode::MissingScope
    );
    let unknown = PolicyTarget::Tool {
        server: Observatory::slug(),
        tool: LocalToolName::parse("undeclared").unwrap(),
    };
    assert_eq!(
        crate::policy::decide(&catalog, request(&caller, &unknown)).reason,
        PolicyReasonCode::UnknownTool
    );
    for (target, action) in [
        (
            PolicyTarget::Server {
                server: Observatory::slug(),
            },
            HubAction::ResourcesList,
        ),
        (
            PolicyTarget::PlatformTask {
                server: Observatory::slug(),
                task_id: veoveo_types::TaskId::new(),
            },
            HubAction::TasksGet,
        ),
    ] {
        let denied = crate::policy::decide(
            &catalog,
            PolicyRequest {
                principal: &caller,
                profile: &profile,
                action: action.into(),
                target: &target,
                trace_id: &trace,
            },
        );
        assert_eq!(denied.effect, PolicyEffect::Deny);
        assert_eq!(denied.reason, PolicyReasonCode::MissingPermission);
    }
}

#[tokio::test]
async fn hosted_publication_requires_prepared_state_and_keeps_inflight_catalog_snapshots() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let initial = native_catalog(None)
        .prepare_runtime(None, "https://hub.example", reqwest::Client::new())
        .await
        .unwrap();
    let handle = HubCatalogHandle::new(Arc::new(initial));
    let inflight = handle.snapshot();
    let old_runtime = inflight.catalog().runtime.as_ref().unwrap().clone();
    let mut plane = handle.current().control_plane().clone();
    plane.metadata = serde_json::json!({"revision":"live"});
    let next = HubCatalog::from_control_plane(plane, crate::test_catalog::binding()).unwrap();
    let changes = handle.subscribe();
    assert!(
        handle.replace(Arc::new(next.clone())).is_err(),
        "a typed document alone is not prepared hosted state"
    );
    assert!(!changes.has_changed().unwrap());
    assert_eq!(handle.snapshot().generation(), inflight.generation());
    let prepared = next
        .prepare_runtime(
            Some(&handle.current()),
            "https://hub.example",
            reqwest::Client::new(),
        )
        .await
        .unwrap();
    handle.replace(Arc::new(prepared)).unwrap();
    assert!(changes.has_changed().unwrap());
    assert_eq!(
        handle.current().control_plane().metadata["revision"],
        "live"
    );
    assert_ne!(
        handle.current().configuration_sha256(),
        inflight.catalog().configuration_sha256()
    );
    assert!(Arc::ptr_eq(
        inflight.catalog().runtime.as_ref().unwrap(),
        &old_runtime
    ));
    assert!(!Arc::ptr_eq(
        handle.current().runtime.as_ref().unwrap(),
        &old_runtime
    ));
}
