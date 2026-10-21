//! Native wire qualification against the actual hosted process. Approval below is
//! a trusted fixture identity, not an Entra browser/login acceptance claim.
#[path = "support/database.rs"]
mod database;
#[path = "support/hub.rs"]
mod fixture;
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, time::Duration};
use veoveo_hub::{contract::PrincipalDisplayName, store::HubStore};
use veoveo_hub_sdk::{Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{PrincipalId, Sha256Digest, TenantId};
const RESOURCE: &str = "https://native.example/source";
const REPO: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "ffffffffffffffffffffffffffffffff";
const SECRET: &str = "synthetic-native-factory-secret";
const EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const ACCESS: &str = "urn:ietf:params:oauth:token-type:access_token";
fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::parse(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(value.as_bytes()))
    ))
    .unwrap()
}
fn configure(config: &mut Value) {
    let issuer = config["authorizationServers"][0]["issuer"]
        .as_str()
        .unwrap()
        .to_owned();
    // Installation role membership is not Hub administrative action authority.
    // Grant only this fixture's existing admin principal the two publication actions.
    config["policies"][0]["rules"] = json!([{
        "id":"native-fixture-publication", "effect":"allow",
        "actions":["admin_read","admin_write"], "profiles":["local"],
        "principalIds":[format!("{issuer}#acceptance-admin")], "tenantIds":["local"]
    }]);
    config["identityProviders"][0]["authorizationEndpoint"] =
        json!("https://identity.example/authorize");
    config["identityProviders"][0]["tokenEndpoint"] = json!("https://identity.example/token");
    config["lore"] = json!({"deployments":[{"id":"source","resourceName":"native-source","audience":RESOURCE,"endpoint":"grpcs://10.0.0.8:41337","authorizationServer":"local","policyVersion":"local","loginScopes":["native:login","offline_access"],"scopeTemplate":"source-{id}","resourceIdTemplate":"{id}","tokenLifetimeSeconds":120}],
        "repositories":[{"deployment":"source","id":REPO,"tenant":"local","title":"Assigned"},{"deployment":"source","id":OTHER,"tenant":"local","title":"Unassigned"}],
        "assignments":[{"deployment":"source","repository":REPO,"tenant":"local","principal":"native-user","actions":[]},{"deployment":"source","repository":REPO,"tenant":"local","principal":format!("{issuer}#native-factory"),"actions":[]}]});
    let clients = config["oauthClients"].as_array_mut().unwrap();
    clients.push(json!({"id":"native-cli","authorizationServer":"local","defaultWorkContext":"local","invocationMode":"direct","allowedResources":[RESOURCE],"grantTypes":["device_authorization","refresh_token","token_exchange"],"authMethods":["none"],"allowedScopes":["native:login","offline_access"]}));
    clients.push(json!({"id":"native-factory","authorizationServer":"local","defaultWorkContext":"local","invocationMode":"automated","allowedResources":[RESOURCE],"grantTypes":["client_credentials"],"authMethods":["client_secret_post"],"allowedScopes":["native:login","offline_access"],"credentialSecret":"native-client-secret","tenant":"local"}));
    config["secrets"].as_array_mut().unwrap().push(json!({"id":"native-client-secret","source":"env","purpose":"oauth_client_secret","locator":"VEOVEO_NATIVE_FIXTURE_SECRET","owner":{"kind":"hub"}}));
    config["oidcClients"] = json!([{"id":"native-broker","identityProvider":"local","authorizationServer":"local","allowedResources":[RESOURCE],"clientId":"synthetic-broker","redirectUri":"https://hub.example/oauth/callback","authMethod":"client_secret_post","credentialSecret":"native-client-secret","scopes":["openid"]}]);
}
async fn form(
    http: &reqwest::Client,
    url: &str,
    pairs: &[(&str, &str)],
) -> (reqwest::StatusCode, Value) {
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.extend_pairs(pairs.iter().copied());
        form.finish()
    };
    let response = http
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    (status, response.json().await.unwrap())
}
fn verified(token: &str, keys: &JwkSet, issuer: &str) -> Value {
    let header = decode_header(token).unwrap();
    assert_eq!(header.typ.as_deref(), Some("at+jwt"));
    let key = DecodingKey::from_jwk(keys.find(header.kid.as_deref().unwrap()).unwrap()).unwrap();
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[RESOURCE]);
    decode::<Value>(token, &key, &validation).unwrap().claims
}
#[tokio::test]
async fn native_device_exchange_refresh_and_live_grants_use_one_issuer() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("native issuer qualification exceeded three minutes");
}
async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = database::TestDb::new().await;
    let mut hub = fixture::Hub::start_with(
        directory.path(),
        &db,
        Duration::from_secs(60),
        &[("VEOVEO_NATIVE_FIXTURE_SECRET", SECRET.to_owned())],
        configure,
    )
    .await;
    let http = reqwest::Client::new();
    let token_url = format!("{}/oauth/token", hub.base);
    let metadata: Value = http
        .get(format!(
            "{}/oauth/.well-known/openid-configuration",
            hub.base
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        metadata["device_authorization_endpoint"],
        format!("{}/oauth/device_authorization", hub.base)
    );
    let keys: JwkSet = http
        .get(metadata["jwks_uri"].as_str().unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let (status, device) = form(
        &http,
        metadata["device_authorization_endpoint"].as_str().unwrap(),
        &[
            ("client_id", "native-cli"),
            ("scope", "native:login offline_access"),
        ],
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let manual = http
        .get(device["verification_uri"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(
        manual.headers()["content-security-policy"],
        "default-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    );
    let manual = manual.text().await.unwrap();
    assert!(manual.contains("<form method=get>"));
    assert!(!manual.contains("name=browser"));
    let approval = http
        .get(device["verification_uri_complete"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(
        approval.headers()["content-security-policy"],
        "default-src 'none'; form-action 'self' https://identity.example; frame-ancestors 'none'; base-uri 'none'"
    );
    assert!(
        approval
            .text()
            .await
            .unwrap()
            .contains("<form method=post>")
    );
    let device_code = device["device_code"].as_str().unwrap();
    let device_form = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", "native-cli"),
        ("device_code", device_code),
    ];
    let (_, early) = form(&http, &token_url, &device_form).await;
    assert_eq!(early["error"], "slow_down");
    let store = HubStore::new(db.a.clone());
    let user_code = device["user_code"].as_str().unwrap().replace('-', "");
    let pending = store
        .device_authorization(&digest(&user_code), Utc::now())
        .await
        .unwrap()
        .unwrap();
    let browser = digest("trusted-fixture-browser");
    store
        .begin_device_authorization(pending.id, &digest(&user_code), &browser, Utc::now())
        .await
        .unwrap()
        .unwrap();
    let principal = Principal {
        id: PrincipalId::parse("native-user").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://identity.example").unwrap(),
        subject: TokenSubject::parse("upstream-user").unwrap(),
        tenant: Some(TenantId::parse("local").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    };
    assert!(
        store
            .approve_device_authorization(
                pending.id,
                &browser,
                &principal,
                &PrincipalDisplayName::new("Native fixture").unwrap(),
                Utc::now()
            )
            .await
            .unwrap()
    );
    // Earlier polling increases the RFC8628 interval to ten seconds.
    tokio::time::sleep(Duration::from_secs(11)).await;
    let (status, login) = form(&http, &token_url, &device_form).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let raw = login["access_token"].as_str().unwrap();
    let login_claims = verified(raw, &keys, metadata["issuer"].as_str().unwrap());
    let purpose = "https://veoveo.ai/claims/token-purpose";
    assert_eq!(login_claims[purpose], "login");
    assert_eq!(login_claims["resources"], json!([]));
    assert_eq!(login_claims["identity"], "native-user");
    assert_eq!(login_claims["root_domains"], json!(["10.0.0.8"]));
    assert!(login["expires_in"].as_u64().unwrap() <= 120);
    let (_, consumed) = form(&http, &token_url, &device_form).await;
    assert_eq!(consumed["error"], "invalid_grant");
    let target = format!("source-{REPO}");
    let exchange = |subject: &str, scope: &str| {
        vec![
            ("grant_type", EXCHANGE.to_owned()),
            ("client_id", "native-cli".to_owned()),
            ("subject_token", subject.to_owned()),
            ("subject_token_type", ACCESS.to_owned()),
            ("requested_token_type", ACCESS.to_owned()),
            ("scope", scope.to_owned()),
        ]
    };
    let exchange_request = exchange(raw, &target);
    let pairs = exchange_request
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    let (status, granted) = form(&http, &token_url, &pairs).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(granted["issued_token_type"], ACCESS);
    let resource_raw = granted["access_token"].as_str().unwrap();
    let claims = verified(resource_raw, &keys, metadata["issuer"].as_str().unwrap());
    assert_eq!(claims["sub"], login_claims["sub"]);
    assert_eq!(claims[purpose], "resource");
    assert_eq!(
        claims["resources"],
        json!([{"resource_id":REPO,"permission":[]}])
    );
    let request = exchange(resource_raw, &format!("source-{OTHER}"));
    let pairs = request
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    let (_, refused) = form(&http, &token_url, &pairs).await;
    assert_eq!(refused["error"], "invalid_grant");
    let request = exchange(raw, &format!("source-{OTHER}"));
    let pairs = request
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    let (_, refused) = form(&http, &token_url, &pairs).await;
    assert_eq!(refused["error"], "invalid_scope");
    let (_, refreshed) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "native-cli"),
            ("refresh_token", login["refresh_token"].as_str().unwrap()),
        ],
    )
    .await;
    let refreshed_claims = verified(
        refreshed["access_token"].as_str().unwrap(),
        &keys,
        metadata["issuer"].as_str().unwrap(),
    );
    assert_eq!(refreshed_claims["sub"], claims["sub"]);
    assert_eq!(refreshed_claims["identity"], claims["identity"]);
    assert_eq!(refreshed_claims[purpose], "login");

    // A live client-resource addition must not change the original refresh grant.
    // This is the native custody request shape: no resource or scope parameter.
    let original = hub.control_plane().await;
    let mut multiple = original.clone();
    let other_resource = "https://native.example/other-source";
    let mut other = multiple["lore"]["deployments"][0].clone();
    other["id"] = json!("other-source");
    other["resourceName"] = json!("other-native-source");
    other["audience"] = json!(other_resource);
    multiple["lore"]["deployments"]
        .as_array_mut()
        .unwrap()
        .push(other);
    let client = multiple["oauthClients"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|client| client["id"] == "native-cli")
        .unwrap();
    client["allowedResources"] = json!([RESOURCE, other_resource]);
    hub.publish_control_plane(&multiple).await;
    let predecessor = refreshed["refresh_token"].as_str().unwrap();
    let (_, wrong_resource) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "native-cli"),
            ("refresh_token", predecessor),
            ("resource", other_resource),
        ],
    )
    .await;
    assert_eq!(wrong_resource["error"], "invalid_grant");
    let (status, bound) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "native-cli"),
            ("refresh_token", predecessor),
        ],
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{bound}");
    let bound_claims = verified(
        bound["access_token"].as_str().unwrap(),
        &keys,
        metadata["issuer"].as_str().unwrap(),
    );
    assert_eq!(bound_claims["sub"], claims["sub"]);
    assert_eq!(bound_claims["identity"], claims["identity"]);
    assert_eq!(bound_claims[purpose], "login");
    // A consumed predecessor still reaches the existing duplicate-delivery path.
    // Whitespace retains the resolver's existing empty-means-omitted convention.
    let (status, duplicate) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "native-cli"),
            ("refresh_token", predecessor),
            ("resource", "  "),
        ],
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{duplicate}");
    assert_eq!(duplicate["refresh_token"], bound["refresh_token"]);
    let client = multiple["oauthClients"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|client| client["id"] == "native-cli")
        .unwrap();
    client["allowedResources"] = json!([other_resource]);
    hub.publish_control_plane(&multiple).await;
    let (_, withdrawn) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "native-cli"),
            ("refresh_token", bound["refresh_token"].as_str().unwrap()),
        ],
    )
    .await;
    assert_eq!(withdrawn["error"], "invalid_target");
    hub.publish_control_plane(&original).await;
    let (status, factory) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "client_credentials"),
            ("client_id", "native-factory"),
            ("client_secret", SECRET),
            ("scope", &target),
        ],
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let factory_claims = verified(
        factory["access_token"].as_str().unwrap(),
        &keys,
        metadata["issuer"].as_str().unwrap(),
    );
    assert_eq!(factory_claims["resources"], claims["resources"]);
    assert_eq!(factory_claims[purpose], "resource");
    assert!(factory_claims.get("is_service_account").is_none());
    let (_, refused) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "client_credentials"),
            ("client_id", "native-factory"),
            ("client_secret", "wrong"),
            ("scope", &target),
        ],
    )
    .await;
    assert_eq!(refused["error"], "invalid_client");
    let admin = hub.access_token(&http, hub.caller("admin")).await;
    let mut replacement: Value =
        serde_json::from_slice(&std::fs::read(&hub.installation.config).unwrap()).unwrap();
    replacement["lore"]["assignments"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["principal"] != "native-user");
    let response = http
        .put(format!("{}/admin/local/control-plane", hub.base))
        .bearer_auth(&admin)
        .json(&replacement)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    // The same running process immediately uses the newly published assignments.
    let (_, refused) = form(
        &http,
        &token_url,
        &exchange_request
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>(),
    )
    .await;
    assert_eq!(refused["error"], "invalid_scope");
    let (_, refused_factory) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "client_credentials"),
            ("client_id", "native-factory"),
            ("client_secret", SECRET),
            ("scope", &format!("source-{OTHER}")),
        ],
    )
    .await;
    assert_eq!(
        refused_factory["error"], "invalid_scope",
        "missing assignment is authorization denial, not signer failure"
    );
    // Withdraw the factory's existing assignment too, using the normal live API.
    replacement["lore"]["assignments"] = json!([]);
    let mut left = replacement.clone();
    left["metadata"]["publisher"] = json!("left");
    let mut right = replacement;
    right["metadata"]["publisher"] = json!("right");
    let endpoint = format!("{}/admin/local/control-plane", hub.base);
    let (one, two) = tokio::join!(
        http.put(&endpoint).bearer_auth(&admin).json(&left).send(),
        http.put(&endpoint).bearer_auth(&admin).json(&right).send()
    );
    assert_eq!(one.unwrap().status(), reqwest::StatusCode::OK);
    assert_eq!(two.unwrap().status(), reqwest::StatusCode::OK);
    let observed: Value = http
        .get(&endpoint)
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let active = store.active_control_revision().await.unwrap().unwrap();
    assert_eq!(
        observed["revisionId"],
        active.revision_id.to_string(),
        "ordered publications must leave process on the authoritative revision"
    );
    let (_, denied_factory) = form(
        &http,
        &token_url,
        &[
            ("grant_type", "client_credentials"),
            ("client_id", "native-factory"),
            ("client_secret", SECRET),
            ("scope", &target),
        ],
    )
    .await;
    assert_eq!(
        denied_factory["error"], "invalid_scope",
        "withdrawal must deny the next factory issuance"
    );
    // Leave the old seed on disk. Restart must load the published DB revision.
    hub.restart().await;
    let (_, refused) = form(
        &http,
        &token_url,
        &exchange_request
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>(),
    )
    .await;
    assert_eq!(refused["error"], "invalid_scope");
    println!(
        "native synthetic fixture: device redemption, signed login/resource distinction, refresh, factory credentials, live grant withdrawal and DB-authoritative restart checked; no real IdP or native Lore server used"
    );
}
