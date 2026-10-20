//! A person's browser session through a real OpenID provider, and what ends a durable
//! Task's delegation for that person. A user logs in at Keycloak through the Hub's
//! authorization code flow with PKCE, and a Relay Task created for the user's call
//! exchanges for calls to Ledger while it lives. The user's logout leaves that
//! delegation in place. A replayed refresh token revokes the session: the Task's
//! exchange is refused, and a Task-bound token issued before is no longer admitted.
//! Each step leaves its audit record.

#[path = "support/hub.rs"]
mod hub;
#[path = "support/keycloak.rs"]
mod keycloak;
#[path = "support/servers.rs"]
mod servers;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hub::{DISCOVER, Hub};
use keycloak::{TestKeycloak, form, random_text};
use rmcp::model::{CallToolRequestParams, CallToolResponse};
use serde_json::{Value, json};
use servers::{exchange, hosted, mcp_client, relay_call, serve};
use sha2::{Digest, Sha256};
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::{
    HubProfileId,
    calls::{TASK_TOKEN_TYPE, TokenErrorCode},
    client::CallError,
    hosting::ServerCalls,
};
use veoveo_hub_sdk_fixture::{
    calls::{Inspect, Inspection},
    ledger::{self, LedgerDomain, LedgerTasks},
    relay::{self, RelayDomain, RelayTasks},
};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_task::TaskRuntime;
use veoveo_types::{PermissionName, ServerSlug, TaskId};

const REALM: &str = "veoveo";
/// The Hub's client at Keycloak.
const OIDC_CLIENT: &str = "veoveo-hub";
/// The public OAuth client the browser logs in with.
const BROWSER: &str = "browser";
/// Where the Hub sends the browser with its authorization code. Nothing listens
/// there: the test reads the code from the redirect.
const REDIRECT: &str = "http://127.0.0.1:9/callback";
const USER: &str = "alice";
const PROFILE: &str = "local";

#[tokio::test]
async fn a_task_stops_acting_for_its_caller_only_after_a_refresh_token_replay() {
    tokio::time::timeout(Duration::from_secs(300), qualify())
        .await
        .expect("the browser session qualification exceeded five minutes");
}

/// The tokens a login delivered.
struct Session {
    access: String,
    refresh: String,
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let keycloak = TestKeycloak::start().await;
    let client_secret = random_text();
    let password = random_text();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start_with(
        directory.path(),
        &db,
        Duration::from_secs(5),
        &[
            ("VEOVEO_KEYCLOAK_CLIENT_SECRET", client_secret.clone()),
            ("VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS", "1".to_owned()),
        ],
        |config| browser_login(config, &keycloak),
    )
    .await;
    keycloak
        .create_realm(&realm(&hub, &client_secret, &password))
        .await;
    let user = format!("{}#{USER}", keycloak.issuer(REALM));
    let http = reqwest::Client::new();
    let stop = tokio_util::sync::CancellationToken::new();
    let _stop = stop.clone().drop_guard();

