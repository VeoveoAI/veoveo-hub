//! One composed-routing scenario against the real Hub binary and SDK fixture.
//! Revisions use a synthetic installation namespace signer, NOT independently
//! keyed user admission. That authority boundary has separate DB/deployed tests.
//! No native runtime, source transfer or application inference is simulated here.
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
    Served, decoded_claims, hosted, mcp_client_selected, relay_call, serve, serve_revision,
};
use std::time::Duration;
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::{
    EnvironmentId, ExecutionContext,
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
async fn connection(hub: &Hub, http: &reqwest::Client, environment: EnvironmentId) -> Client {
    mcp_client_selected(
        hub,
        hub.access_token(http, hub.caller("member")).await,
        Some(environment),
    )
    .await
}
fn capture(server: &Served, environment: EnvironmentId) -> ExecutionContext {
    let claims = decoded_claims(&server.bearers.last());
    assert_eq!(
        claims[INVOCATION]["deployment"],
        json!(server.metadata.registration.deployment)
    );
    let execution: ExecutionContext =
        serde_json::from_value(claims[INVOCATION]["request_context"]["execution"].clone()).unwrap();
    assert_eq!(execution.environment, environment);
    execution
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
async fn tool_descriptor(client: &Client) -> rmcp::model::Tool {
    let tools = wire_tools(client).await.tools;
    let descriptors = tools
        .iter()
        .filter(|tool| tool.name == "ledger__inspect")
        .collect::<Vec<_>>();
    assert_eq!(
        descriptors.len(),
        1,
        "only the selected revision may advertise a logical tool"
    );
    assert_eq!(
        descriptors[0].input_schema.get("type"),
        Some(&json!("object"))
    );
    (*descriptors[0]).clone()
}
/// Authoritative catalog assertions must reach the Hub, rather than observing
/// the native client's independently valid private five-second response cache.
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
    tool_descriptor(client)
        .await
        .description
        .as_deref()
        .unwrap()
        .to_owned()
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

#[tokio::test]
async fn simultaneous_selected_revisions_keep_discovery_delegation_and_retained_tasks() {
    tokio::time::timeout(Duration::from_secs(120), scenario())
        .await
        .expect("environment scenario exceeded two minutes");
}
async fn scenario() {
    let started = std::time::Instant::now();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let mut hub = Hub::start(directory.path(), &db, Duration::from_secs(5)).await;
    let http = reqwest::Client::new();
    let mut processes = Vec::new();
    let base = serve(
        &hub,
        &http,
        "ledger",
        ledger::authorization().unwrap(),
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            hosted::<LedgerDomain>(registration, &http)
                .handler(|| ledger::hosted(LedgerTasks::default()))
                .build()
        },
    )
    .await;
    let first_tasks = LedgerTasks::new(Duration::from_secs(30));
    let first = serve_revision(
        &hub,
        &http,
        "ledger",
        FirstRevision::routes().authorization(),
        &base.key,
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            let tasks = first_tasks.clone();
            hosted::<FirstRevision>(registration, &http)
                .handler(move || Hosted::new(FirstRevision).with_tasks(tasks.clone()))
                .build()
        },
    )
    .await;
    let second_tasks = LedgerTasks::default();
    let second = serve_revision(
        &hub,
        &http,
        "ledger",
        SecondRevision::routes().authorization(),
        &base.key,
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            let tasks = second_tasks.clone();
            hosted::<SecondRevision>(registration, &http)
                .handler(move || Hosted::new(SecondRevision).with_tasks(tasks.clone()))
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

    let mut control = hub.control_plane().await;
    let base_environment =
        EnvironmentId::parse(control["execution"]["defaultEnvironment"].as_str().unwrap()).unwrap();
    let first_environment = EnvironmentId::new();
    let second_environment = EnvironmentId::new();
    for (environment, served, label) in [
        (first_environment, &first, "First experiment"),
        (second_environment, &second, "Second experiment"),
    ] {
        let mut record = control["execution"]["environments"][base_environment.to_string()].clone();
        record["displayName"] = json!(label);
        record["base"] = json!({"kind":"follow","environment":base_environment});
        record["components"] = json!({"ledger":{"kind":"deployment","deployment":served.metadata.registration.deployment}});
        control["execution"]["environments"][environment.to_string()] = record;
    }
    hub.publish_control_plane(&control).await;
    let base_client = connection(&hub, &http, base_environment).await;
    let first_client = connection(&hub, &http, first_environment).await;
    let second_client = connection(&hub, &http, second_environment).await;
    // Newer registrations never enroll themselves or steal the base's namespace.
    let (base_descriptor, first_descriptor, second_descriptor) = tokio::join!(
        descriptor(&base_client),
        descriptor(&first_client),
        descriptor(&second_client)
    );
    assert_eq!(base_descriptor, "Inspect caller identity");
    assert_eq!(first_descriptor, "Inspect identity from source revision A");
    assert_eq!(second_descriptor, "Inspect identity from source revision B");
    let first_tool = tool_descriptor(&first_client).await;
    let second_tool = tool_descriptor(&second_client).await;
    assert_ne!(first_tool.input_schema, second_tool.input_schema);
    assert_ne!(first_tool.output_schema, second_tool.output_schema);
    assert!(
        first_tool
            .input_schema
            .get("properties")
            .and_then(|p| p.get("probe"))
            .is_none()
    );
    assert!(
        second_tool
            .input_schema
            .get("properties")
            .and_then(|p| p.get("probe"))
            .is_some()
    );
    let probe = CallToolRequestParams::new("ledger__inspect").with_arguments(
        json!({"probe":"revision-b-only"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let answer = second_client.call_tool(probe.clone()).await.unwrap();
    assert_ne!(answer.is_error, Some(true), "{answer:?}");
    assert_eq!(
        answer.structured_content.unwrap()["probe"],
        "revision-b-only"
    );
    match first_client.call_tool(probe).await {
        Ok(answer) => assert_eq!(
            answer.is_error,
            Some(true),
            "revision A must reject B's typed argument"
        ),
        Err(error) => assert!(
            error.to_string().contains("probe"),
            "expected typed argument refusal: {error}"
        ),
    }
    tokio::join!(
        inspect(&base_client),
        inspect(&first_client),
        inspect(&second_client)
    );
    let base_capture = capture(&base, base_environment);
    let first_capture = capture(&first, first_environment);
    let second_capture = capture(&second, second_environment);
    assert_ne!(first_capture.composition, second_capture.composition);
    // The same inherited Relay deployment delegates through each captured map.
    let (left, right) = tokio::join!(
        relay_call(&first_client, "relay_inspect", json!({})),
        relay_call(&second_client, "relay_inspect", json!({}))
    );
    assert_eq!(left.unwrap()["acting_server"], "relay");
    assert_eq!(right.unwrap()["acting_server"], "relay");
    assert_eq!(capture(&first, first_environment), first_capture);
    assert_eq!(capture(&second, second_environment), second_capture);

    let retained = task(&first_client).await;
    assert!(matches!(
        first_client
            .get_task(GetTaskParams::new(&retained))
            .await
            .unwrap()
            .task
            .payload,
        TaskPayload::Working
    ));
    let captured_task = capture(&first, first_environment);
    assert_eq!(captured_task, first_capture);
    // Also exercise the ordinary client's advertised cache contract. It may
    // retain A until expiry without a catalog-change subscription; wire requests
    // below must immediately observe the newly selected composition.
    let cached = first_client.list_tools(None).await.unwrap();
    assert_eq!(cached.ttl_ms, Some(veoveo_hub_sdk::PRIVATE_CATALOG_TTL_MS));
    assert_eq!(
        cached
            .tools
            .iter()
            .find(|tool| tool.name == "ledger__inspect")
            .unwrap()
            .description
            .as_deref(),
        Some(first_descriptor.as_str())
    );
    // Select another deployment through the real authoritative publisher.
    control["execution"]["environments"][first_environment.to_string()]["components"]["ledger"] =
        json!({"kind":"deployment","deployment":base.metadata.registration.deployment});
    hub.publish_control_plane(&control).await;
    let refresh_started = std::time::Instant::now();
    assert_eq!(descriptor(&first_client).await, base_descriptor);
    assert_eq!(descriptor(&second_client).await, second_descriptor);
    tokio::time::timeout(
        Duration::from_millis(veoveo_hub_sdk::PRIVATE_CATALOG_TTL_MS + 1_000),
        async {
            loop {
                let tools = first_client.list_tools(None).await.unwrap().tools;
                let tool = tools
                    .iter()
                    .find(|tool| tool.name == "ledger__inspect")
                    .unwrap();
                if tool.description.as_deref() == Some(base_descriptor.as_str()) {
                    break;
                }
                assert_eq!(tool.description.as_deref(), Some(first_descriptor.as_str()));
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        },
    )
    .await
    .expect("normal catalog client did not refresh within its advertised TTL");
    println!(
        "Normal client catalog refreshed after {}ms (advertised TTL {}ms)",
        refresh_started.elapsed().as_millis(),
        veoveo_hub_sdk::PRIVATE_CATALOG_TTL_MS
    );
    inspect(&second_client).await;
    assert_eq!(capture(&second, second_environment), second_capture);
    assert!(matches!(
        first_client
            .get_task(GetTaskParams::new(&retained))
            .await
            .unwrap()
            .task
            .payload,
        TaskPayload::Working
    ));
    assert_eq!(capture(&first, first_environment), captured_task);

    control["execution"]["environments"][first_environment.to_string()]["components"]["ledger"] =
        json!({"kind":"unavailable"});
    hub.publish_control_plane(&control).await;
    let tools = wire_tools(&first_client).await.tools;
    assert!(
        !tools.iter().any(|tool| tool.name == "ledger__inspect"),
        "explicit unavailable must not inherit the base"
    );
    assert!(
        relay_call(&first_client, "relay_inspect", json!({}))
            .await
            .is_err()
    );
    inspect(&base_client).await;
    assert_eq!(capture(&base, base_environment), base_capture);
    control["execution"]["environments"][first_environment.to_string()]["components"] = json!({});
    hub.publish_control_plane(&control).await;
    assert_eq!(
        descriptor(&first_client).await,
        base_descriptor,
        "removal explicitly resumes inheritance"
    );

    let result = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            match first_client
                .get_task(GetTaskParams::new(&retained))
                .await
                .unwrap()
                .task
                .payload
            {
                TaskPayload::Completed { result } => break result,
                TaskPayload::Failed { error } => panic!("retained Task failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(result["structuredContent"]["entries"], 7);
    assert_eq!(capture(&first, first_environment), captured_task);
    // The canonical terminal route remains exact after Hub restart too. Workers
    // and live kernel instances are deliberately retained, not replayed/rebuilt.
    first_client.cancel().await.unwrap();
    second_client.cancel().await.unwrap();
    base_client.cancel().await.unwrap();
    hub.restart().await;
    let after = connection(&hub, &http, first_environment).await;
    assert!(matches!(
        after
            .get_task(GetTaskParams::new(&retained))
            .await
            .unwrap()
            .task
            .payload,
        TaskPayload::Completed { .. }
    ));
    assert_eq!(capture(&first, first_environment), captured_task);
    eprintln!(
        "synthetic environment scenario: base={} first={} second={} ledger_base={} ledger_first={} ledger_second={} retained_task={} captured_composition={} elapsed_ms={}",
        base_environment,
        first_environment,
        second_environment,
        base.metadata.registration.deployment,
        first.metadata.registration.deployment,
        second.metadata.registration.deployment,
        retained,
        captured_task.composition.digest(),
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
