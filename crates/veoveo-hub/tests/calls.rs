//! Calls between servers through the Hub binary: the SDK's Relay fixture calls its
//! Ledger fixture for Relay's caller with an RFC 8693 exchanged token, inline, through a
//! Ledger Task and from a durable Relay Task after the call that created it returned.
//! The Hub narrows each call to the uses Relay declared and the caller holds, names
//! a missing provider, and refuses another server's subject, a delegated subject and an
//! ended Task.

#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use hub::Hub;
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
    HubProfileId,
    calls::{TokenErrorCode, ToolCall},
    client::CallError,
    hosting::ServerCalls,
};
use veoveo_hub_sdk_fixture::{
    calls::{EntryState, FollowedEntry, Inspect, Inspection},
    ledger::{self, LedgerDomain, LedgerTasks},
    relay::{self, RelayDomain, RelayTasks},
};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_task::TaskRuntime;
use veoveo_types::{PermissionName, ServerSlug, TaskId};

const PROFILE: &str = "local";

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
                TaskRuntime::new(store, relay_slug.clone(), instance),
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

    // Narrowing: the member holds no sensitive permission, the admin does.
    let refused = relay_call(&member, "relay_seal", json!({}))
        .await
        .unwrap_err();
    assert!(refused.contains("rejected"), "{refused}");
    let seal = relay_call(&admin, "relay_seal", json!({})).await.unwrap();
    assert_eq!(seal["value"], "seal-7");

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
    // A delegated identity is not delegated again.
    let (_, answer) = exchange(&hub, &http, &ledger, (&ledger_token, access), None, None).await;
    assert_eq!(answer["error"], "invalid_grant", "{answer}");
    assert!(
        answer["error_description"]
            .as_str()
            .unwrap()
            .contains("delegated again"),
        "{answer}"
    );
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
    drop(hub);
}
