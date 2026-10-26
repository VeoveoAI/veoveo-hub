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

const THIRD: &str = "22222222222222222222222222222222";
const FOURTH: &str = "33333333333333333333333333333333";

/// A section whose deployment `source` renders `partition-{id}` and in which alice of
/// tenant-a holds three repositories; bob, alice in another tenant and alice on another
/// deployment hold others.
fn listed() -> Lore {
    let mut section = plane().extensions.remove("lore").unwrap();
    let mut other = section["deployments"][0].clone();
    other["id"] = serde_json::json!("other");
    other["resourceName"] = serde_json::json!("native-other");
    other["audience"] = serde_json::json!("https://other.example/native");
    section["deployments"].as_array_mut().unwrap().push(other);
    let owner: &[&str] = &["owner"];
    let assignments: [(&str, &str, &str, &str, &[&str]); 6] = [
        (
            "source",
            THIRD,
            "tenant-a",
            "native-alice",
            &["admin", "owner"],
        ),
        ("source", FIRST, "tenant-a", "native-alice", &[]),
        ("source", SECOND, "tenant-a", "native-alice", owner),
        ("source", FOURTH, "tenant-a", "native-bob", owner),
        ("source", FOURTH, "tenant-b", "native-alice", owner),
        ("other", FOURTH, "tenant-a", "native-alice", owner),
    ];
    section["assignments"] = assignments
        .iter()
        .map(|(deployment, repository, tenant, principal, actions)| {
            serde_json::json!({"deployment":deployment,"repository":repository,"tenant":tenant,"principal":principal,"actions":actions})
        })
        .collect();
    serde_json::from_value(section).unwrap()
}

fn lookup(
    filter: &str,
    page_size: Option<i32>,
    page_token: Option<&str>,
) -> lore_proto::auth::LookupUserPermissionsRequest {
    lore_proto::auth::LookupUserPermissionsRequest {
        resource_filter: filter.to_owned(),
        context_filter: None,
        page_size,
        page_token: page_token.map(str::to_owned),
    }
}

#[test]
fn a_listing_names_the_callers_own_assignments_on_the_deployment_in_pages() {
    let section = listed();
    let source = &section.deployments[0];
    let tenant = TenantId::parse("tenant-a").unwrap();
    let alice = PrincipalId::parse("native-alice").unwrap();
    let list = |request| super::catalog::listing(&section, source, &tenant, &alice, &request);
    let entry = |id: &str, permission: &[&str]| lore_proto::auth::ResourcePermission {
        resource_id: format!("partition-{id}"),
        permission: permission.iter().map(|name| (*name).to_owned()).collect(),
    };
    let all = vec![
        entry(FIRST, &[]),
        entry(SECOND, &["owner"]),
        entry(THIRD, &["owner", "admin"]),
    ];

    // What Lore sends for its own resource type, and an empty filter: every entry of the
    // caller, in resource-ID order, on one page.
    for request in [
        lookup("partition", None, None),
        lookup("partition", Some(0), None),
        lookup("", None, None),
    ] {
        let page = list(request).unwrap();
        assert_eq!(page.resource_permission, all);
        assert_eq!(page.next_page_token, None);
    }
    // Another resource type, and a type that only shares a prefix, name nothing.
    for filter in ["urc", "part"] {
        let page = list(lookup(filter, None, None)).unwrap();
        assert!(page.resource_permission.is_empty(), "{filter}");
        assert_eq!(page.next_page_token, None);
    }

    // Two entries a page: the token is the last ID of a page that more follow.
    let first = list(lookup("partition", Some(2), None)).unwrap();
    assert_eq!(first.resource_permission, all[..2]);
    let token = first.next_page_token.unwrap();
    assert_eq!(token, format!("partition-{SECOND}"));
    let second = list(lookup("partition", Some(2), Some(&token))).unwrap();
    assert_eq!(second.resource_permission, all[2..]);
    assert_eq!(second.next_page_token, None);
    // A full last page ends the listing too, and a token past every entry names none.
    let exact = list(lookup("partition", Some(3), None)).unwrap();
    assert_eq!(exact.resource_permission, all);
    assert_eq!(exact.next_page_token, None);
    let past = list(lookup(
        "partition",
        None,
        Some(&format!("partition-{FOURTH}")),
    ))
    .unwrap();
    assert!(past.resource_permission.is_empty());
    assert_eq!(past.next_page_token, None);
    // An empty token is the first page.
    assert_eq!(
        list(lookup("partition", None, Some("")))
            .unwrap()
            .resource_permission,
        all
    );

    // Another caller, the caller in another tenant and one with nothing see only theirs.
    let bob = PrincipalId::parse("native-bob").unwrap();
    assert_eq!(
        super::catalog::listing(
            &section,
            source,
            &tenant,
            &bob,
            &lookup("partition", None, None)
        )
        .unwrap()
        .resource_permission,
        vec![entry(FOURTH, &["owner"])]
    );
    let elsewhere = TenantId::parse("tenant-b").unwrap();
    assert_eq!(
        super::catalog::listing(
            &section,
            source,
            &elsewhere,
            &alice,
            &lookup("", None, None)
        )
        .unwrap()
        .resource_permission,
        vec![entry(FOURTH, &["owner"])]
    );
    let carol = PrincipalId::parse("native-carol").unwrap();
    let none = super::catalog::listing(&section, source, &tenant, &carol, &lookup("", None, None))
        .unwrap();
    assert!(none.resource_permission.is_empty());
    assert_eq!(none.next_page_token, None);

    // What the Hub does not define is refused, never ignored.
    let mut context = lookup("partition", None, None);
    context.context_filter = Some(FIRST.to_owned());
    for (case, request) in [
        ("a context filter", context),
        ("a negative page size", lookup("partition", Some(-1), None)),
        (
            "a token of another form",
            lookup("partition", None, Some(&format!("urc-{FIRST}"))),
        ),
        (
            "a token that is no ID",
            lookup("partition", None, Some("partition-x")),
        ),
    ] {
        assert_eq!(
            list(request).unwrap_err().code(),
            tonic::Code::InvalidArgument,
            "{case}"
        );
    }
}

#[test]
fn a_listed_action_is_named_as_the_token_names_it() {
    for action in [
        NativeAction::Owner,
        NativeAction::Admin,
        NativeAction::Obliterate,
        NativeAction::Migrate,
    ] {
        assert_eq!(
            serde_json::json!(action.as_str()),
            serde_json::json!(action)
        );
    }
}
