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
        scopes(Request::Login),
        BTreeSet::from([
            ScopeName::parse("native:login").unwrap(),
            ScopeName::parse("offline_access").unwrap(),
        ])
    );
}

const THIRD: &str = "22222222222222222222222222222222";
const FOURTH: &str = "33333333333333333333333333333333";
const FIFTH: &str = "55555555555555555555555555555555";

/// A section whose deployment `source` renders `partition-{id}` and in which alice of
/// tenant-a holds three repositories; bob, alice in another tenant and alice on another
/// deployment hold others, and nobody holds the last.
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
        ("source", FIFTH, "tenant-b", "native-alice", owner),
        ("other", FOURTH, "tenant-a", "native-alice", owner),
    ];
    let registered: [(&str, &str, &str); 7] = [
        ("source", FIRST, "tenant-a"),
        ("source", SECOND, "tenant-a"),
        ("source", THIRD, "tenant-a"),
        ("source", FOURTH, "tenant-a"),
        ("source", FIFTH, "tenant-b"),
        ("other", FOURTH, "tenant-a"),
        ("other", FIFTH, "tenant-a"),
    ];
    section["repositories"] = registered
        .iter()
        .map(|(deployment, id, tenant)| {
            serde_json::json!({"deployment":deployment,"id":id,"tenant":tenant,"title":id})
        })
        .collect();
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
fn a_listing_names_each_repository_the_caller_holds_on_the_deployment_in_pages() {
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
        vec![entry(FIFTH, &["owner"])]
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

const GROUP: &str = "44444444444444444444444444444444";

/// [`plane`] with the group `bioma` of tenant-a, whose owner is bob and whose member is
/// alice, assigned `owner` on FIRST, where alice's own assignment names no action, and
/// `admin` on SECOND, which alice holds only through it.
fn grouped() -> HubControlPlane {
    let mut plane = plane();
    let lore = plane.extensions.get_mut("lore").unwrap();
    lore["groups"] = serde_json::json!([{
        "id": GROUP, "tenant": "tenant-a", "name": "bioma",
        "members": [
            {"principal": "native-bob", "role": "owner"},
            {"principal": "native-alice", "role": "member"}
        ]
    }]);
    lore["assignments"].as_array_mut().unwrap().extend([
        serde_json::json!({"deployment":"source","repository":FIRST,"tenant":"tenant-a","group":GROUP,"actions":["owner"]}),
        serde_json::json!({"deployment":"source","repository":SECOND,"tenant":"tenant-a","group":GROUP,"actions":["admin"]}),
    ]);
    plane
}

fn actions(
    section: &Lore,
    principal: &str,
    tenant: &str,
    repository: &str,
) -> Option<Vec<NativeAction>> {
    section
        .actions(
            &DeploymentId::parse("source").unwrap(),
            &RepositoryId::parse(repository).unwrap(),
            &TenantId::parse(tenant).unwrap(),
            &PrincipalId::parse(principal).unwrap(),
        )
        .map(|actions| actions.into_iter().collect())
}

#[test]
fn a_persons_actions_unite_their_own_assignment_and_their_groups() {
    let current = catalog(grouped());
    let section = Lore::of(&current).unwrap();
    use NativeAction::{Admin, Owner};
    // Their own assignment and their group's on one repository: the union.
    assert_eq!(
        actions(&section, "native-alice", "tenant-a", FIRST),
        Some(vec![Owner])
    );
    // Only through the group, whatever their role in it.
    assert_eq!(
        actions(&section, "native-alice", "tenant-a", SECOND),
        Some(vec![Admin])
    );
    assert_eq!(
        actions(&section, "native-bob", "tenant-a", SECOND),
        Some(vec![Admin])
    );
    // Nobody else, and not the same principal in another tenant.
    assert_eq!(actions(&section, "native-carol", "tenant-a", FIRST), None);
    assert_eq!(actions(&section, "native-alice", "tenant-b", FIRST), None);

    // Every reader uses the rule: the exchange, the listing and the status routes' check.
    let alice = principal();
    let exchange = |current: &HubCatalog, id: &str| {
        authorize(
            current,
            &alice,
            Request::Exchange {
                resource: None,
                scope: Some(format!("repo/{id}/access")),
            },
        )
    };
    assert_eq!(
        exchange(&current, SECOND).unwrap()["resources"],
        serde_json::json!([{"resource_id":format!("partition-{SECOND}"),"permission":["admin"]}])
    );
    assert_eq!(
        exchange(&current, FIRST).unwrap()["resources"],
        serde_json::json!([{"resource_id":format!("partition-{FIRST}"),"permission":["owner"]}])
    );
    let listed = super::catalog::listing(
        &section,
        &section.deployments[0],
        alice.tenant.as_ref().unwrap(),
        &alice.id,
        &lookup("", None, None),
    )
    .unwrap();
    assert_eq!(
        listed.resource_permission,
        [(FIRST, "owner"), (SECOND, "admin")]
            .map(|(id, action)| lore_proto::auth::ResourcePermission {
                resource_id: format!("partition-{id}"),
                permission: vec![action.to_owned()],
            })
            .to_vec(),
        "one entry per repository, with the union of its actions"
    );
    let second = RepositoryId::parse(SECOND).unwrap();
    assert!(section.grants(&alice, &second));
    let mut elsewhere = alice.clone();
    elsewhere.tenant = Some(TenantId::parse("tenant-b").unwrap());
    assert!(!section.grants(&elsewhere, &second));

    // A removed membership takes the group's assignments with it at the next revision.
    let mut removed = grouped();
    removed.extensions.get_mut("lore").unwrap()["groups"][0]["members"]
        .as_array_mut()
        .unwrap()
        .retain(|member| member["principal"] != "native-alice");
    let current = catalog(removed);
    let section = Lore::of(&current).unwrap();
    assert_eq!(
        actions(&section, "native-alice", "tenant-a", FIRST),
        Some(vec![])
    );
    assert_eq!(actions(&section, "native-alice", "tenant-a", SECOND), None);
    assert!(exchange(&current, SECOND).is_err());
    assert!(!section.grants(&alice, &second));
    assert_eq!(
        actions(&section, "native-bob", "tenant-a", SECOND),
        Some(vec![Admin])
    );
}

#[test]
fn a_groups_members_hold_it_by_its_id_writing_and_its_owners_administering() {
    use veoveo_hub_sdk::{GroupMembership, GroupRole};
    let current = catalog(grouped());
    let memberships = |principal: &str, tenant: &str| {
        current
            .sections()
            .group_memberships(
                &TenantId::parse(tenant).unwrap(),
                &PrincipalId::parse(principal).unwrap(),
            )
            .cloned()
            .collect::<Vec<_>>()
    };
    let group = veoveo_types::GroupId::parse(GROUP).unwrap();
    assert_eq!(
        memberships("native-alice", "tenant-a"),
        vec![GroupMembership {
            group: group.clone(),
            role: GroupRole::Write,
        }]
    );
    assert_eq!(
        memberships("native-bob", "tenant-a"),
        vec![GroupMembership {
            group,
            role: GroupRole::Admin,
        }]
    );
    assert!(memberships("native-carol", "tenant-a").is_empty());
    assert!(memberships("native-alice", "tenant-b").is_empty());
}

/// Whether the configuration's admission accepts `plane`.
fn admits(plane: HubControlPlane) -> bool {
    HubCatalog::from_control_plane(
        plane,
        HubCatalogAdmission::unbound()
            .bind(super::super::bindings().unwrap().catalog.clone())
            .unwrap(),
    )
    .is_ok()
}

#[test]
fn groups_and_the_subjects_of_assignments_are_admitted_only_when_coherent() {
    assert!(admits(grouped()));
    let changed = |change: &dyn Fn(&mut serde_json::Value)| {
        let mut plane = grouped();
        change(plane.extensions.get_mut("lore").unwrap());
        plane
    };
    let group_assignment = |lore: &mut serde_json::Value| -> serde_json::Value {
        lore["assignments"].as_array().unwrap()[1].clone()
    };
    type Change<'a> = dyn Fn(&mut serde_json::Value) + 'a;
    let refused: [(&str, &Change); 9] = [
        ("both subjects", &|lore| {
            lore["assignments"][1]["principal"] = serde_json::json!("native-carol");
        }),
        ("no subject", &|lore| {
            lore["assignments"][0]
                .as_object_mut()
                .unwrap()
                .remove("principal");
        }),
        ("an unknown group", &|lore| {
            lore["assignments"][1]["group"] = serde_json::json!(THIRD);
        }),
        ("the same group twice on a repository", &|lore| {
            let again = group_assignment(lore);
            lore["assignments"].as_array_mut().unwrap().push(again);
        }),
        ("a duplicate group ID", &|lore| {
            let mut again = lore["groups"][0].clone();
            again["name"] = serde_json::json!("other");
            lore["groups"].as_array_mut().unwrap().push(again);
        }),
        ("a duplicate name in a tenant", &|lore| {
            let mut again = lore["groups"][0].clone();
            again["id"] = serde_json::json!(THIRD);
            lore["groups"].as_array_mut().unwrap().push(again);
        }),
        ("a duplicate member", &|lore| {
            let again = lore["groups"][0]["members"][0].clone();
            lore["groups"][0]["members"]
                .as_array_mut()
                .unwrap()
                .push(again);
        }),
        ("a group of an unknown tenant", &|lore| {
            lore["groups"][0]["tenant"] = serde_json::json!("tenant-c");
        }),
        ("an unknown role", &|lore| {
            lore["groups"][0]["members"][0]["role"] = serde_json::json!("admin");
        }),
    ];
    for (case, change) in refused {
        assert!(!admits(changed(change)), "{case}");
    }
    for name in [
        "",
        "Bioma",
        "1bioma",
        "-bioma",
        "bio_ma",
        "bio ma",
        &"a".repeat(65),
    ] {
        assert!(GroupName::parse(name).is_err(), "{name:?}");
        assert!(!admits(changed(
            &|lore| lore["groups"][0]["name"] = serde_json::json!(name)
        )));
    }
    for name in ["a", "bioma-2", &format!("b{}", "0".repeat(63))] {
        assert!(GroupName::parse(name).is_ok(), "{name:?}");
    }

    // A group of another tenant is no subject for this tenant's repositories, and the
    // same name may name a group in each tenant.
    let mut tenants = grouped();
    tenants
        .tenants
        .push(serde_json::from_value(serde_json::json!({"id": "tenant-b"})).unwrap());
    let lore = tenants.extensions.get_mut("lore").unwrap();
    let mut other = lore["groups"][0].clone();
    other["id"] = serde_json::json!(THIRD);
    other["tenant"] = serde_json::json!("tenant-b");
    lore["groups"].as_array_mut().unwrap().push(other);
    assert!(admits(tenants.clone()), "one name in each of two tenants");
    tenants.extensions.get_mut("lore").unwrap()["assignments"][1]["group"] =
        serde_json::json!(THIRD);
    assert!(!admits(tenants), "a group of another tenant");

    // Today's documents read as before, and a person's assignment is written as before.
    let today = plane().extensions.remove("lore").unwrap();
    let written =
        serde_json::to_value(serde_json::from_value::<Lore>(today.clone()).unwrap()).unwrap();
    assert_eq!(
        written["assignments"], today["assignments"],
        "no group subject appears"
    );
    assert!(written.get("groups").is_none(), "no groups member appears");
}
#[test]
fn a_computers_client_asks_for_its_owners_login_or_one_repositorys_token() {
    let current = catalog(grouped());
    let section = Lore::of(&current).unwrap();
    let deployment = &section.deployments[0];
    let alice = principal();
    let requested = |scope: Option<&str>| super::computer_client::requested(deployment, scope);

    // No scope: the owner's login token, which names them and carries no repository.
    let login = authorize(&current, &alice, requested(None).unwrap()).unwrap();
    assert_eq!(login["identity"], "native-alice");
    assert_eq!(login["resources"], serde_json::json!([]));

    // A repository's scope: that repository's token, with the owner's actions on it, own
    // and through their group.
    for (id, permission) in [(FIRST, ["owner"]), (SECOND, ["admin"])] {
        let scope = format!("repo/{id}/access");
        let grant = authorize(&current, &alice, requested(Some(&scope)).unwrap()).unwrap();
        assert_eq!(
            grant["resources"],
            serde_json::json!([{"resource_id":format!("partition-{id}"),"permission":permission}])
        );
    }
    // A repository the owner holds no grant on is refused at admission.
    let mut carol = alice.clone();
    carol.id = PrincipalId::parse("native-carol").unwrap();
    let first = requested(Some(&format!("repo/{FIRST}/access"))).unwrap();
    assert!(authorize(&current, &carol, first).is_err());
    assert_eq!(
        authorize(&current, &carol, requested(None).unwrap()).unwrap()["resources"],
        serde_json::json!([]),
        "an owner without grants still names themselves"
    );

    // Any other scope is refused before admission.
    for scope in [
        "native:login",
        "offline_access",
        "",
        &format!("repo/{FIRST}/access repo/{SECOND}/access"),
        &format!("repo/{FIRST}x/access"),
        &format!("partition-{FIRST}"),
    ] {
        assert!(requested(Some(scope)).is_err(), "{scope:?}");
    }
}

#[test]
fn no_token_carries_more_than_one_repository() {
    let current = catalog(grouped());
    let mut requests = vec![Request::Login];
    for id in [FIRST, SECOND] {
        requests.push(Request::Exchange {
            resource: None,
            scope: Some(format!("repo/{id}/access")),
        });
    }
    let mut issued = 0;
    for name in ["native-alice", "native-bob", "native-carol"] {
        let mut person = principal();
        person.id = PrincipalId::parse(name).unwrap();
        for request in &requests {
            if let Ok(token) = authorize(&current, &person, request.clone()) {
                issued += 1;
                assert!(
                    token["resources"].as_array().unwrap().len() <= 1,
                    "{name}: {token}"
                );
            }
        }
    }
    assert_eq!(
        issued, 7,
        "three login tokens, and alice's and bob's two repositories"
    );
}

/// The section of [`grouped`]: alice's own assignment on FIRST names no action, and the
/// group `bioma`, which bob owns and alice belongs to, holds `owner` there.
fn shared() -> Lore {
    Lore::of(&catalog(grouped())).unwrap()
}

fn caller<'a>(tenant: &'a TenantId, principal: &'a PrincipalId) -> super::sharing::Caller<'a> {
    super::sharing::Caller { tenant, principal }
}

#[test]
fn an_owner_shares_a_repository_and_never_loses_its_ownership_by_it() {
    use super::sharing::{Named, Refusal, Setting};
    let tenant = TenantId::parse("tenant-a").unwrap();
    let [alice, bob, carol] = ["native-alice", "native-bob", "native-carol"]
        .map(|principal| PrincipalId::parse(principal).unwrap());
    let source = DeploymentId::parse("source").unwrap();
    let [first, second] = [FIRST, SECOND].map(|id| RepositoryId::parse(id).unwrap());
    let bioma = Named::Group(GroupName::parse("bioma").unwrap());
    let mut section = shared();

    // Alice and bob own FIRST through bioma; carol, who holds nothing, may not share it.
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &carol),
            &Named::Person(alice.clone()),
            Setting::Owner
        ),
        Err(Refusal::NotRepositoryOwner)
    );
    assert_eq!(
        section.readable(&source, &first, caller(&tenant, &carol)),
        Err(Refusal::NotGranted)
    );
    // Nor may an owner of SECOND's `admin` share SECOND.
    assert_eq!(
        section.set_access(
            &source,
            &second,
            caller(&tenant, &alice),
            &Named::Person(carol.clone()),
            Setting::Member
        ),
        Err(Refusal::NotRepositoryOwner)
    );

    // An owner shares with a person, then replaces the level, then removes it.
    let carol_level = |section: &Lore| {
        section
            .access(&source, &first)
            .find(|a| a.subject == Subject::Principal(carol.clone()))
            .map(|a| a.actions.iter().copied().collect::<Vec<_>>())
    };
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(carol.clone()),
            Setting::Member
        ),
        Ok(true)
    );
    assert_eq!(carol_level(&section), Some(vec![]));
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(carol.clone()),
            Setting::Member
        ),
        Ok(false)
    );
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(carol.clone()),
            Setting::Owner
        ),
        Ok(true)
    );
    assert_eq!(carol_level(&section), Some(vec![NativeAction::Owner]));
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(carol.clone()),
            Setting::None
        ),
        Ok(true)
    );
    assert_eq!(carol_level(&section), None);
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(carol.clone()),
            Setting::None
        ),
        Ok(false)
    );

    // Bob owns FIRST only through bioma: lowering the group would leave him no owner.
    let before = section.assignments.clone();
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &bioma,
            Setting::Member
        ),
        Err(Refusal::KeepRepositoryOwner)
    );
    assert_eq!(
        section.assignments, before,
        "a refused change changes nothing"
    );
    // Once his own assignment holds `owner`, it may.
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(bob.clone()),
            Setting::Owner
        ),
        Ok(true)
    );
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &bioma,
            Setting::Member
        ),
        Ok(true)
    );
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(bob.clone()),
            Setting::None
        ),
        Err(Refusal::KeepRepositoryOwner)
    );
    // An administrator's other actions are replaced by the level an owner sets.
    section.assignments.iter_mut().for_each(|a| {
        if a.repository == first && a.subject == Subject::Principal(alice.clone()) {
            a.actions = BTreeSet::from([NativeAction::Admin, NativeAction::Migrate]);
        }
    });
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &Named::Person(alice.clone()),
            Setting::Member
        ),
        Ok(true)
    );
    assert!(
        section
            .access(&source, &first)
            .any(|a| a.subject == Subject::Principal(alice.clone()) && a.actions.is_empty())
    );

    // Unknown repositories and groups, and repositories of another tenant.
    let elsewhere = TenantId::parse("tenant-b").unwrap();
    let third = RepositoryId::parse(THIRD).unwrap();
    assert_eq!(
        section.set_access(
            &source,
            &third,
            caller(&tenant, &bob),
            &bioma,
            Setting::Member
        ),
        Err(Refusal::UnknownRepository)
    );
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&elsewhere, &bob),
            &bioma,
            Setting::Member
        ),
        Err(Refusal::UnknownRepository)
    );
    let unknown = Named::Group(GroupName::parse("nobody").unwrap());
    assert_eq!(
        section.set_access(
            &source,
            &first,
            caller(&tenant, &bob),
            &unknown,
            Setting::Member
        ),
        Err(Refusal::UnknownGroup)
    );

    // What the changes leave is still admitted.
    let mut plane = grouped();
    plane
        .extensions
        .insert("lore".into(), serde_json::to_value(&section).unwrap());
    assert!(admits(plane));
}

