use super::*;
use crate::{
    HubCatalogAdmission,
    contract::{CatalogFacts, ProtectedResourceDescriptor, ProtectedResourceName},
    control_plane::HubControlPlane,
};
use schemars::JsonSchema;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use veoveo_types::{ExtensionError, ExtensionName, PolicyVersion, PrincipalId};

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureSection {
    resource: ProtectedResourceId,
    authorization_server: crate::contract::AuthorizationServerId,
    policy_version: PolicyVersion,
    assigned: PrincipalId,
}
impl CatalogSection for FixtureSection {
    fn validate(&self, facts: &CatalogFacts) -> std::result::Result<(), ExtensionError> {
        if facts
            .authorization_servers
            .contains(&self.authorization_server)
            && facts.policies.contains(&self.policy_version)
        {
            Ok(())
        } else {
            Err(ExtensionError::new("fixture authority is unknown"))
        }
    }
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor> {
        vec![ProtectedResourceDescriptor {
            name: ProtectedResourceName::parse("fixture").unwrap(),
            resource: self.resource.clone(),
            authorization_server: self.authorization_server.clone(),
            policy_version: self.policy_version.clone(),
            required_scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
            grant_types: BTreeSet::from([
                crate::contract::OAuthGrantType::AuthorizationCodePkce,
                crate::contract::OAuthGrantType::RefreshToken,
            ]),
        }]
    }
}
struct FixtureProfile {
    key: SectionKey<FixtureSection>,
    calls: Arc<AtomicUsize>,
    claim: &'static str,
}
struct FixtureGrant {
    identity: PrincipalId,
}
impl TokenProfile for FixtureProfile {
    type Section = FixtureSection;
    type Request = ();
    type AuthorizedGrant = FixtureGrant;
    type Claims = BTreeMap<String, String>;
    fn section(&self) -> &SectionKey<FixtureSection> {
        &self.key
    }
    fn authorize(
        &self,
        section: &FixtureSection,
        context: &TokenContext<'_>,
        _: &(),
    ) -> Result<FixtureGrant> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ensure!(
            context.principal().id == section.assigned,
            "fixture principal has no assignment"
        );
        Ok(FixtureGrant {
            identity: section.assigned.clone(),
        })
    }
    fn purpose(&self, _: &FixtureGrant) -> TokenPurpose {
        TokenPurpose::Resource
    }
    fn lifetime(&self, _: &FixtureGrant) -> TokenLifetime {
        TokenLifetime::from_seconds(std::num::NonZeroU32::new(300).unwrap())
    }
    fn encode(&self, grant: &FixtureGrant) -> Self::Claims {
        BTreeMap::from([(self.claim.to_owned(), grant.identity.to_string())])
    }
}
fn registry() -> (crate::contract::CatalogRegistry, SectionKey<FixtureSection>) {
    let mut builder = crate::kernel_catalog_registry_builder().unwrap();
    let key = builder
        .register_section::<FixtureSection>(ExtensionName::parse("fixtureIssuer").unwrap())
        .unwrap();
    (builder.build().unwrap(), key)
}
fn catalog(registry: crate::contract::CatalogRegistry, assigned: &str) -> HubCatalog {
    let mut plane: HubControlPlane =
        serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap();
    let resource = ProtectedResourceId::parse("https://resource.example/native").unwrap();
    let section = FixtureSection {
        resource: resource.clone(),
        authorization_server: plane.authorization_servers[0].id.clone(),
        policy_version: plane.policies[0].version.clone(),
        assigned: PrincipalId::parse(assigned).unwrap(),
    };
    plane.extensions.insert(
        "fixtureIssuer".to_owned(),
        serde_json::to_value(section).unwrap(),
    );
    plane
        .oauth_clients
        .iter_mut()
        .find(|client| client.id.as_str() == "operator-local-public")
        .unwrap()
        .allowed_resources
        .insert(resource.clone());
    plane.oidc_clients[0].allowed_resources.insert(resource);
    HubCatalog::from_control_plane(
        plane,
        HubCatalogAdmission::unbound().bind(registry).unwrap(),
    )
    .unwrap()
}
fn principal() -> Principal {
    Principal {
        id: PrincipalId::parse("https://idp.example.com#alice").unwrap(),
        kind: veoveo_hub_sdk::PrincipalKind::User,
        issuer: veoveo_hub_sdk::TokenIssuer::parse("https://idp.example.com").unwrap(),
        subject: veoveo_hub_sdk::TokenSubject::parse("alice").unwrap(),
        tenant: Some(veoveo_types::TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    }
}
fn claims_registry() -> ExtensionRegistry {
    let mut builder = crate::auth::access_token_extension_registry_builder();
    let name = ExtensionName::parse("fixture_grant").unwrap();
    builder.reserve(name.clone()).unwrap();
    builder.bind_serde::<String>(&name).unwrap();
    builder.build()
}
fn context<'a>(catalog: &'a HubCatalog, principal: &'a Principal) -> TokenContext<'a> {
    let resource = catalog
        .contributed_protected_resource(
            &ProtectedResourceId::parse("https://resource.example/native").unwrap(),
        )
        .unwrap();
    let client = catalog
        .oauth_client(&OAuthClientId::parse("operator-local-public").unwrap())
        .unwrap();
    TokenContext::admit(
        catalog,
        &resource.resource,
        &client.id,
        principal,
        &client.default_work_context,
        &resource.required_scopes,
        None,
    )
    .unwrap()
}
#[test]
fn owner_grants_use_current_assignments_and_do_not_acquire_a_work_context_gate() {
    let (registry, key) = registry();
    let mut principal = principal();
    principal.tenant = Some(veoveo_types::TenantId::parse("tenant-b").unwrap());
    let catalog = catalog(registry.clone(), principal.id.as_str());
    let profile = FixtureProfile {
        key,
        calls: Arc::new(AtomicUsize::new(0)),
        claim: "fixture_grant",
    };
    let token = authorize_token(
        &profile,
        context(&catalog, &principal),
        &(),
        &claims_registry(),
    )
    .unwrap();
    assert_eq!(token.purpose(), TokenPurpose::Resource);
    assert_eq!(token.context().resource().profile().map(|p| &p.id), None);
    assert_eq!(token.grant().identity, principal.id);
    let next = super::tests::catalog(registry, "another-principal");
    assert!(
        authorize_token(
            &profile,
            context(&next, &principal),
            &(),
            &claims_registry()
        )
        .is_err()
    );
}
#[test]
fn foreign_registry_and_wrong_resource_are_refused_before_owner_authorization() {
    let (registry, _) = registry();
    let catalog = catalog(registry, principal().id.as_str());
    let (_, key) = super::tests::registry();
    let principal = principal();
    let profile = FixtureProfile {
        key,
        calls: Arc::new(AtomicUsize::new(0)),
        claim: "fixture_grant",
    };
    assert!(
        authorize_token(
            &profile,
            context(&catalog, &principal),
            &(),
            &claims_registry()
        )
        .is_err()
    );
    assert_eq!(profile.calls.load(Ordering::SeqCst), 0);
    let key = catalog
        .registry()
        .section_key::<FixtureSection>(&ExtensionName::parse("fixtureIssuer").unwrap())
        .unwrap();
    let profile = FixtureProfile { key, ..profile };
    let client = catalog
        .oauth_client(&OAuthClientId::parse("operator-local-public").unwrap())
        .unwrap();
    let resource = catalog
        .profile(&veoveo_hub_sdk::HubProfileId::parse("operator").unwrap())
        .unwrap();
    let scopes = BTreeSet::new();
    let context = TokenContext::admit(
        &catalog,
        &resource.protected_resource,
        &client.id,
        &principal,
        &client.default_work_context,
        &scopes,
        None,
    )
    .unwrap();
    assert!(authorize_token(&profile, context, &(), &claims_registry()).is_err());
    assert_eq!(profile.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn owner_cannot_override_core_purpose_or_smuggle_undeclared_claims() {
    let (registry, key) = registry();
    let principal = principal();
    let catalog = catalog(registry, principal.id.as_str());
    for claim in ["aud", TOKEN_PURPOSE_CLAIM, "undeclared_claim"] {
        let profile = FixtureProfile {
            key: key.clone(),
            calls: Arc::new(AtomicUsize::new(0)),
            claim,
        };
        assert!(
            authorize_token(
                &profile,
                context(&catalog, &principal),
                &(),
                &claims_registry()
            )
            .is_err()
        );
    }
}