    let ledger_tasks = LedgerTasks::default();
    let mut ledger_declaration = ledger::authorization().unwrap();
    ledger_declaration
        .uses
        .push(PermissionName::parse("relay.calls.make").unwrap());
    let ledger = serve(
        &hub,
        &http,
        "ledger",
        ledger_declaration,
        async |registration, _, _| {
            let tasks = ledger_tasks.clone();
            hosted::<LedgerDomain>(registration, &http)
                .handler(move || ledger::hosted(tasks.clone()))
                .build()
        },
    )
    .await;
    // Relay keeps its `relay_later` Tasks in core's Task runtime, and no worker claims
    // them, so each stays queued and live while the test acts for it.
    let relay_slug = ServerSlug::parse("relay").unwrap();
    let relay_calls: Arc<Mutex<Option<ServerCalls>>> = Arc::default();
    let relay_runtime: Arc<Mutex<Option<TaskRuntime>>> = Arc::default();
    let relay = serve(
        &hub,
        &http,
        "relay",
        relay::authorization().unwrap(),
        async |registration, metadata, key| {
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            *relay_calls.lock().unwrap() = Some(calls.clone());
            let grant = registration.database().unwrap();
            let store = PlatformStore::connect(
                StoreConfig::builder(grant.url.as_str(), grant.password.clone())
                    .username(grant.username.as_str())
                    .max_connections(4)
                    .build()
                    .unwrap(),
            )
            .await
            .unwrap();
            let instance = Arc::new(Instance::start(&store, &relay_slug).await.unwrap());
            let runtime = TaskRuntime::new(
                store,
                relay_slug.clone(),
                metadata.registration.deployment,
                instance,
            );
            *relay_runtime.lock().unwrap() = Some(runtime.clone());
            let tasks = RelayTasks::new(runtime, calls.clone());
            hosted::<RelayDomain>(registration, &http)
                .calls(calls)
                .handler(move || relay::hosted_with_tasks(tasks.clone()))
                .build()
        },
    )
    .await;
    let relay_calls = relay_calls.lock().unwrap().clone().unwrap();
    let relay_runtime = relay_runtime.lock().unwrap().clone().unwrap();
    let profile = HubProfileId::parse(PROFILE).unwrap();
    let act_for = async |task: &str| {
        relay_calls
            .for_task_id(TaskId::parse(task).unwrap(), &profile)
            .call(Inspect {})
            .await
    };
    let task_bound = async |task: &str| {
        let (status, answer) =
            exchange(&hub, &http, &relay, (task, TASK_TOKEN_TYPE), None, None).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{answer}");
        answer["access_token"].as_str().unwrap().to_owned()
    };
    let admitted = async |token: &str| {
        hub.mcp(&http, Some(&format!("Bearer {token}")), DISCOVER)
            .await
            .status()
    };
    let nested_bound = async |task: &str| {
        act_for(task).await.unwrap();
        let (status, answer) = exchange(
            &hub,
            &http,
            &ledger,
            (
                &ledger.bearers.last(),
                veoveo_hub_sdk::calls::ACCESS_TOKEN_TOKEN_TYPE,
            ),
            None,
            None,
        )
        .await;
        assert_eq!(status, reqwest::StatusCode::OK, "{answer}");
        let token = answer["access_token"].as_str().unwrap().to_owned();
        let client = mcp_client(&hub, token.clone()).await;
        let seen: Inspection = serde_json::from_value(
            relay_call(&client, "relay_inspect", json!({}))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(seen.client, BROWSER);
        assert_eq!(seen.acting_server.as_deref(), Some("relay"));
        assert_eq!(seen.acting_task, None);
        assert_eq!(seen.prior_servers, ["relay", "ledger"]);
        assert_eq!(
            seen.originating_task,
            Some(("relay".to_owned(), task.to_owned()))
        );
        let forwarded = ledger.bearers.last();
        client.cancel().await.unwrap();
        (token, forwarded)
    };
    let continued = async |forwarded: &str| {
        exchange(
            &hub,
            &http,
            &ledger,
            (forwarded, veoveo_hub_sdk::calls::ACCESS_TOKEN_TOKEN_TYPE),
            None,
            None,
        )
        .await
    };

    // The user logs in, and a Task created for the user's call acts for the user.
    let mark = sequence(&db).await;
    let first = log_in(&hub, &keycloak, &password).await;
    assert_eq!(servers::decoded_claims(&first.access)["sub"], USER);
    let logged_in = records_since(&db, mark).await;
    assert!(
        logged_in.contains(&record("hub.authentication", "success", None, Some(&user))),
        "{logged_in:?}"
    );
    let first_task = relay_task(&hub, &db, &first).await;
    let mark = sequence(&db).await;
    let inspection: Inspection = act_for(&first_task).await.unwrap();
    assert_eq!(inspection.client, BROWSER);
    assert_eq!(inspection.acting_task.as_deref(), Some(first_task.as_str()));
    let exchanged = records_since(&db, mark).await;
    assert!(
        exchanged.contains(&record("hub.token_exchange", "success", None, Some(&user))),
        "{exchanged:?}"
    );
    let first_bound = task_bound(&first_task).await;
    assert_eq!(admitted(&first_bound).await, reqwest::StatusCode::OK);
    let (first_nested, first_forwarded) = nested_bound(&first_task).await;
    assert_eq!(admitted(&first_nested).await, reqwest::StatusCode::OK);

    // A logout ends the session, and the browser's access token with it, but the Task
    // keeps acting for the user.
    let mark = sequence(&db).await;
    revoke(&hub, &first.refresh).await;
    let logged_out = records_since(&db, mark).await;
    assert!(
        logged_out.contains(&record(
            "hub.authentication",
            "success",
            Some("refresh_token_revoked"),
            Some(&user)
        )),
        "{logged_out:?}"
    );
    assert_eq!(
        admitted(&first.access).await,
        reqwest::StatusCode::UNAUTHORIZED
    );
    let (status, answer) = refresh(&hub, &first.refresh).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{answer}");
    // Relay's client reuses the token it holds; a fresh exchange shows the delegation
    // itself survived.
    let mark = sequence(&db).await;
    let after_logout = task_bound(&first_task).await;
    let exchanged = records_since(&db, mark).await;
    assert!(
        exchanged.contains(&record("hub.token_exchange", "success", None, Some(&user))),
        "{exchanged:?}"
    );
    assert_eq!(admitted(&after_logout).await, reqwest::StatusCode::OK);
    assert_eq!(admitted(&first_bound).await, reqwest::StatusCode::OK);
    let inspection: Inspection = act_for(&first_task).await.unwrap();
    assert_eq!(inspection.client, BROWSER);
    // Ordinary logout preserves all inherited Task authority, including a later
    // request-origin exchange and an already issued nested token.
    assert_eq!(admitted(&first_nested).await, reqwest::StatusCode::OK);
    assert_eq!(continued(&first_forwarded).await.0, reqwest::StatusCode::OK);

    // A second login rotates its refresh token. Presenting the consumed token again
    // after the delivery window revokes the session as a replay.
    let second = log_in(&hub, &keycloak, &password).await;
    let second_task = relay_task(&hub, &db, &second).await;
    act_for(&second_task).await.unwrap();
    let second_bound = task_bound(&second_task).await;
    assert_eq!(admitted(&second_bound).await, reqwest::StatusCode::OK);
    let (second_nested, second_forwarded) = nested_bound(&second_task).await;
    let (status, rotated) = refresh(&hub, &second.refresh).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{rotated}");
    let successor = rotated["refresh_token"].as_str().unwrap().to_owned();
    assert_ne!(successor, second.refresh);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mark = sequence(&db).await;
    let (status, replayed) = refresh(&hub, &second.refresh).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{replayed}");
    assert_eq!(replayed["error"], "invalid_grant", "{replayed}");
    let replay = records_since(&db, mark).await;
    assert!(
        replay.contains(&record(
            "hub.authentication",
            "denied",
            Some("refresh_token_replay"),
            Some(&user)
        )),
        "{replay:?}"
    );
    let (status, answer) = refresh(&hub, &successor).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{answer}");

    // The replay ends the second Task's delegation: its exchange is refused, and the
    // token it was issued before is no longer admitted.
    let mark = sequence(&db).await;
    match act_for(&second_task).await {
        Err(CallError::Refused {
            error: TokenErrorCode::InvalidGrant,
            description,
        }) => assert!(
            description
                .as_deref()
                .unwrap_or_default()
                .contains("replayed"),
            "{description:?}"
        ),
        other => panic!("a replayed session still delegates: {other:?}"),
    }
    let (status, refused) = exchange(
        &hub,
        &http,
        &relay,
        (&second_task, TASK_TOKEN_TYPE),
        None,
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(refused["error"], "invalid_grant", "{refused}");
    assert_eq!(
        admitted(&second_bound).await,
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        admitted(&second_nested).await,
        reqwest::StatusCode::UNAUTHORIZED
    );
    let (status, replayed_nested) = continued(&second_forwarded).await;
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "{replayed_nested}"
    );
    assert_eq!(replayed_nested["error"], "invalid_grant");
    let ended = records_since(&db, mark).await;
    for expected in [
        record("hub.token_exchange", "denied", Some("invalid_grant"), None),
        record(
            "hub.authentication",
            "denied",
            Some("invalid_bearer_token"),
            None,
        ),
    ] {
        assert!(ended.contains(&expected), "{expected:?} in {ended:?}");
    }

    // The other session's logout ended no delegation: the first Task still exchanges
    // and acts for the user.
    assert_eq!(
        admitted(&task_bound(&first_task).await).await,
        reqwest::StatusCode::OK
    );
    act_for(&first_task).await.unwrap();

    // Native Task cancellation independently ends the nested grant at admission
    // and at a subsequent exchange; it cannot rename Ledger as the Task owner.
    relay_runtime
        .cancel(TaskId::parse(&first_task).unwrap())
        .await
        .unwrap();
    assert_eq!(
        admitted(&first_nested).await,
        reqwest::StatusCode::UNAUTHORIZED
    );
    let (status, ended_nested) = continued(&first_forwarded).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{ended_nested}");
    assert_eq!(ended_nested["error"], "invalid_grant");

    drop(hub);
}

/// The Hub's local installation with Keycloak as its identity provider: the
/// profile accepts the authorization code flow beside client credentials, the browser
/// client logs in with PKCE, and the user holds the member role in the local Work
/// Context.
fn browser_login(config: &mut Value, keycloak: &TestKeycloak) {
    let base = config["authorizationServers"][0]["tokenEndpoint"]
        .as_str()
        .unwrap()
        .trim_end_matches("/oauth/token")
        .to_owned();
    let resource = config["profiles"][0]["protectedResource"].clone();
    let issuer = keycloak.issuer(REALM);
    let push = |config: &mut Value, field: &str, value: Value| {
        config[field].as_array_mut().unwrap().push(value);
    };
    push(
        config,
        "identityProviders",
        json!({
            "id": "keycloak",
            "issuer": issuer,
            "jwks": { "source": "remote", "jwksUri": format!("{issuer}/protocol/openid-connect/certs") },
            "claimMapping": { "subject": "preferred_username", "tenant": { "claim": "tenant" } },
            "trustedCertificateAuthorities": [
                { "source": "file", "path": keycloak.certificate_authority() }
            ],
            "authorizationEndpoint": format!("{issuer}/protocol/openid-connect/auth"),
            "tokenEndpoint": format!("{issuer}/protocol/openid-connect/token"),
            "metadata": {}
        }),
    );
    config["authorizationServers"][0]["identityProvider"] = json!("keycloak");
    config["authorizationServers"][0]["authorizationEndpoint"] =
        json!(format!("{base}/oauth/authorize"));
    config["profiles"][0]["identityProvider"] = json!("keycloak");
    config["profiles"][0]["authModes"] =
        json!(["oauth_client_credentials", "oidc_authorization_code_pkce"]);
    push(
        config,
        "oauthClients",
        json!({
            "id": BROWSER,
            "authorizationServer": "local",
            "defaultWorkContext": "local",
            "invocationMode": "direct",
            "displayName": "Browser",
            "allowedResources": [resource],
            "grantTypes": ["authorization_code_pkce", "refresh_token"],
            "authMethods": ["none"],
            "redirectUris": [REDIRECT],
            "allowedScopes": ["mcp"],
            "metadata": {}
        }),
    );
    push(
        config,
        "oidcClients",
        json!({
            "id": "keycloak",
            "identityProvider": "keycloak",
            "authorizationServer": "local",
            "allowedResources": [resource],
            "clientId": OIDC_CLIENT,
            "redirectUri": format!("{base}/oauth/callback"),
            "authMethod": "client_secret_post",
            "credentialSecret": "keycloak_client_secret",
            "scopes": ["openid"],
            "metadata": {}
        }),
    );
    push(
        config,
        "secrets",
        json!({
            "id": "keycloak_client_secret",
            "source": "env",
            "purpose": "oauth_client_secret",
            "locator": "VEOVEO_KEYCLOAK_CLIENT_SECRET",
            "owner": { "kind": "hub" },
            "metadata": {}
        }),
    );
    let environment = config["execution"]["defaultEnvironment"]
        .as_str()
        .unwrap()
        .to_owned();
    config["execution"]["environments"][&environment]["readers"]
        .as_array_mut()
        .unwrap()
        .push(json!(format!("{issuer}#{USER}")));
    config["authorization"]["grants"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "role": "member", "subject": { "principal": format!("{issuer}#{USER}") } }));
    config["workContexts"][0]["memberships"][0]["oauthClients"]
        .as_array_mut()
        .unwrap()
        .push(json!(BROWSER));
}

