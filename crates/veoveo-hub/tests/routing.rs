//! Routing without named environments, against the real Hub binary and SDK fixture.
//! Registering is serving: a server needs no enrollment. When several deployments of
//! one slug are registered, the newest serves new requests. A retained Task and the
//! calls it makes keep the routing their request captured, also across a Hub restart.
//! The retired `veoveo-environment` header is ignored. Revisions register under their
//! synthetic namespace signer; no native runtime or source transfer is simulated.
#[path = "support/hub.rs"]
mod hub;
#[path = "support/ledger_revisions.rs"]
mod revisions;
#[path = "support/servers.rs"]
mod servers;

use hub::Hub;
use revisions::{FirstRevision, InspectionRelay, SecondRevision};
use rmcp::{
    RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientConfig, ClientRequest, GetTaskParams,
        ListToolsRequest, ListToolsResult, ServerResult, TaskPayload,
    },
    service::RunningService,
};
use serde_json::json;
use servers::{
    Served, decoded_claims, hosted, mcp_client, mcp_client_with, relay_call, serve, serve_revision,
};
use std::time::Duration;
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::{
    ExecutionContext,
    hosting::{DomainServer, Hosted, ServerCalls},
    registration::client::Registration,
};
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_types::ServerSlug;

const INVOCATION: &str = "https://veoveo.ai/claims/invocation";
type Client = RunningService<RoleClient, ClientConfig>;

async fn process(registration: &Registration, slug: &str) -> (PlatformStore, Instance) {
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
    let instance = Instance::start(&store, &ServerSlug::parse(slug).unwrap())
        .await
        .unwrap();
    (store, instance)
}

/// The routing `server` last received, which it was addressed as.
fn capture(server: &Served) -> ExecutionContext {
    let claims = decoded_claims(&server.bearers.last());
    assert_eq!(
        claims[INVOCATION]["deployment"],
        json!(server.metadata.registration.deployment)
    );
    serde_json::from_value(claims[INVOCATION]["request_context"]["execution"].clone()).unwrap()
}

/// Authoritative catalog reads reach the Hub, rather than the native client's private
/// five-second response cache.
async fn wire_tools(client: &Client) -> ListToolsResult {
    match client
        .send_request(ClientRequest::ListToolsRequest(ListToolsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        }))
        .await
        .unwrap()
    {
        ServerResult::ListToolsResult(result) => result,
        other => panic!("tools/list returned an unexpected result: {other:?}"),
    }
}

async fn descriptor(client: &Client) -> String {
    let tools = wire_tools(client).await.tools;
    let descriptors = tools
        .iter()
        .filter(|tool| tool.name == "ledger__inspect")
        .collect::<Vec<_>>();
    assert_eq!(descriptors.len(), 1, "one deployment advertises the tool");
    descriptors[0].description.as_deref().unwrap().to_owned()
}

async fn inspect(client: &Client) {
    let result = client
        .call_tool(CallToolRequestParams::new("ledger__inspect").with_arguments(Default::default()))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    assert_eq!(
        result.structured_content.unwrap()["client"],
        "acceptance-member"
    );
}

