//! Owners share Lore repositories with people and groups, against the hosted process: the
//! access and group routes, as the acceptance callers call them, and what their changes
//! do to Lore tokens and to Lore's repository listing. The people are synthetic rows of
//! `hub.person`, written as a sign-in writes them; no identity provider or Lore server is
//! involved.
#[path = "support/database.rs"]
mod database;
#[path = "support/hub.rs"]
mod fixture;

use std::time::Duration;

use chrono::Utc;
use lore_proto::auth::{
    LookupUserPermissionsRequest, ResourcePermission, urc_auth_api_client::UrcAuthApiClient,
};
use serde_json::{Value, json};
use veoveo_hub::{
    contract::PrincipalDisplayName,
    person::{Email, Person},
    store::HubStore,
};
use veoveo_types::{PrincipalId, TenantId};

const RESOURCE: &str = "https://native.example/source";
const SECRET: &str = "synthetic-native-factory-secret";
/// The repository the member owns.
const OWNED: &str = "0123456789abcdef0123456789abcdef";
/// The repository the factory client's principal is assigned, for its Lore token.
const FACTORY: &str = "1123456789abcdef0123456789abcdef";
/// A repository nobody registered.
const UNKNOWN: &str = "e123456789abcdef0123456789abcdef";
const MEMBER: &str = "member@example.test";
const ADMIN: &str = "admin@example.test";
const BUILDER: &str = "builder@example.test";

/// One Lore deployment, the member's repository and the factory client's.
fn configure(config: &mut Value) {
    let issuer = config["authorizationServers"][0]["issuer"]
        .as_str()
        .unwrap()
        .to_owned();
    let repository = |id: &str| json!({"deployment":"source","id":id,"tenant":"local","title":id});
    let assignment = |id: &str, principal: String, actions: Value| json!({"deployment":"source","repository":id,"tenant":"local","principal":principal,"actions":actions});
    config["lore"] = json!({
    "deployments":[{"id":"source","resourceName":"native-source","audience":RESOURCE,"endpoint":"grpcs://10.0.0.8:41337","authorizationServer":"local","policyVersion":"local","loginScopes":["native:login","offline_access"],"scopeTemplate":"source-{id}","resourceIdTemplate":"urc-{id}","tokenLifetimeSeconds":120}],
    "repositories":[repository(OWNED), repository(FACTORY)],
    "assignments":[
        assignment(OWNED, format!("{issuer}#acceptance-member"), json!(["owner"])),
        assignment(FACTORY, format!("{issuer}#native-factory"), json!([])),
    ]});
    config["identityProviders"][0]["authorizationEndpoint"] =
        json!("https://identity.example/authorize");
    config["identityProviders"][0]["tokenEndpoint"] = json!("https://identity.example/token");
    let clients = config["oauthClients"].as_array_mut().unwrap();
    clients.push(json!({"id":"native-cli","authorizationServer":"local","defaultWorkContext":"local","invocationMode":"direct","allowedResources":[RESOURCE],"grantTypes":["device_authorization","refresh_token","token_exchange"],"authMethods":["none"],"allowedScopes":["native:login","offline_access"]}));
    clients.push(json!({"id":"native-factory","authorizationServer":"local","defaultWorkContext":"local","invocationMode":"automated","allowedResources":[RESOURCE],"grantTypes":["client_credentials"],"authMethods":["client_secret_post"],"allowedScopes":["native:login","offline_access"],"credentialSecret":"native-client-secret","tenant":"local"}));
    config["secrets"].as_array_mut().unwrap().push(json!({"id":"native-client-secret","source":"env","purpose":"oauth_client_secret","locator":"VEOVEO_NATIVE_FIXTURE_SECRET","owner":{"kind":"hub"}}));
    config["oidcClients"] = json!([{"id":"native-broker","identityProvider":"local","authorizationServer":"local","allowedResources":[RESOURCE],"clientId":"synthetic-broker","redirectUri":"https://hub.example/oauth/callback","authMethod":"client_secret_post","credentialSecret":"native-client-secret","scopes":["openid"]}]);
}

/// The acceptance callers and the factory client's principal as their sign-ins record them.
async fn signed_in(store: &HubStore, issuer: &str) {
    for (subject, name, email) in [
        ("acceptance-member", "Member", MEMBER),
        ("acceptance-admin", "Admin", ADMIN),
        ("native-factory", "Builder", BUILDER),
    ] {
        let person = Person {
            tenant: TenantId::parse("local").unwrap(),
            principal: PrincipalId::parse(format!("{issuer}#{subject}")).unwrap(),
            name: PrincipalDisplayName::new(name).unwrap(),
            email: Some(Email::parse(email).unwrap()),
        };
        store.record_person(&person, Utc::now()).await.unwrap();
    }
}

