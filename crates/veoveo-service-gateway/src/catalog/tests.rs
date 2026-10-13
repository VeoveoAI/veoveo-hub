use std::collections::BTreeSet;
use veoveo_gateway_contract::{
    AuthorizationServerId, GatewayAction, ProtectedResourceId, SecretLocator, SecretOwner,
    SecretPurpose, SecretReference, SecretReferenceId, SecretSource,
};

use chrono::{DateTime, Utc};
use serde_json::Value;
use veoveo_mcp_contract::{
    AuthMode, CanonicalTaskId, DataLabelDefinition, DiscoveryFailureMode, HttpsUrl,
    IdentityProvider, IdentityProviderId, IdentityProviderOidcClientRegistration, JwksSource,
    JwtId, MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION, MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION,
    OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration, OAuthClientSurface,
    OAuthEndpointUrl, OAuthGrantType, OAuthRedirectUri, OidcClientAuthMethod, OidcClientId,
    OidcClientRegistrationId, PolicyEffect, PolicyReasonCode, PolicyRule, PolicyRuleId,
    PolicyTarget, Principal, PrincipalKind, ResourceAuthorizationServer, TenantDefinition,
    TokenIssuer, TokenSubject, TraceId, WorkContextDefinition, WorkContextMembershipRule,
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
        claim_mapping: veoveo_mcp_contract::IdentityProviderClaimMapping::default(),
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
        access_token_key_id: JwtId::parse("test-key").unwrap(),
        access_token_signing_key: SecretReferenceId::parse("veoveo_access_token_private_key")
            .unwrap(),
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
        owner: SecretOwner::Gateway,
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
        owner: SecretOwner::Gateway,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn policy() -> PolicySet {
    PolicySet {
        version: PolicyVersion::parse("2026-07-02").unwrap(),
        rules: vec![PolicyRule {
            id: veoveo_mcp_contract::PolicyRuleId::parse("allow_admin_read").unwrap(),
            effect: PolicyEffect::Allow,
            actions: BTreeSet::from([GatewayAction::AdminRead.into()]),
            profiles: BTreeSet::from([GatewayProfileId::parse("default").unwrap()]),
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

fn profile() -> GatewayProfile {
    GatewayProfile {
        id: GatewayProfileId::parse("default").unwrap(),
        identity_provider: IdentityProviderId::parse("enterprise").unwrap(),
        authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
        protected_resource: ProtectedResourceId::parse("https://veoveo.example/mcp/operator")
            .unwrap(),
        policy_version: PolicyVersion::parse("2026-07-02").unwrap(),
        artifact_upload: None,
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

fn catalog() -> GatewayCatalog {
    catalog_with_policy(policy())
}

fn catalog_with_policy(policy: PolicySet) -> GatewayCatalog {
    catalog_with_profile_and_policy(profile(), policy)
}

fn catalog_with_profile_and_policy(profile: GatewayProfile, policy: PolicySet) -> GatewayCatalog {
    GatewayCatalog::from_control_plane(
        GatewayControlPlane {
            branding: None,
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
    let parsed: GatewayControlPlane = serde_json::from_str(&text).unwrap();
    let catalog =
        GatewayCatalog::from_control_plane(parsed, crate::test_catalog::binding()).unwrap();

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
        .protected_resource_metadata(&GatewayProfileId::parse("default").unwrap())
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
        .authorization_server_metadata(&GatewayProfileId::parse("default").unwrap())
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
        actions: BTreeSet::from([GatewayAction::AdminWrite.into()]),
        profiles: BTreeSet::from([GatewayProfileId::parse("default").unwrap()]),
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
        .protected_resource_metadata(&GatewayProfileId::parse("default").unwrap())
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
fn gateway_policy_rules_require_their_groups() {
    let mut policy = policy();
    policy.rules[0].actions = BTreeSet::from([GatewayAction::AdminWrite.into()]);
    policy.rules[0].groups = BTreeSet::from([GroupId::parse("auditors").unwrap()]);
    let catalog = catalog_with_policy(policy);
    let principal = principal(&["operator:use"]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::parse("default").unwrap(),
        action: GatewayAction::AdminWrite.into(),
        target: &PolicyTarget::Gateway,
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