/// The test realm: the Hub's confidential client, which must use PKCE and whose
/// ID tokens name the local tenant, and one user.
fn realm(hub: &Hub, client_secret: &str, password: &str) -> Value {
    json!({
        "realm": REALM,
        "enabled": true,
        "clients": [{
            "clientId": OIDC_CLIENT,
            "protocol": "openid-connect",
            "enabled": true,
            "publicClient": false,
            "clientAuthenticatorType": "client-secret",
            "secret": client_secret,
            "standardFlowEnabled": true,
            "directAccessGrantsEnabled": false,
            "redirectUris": [format!("{}/oauth/callback", hub.base)],
            "webOrigins": [],
            "attributes": { "pkce.code.challenge.method": "S256" },
            "protocolMappers": [{
                "name": "tenant",
                "protocol": "openid-connect",
                "protocolMapper": "oidc-hardcoded-claim-mapper",
                "config": {
                    "claim.name": "tenant",
                    "claim.value": "local",
                    "jsonType.label": "String",
                    "id.token.claim": "true",
                    "access.token.claim": "false",
                    "userinfo.token.claim": "false"
                }
            }]
        }],
        "users": [{
            "username": USER,
            "enabled": true,
            "email": "alice@example.test",
            "emailVerified": true,
            "firstName": "Alice",
            "lastName": "Example",
            "credentials": [{ "type": "password", "value": password, "temporary": false }]
        }]
    })
}

