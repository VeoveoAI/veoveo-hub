//! Lore's repository listing against the hosted process: `UrcAuthApi.LookupUserPermissions`
//! as gRPC over cleartext HTTP/2 on the Hub's own listener, asked the way Lore's
//! `auth_service` catalog asks it. The caller is a configured factory client's service
//! principal holding synthetic assignments, its own and through a group; no Lore server is
//! involved.
#[path = "support/database.rs"]
mod database;
#[path = "support/hub.rs"]
mod fixture;

use std::time::Duration;

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use lore_proto::auth::{
    CheckUserPermissionRequest, LookupUserPermissionsRequest, LookupUserPermissionsResponse,
    ResourcePermission, urc_auth_api_client::UrcAuthApiClient,
};
use serde_json::{Value, json};
use tonic::{Code, Status, transport::Channel};
use veoveo_hub_sdk::registration::statement::ServerKeyPair;

const RESOURCE: &str = "https://native.example/source";
const SECRET: &str = "synthetic-native-factory-secret";
const FIRST: &str = "0123456789abcdef0123456789abcdef";
const SECOND: &str = "1123456789abcdef0123456789abcdef";
const THIRD: &str = "2123456789abcdef0123456789abcdef";
const GROUPED: &str = "3123456789abcdef0123456789abcdef";
const OTHERS: &str = "f123456789abcdef0123456789abcdef";
const GROUP: &str = "9123456789abcdef0123456789abcdef";

