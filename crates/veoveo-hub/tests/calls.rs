//! Calls between servers through the Hub binary: the SDK's Relay fixture calls its
//! Ledger fixture for Relay's caller with an RFC 8693 exchanged token, inline, through a
//! Ledger Task and from a durable Relay Task after the call that created it returned.
//! The Hub narrows each call to the uses Relay declared and the caller holds, names
//! a missing provider, preserves nested request actors and refuses another server's
//! subject and an ended originating Task.

#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use hub::{DISCOVER, Hub};
use rmcp::{
    RoleClient,
    model::{CallToolRequestParams, CallToolResponse, ClientConfig, GetTaskParams, TaskPayload},
    service::RunningService,
};
use serde_json::json;
use servers::{decoded_claims, exchange, hosted, mcp_client, relay_call, serve};
use tokio_util::sync::CancellationToken;
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::{
    ForwardedRequest, HubInternalSigningKey, HubInternalTokenIssuer, HubInternalTokenVerifier,
    HubInternalTrustBundle, HubProfileId, TokenAudience, TokenIssuer,
    calls::{TokenErrorCode, ToolCall},
    client::CallError,
    hosting::ServerCalls,
};
use veoveo_hub_sdk_fixture::{
    calls::{EntryState, FollowedEntry, Inspect, Inspection},
    gateway::{self, Gateway},
    ledger::{self, LedgerDomain, LedgerTasks},
    relay::{self, RelayDomain, RelayTasks},
};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_task::TaskRuntime;
use veoveo_types::{PermissionName, ServerSlug, TaskId};

const PROFILE: &str = "local";

fn forwarded_issuer(hub: &Hub) -> (HubInternalTokenIssuer, HubInternalTrustBundle) {
    use base64::Engine as _;
    let key = HubInternalSigningKey::new(
        base64::engine::general_purpose::STANDARD
            .decode(hub.secret("VEOVEO_FORWARDED_TOKEN_KEY_B64"))
            .unwrap(),
    )
    .unwrap();
    let trust = HubInternalTrustBundle::from_key_set(
        &veoveo_hub_sdk::registration::Ed25519JwkSet::new(vec![key.verification_key()]).unwrap(),
    )
    .unwrap();
    (
        HubInternalTokenIssuer::new(TokenIssuer::parse(&hub.base).unwrap(), key).unwrap(),
        trust,
    )
}

#[tokio::test]
async fn servers_call_each_other_for_their_caller_through_the_hub() {
    tokio::time::timeout(Duration::from_secs(240), qualify())
        .await
        .expect("calls between servers exceeded four minutes");
}

async fn caller(
    hub: &Hub,
    http: &reqwest::Client,
    role: &str,
) -> RunningService<RoleClient, ClientConfig> {
    mcp_client(hub, hub.access_token(http, hub.caller(role)).await).await
}