async fn task(client: &Client) -> String {
    match client
        .call_tool_once(
            CallToolRequestParams::new("ledger__tally")
                .with_arguments(json!({"entries":7}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
    {
        CallToolResponse::Task(created) => created.task.task_id,
        other => panic!("fixture did not create a Task: {other:?}"),
    }
}

async fn payload(client: &Client, task: &str) -> TaskPayload {
    client
        .get_task(GetTaskParams::new(task))
        .await
        .unwrap()
        .task
        .payload
}

#[tokio::test]
async fn the_newest_registered_deployment_serves_and_captures_keep_their_routing() {
    tokio::time::timeout(Duration::from_secs(120), scenario())
        .await
        .expect("routing scenario exceeded two minutes");
}

async fn scenario() {
    let started = std::time::Instant::now();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let mut hub = Hub::start(directory.path(), &db, Duration::from_secs(5)).await;
    let http = reqwest::Client::new();
    let mut processes = Vec::new();
    let base_tasks = LedgerTasks::new(Duration::from_secs(8));
    let base = serve(
        &hub,
        &http,
        "ledger",
        ledger::authorization().unwrap(),
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            let tasks = base_tasks.clone();
            hosted::<LedgerDomain>(registration, &http)
                .handler(move || ledger::hosted(tasks.clone()))
                .build()
        },
    )
    .await;
    let relay = serve(
        &hub,
        &http,
        "relay",
        InspectionRelay::routes().authorization(),
        async |registration, metadata, key| {
            processes.push(process(registration, "relay").await);
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            hosted::<InspectionRelay>(registration, &http)
                .calls(calls)
                .handler(|| Hosted::new(InspectionRelay))
                .build()
        },
    )
    .await;
    let token = hub.access_token(&http, hub.caller("member")).await;
    let client = mcp_client(&hub, token.clone()).await;

    // A registered server serves at once: nothing enrolls it.
    let base_descriptor = descriptor(&client).await;
    assert_eq!(base_descriptor, "Inspect caller identity");
    inspect(&client).await;
    let base_capture = capture(&base);
    let retained = task(&client).await;
    assert!(matches!(
        payload(&client, &retained).await,
        TaskPayload::Working
    ));
    assert_eq!(capture(&base), base_capture);

    // A newer deployment of the same slug serves new requests; the older one is set
    // aside, not unavailable.
    let first = serve_revision(
        &hub,
        &http,
        "ledger",
        FirstRevision::routes().authorization(),
        &base.key,
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            hosted::<FirstRevision>(registration, &http)
                .handler(|| Hosted::new(FirstRevision).with_tasks(LedgerTasks::default()))
                .build()
        },
    )
    .await;
    let first_descriptor = "Inspect identity from source revision A";
    tokio::time::timeout(Duration::from_secs(10), async {
        while descriptor(&client).await != first_descriptor {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the newest registered deployment serves new requests");
    inspect(&client).await;
    let first_capture = capture(&first);
    assert_ne!(first_capture.composition, base_capture.composition);

    // The header that once selected a named environment changes nothing.
    let selecting = mcp_client_with(
        &hub,
        token.clone(),
        &[("veoveo-environment", "019a05cb-6c00-7000-8712-3456789abcdf")],
    )
    .await;
    assert_eq!(descriptor(&selecting).await, first_descriptor);
    selecting.cancel().await.unwrap();

    // Delegation through the relay routes with the request's capture.
    let delegated = relay_call(&client, "relay_inspect", json!({})).await;
    assert_eq!(delegated.unwrap()["acting_server"], "relay");
    assert_eq!(capture(&first).composition, capture(&relay).composition);

    // The retained Task keeps the deployment and routing that created it.
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match payload(&client, &retained).await {
                TaskPayload::Completed { result } => break result,
                TaskPayload::Failed { error } => panic!("retained Task failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(result["structuredContent"]["entries"], 7);
    assert_eq!(capture(&base), base_capture);

    // A third deployment with a new typed argument takes over in turn.
    let second = serve_revision(
        &hub,
        &http,
        "ledger",
        SecondRevision::routes().authorization(),
        &base.key,
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            hosted::<SecondRevision>(registration, &http)
                .handler(|| Hosted::new(SecondRevision).with_tasks(LedgerTasks::default()))
                .build()
        },
    )
    .await;
    let second_descriptor = "Inspect identity from source revision B";
    tokio::time::timeout(Duration::from_secs(10), async {
        while descriptor(&client).await != second_descriptor {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the newest registered deployment serves new requests");
    // Each change of the set-aside deployments is logged once: the base, then also
    // the first revision.
    let warnings = || {
        hub.log()
            .matches("several deployments of a server are registered")
            .count()
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while warnings() < 2 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the Hub logs the deployments it sets aside");
    assert_eq!(
        warnings(),
        2,
        "the Hub logs a set-aside deployment once per change"
    );
    let probed = client
        .call_tool(
            CallToolRequestParams::new("ledger__inspect").with_arguments(
                json!({"probe": "revision-b-only"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(probed.is_error, Some(true), "{probed:?}");
    assert_eq!(
        probed.structured_content.unwrap()["probe"],
        "revision-b-only"
    );

    // The Task's route is exact after a Hub restart too, and the Hub resumes routing
    // to the newest deployment.
    client.cancel().await.unwrap();
    hub.restart().await;
    let after = mcp_client(&hub, hub.access_token(&http, hub.caller("member")).await).await;
    assert!(matches!(
        payload(&after, &retained).await,
        TaskPayload::Completed { .. }
    ));
    assert_eq!(capture(&base), base_capture);
    assert_eq!(descriptor(&after).await, second_descriptor);
    eprintln!(
        "routing scenario: ledger_base={} ledger_first={} ledger_second={} retained_task={} base_composition={} first_composition={} elapsed_ms={}",
        base.metadata.registration.deployment,
        first.metadata.registration.deployment,
        second.metadata.registration.deployment,
        retained,
        base_capture.composition.digest(),
        first_capture.composition.digest(),
        started.elapsed().as_millis()
    );
    after.cancel().await.unwrap();
    for (store, instance) in processes {
        instance.shutdown().await.unwrap();
        store.close().await;
    }
    for served in [base, first, second, relay] {
        served.client.deregister(served.registration).await.unwrap();
    }
    drop(hub);
}