/// Log the user in as a browser does: the Hub's authorization endpoint sends it to
/// Keycloak's login form, Keycloak back to the Hub's callback, and the Hub to
/// the client's redirect URI with a code the client exchanges with its PKCE verifier.
async fn log_in(hub: &Hub, keycloak: &TestKeycloak, password: &str) -> Session {
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let verifier = random_text();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_text();
    let mut authorize = url::Url::parse(&format!("{}/oauth/authorize", hub.base)).unwrap();
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", BROWSER)
        .append_pair("redirect_uri", REDIRECT)
        .append_pair("scope", "mcp")
        .append_pair("resource", &hub.installation.resource)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    let to_keycloak = location(browser.get(authorize).send().await.unwrap());
    assert!(
        to_keycloak.starts_with(&format!(
            "{}/protocol/openid-connect/auth",
            keycloak.issuer(REALM)
        )),
        "{to_keycloak}"
    );
    let page = keycloak.http().get(&to_keycloak).send().await.unwrap();
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let cookies = page
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok()?.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    let action = login_action(&page.text().await.unwrap());
    let to_hub = location(
        form(
            keycloak
                .http()
                .post(action)
                .header(reqwest::header::COOKIE, cookies),
            &[
                ("username", USER),
                ("password", password),
                ("credentialId", ""),
            ],
        )
        .send()
        .await
        .unwrap(),
    );
    assert!(
        to_hub.starts_with(&format!("{}/oauth/callback?", hub.base)),
        "{to_hub}"
    );
    let to_client = url::Url::parse(&location(browser.get(&to_hub).send().await.unwrap())).unwrap();
    assert_eq!(
        to_client[..url::Position::AfterPath],
        *REDIRECT,
        "{to_client}"
    );
    let query = |name: &str| {
        to_client
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .unwrap_or_else(|| panic!("the redirect carries {name}: {to_client}"))
    };
    assert_eq!(query("state"), state);
    let (status, tokens) = token(
        hub,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", BROWSER),
            ("code", &query("code")),
            ("redirect_uri", REDIRECT),
            ("code_verifier", &verifier),
        ],
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{tokens}");
    Session {
        access: tokens["access_token"].as_str().unwrap().to_owned(),
        refresh: tokens["refresh_token"].as_str().unwrap().to_owned(),
    }
}