#[test]
fn a_group_is_created_by_its_owner_whose_owners_set_its_memberships() {
    use super::sharing::{Refusal, Setting};
    let tenant = TenantId::parse("tenant-a").unwrap();
    let [alice, bob, carol] = ["native-alice", "native-bob", "native-carol"]
        .map(|principal| PrincipalId::parse(principal).unwrap());
    let bioma = GroupName::parse("bioma").unwrap();
    let builders = GroupName::parse("builders").unwrap();
    let mut section = shared();

    let created = section
        .create_group(
            caller(&tenant, &carol),
            &builders,
            GroupId::parse(THIRD).unwrap(),
        )
        .unwrap();
    assert_eq!(
        created.members,
        [Member {
            principal: carol.clone(),
            role: GroupRole::Owner
        }]
    );
    assert_eq!(
        section
            .create_group(
                caller(&tenant, &alice),
                &bioma,
                GroupId::parse(FOURTH).unwrap()
            )
            .err(),
        Some(Refusal::GroupTaken)
    );

    // A member who is no owner, and someone outside the group, change nothing.
    assert_eq!(
        section.set_membership(caller(&tenant, &alice), &bioma, &carol, Setting::Member),
        Err(Refusal::NotGroupOwner)
    );
    assert_eq!(
        section.set_membership(caller(&tenant, &carol), &bioma, &carol, Setting::Member),
        Err(Refusal::NotGroupOwner)
    );
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &builders, &carol, Setting::None),
        Err(Refusal::NotGroupOwner)
    );
    let nobody = GroupName::parse("nobody").unwrap();
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &nobody, &carol, Setting::Member),
        Err(Refusal::UnknownGroup)
    );

    // An owner adds, promotes and removes members.
    let role =
        |section: &Lore, who: &PrincipalId| section.group_named(&tenant, &bioma).unwrap().role(who);
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &bioma, &carol, Setting::Member),
        Ok(true)
    );
    assert_eq!(role(&section, &carol), Some(GroupRole::Member));
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &bioma, &carol, Setting::Member),
        Ok(false)
    );
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &bioma, &alice, Setting::Owner),
        Ok(true)
    );
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &bioma, &carol, Setting::None),
        Ok(true)
    );
    assert_eq!(role(&section, &carol), None);

    // An owner never lowers or removes themselves, even with another owner beside them.
    for setting in [Setting::Member, Setting::None] {
        assert_eq!(
            section.set_membership(caller(&tenant, &bob), &bioma, &bob, setting),
            Err(Refusal::KeepGroupOwner)
        );
    }
    assert_eq!(
        section.set_membership(caller(&tenant, &bob), &bioma, &bob, Setting::Owner),
        Ok(false)
    );
    // Another owner may.
    assert_eq!(
        section.set_membership(caller(&tenant, &alice), &bioma, &bob, Setting::None),
        Ok(true)
    );
    assert_eq!(role(&section, &bob), None);

    let mut plane = grouped();
    plane
        .extensions
        .insert("lore".into(), serde_json::to_value(&section).unwrap());
    assert!(admits(plane));
}