/// One Lore deployment whose resource IDs are Lore's own `urc-{id}`, the factory client's
/// principal assigned three repositories and another person a fourth, and a group the
/// principal belongs to assigned a fifth and, with another action, the first.
fn configure(config: &mut Value) {
    let issuer = config["authorizationServers"][0]["issuer"]
        .as_str()
        .unwrap()
        .to_owned();
    let factory = format!("{issuer}#native-factory");
    let repository = |id: &str| json!({"deployment":"source","id":id,"tenant":"local","title":id});
    let assignment = |id: &str, principal: &str, actions: Value| json!({"deployment":"source","repository":id,"tenant":"local","principal":principal,"actions":actions});
    let grouped = |id: &str, actions: Value| json!({"deployment":"source","repository":id,"tenant":"local","group":GROUP,"actions":actions});
    config["lore"] = json!({
    "deployments":[{"id":"source","resourceName":"native-source","audience":RESOURCE,"endpoint":"grpcs://10.0.0.8:41337","authorizationServer":"local","policyVersion":"local","loginScopes":["native:login","offline_access"],"scopeTemplate":"source-{id}","resourceIdTemplate":"urc-{id}","tokenLifetimeSeconds":120}],
    "repositories":[repository(FIRST), repository(SECOND), repository(THIRD), repository(GROUPED), repository(OTHERS)],
    "groups":[{"id":GROUP,"tenant":"local","name":"builders","members":[{"principal":&factory,"role":"member"},{"principal":"native-user","role":"owner"}]}],
    "assignments":[
        assignment(THIRD, &factory, json!(["owner"])),
        assignment(FIRST, &factory, json!([])),
        assignment(SECOND, &factory, json!(["admin"])),
        assignment(OTHERS, "native-user", json!(["owner"])),
        grouped(FIRST, json!(["owner"])),
        grouped(GROUPED, json!(["migrate"])),
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

/// The request Lore's catalog sends: its resource type, no context and no page size.
fn lore_request(
    filter: &str,
    page_size: Option<i32>,
    page_token: Option<String>,
) -> LookupUserPermissionsRequest {
    LookupUserPermissionsRequest {
        resource_filter: filter.to_owned(),
        context_filter: None,
        page_size,
        page_token,
    }
}

async fn lookup(
    client: &mut UrcAuthApiClient<Channel>,
    authorization: Option<&str>,
    request: LookupUserPermissionsRequest,
) -> Result<LookupUserPermissionsResponse, Status> {
    let mut request = tonic::Request::new(request);
    if let Some(authorization) = authorization {
        request
            .metadata_mut()
            .insert("authorization", authorization.parse().unwrap());
    }
    client
        .lookup_user_permissions(request)
        .await
        .map(tonic::Response::into_inner)
}

/// Every page, following `next_page_token` as Lore's catalog does.
async fn walk(
    client: &mut UrcAuthApiClient<Channel>,
    authorization: &str,
    page_size: Option<i32>,
) -> (Vec<ResourcePermission>, usize) {
    let (mut entries, mut pages, mut token) = (Vec::new(), 0, None);
    loop {
        let page = lookup(
            client,
            Some(authorization),
            lore_request("urc", page_size, token),
        )
        .await
        .unwrap();
        pages += 1;
        entries.extend(page.resource_permission);
        match page.next_page_token {
            Some(next) => token = Some(next),
            None => return (entries, pages),
        }
    }
}

fn entry(id: &str, permission: &[&str]) -> ResourcePermission {
    ResourcePermission {
        resource_id: format!("urc-{id}"),
        permission: permission.iter().map(|name| (*name).to_owned()).collect(),
    }
}

/// `token` with its claims changed and signed again by `key`, under its own `kid`.
fn resigned(token: &str, key: &EncodingKey, change: impl FnOnce(&mut Value)) -> String {
    let kid = jsonwebtoken::decode_header(token).unwrap().kid;
    let mut claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    change(&mut claims);
    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("at+jwt".to_owned());
    header.kid = kid;
    format!(
        "Bearer {}",
        jsonwebtoken::encode(&header, &claims, key).unwrap()
    )
}

#[tokio::test]
async fn lore_lists_the_callers_assigned_repositories_through_the_hub() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("the listing qualification exceeded two minutes");
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
    let http = reqwest::Client::new();
    let form = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("grant_type", "client_credentials"),
            ("client_id", "native-factory"),
            ("client_secret", SECRET),
            ("scope", &format!("source-{FIRST}")),
        ])
        .finish();
    let response = http
        .post(format!("{}/oauth/token", hub.base))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let issued: Value = response.json().await.unwrap();
    let token = issued["access_token"].as_str().unwrap().to_owned();
    let bearer = format!("Bearer {token}");
    let mut client = UrcAuthApiClient::connect(hub.base.clone()).await.unwrap();

    // The person's grants in resource-ID order, own and through the group, one entry per
    // repository with the union of its actions, not only the one repository the token
    // holds, and not another person's.
    let all = vec![
        entry(FIRST, &["owner"]),
        entry(SECOND, &["admin"]),
        entry(THIRD, &["owner"]),
        entry(GROUPED, &["migrate"]),
    ];
    let page = lookup(&mut client, Some(&bearer), lore_request("urc", None, None))
        .await
        .unwrap();
    assert_eq!(page.resource_permission, all);
    assert_eq!(page.next_page_token, None);
    assert_eq!(walk(&mut client, &bearer, Some(2)).await, (all.clone(), 2));
    assert_eq!(walk(&mut client, &bearer, Some(1)).await, (all.clone(), 4));
    let other_type = lookup(
        &mut client,
        Some(&bearer),
        lore_request("other", None, None),
    )
    .await
    .unwrap();
    assert!(other_type.resource_permission.is_empty());
    let mut context = lore_request("urc", None, None);
    context.context_filter = Some(FIRST.to_owned());
    assert_eq!(
        lookup(&mut client, Some(&bearer), context)
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );

    // Only a Lore token this Hub issued, for the deployment's audience and unexpired.
    let hub_key = EncodingKey::from_ed_der(
        &STANDARD
            .decode(hub.secret("VEOVEO_ACCESS_TOKEN_SIGNING_KEY_B64"))
            .unwrap(),
    );
    let other_key = ServerKeyPair::generate().unwrap();
    let now = Utc::now().timestamp();
    let mcp = format!(
        "Bearer {}",
        hub.access_token(&http, hub.caller("member")).await
    );
    for (case, authorization) in [
        ("no token", None),
        ("not a JWT", Some("Bearer not-a-jwt".to_owned())),
        (
            "another signer",
            Some(resigned(
                &token,
                &EncodingKey::from_ed_der(other_key.pkcs8_der()),
                |_| {},
            )),
        ),
        (
            "another audience",
            Some(resigned(&token, &hub_key, |claims| {
                claims["aud"] = json!("https://native.example/other");
            })),
        ),
        (
            "expired",
            Some(resigned(&token, &hub_key, |claims| {
                claims["iat"] = json!(now - 600);
                claims["nbf"] = json!(now - 600);
                claims["exp"] = json!(now - 120);
            })),
        ),
        ("the Hub's MCP token", Some(mcp)),
    ] {
        let refused = lookup(
            &mut client,
            authorization.as_deref(),
            lore_request("urc", None, None),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code(), Code::Unauthenticated, "{case}: {refused:?}");
    }
    // The Hub answers no other method of the auth service.
    let mut check = tonic::Request::new(CheckUserPermissionRequest {
        resource_id: vec![format!("urc-{FIRST}")],
        target_user: None,
    });
    check
        .metadata_mut()
        .insert("authorization", bearer.parse().unwrap());
    assert_eq!(
        client
            .check_user_permission(check)
            .await
            .unwrap_err()
            .code(),
        Code::Unimplemented
    );

    // The listing follows the active control plane, with the token unchanged.
    let mut plane = hub.control_plane().await;
    plane["lore"]["assignments"]
        .as_array_mut()
        .unwrap()
        .retain(|assignment| assignment["repository"] != SECOND);
    hub.publish_control_plane(&plane).await;
    let page = lookup(&mut client, Some(&bearer), lore_request("urc", None, None))
        .await
        .unwrap();
    assert_eq!(
        page.resource_permission,
        vec![
            entry(FIRST, &["owner"]),
            entry(THIRD, &["owner"]),
            entry(GROUPED, &["migrate"])
        ]
    );
    // A membership removed takes the group's repositories and actions with it.
    plane["lore"]["groups"][0]["members"]
        .as_array_mut()
        .unwrap()
        .retain(|member| member["principal"] == "native-user");
    hub.publish_control_plane(&plane).await;
    let page = lookup(&mut client, Some(&bearer), lore_request("urc", None, None))
        .await
        .unwrap();
    assert_eq!(
        page.resource_permission,
        vec![entry(FIRST, &[]), entry(THIRD, &["owner"])]
    );
}