/// The `Location` of a redirect.
fn location(response: reqwest::Response) -> String {
    assert!(
        response.status().is_redirection(),
        "expected a redirect, got {}",
        response.status()
    );
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

/// The action URL of Keycloak's login form.
fn login_action(page: &str) -> String {
    let form = page
        .find("id=\"kc-form-login\"")
        .and_then(|at| page[..at].rfind("<form"))
        .unwrap_or_else(|| panic!("Keycloak served no login form:\n{page}"));
    let tag = &page[form..form + page[form..].find('>').unwrap()];
    let action = tag
        .split_once("action=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(action, _)| action)
        .expect("the login form names its action");
    action.replace("&amp;", "&")
}

/// Send a form to the Hub's token endpoint.
async fn token(hub: &Hub, pairs: &[(&str, &str)]) -> (reqwest::StatusCode, Value) {
    let response = form(reqwest::Client::new().post(hub.token_endpoint()), pairs)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
}

async fn refresh(hub: &Hub, refresh_token: &str) -> (reqwest::StatusCode, Value) {
    token(
        hub,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", BROWSER),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}

/// Log out: revoke the session through its refresh token.
async fn revoke(hub: &Hub, refresh_token: &str) {
    let response = form(
        reqwest::Client::new().post(format!("{}/oauth/revoke", hub.base)),
        &[
            ("token", refresh_token),
            ("token_type_hint", "refresh_token"),
            ("client_id", BROWSER),
        ],
    )
    .send()
    .await
    .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

/// Start `relay_later` for the session's user and return Relay's identifier of the
/// Task, after checking that its route keeps the user's session.
async fn relay_task(hub: &Hub, db: &TestDb, session: &Session) -> String {
    let client = mcp_client(hub, session.access.clone()).await;
    let created = match client
        .call_tool_once(
            CallToolRequestParams::new("relay__relay_later").with_arguments(Default::default()),
        )
        .await
        .unwrap()
    {
        CallToolResponse::Task(created) => created.task.task_id,
        other => panic!("relay_later did not start a Task: {other:?}"),
    };
    client.cancel().await.unwrap();
    let (source, family): (String, Option<String>) = sqlx::query_as(
        "SELECT source_task_id, delegated_caller #>> '{request_context,access_token,session_family}' \
         FROM hub.task_route WHERE id = $1",
    )
    .bind(&created)
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(
        family.as_deref(),
        servers::decoded_claims(&session.access)["https://veoveo.ai/claims/session-family"]
            .as_str(),
        "the route keeps the caller's session"
    );
    source
}

/// One audit record: its class, outcome, reason and actor.
type Record = (String, String, Option<String>, Option<String>);

fn record(class: &str, outcome: &str, reason: Option<&str>, actor: Option<&str>) -> Record {
    (
        class.to_owned(),
        outcome.to_owned(),
        reason.map(str::to_owned),
        actor.map(str::to_owned),
    )
}

/// The latest audit record's sequence number.
async fn sequence(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT coalesce(max(sequence), 0) FROM audit.record")
        .fetch_one(db.a.pool())
        .await
        .unwrap()
}

/// The audit records written after `mark`, in order.
async fn records_since(db: &TestDb, mark: i64) -> Vec<Record> {
    sqlx::query_as(
        "SELECT class, outcome, reason, actor FROM audit.record WHERE sequence > $1 ORDER BY sequence",
    )
    .bind(mark)
    .fetch_all(db.a.pool())
    .await
    .unwrap()
}