#[test]
fn an_owner_deletes_a_repository_with_every_assignment_on_it() {
    use super::sharing::Refusal;
    let [tenant, elsewhere] = ["tenant-a", "tenant-b"].map(|t| TenantId::parse(t).unwrap());
    let [alice, carol] =
        ["native-alice", "native-carol"].map(|principal| PrincipalId::parse(principal).unwrap());
    let source = DeploymentId::parse("source").unwrap();
    let [first, second, unregistered] =
        [FIRST, SECOND, THIRD].map(|id| RepositoryId::parse(id).unwrap());
    let mut section = shared();
    let before = section.clone();

    // Carol holds nothing on FIRST, alice holds `admin` but not `owner` on SECOND, and
    // alice of another tenant is not the alice who owns FIRST.
    for (repository, caller) in [
        (&first, caller(&tenant, &carol)),
        (&second, caller(&tenant, &alice)),
        (&first, caller(&elsewhere, &alice)),
    ] {
        assert_eq!(
            section.delete_repository(&source, repository, caller),
            Err(Refusal::NotRepositoryOwner)
        );
    }
    assert_eq!(section.assignments, before.assignments);

    // A repository the section does not register leaves nothing to remove.
    assert_eq!(
        section.delete_repository(&source, &unregistered, caller(&tenant, &alice)),
        Ok(false)
    );

    // Alice owns FIRST through bioma: it goes, with her own assignment and the group's.
    assert_eq!(
        section.delete_repository(&source, &first, caller(&tenant, &alice)),
        Ok(true)
    );
    assert!(section.repositories.iter().all(|r| r.id != first));
    assert!(section.assignments.iter().all(|a| a.repository != first));
    assert_eq!(
        section.assignments.len(),
        before.assignments.len() - 2,
        "SECOND keeps its assignment"
    );
}
