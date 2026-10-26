use super::*;
use std::sync::Arc;
use veoveo_hub::{
    HubCatalog, HubCatalogAdmission, HubCatalogHandle, control_plane::HubControlPlane,
    token_profile::authorize_token,
};
use veoveo_hub_sdk::{Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{OAuthClientId, RoleId};

const FIRST: &str = "00000000000000000000000000000000";
const SECOND: &str = "11111111111111111111111111111111";
const RESOURCE: &str = "https://source.example/native";
fn principal() -> Principal {
    Principal {
        id: PrincipalId::parse("native-alice").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://idp.example.com").unwrap(),
        subject: TokenSubject::parse("alice").unwrap(),
        tenant: Some(TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::from([RoleId::parse("admin").unwrap()]),
        scopes: BTreeSet::new(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    }
}
pub(super) fn plane() -> HubControlPlane {
    let mut plane: HubControlPlane =
        serde_json::from_str(include_str!("../../../../../testdata/hub.json")).unwrap();
    let section = serde_json::json!({
        "deployments": [{"id":"source", "resourceName":"native-source", "audience":RESOURCE, "endpoint":"grpcs://10.0.0.7:41337", "authorizationServer":"veoveo", "policyVersion":"2026-07-02", "loginScopes":["offline_access", "native:login"], "scopeTemplate":"repo/{id}/access", "resourceIdTemplate":"partition-{id}", "tokenLifetimeSeconds":120}],
        "repositories":[{"deployment":"source","id":FIRST,"tenant":"tenant-a","title":"Shared"},{"deployment":"source","id":SECOND,"tenant":"tenant-a","title":"Other"}],
        "assignments":[{"deployment":"source","repository":FIRST,"tenant":"tenant-a","principal":"native-alice","actions":[]}]
    });
    plane.extensions.insert("lore".to_owned(), section);
    let mut client = plane
        .oauth_clients
        .iter()
        .find(|c| c.id.as_str() == "operator-local-public")
        .unwrap()
        .clone();
    client.id = OAuthClientId::parse("native-public").unwrap();
    client.allowed_resources = BTreeSet::from([ProtectedResourceId::parse(RESOURCE).unwrap()]);
    client.allowed_scopes = BTreeSet::from([
        ScopeName::parse("native:login").unwrap(),
        ScopeName::parse("offline_access").unwrap(),
    ]);
    client.grant_types = BTreeSet::from([
        OAuthGrantType::DeviceAuthorization,
        OAuthGrantType::RefreshToken,
        OAuthGrantType::TokenExchange,
    ]);
    client.redirect_uris.clear();
    plane.oauth_clients.push(client);
    plane.oidc_clients[0]
        .allowed_resources
        .insert(ProtectedResourceId::parse(RESOURCE).unwrap());
    plane
}
#[test]
fn native_tls_endpoint_preserves_authority_and_rejects_invalid_native_urls() {
    let source: Lore = serde_json::from_value(plane().extensions.remove("lore").unwrap()).unwrap();
    let mut deployment = source.deployments.into_iter().next().unwrap();
    for (native, transport) in [
        (
            "grpcs://source.example:41337",
            "https://source.example:41337/",
        ),
        (
            "grpcs://[2001:db8::7]:41337",
            "https://[2001:db8::7]:41337/",
        ),
    ] {
        deployment.endpoint = url::Url::parse(native).unwrap();
        let endpoint = deployment.tls_endpoint().unwrap();
        assert_eq!(endpoint.uri().to_string(), transport);
        assert_eq!(endpoint.uri().port_u16(), Some(41337));
    }
    for invalid in [
        "https://source.example:41337",
        "grpc://source.example:41337",
        "grpcs:/relative",
        "grpcs://user@source.example:41337",
        "grpcs://source.example:41337/repository",
        "grpcs://source.example:41337?query=value",
        "grpcs://source.example:41337#fragment",
    ] {
        deployment.endpoint = url::Url::parse(invalid).unwrap();
        assert!(deployment.tls_endpoint().is_err(), "{invalid}");
    }
}
fn catalog(plane: HubControlPlane) -> HubCatalog {
    HubCatalog::from_control_plane(
        plane,
        HubCatalogAdmission::unbound()
            .bind(super::super::bindings().unwrap().catalog.clone())
            .unwrap(),
    )
    .unwrap()
}
fn context<'a>(catalog: &'a HubCatalog, principal: &'a Principal) -> TokenContext<'a> {
    let resource = catalog
        .contributed_protected_resource(&ProtectedResourceId::parse(RESOURCE).unwrap())
        .unwrap();
    let client = catalog
        .oauth_client(&OAuthClientId::parse("native-public").unwrap())
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
fn authorize(
    catalog: &HubCatalog,
    principal: &Principal,
    request: Request,
) -> Result<serde_json::Value> {
    let profile = Profile {
        key: catalog
            .admission()
            .registry()?
            .section_key::<Lore>(&ExtensionName::parse("lore")?)?,
    };
    let token = authorize_token(
        &profile,
        context(catalog, principal),
        &request,
        &super::super::bindings()?.profiles.claim_registry(),
    )?;
    Ok(serde_json::to_value(token.into_extensions().wire())?)
}
#[test]
fn native_claims_are_exact_current_and_do_not_infer_administrator_rights() {
    let current = catalog(plane());
    let alice = principal();
    let login = authorize(&current, &alice, Request::Login).unwrap();
    assert_eq!(login["identity"], "native-alice");
    assert_eq!(login["root_domains"], serde_json::json!(["10.0.0.7"]));
    assert_eq!(login["resources"], serde_json::json!([]));
    let request = |id| Request::Exchange {
        resource: None,
        scope: Some(format!("repo/{id}/access")),
    };
    let grant = authorize(&current, &alice, request(FIRST)).unwrap();
    assert_eq!(
        grant["resources"],
        serde_json::json!([{"resource_id":format!("partition-{FIRST}"),"permission":[]}])
    );
    assert!(authorize(&current, &alice, request(SECOND)).is_err());
    let mut stranger = alice.clone();
    stranger.id = PrincipalId::parse("native-bob").unwrap();
    assert!(authorize(&current, &stranger, request(FIRST)).is_err());
    stranger.id = alice.id.clone();
    stranger.tenant = Some(TenantId::parse("tenant-b").unwrap());
    assert!(authorize(&current, &stranger, request(FIRST)).is_err());
    for target in [
        format!("repo/{FIRST}/access extra"),
        format!("lore-repo-{FIRST}"),
        format!("repo/{FIRST}x/access"),
    ] {
        assert!(
            authorize(
                &current,
                &alice,
                Request::Exchange {
                    resource: None,
                    scope: Some(target)
                }
            )
            .is_err()
        );
    }
    let handle = HubCatalogHandle::new(Arc::new(current));
    let mut replacement = plane();
    replacement.extensions.get_mut("lore").unwrap()["assignments"] = serde_json::json!([]);
    handle.replace(Arc::new(catalog(replacement))).unwrap();
    assert!(
        authorize(&handle.current(), &alice, request(FIRST)).is_err(),
        "next admitted revision must not retain removed rights"
    );
}
#[test]
fn native_templates_and_actions_are_data_with_closed_protocol_encoding() {
    let mut source = plane();
    source.extensions.get_mut("lore").unwrap()["deployments"][0]["scopeTemplate"] =
        serde_json::json!("another-{id}");
    source.extensions.get_mut("lore").unwrap()["assignments"][0]["actions"] =
        serde_json::json!(["owner", "migrate"]);
    let current = catalog(source);
    let grant = authorize(
        &current,
        &principal(),
        Request::Exchange {
            resource: None,
            scope: Some(format!("another-{FIRST}")),
        },
    )
    .unwrap();
    assert_eq!(
        grant["resources"][0]["permission"],
        serde_json::json!(["owner", "migrate"])
    );
    for template in ["constant", "{id}{id}", "repo-{wrong}", "repo {id}"] {
        assert!(serde_json::from_value::<RepositoryTemplate>(serde_json::json!(template)).is_err());
    }
    assert!(serde_json::from_value::<NativeAction>(serde_json::json!("service_account")).is_err());
    let mut source = plane();
    source.extensions.get_mut("lore").unwrap()["assignments"][0]["tenant"] =
        serde_json::json!("tenant-b");
    assert!(
        HubCatalog::from_control_plane(
            source,
            HubCatalogAdmission::unbound()
                .bind(super::super::bindings().unwrap().catalog.clone())
                .unwrap()
        )
        .is_err()
    );
}
#[test]
fn an_assigned_request_carries_each_assignment_of_the_principal_on_the_deployment() {
    const OTHER: &str = "https://other.example/native";
    let mut source = plane();
    let lore = source.extensions.get_mut("lore").unwrap();
    let mut other = lore["deployments"][0].clone();
    other["id"] = serde_json::json!("other");
    other["resourceName"] = serde_json::json!("native-other");
    other["audience"] = serde_json::json!(OTHER);
    lore["deployments"].as_array_mut().unwrap().push(other);
    lore["repositories"].as_array_mut().unwrap().push(
        serde_json::json!({"deployment":"other","id":FIRST,"tenant":"tenant-a","title":"Elsewhere"}),
    );
    lore["assignments"].as_array_mut().unwrap().extend([
        serde_json::json!({"deployment":"source","repository":SECOND,"tenant":"tenant-a","principal":"native-alice","actions":["owner"]}),
        serde_json::json!({"deployment":"other","repository":FIRST,"tenant":"tenant-a","principal":"native-alice","actions":["admin"]}),
    ]);
    source.oidc_clients[0]
        .allowed_resources
        .insert(ProtectedResourceId::parse(OTHER).unwrap());
    let current = catalog(source);
    let alice = principal();
    let grant = authorize(&current, &alice, Request::Assigned).unwrap();
    assert_eq!(grant["identity"], "native-alice");
    assert_eq!(
        grant["resources"],
        serde_json::json!([
            {"resource_id":format!("partition-{FIRST}"),"permission":[]},
            {"resource_id":format!("partition-{SECOND}"),"permission":["owner"]}
        ]),
        "every assignment on this deployment, with its own actions, and none on another"
    );
    let mut unassigned = alice.clone();
    unassigned.id = PrincipalId::parse("native-bob").unwrap();
    let mut elsewhere = alice.clone();
    elsewhere.tenant = Some(TenantId::parse("tenant-b").unwrap());
    for principal in [unassigned, elsewhere] {
        assert_eq!(
            authorize(&current, &principal, Request::Assigned).unwrap()["resources"],
            serde_json::json!([]),
            "no assignment of this principal in its tenant is a token without resources"
        );
    }
}
#[test]
fn changed_owners_are_those_whose_assignments_were_added_removed_or_changed() {
    use veoveo_hub::computers::ComputerOwner;
    let owner = |principal: &str| ComputerOwner {
        tenant: TenantId::parse("tenant-a").unwrap(),
        principal: PrincipalId::parse(principal).unwrap(),
    };
    let base = plane();
    let with = |change: &dyn Fn(&mut serde_json::Value)| {
        let mut changed = plane();
        change(changed.extensions.get_mut("lore").unwrap());
        changed
    };
    let changed = |previous: Option<&HubControlPlane>, active: &HubControlPlane| {
        super::renewal::changed_owners(previous, active)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let added = with(&|lore| {
        lore["assignments"].as_array_mut().unwrap().push(serde_json::json!(
            {"deployment":"source","repository":SECOND,"tenant":"tenant-a","principal":"native-bob","actions":[]}
        ));
    });
    assert_eq!(changed(Some(&base), &added), [owner("native-bob")]);
    assert_eq!(
        changed(Some(&added), &base),
        [owner("native-bob")],
        "removal"
    );
    let promoted = with(&|lore| {
        lore["assignments"][0]["actions"] = serde_json::json!(["owner"]);
    });
    assert_eq!(changed(Some(&base), &promoted), [owner("native-alice")]);
    let retitled = with(&|lore| {
        lore["repositories"][0]["title"] = serde_json::json!("Renamed");
    });
    assert_eq!(changed(Some(&base), &retitled), []);
    let mut unrelated = plane();
    unrelated.metadata = serde_json::json!({"note": "unrelated"});
    assert_eq!(changed(Some(&base), &unrelated), []);
    assert_eq!(
        changed(None, &base),
        [owner("native-alice")],
        "a first revision assigns its owners"
    );
}
#[test]
fn only_an_exchange_for_one_repository_names_it_as_a_scope() {
    let current = catalog(plane());
    let alice = principal();
    let profile = Profile {
        key: current
            .admission()
            .registry()
            .unwrap()
            .section_key::<Lore>(&ExtensionName::parse("lore").unwrap())
            .unwrap(),
    };
    let section = Lore::of(&current).unwrap();
    let scopes = |request: Request| {
        let context = context(&current, &alice);
        let grant = profile.authorize(&section, &context, &request).unwrap();
        profile.issued_scopes(&grant, &context)
    };
    let repository = format!("repo/{FIRST}/access");
    assert_eq!(
        scopes(Request::Exchange {
            resource: None,
            scope: Some(repository.clone()),
        }),
        BTreeSet::from([ScopeName::parse(repository).unwrap()])
    );
    assert_eq!(
        scopes(Request::Assigned),
        BTreeSet::new(),
        "the token of every assigned repository carries its resources and no scope"
    );
    assert_eq!(
        scopes(Request::Login),
        BTreeSet::from([
            ScopeName::parse("native:login").unwrap(),
            ScopeName::parse("offline_access").unwrap(),
        ])
    );
}