struct Calls<'a> {
    hub: &'a fixture::Hub,
    http: reqwest::Client,
}

impl Calls<'_> {
    /// The answer to `method` of `path` under `/lore/local` as `caller`.
    async fn call(
        &self,
        caller: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> (reqwest::StatusCode, Value) {
        let token = self
            .hub
            .access_token(&self.http, self.hub.caller(caller))
            .await;
        let mut request = self
            .http
            .request(method, format!("{}/lore/local{path}", self.hub.base))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        if status.is_success() {
            assert_eq!(response.headers()["cache-control"], "no-store");
        } else {
            assert_eq!(
                response.headers()["content-type"],
                "application/problem+json"
            );
        }
        (status, response.json().await.unwrap())
    }

    async fn access(&self, caller: &str, repository: &str) -> (reqwest::StatusCode, Value) {
        let path = format!("/repositories/{repository}/access");
        self.call(caller, reqwest::Method::GET, &path, None).await
    }

    async fn share(
        &self,
        caller: &str,
        repository: &str,
        body: Value,
    ) -> (reqwest::StatusCode, Value) {
        let path = format!("/repositories/{repository}/access");
        self.call(caller, reqwest::Method::PUT, &path, Some(body))
            .await
    }

    async fn membership(
        &self,
        caller: &str,
        group: &str,
        body: Value,
    ) -> (reqwest::StatusCode, Value) {
        let path = format!("/groups/{group}/members");
        self.call(caller, reqwest::Method::PUT, &path, Some(body))
            .await
    }

    /// A Lore token of the factory client's principal: for the repository `id` with a
    /// scope, or an error code.
    async fn factory_token(&self, id: &str) -> Result<String, String> {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("grant_type", "client_credentials"),
                ("client_id", "native-factory"),
                ("client_secret", SECRET),
                ("scope", &format!("source-{id}")),
            ])
            .finish();
        let response = self
            .http
            .post(format!("{}/oauth/token", self.hub.base))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form)
            .send()
            .await
            .unwrap();
        let body: Value = response.json().await.unwrap();
        match body["access_token"].as_str() {
            Some(token) => Ok(token.to_owned()),
            None => Err(body["error"].as_str().unwrap().to_owned()),
        }
    }
}

/// The problem type of an answer.
fn problem(body: &Value) -> &str {
    body["type"]
        .as_str()
        .unwrap()
        .strip_prefix("https://veoveo.ai/problems/")
        .unwrap()
}