// A hosted fixture must hold the same login-scoped liveness as a real MCP process.
// Otherwise orphan collection legitimately removes its registration during auth tests.
async fn native_process(
    registration: &veoveo_hub_sdk::registration::client::Registration,
    server: &str,
) -> (PlatformStore, Instance) {
    let grant = registration.database().unwrap();
    let store = PlatformStore::connect(
        StoreConfig::builder(grant.url.as_str(), grant.password.clone())
            .username(grant.username.as_str())
            .max_connections(2)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    let instance = Instance::start(&store, &ServerSlug::parse(server).unwrap())
        .await
        .unwrap();
    (store, instance)
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start(directory.path(), &db, Duration::from_secs(5)).await;
    let http = reqwest::Client::new();
    let stop = CancellationToken::new();
    let _stop = stop.clone().drop_guard();

    // Ledger declares a use of Relay's permission too, so it can try to pass on a
    // delegated identity it received.
    let mut ledger_declaration = ledger::authorization().unwrap();
    ledger_declaration
        .uses
        .push(PermissionName::parse("relay.calls.make").unwrap());
    let ledger_tasks = LedgerTasks::default();
    let mut ledger_process = None;
    let ledger = serve(
        &hub,
        &http,
        "ledger",
        ledger_declaration,
        async |registration, _, _| {
            ledger_process = Some(native_process(registration, "ledger").await);
            let tasks = ledger_tasks.clone();
            hosted::<LedgerDomain>(registration, &http)
                .handler(move || ledger::hosted(tasks.clone()))
                .build()
        },
    )
    .await;

    // Relay runs `relay_later` as a durable Task in core's Task runtime, on the login its
    // registration grants.
    let relay_slug = ServerSlug::parse("relay").unwrap();
    let relay_calls: Arc<Mutex<Option<ServerCalls>>> = Arc::default();
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
            let tasks = RelayTasks::new(
                TaskRuntime::new(
                    store,
                    relay_slug.clone(),
                    metadata.registration.deployment,
                    instance,
                ),
                calls.clone(),
            );
            tokio::spawn(tasks.clone().work(stop.child_token()));
            hosted::<RelayDomain>(registration, &http)
                .calls(calls)
                .handler(move || relay::hosted_with_tasks(tasks.clone()))
                .build()
        },
    )
    .await;
    let relay_calls = relay_calls.lock().unwrap().clone().unwrap();
    let mut gateway_process = None;
    let gateway = serve(
        &hub,
        &http,
        "gateway",
        gateway::authorization(),
        async |registration, metadata, key| {
            gateway_process = Some(native_process(registration, "gateway").await);
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            hosted::<Gateway>(registration, &http)
                .calls(calls)
                .handler(gateway::hosted)
                .build()
        },
    )
    .await;
    let member = caller(&hub, &http, "member").await;
    let admin = caller(&hub, &http, "admin").await;

    // Inline: Ledger sees the member as the caller and Relay as the acting server, with
    // only the Ledger permissions the member holds.
    let inspection: Inspection = serde_json::from_value(
        relay_call(&member, "relay_inspect", json!({}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(inspection.client, "acceptance-member");
    assert_eq!(inspection.acting_server.as_deref(), Some("relay"));
    assert_eq!(inspection.acting_task, None);
    assert_eq!(
        inspection.permissions,
        ["ledger.entries.read", "ledger.entries.write"]
    );
    let delegated = decoded_claims(&ledger.bearers.last());
    assert_eq!(delegated["act"]["sub"], hub.base);
    assert_eq!(
        delegated["act"]["act"]["sub"],
        relay.registration.client_id().as_str()
    );
    assert_eq!(
        delegated["https://veoveo.ai/claims/invocation"]["acting_server"],
        "relay"
    );

    // Three independent servers use their native typed SDK clients. The final
    // server receives the original caller/context and both actors, not a new identity.
    let answer = member
        .call_tool(
            CallToolRequestParams::new("gateway__gateway_inspect")
                .with_arguments(Default::default()),
        )
        .await
        .unwrap();
    assert_ne!(answer.is_error, Some(true), "{answer:?}");
    let seen: Inspection = serde_json::from_value(answer.structured_content.unwrap()).unwrap();
    assert_eq!(seen.client, "acceptance-member");
    assert_eq!(seen.acting_server.as_deref(), Some("relay"));
    assert_eq!(seen.prior_servers, ["gateway"]);
    assert_eq!(seen.originating_task, None);
    assert_eq!(
        seen.permissions,
        ["ledger.entries.read", "ledger.entries.write"]
    );
    let first = decoded_claims(&gateway.bearers.last());
    let last = decoded_claims(&ledger.bearers.last());
    let claim = "https://veoveo.ai/claims/invocation";
    for field in ["actor", "authority", "request_context"] {
        assert_eq!(last[claim][field], first[claim][field], "{field}");
    }
    assert_eq!(
        last["act"]["act"]["sub"],
        relay.registration.client_id().as_str()
    );
    assert_eq!(
        last["act"]["act"]["act"]["sub"],
        gateway.registration.client_id().as_str()
    );
    assert!(last["exp"].as_i64().unwrap() <= first["exp"].as_i64().unwrap());

    // Narrowing: the member holds no sensitive permission, the admin does.
    let refused = relay_call(&member, "relay_seal", json!({}))
        .await
        .unwrap_err();
    assert!(refused.contains("rejected"), "{refused}");
    let seal = relay_call(&admin, "relay_seal", json!({})).await.unwrap();
    assert_eq!(seal["value"], "seal-7");
    // A server acting for the admin never reaches an administrative route, which the
    // admin's own token does.
    let admin_forwarded = relay.bearers.last();
    let (status, delegated) = exchange(
        &hub,
        &http,
        &relay,
        (
            &admin_forwarded,
            veoveo_hub_sdk::calls::ACCESS_TOKEN_TOKEN_TYPE,
        ),
        None,
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{delegated}");
    let administrative = http
        .get(format!("{}/admin/local/control-plane", hub.base))
        .bearer_auth(delegated["access_token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(administrative.status(), reqwest::StatusCode::FORBIDDEN);
    // A server that calls the Hub for its caller exchanges for no permissions, with the
    // caller's profile as the resource, and reaches the caller's own user routes.
    let (status, for_hub) = exchange(
        &hub,
        &http,
        &relay,
        (
            &admin_forwarded,
            veoveo_hub_sdk::calls::ACCESS_TOKEN_TOKEN_TYPE,
        ),
        Some(json!([{"type": "https://veoveo.ai/oauth/permissions", "actions": []}])),
        Some(&format!("{}/mcp/local", hub.base)),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{for_hub}");
    let selected = http
        .put(format!("{}/selection/local/relay", hub.base))
        .bearer_auth(for_hub["access_token"].as_str().unwrap())
        .json(&json!({ "branch": "feature" }))
        .send()
        .await
        .unwrap();
    assert_eq!(selected.status(), reqwest::StatusCode::NO_CONTENT);

    // A Ledger Task through the Hub resolves Relay's Task handle.
    let tallied = relay_call(&member, "relay_tally", json!({"entries": 2}))
        .await
        .unwrap();
    assert_eq!(tallied["entries"], 2);
    assert_eq!(tallied["inspection"]["acting_server"], "relay");

    // Relay reads a Ledger entry through the Hub and follows its updates through a
    // resource subscription until it closes.
    let followed: FollowedEntry = serde_json::from_value(
        relay_call(&member, "relay_follow_entry", json!({"entry": "hub-1"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(followed.states, [EntryState::Open, EntryState::Closed]);
    assert_eq!(followed.waited.state, EntryState::Closed);
    assert_eq!(followed.read, followed.waited);

    // A durable Relay Task calls Ledger after the call that created it returned.
    let created = match member
        .call_tool_once(
            CallToolRequestParams::new("relay__relay_later").with_arguments(Default::default()),
        )
        .await
        .unwrap()
    {
        CallToolResponse::Task(created) => created.task.task_id,
        other => panic!("relay_later did not start a Task: {other:?}"),
    };
    let later = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let task = member
                .get_task(GetTaskParams::new(&created))
                .await
                .unwrap()
                .task;
            match task.payload {
                TaskPayload::Completed { result } => return result,
                TaskPayload::Failed { error } => panic!("relay_later failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    })
    .await
    .expect("relay_later finished");
    let later: Inspection = serde_json::from_value(later["structuredContent"].clone()).unwrap();
    assert_eq!(later.client, "acceptance-member");
    assert_eq!(later.acting_server.as_deref(), Some("relay"));
    // Ledger sees the Relay Task the call came from, by the identifier Relay issued.
    let source: String = sqlx::query_scalar(
        "SELECT source_task_id FROM hub.task_route WHERE server = 'relay' AND delegated_caller IS NOT NULL",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(later.acting_task.as_deref(), Some(source.as_str()));
    let from_task = decoded_claims(&ledger.bearers.last());
    assert_eq!(
        from_task["https://veoveo.ai/claims/invocation"]["acting_task"],
        source.as_str()
    );

    // A retained Task keeps its first routing capture, which names no environment.
    let retained = member.get_task(GetTaskParams::new(&created)).await.unwrap();
    assert!(matches!(
        retained.task.payload,
        TaskPayload::Completed { .. }
    ));
    let retained_claims = decoded_claims(&relay.bearers.last());
    let execution = &retained_claims[claim]["request_context"]["execution"];
    assert!(execution["composition"].is_string(), "{execution}");
    assert!(execution.get("environment").is_none(), "{execution}");

    // Once the Task ended, it acts for nobody.
    let profile = HubProfileId::parse(PROFILE).unwrap();
    let ended = relay_calls
        .for_task_id(TaskId::parse(source.clone()).unwrap(), &profile)
        .call(Inspect {})
        .await;
    assert!(
        matches!(
            ended,
            Err(CallError::Refused {
                error: TokenErrorCode::InvalidGrant,
                ..
            })
        ),
        "{ended:?}"
    );
    // An unexpired request token derived from that Task cannot exchange after the
    // originating Task ends, even though Ledger is now the immediate server.
    let (_, ended_request) = exchange(
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
    assert_eq!(ended_request["error"], "invalid_grant", "{ended_request}");

    // Refusals at the token endpoint, with Relay's own assertion.
    let access = veoveo_hub_sdk::calls::ACCESS_TOKEN_TOKEN_TYPE;
    relay_call(&member, "relay_inspect", json!({}))
        .await
        .unwrap();
    let relay_token = relay.bearers.last();
    let ledger_token = ledger.bearers.last();
    let undeclared = json!([{"type": "https://veoveo.ai/oauth/permissions", "actions": ["observatory.readings.read"]}]);
    let (status, answer) = exchange(
        &hub,
        &http,
        &relay,
        (&relay_token, access),
        Some(undeclared),
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(answer["error"], "invalid_authorization_details", "{answer}");
    let (_, answer) = exchange(
        &hub,
        &http,
        &relay,
        (&relay_token, access),
        None,
        Some("http://127.0.0.1:1/mcp/elsewhere"),
    )
    .await;
    assert_eq!(answer["error"], "invalid_target", "{answer}");
    // Another server's forwarded token is no subject of Relay's.
    let (_, answer) = exchange(&hub, &http, &relay, (&ledger_token, access), None, None).await;
    assert_eq!(answer["error"], "invalid_grant", "{answer}");
    // The addressed registered server can continue the original caller through a
    // declared use. It cannot acquire the caller's withheld sensitive permission.
    let (status, nested) =
        exchange(&hub, &http, &ledger, (&ledger_token, access), None, None).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{nested}");
    assert_eq!(
        nested["authorization_details"][0]["actions"],
        json!(["relay.calls.make"])
    );
    let nested_claims = decoded_claims(nested["access_token"].as_str().unwrap());
    assert!(
        nested_claims["exp"].as_i64().unwrap()
            <= decoded_claims(&ledger_token)["exp"].as_i64().unwrap()
    );
    let (_, undeclared_nested) = exchange(
        &hub,
        &http,
        &ledger,
        (&ledger_token, access),
        Some(
            json!([{"type":"https://veoveo.ai/oauth/permissions","actions":["ledger.seals.read"]}]),
        ),
        None,
    )
    .await;
    assert_eq!(
        undeclared_nested["error"], "invalid_authorization_details",
        "{undeclared_nested}"
    );
    // The production token endpoint bounds repeated composition at the SDK depth.
    // A new client assertion and HTTP request do not renew the original deadline.
    let mut token = ledger_token.clone();
    let mut from_ledger = true;
    for depth in 2..=veoveo_hub_sdk::MAX_DELEGATION_DEPTH {
        let server = if from_ledger { &ledger } else { &relay };
        let (status, answer) = exchange(&hub, &http, server, (&token, access), None, None).await;
        assert_eq!(status, reqwest::StatusCode::OK, "depth {depth}: {answer}");
        let client = mcp_client(&hub, answer["access_token"].as_str().unwrap().to_owned()).await;
        let tool = if from_ledger {
            "relay__relay_whoami"
        } else {
            "ledger__inspect"
        };
        let result = client
            .call_tool(CallToolRequestParams::new(tool).with_arguments(Default::default()))
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true), "{result:?}");
        token = if from_ledger {
            relay.bearers.last()
        } else {
            ledger.bearers.last()
        };
        let claims = decoded_claims(&token);
        assert!(
            claims["exp"].as_i64().unwrap()
                <= decoded_claims(&ledger_token)["exp"].as_i64().unwrap()
        );
        client.cancel().await.unwrap();
        from_ledger = !from_ledger;
    }
    let server = if from_ledger { &ledger } else { &relay };
    let (_, too_deep) = exchange(&hub, &http, server, (&token, access), None, None).await;
    assert_eq!(too_deep["error"], "invalid_grant", "{too_deep}");

    // Short-lived native signed source fixtures exercise production admission and
    // exchange expiry without waiting for the ordinary 60-second request lifetime.
    let (issuer, trust) = forwarded_issuer(&hub);
    let identity = HubInternalTokenVerifier::new(
        TokenIssuer::parse(&hub.base).unwrap(),
        TokenAudience {
            deployment: ledger.metadata.registration.deployment,
            server: ledger.metadata.slug().clone(),
            resource: veoveo_hub_sdk::registration::ResourceIndicator::from(
                &ledger.metadata.registration.endpoint,
            ),
        },
        trust,
    )
    .verify(&ledger_token)
    .unwrap();
    let short = issuer
        .issue(ForwardedRequest {
            deployment: identity.deployment,
            profile: identity.profile,
            server: identity.server,
            audience: identity.audience,
            client: identity.client,
            actor: identity.actor,
            authority: identity.authority,
            permissions: identity.permissions,
            request_context: identity.request_context,
            acting_server: identity.acting_server,
            acting_task: identity.acting_task,
            delegation_context: identity.delegation_context,
            expires_at: chrono::Utc::now() + chrono::TimeDelta::seconds(3),
        })
        .unwrap();
    let (status, short_exchange) = exchange(
        &hub,
        &http,
        &ledger,
        (&short.bearer_token, access),
        None,
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{short_exchange}");
    let bearer = format!(
        "Bearer {}",
        short_exchange["access_token"].as_str().unwrap()
    );
    assert_eq!(
        hub.mcp(&http, Some(&bearer), DISCOVER).await.status(),
        reqwest::StatusCode::OK
    );
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(
        hub.mcp(&http, Some(&bearer), DISCOVER).await.status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let (_, expired) = exchange(
        &hub,
        &http,
        &ledger,
        (&short.bearer_token, access),
        None,
        None,
    )
    .await;
    assert_eq!(expired["error"], "invalid_grant", "{expired}");

    // Modified signatures do not become valid when they carry a nested actor chain.
    let mut forged = ledger_token.clone().into_bytes();
    let signature = forged.iter().rposition(|b| *b == b'.').unwrap() + 1;
    forged[signature] = if forged[signature] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let (_, forged) = exchange(
        &hub,
        &http,
        &ledger,
        (std::str::from_utf8(&forged).unwrap(), access),
        None,
        None,
    )
    .await;
    assert_eq!(forged["error"], "invalid_grant", "{forged}");

    // Relay's own subject exchanges; the response names what the token carries: the
    // member's Ledger permissions among Relay's uses, without the sensitive seal.
    let (status, answer) = exchange(&hub, &http, &relay, (&relay_token, access), None, None).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{answer}");
    assert_eq!(
        answer["authorization_details"][0]["actions"],
        json!(["ledger.entries.read", "ledger.entries.write"])
    );

    // Each exchange and each delegated admission is in the audit chain.
    let exchanges: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit.record WHERE class = 'hub.token_exchange'")
            .fetch_one(db.a.pool())
            .await
            .unwrap();
    assert!(exchanges >= 6, "{exchanges} exchange records");
    let acting: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.record WHERE class = 'mcp.tool_admission' AND draft #>> '{via,server}' = 'relay'",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert!(acting >= 3, "{acting} delegated admissions");
    let from_tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.record WHERE class IN ('mcp.tool_admission', 'mcp.tool_completion') AND draft #>> '{via,task}' = $1",
    )
    .bind(&source)
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(
        from_tasks, 2,
        "the Task's call is admitted and completed once"
    );

    // Without Ledger, the refusal names the permission no server provides.
    let (ledger_store, ledger_instance) = ledger_process.take().unwrap();
    ledger_instance.shutdown().await.unwrap();
    ledger_store.close().await;
    ledger.client.deregister(ledger.registration).await.unwrap();
    let missing = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match relay_call(&member, "relay_inspect", json!({})).await {
                Err(message) if message.contains("no registered server provides") => {
                    return message;
                }
                _ => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    })
    .await
    .expect("the Hub named the missing provider");
    assert!(missing.contains("ledger.entries.read"), "{missing}");
    let _ = (
        Inspect::TOOL,
        &ledger.metadata,
        &relay.metadata,
        &relay.client,
    );
    member.cancel().await.unwrap();
    admin.cancel().await.unwrap();
    let (gateway_store, gateway_instance) = gateway_process.take().unwrap();
    gateway_instance.shutdown().await.unwrap();
    gateway_store.close().await;
    gateway
        .client
        .deregister(gateway.registration)
        .await
        .unwrap();
    drop(hub);
}