/// The repositories Lore's listing names for `token`.
async fn listed(
    client: &mut UrcAuthApiClient<tonic::transport::Channel>,
    token: &str,
) -> Vec<ResourcePermission> {
    let mut request = tonic::Request::new(LookupUserPermissionsRequest {
        resource_filter: "urc".to_owned(),
        context_filter: None,
        page_size: None,
        page_token: None,
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    client
        .lookup_user_permissions(request)
        .await
        .unwrap()
        .into_inner()
        .resource_permission
}

fn entry(id: &str, permission: &[&str]) -> ResourcePermission {
    ResourcePermission {
        resource_id: format!("urc-{id}"),
        permission: permission.iter().map(|name| (*name).to_owned()).collect(),
    }
}

#[tokio::test]
async fn owners_share_repositories_with_people_and_groups() {
    tokio::time::timeout(Duration::from_secs(150), qualify())
        .await
        .expect("the sharing qualification exceeded its bound");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = database::TestDb::new().await;
    let hub = fixture::Hub::start_with(
        directory.path(),
        &db,
        Duration::from_secs(60),
        &[("VEOVEO_NATIVE_FIXTURE_SECRET", SECRET.to_owned())],
        configure,
    )
    .await;
    let issuer = hub.control_plane().await["authorizationServers"][0]["issuer"]
        .as_str()
        .unwrap()
        .to_owned();
    signed_in(&HubStore::new(db.a.clone()), &issuer).await;
    let calls = Calls {
        hub: &hub,
        http: reqwest::Client::new(),
    };
    let member = json!({"email": MEMBER, "name": "Member"});
    let admin = json!({"email": ADMIN, "name": "Admin"});
    let builder = json!({"email": BUILDER, "name": "Builder"});

    // The creator owns the repository; someone without a grant may not read or share it.
    let (status, access) = calls.access("member", OWNED).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{access}");
    assert_eq!(
        access,
        json!({"repository": OWNED, "access": [
            {"person": member, "access": "owner", "actions": ["owner"]}
        ]})
    );
    let (status, refused) = calls.access("admin", OWNED).await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
    assert_eq!(problem(&refused), "repository-access-forbidden");
    let (status, refused) = calls
        .share("admin", OWNED, json!({"person": ADMIN, "access": "owner"}))
        .await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
    assert_eq!(problem(&refused), "repository-owner-required");

    // The owner shares it with a person by email, whatever its case; that person may read
    // its access now, but not share it.
    let (status, shared) = calls
        .share(
            "member",
            OWNED,
            json!({"person": "Admin@Example.TEST", "access": "member"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{shared}");
    assert_eq!(
        shared,
        json!({"repository": OWNED, "access": [
            {"person": admin, "access": "member", "actions": []},
            {"person": member, "access": "owner", "actions": ["owner"]}
        ]})
    );
    assert_eq!(
        calls.access("admin", OWNED).await,
        (reqwest::StatusCode::OK, shared.clone())
    );
    let (status, refused) = calls
        .share("admin", OWNED, json!({"person": ADMIN, "access": "owner"}))
        .await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
    assert_eq!(problem(&refused), "repository-owner-required");

    // Someone who never signed in cannot be named, and the problem says what to do.
    let (status, refused) = calls
        .share(
            "member",
            OWNED,
            json!({"person": "nobody@example.test", "access": "member"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
    assert_eq!(problem(&refused), "person-not-found");
    assert!(
        refused["detail"].as_str().unwrap().contains("sign in once"),
        "{refused}"
    );

    // The owner cannot leave themselves without ownership.
    for access in ["member", "none"] {
        let (status, refused) = calls
            .share("member", OWNED, json!({"person": MEMBER, "access": access}))
            .await;
        assert_eq!(status, reqwest::StatusCode::CONFLICT, "{refused}");
        assert_eq!(problem(&refused), "repository-owner-kept");
    }

    // Requests that name no single subject, an unknown repository and an unknown group.
    for body in [
        json!({"person": ADMIN, "group": "builders", "access": "member"}),
        json!({"access": "member"}),
        json!({"person": ADMIN, "access": "reader"}),
        json!({"person": ADMIN, "access": "member", "actions": ["admin"]}),
        json!({"person": "not an address", "access": "member"}),
    ] {
        let (status, refused) = calls.share("member", OWNED, body.clone()).await;
        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(problem(&refused), "invalid-request");
    }
    for repository in [UNKNOWN, "not-an-id"] {
        let (status, refused) = calls.access("member", repository).await;
        assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
        assert_eq!(problem(&refused), "repository-not-found");
    }
    let (status, refused) = calls
        .share(
            "member",
            OWNED,
            json!({"group": "builders", "access": "member"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
    assert_eq!(problem(&refused), "group-not-found");

    // Anyone creates a group and owns it; a name is taken once in a tenant.
    let (status, created) = calls
        .call(
            "member",
            reqwest::Method::POST,
            "/groups",
            Some(json!({"name": "builders"})),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{created}");
    // Its ID, which grants outside Lore name it by, is random.
    let id = created["id"].as_str().unwrap_or_default().to_owned();
    assert!(
        id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        "{created}"
    );
    assert_eq!(
        created,
        json!({"id": id, "name": "builders", "members": [
            {"email": MEMBER, "name": "Member", "role": "owner"}
        ]})
    );
    let (status, refused) = calls
        .call(
            "admin",
            reqwest::Method::POST,
            "/groups",
            Some(json!({"name": "builders"})),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT);
    assert_eq!(problem(&refused), "group-exists");
    let (status, refused) = calls
        .call(
            "admin",
            reqwest::Method::POST,
            "/groups",
            Some(json!({"name": "Builders"})),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(problem(&refused), "invalid-group-name");
    let groups = |caller| calls.call(caller, reqwest::Method::GET, "/groups", None);
    assert_eq!(groups("admin").await.1, json!({"groups": []}));
    assert_eq!(groups("member").await.1, json!({"groups": [created]}));

    // The owner shares the repository with the group.
    let (status, shared) = calls
        .share(
            "member",
            OWNED,
            json!({"group": "builders", "access": "owner"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{shared}");
    assert_eq!(
        shared["access"][2],
        json!({"group": {"name": "builders"}, "access": "owner", "actions": ["owner"]})
    );

    // Lore's listing and tokens follow the group's membership: the factory client's
    // principal holds the repository while it belongs to the group.
    let mut lore = UrcAuthApiClient::connect(hub.base.clone()).await.unwrap();
    let token = calls.factory_token(FACTORY).await.unwrap();
    assert_eq!(listed(&mut lore, &token).await, [entry(FACTORY, &[])]);
    assert_eq!(
        calls.factory_token(OWNED).await,
        Err("invalid_scope".to_owned())
    );
    let (status, refused) = calls
        .membership(
            "admin",
            "builders",
            json!({"person": BUILDER, "role": "member"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(problem(&refused), "group-owner-required");
    let (status, group) = calls
        .membership(
            "member",
            "builders",
            json!({"person": BUILDER, "role": "member"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{group}");
    assert_eq!(
        group,
        json!({"id": id, "name": "builders", "members": [
            {"email": BUILDER, "name": "Builder", "role": "member"},
            {"email": MEMBER, "name": "Member", "role": "owner"}
        ]})
    );
    assert_eq!(
        listed(&mut lore, &token).await,
        [entry(OWNED, &["owner"]), entry(FACTORY, &[])]
    );
    assert!(
        calls.factory_token(OWNED).await.is_ok(),
        "a token through the group"
    );

    // A member who is no owner of the group changes nothing in it, and an owner cannot
    // lower themselves.
    calls
        .membership(
            "member",
            "builders",
            json!({"person": ADMIN, "role": "member"}),
        )
        .await;
    let (status, refused) = calls
        .membership(
            "admin",
            "builders",
            json!({"person": BUILDER, "role": "none"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
    assert_eq!(problem(&refused), "group-owner-required");
    for role in ["member", "none"] {
        let (status, refused) = calls
            .membership(
                "member",
                "builders",
                json!({"person": MEMBER, "role": role}),
            )
            .await;
        assert_eq!(status, reqwest::StatusCode::CONFLICT, "{refused}");
        assert_eq!(problem(&refused), "group-owner-kept");
    }
    let (status, refused) = calls
        .membership(
            "member",
            "nobody",
            json!({"person": BUILDER, "role": "member"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
    assert_eq!(problem(&refused), "group-not-found");
    assert_eq!(groups("admin").await.1["groups"][0]["name"], "builders");

    // Removing the membership takes the repository away again.
    let (status, _) = calls
        .membership(
            "member",
            "builders",
            json!({"person": BUILDER, "role": "none"}),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(listed(&mut lore, &token).await, [entry(FACTORY, &[])]);
    assert_eq!(
        calls.factory_token(OWNED).await,
        Err("invalid_scope".to_owned())
    );

    // An administrator's assignment with other actions is listed with them, and one of a
    // person who never signed in names no email or name, after the people who did; the
    // level an owner sets replaces those actions.
    let mut plane = hub.control_plane().await;
    let assignments = plane["lore"]["assignments"].as_array_mut().unwrap();
    for (principal, actions) in [
        (
            format!("{issuer}#native-factory"),
            json!(["admin", "migrate"]),
        ),
        ("native-user".to_owned(), json!([])),
    ] {
        assignments.push(json!({"deployment":"source","repository":OWNED,"tenant":"local","principal":principal,"actions":actions}));
    }
    hub.publish_control_plane(&plane).await;
    let (_, access) = calls.access("member", OWNED).await;
    assert_eq!(
        access["access"],
        json!([
            {"person": admin, "access": "member", "actions": []},
            {"person": builder, "access": "member", "actions": ["admin", "migrate"]},
            {"person": member, "access": "owner", "actions": ["owner"]},
            {"person": {"email": null, "name": null}, "access": "member", "actions": []},
            {"group": {"name": "builders"}, "access": "owner", "actions": ["owner"]}
        ])
    );
    let (_, access) = calls
        .share(
            "member",
            OWNED,
            json!({"person": BUILDER, "access": "owner"}),
        )
        .await;
    assert_eq!(
        access["access"][1],
        json!({"person": builder, "access": "owner", "actions": ["owner"]})
    );
    let (_, access) = calls
        .share(
            "member",
            OWNED,
            json!({"group": "builders", "access": "none"}),
        )
        .await;
    assert_eq!(access["access"].as_array().unwrap().len(), 4, "{access}");
}
