//! Data environments. Against the Hub binary, the SDK's Relay fixture runs as two
//! instances of one slug: one in the installation database and one in a database
//! `ensure_database` created. Each registers into its own database and its grant names
//! that database. On each, the administrative Task cancel reaches the instance's
//! Task, a Task created through the Hub completes after its Ledger call from the Task,
//! which the Hub admits only while the Task runs in that instance's database, and once
//! the Task ended an exchange for it is refused. Without the binary, concurrent
//! `ensure_database` calls for one name all succeed, and `drop_database_if_unused`
//! drops a database no instance names but neither one an instance names nor the
//! installation database.

#[path = "support/database.rs"]
mod database;
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
use servers::{Served, hosted, mcp_client, serve, serve_instance};
use tokio_util::sync::CancellationToken;
use veoveo_fixtures::store::TestDb;
use veoveo_hub::data_environment::{DatabaseName, drop_database_if_unused, ensure_database};
use veoveo_hub::registration::{NewInstance, create_instance};
use veoveo_hub_sdk::{
    HubProfileId, calls::TokenErrorCode, client::CallError, hosting::ServerCalls,
    registration::client::Registration,
};
use veoveo_hub_sdk_fixture::{
    calls::{Inspect, Inspection},
    ledger::{self, LedgerDomain, LedgerTasks},
    relay::{self, RelayDomain, RelayTasks},
};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_task::TaskRuntime;
use veoveo_types::{DeploymentId, ServerSlug, TaskId};

/// How long one of these tests may take, Hub start included.
const BOUND: Duration = Duration::from_secs(180);

type Client = RunningService<RoleClient, ClientConfig>;

#[tokio::test]
async fn one_slug_serves_from_the_installation_database_and_a_data_environment() {
    tokio::time::timeout(BOUND, scenario())
        .await
        .expect("the data environment scenario exceeded its bound");
}

async fn scenario() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start(directory.path(), &db, Duration::from_secs(60)).await;
    let http = reqwest::Client::new();
    let stop = CancellationToken::new();
    let _stop = stop.clone().drop_guard();

    // Ledger, a namespace server of the installation database, answers Relay's Tasks.
    let mut ledger_process = None;
    serve(
        &hub,
        &http,
        "ledger",
        ledger::authorization().unwrap(),
        async |registration, _, _| {
            ledger_process = Some(process(registration, "ledger").await);
            hosted::<LedgerDomain>(registration, &http)
                .handler(|| ledger::hosted(LedgerTasks::default()))
                .build()
        },
    )
    .await;

    let installation = DatabaseName::installation(&db.a).unwrap();
    let branch = environment_name();
    ensure_database(&db.a, &branch).await.unwrap();
    let base = relay_instance(
        &hub,
        &http,
        &create_instance(&db.a, &installation, &branch_name("dev"))
            .await
            .unwrap(),
        installation,
    )
    .await;
    let other = relay_instance(
        &hub,
        &http,
        &create_instance(&db.a, &branch, &branch_name("feature"))
            .await
            .unwrap(),
        branch,
    )
    .await;

    // Each grant names its instance's database at the address of the installation
    // database, and the server's schema exists in each database.
    let installation_url = url::Url::parse(&db.url()).unwrap();
    for relay in [&base, &other] {
        let url = relay.served.registration.database().unwrap().url.as_url();
        assert_eq!(url.path(), format!("/{}", relay.database));
        assert_eq!(url.host_str(), installation_url.host_str());
        assert_eq!(url.port(), installation_url.port());
        assert_eq!(url.query(), installation_url.query());
        let store = owner_store(&db, &relay.database).await;
        assert!(schema_exists(&store, "relay").await);
        store.close().await;
    }
    assert_ne!(base.database, other.database);

    // The base branch's instance serves by default; selecting the other branch serves
    // the instance in the data environment.
    hub.enroll(&base.served.metadata).await;
    let mut control = hub.control_plane().await;
    control["execution"]["baseBranch"] = json!("dev");
    hub.publish_control_plane(&control).await;
    let token = hub.access_token(&http, hub.caller("member")).await;
    let client = mcp_client(&hub, token.clone()).await;

    exercise(&hub, &http, &db, &base, &client, &stop).await;
    let response = http
        .put(format!("{}/selection/local/relay", hub.base))
        .bearer_auth(&token)
        .json(&json!({ "branch": "feature" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
    exercise(&hub, &http, &db, &other, &client, &stop).await;
}

/// One Relay instance: its registration, its Tasks, its calls through the Hub and the
/// database its registration grants.
struct RelayInstance {
    served: Served,
    tasks: RelayTasks,
    calls: ServerCalls,
    database: DatabaseName,
}

/// Register Relay as `instance`, running its `relay_later` Tasks in core's Task runtime
/// on the login its registration grants. No worker claims them until [`exercise`]
/// starts one.
async fn relay_instance(
    hub: &Hub,
    http: &reqwest::Client,
    instance: &NewInstance,
    database: DatabaseName,
) -> RelayInstance {
    let slug = ServerSlug::parse("relay").unwrap();
    let parts: Arc<Mutex<Option<(RelayTasks, ServerCalls)>>> = Arc::default();
    let served = serve_instance(
        hub,
        http,
        "relay",
        relay::authorization().unwrap(),
        instance,
        async |registration, metadata, key| {
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            let (store, process) = process(registration, "relay").await;
            let tasks = RelayTasks::new(
                TaskRuntime::new(
                    store,
                    slug.clone(),
                    metadata.registration.deployment,
                    Arc::new(process),
                ),
                calls.clone(),
            );
            *parts.lock().unwrap() = Some((tasks.clone(), calls.clone()));
            hosted::<RelayDomain>(registration, http)
                .calls(calls)
                .handler(move || relay::hosted_with_tasks(tasks.clone()))
                .build()
        },
    )
    .await;
    let (tasks, calls) = parts.lock().unwrap().take().unwrap();
    RelayInstance {
        served,
        tasks,
        calls,
        database,
    }
}

/// Cancel a queued Task through the administrative route, then run a Task to its end
/// and exchange for it afterwards, checking each Task's row in the instance's database.
async fn exercise(
    hub: &Hub,
    http: &reqwest::Client,
    db: &TestDb,
    relay: &RelayInstance,
    client: &Client,
    stop: &CancellationToken,
) {
    let deployment = relay.served.metadata.registration.deployment;
    let store = owner_store(db, &relay.database).await;
    // An instance outside the installation database leaves no Task row there.
    let installation =
        (relay.database != DatabaseName::installation(&db.a).unwrap()).then_some(&db.a);

    // No worker claims the Task yet, so it is queued when the administrator's cancel
    // reaches the instance.
    let queued = source_task(db, &later(client).await).await;
    assert_eq!(queued.1, deployment);
    assert_eq!(
        task_status(&store, &queued.0).await.as_deref(),
        Some("queued")
    );
    let cancelled = http
        .post(format!(
            "{}/admin/local/tasks/relay/{}/cancel",
            hub.base, queued.0
        ))
        .bearer_auth(hub.access_token(http, hub.caller("member")).await)
        .send()
        .await
        .unwrap();
    assert_eq!(cancelled.status(), reqwest::StatusCode::NO_CONTENT);
    assert!(
        matches!(
            task_status(&store, &queued.0).await.as_deref(),
            Some("cancelled" | "cancel_requested")
        ),
        "the cancellation reached the instance's Task"
    );
    let unknown = http
        .post(format!(
            "{}/admin/local/tasks/relay/{}/cancel",
            hub.base,
            TaskId::new()
        ))
        .bearer_auth(hub.access_token(http, hub.caller("member")).await)
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);

    // The worker's Ledger call exchanges with the running Task as its subject, which
    // the Hub admits only after finding the Task live in the instance's database.
    tokio::spawn(relay.tasks.clone().work(stop.child_token()));
    let created = later(client).await;
    let result = completed(client, &created).await;
    let inspection: Inspection =
        serde_json::from_value(result["structuredContent"].clone()).unwrap();
    let (source, routed) = source_task(db, &created).await;
    assert_eq!(routed, deployment);
    assert_eq!(inspection.client, "acceptance-member");
    assert_eq!(inspection.acting_server.as_deref(), Some("relay"));
    assert_eq!(inspection.acting_task.as_deref(), Some(source.as_str()));
    assert_eq!(
        task_status(&store, &source).await.as_deref(),
        Some("succeeded")
    );
    if let Some(installation) = installation {
        assert_eq!(task_status(installation, &source).await, None);
        assert_eq!(task_status(installation, &queued.0).await, None);
    }

    // Once the Task ended, it acts for nobody.
    let ended = relay
        .calls
        .for_task_id(
            TaskId::parse(source).unwrap(),
            &HubProfileId::parse("local").unwrap(),
        )
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
    store.close().await;
}

#[tokio::test]
async fn environment_databases_are_created_once_and_dropped_when_unused() {
    tokio::time::timeout(BOUND, async {
        let db = database::TestDb::new().await;
        let installation = DatabaseName::installation(&db.a).unwrap();

        // Four callers ensure one new database at once, as replicas would.
        let created = environment_name();
        let stores = [
            db.a.clone(),
            db.b.clone(),
            db.connect().await,
            db.connect().await,
        ];
        for ensured in
            futures::future::join_all(stores.iter().map(|store| ensure_database(store, &created)))
                .await
        {
            ensured.unwrap();
        }
        ensure_database(&db.a, &created).await.unwrap();
        let environment = owner_store(&db, &created).await;
        let applied = sqlx::query_file_scalar!("tests/data_environments/kernel_migrations.sql")
            .fetch_one(environment.pool())
            .await
            .unwrap();
        assert_eq!(
            applied,
            i64::try_from(veoveo_store::MIGRATOR.iter().count()).unwrap()
        );
        environment.close().await;

        let used = environment_name();
        ensure_database(&db.a, &used).await.unwrap();
        create_instance(&db.a, &used, &branch_name("used"))
            .await
            .unwrap();

        assert!(drop_database_if_unused(&db.a, &created).await.unwrap());
        assert!(!database_exists(&db.a, &created).await);
        assert!(!drop_database_if_unused(&db.a, &used).await.unwrap());
        assert!(database_exists(&db.a, &used).await);
        assert!(!drop_database_if_unused(&db.a, &installation).await.unwrap());
        assert!(database_exists(&db.a, &installation).await);
    })
    .await
    .expect("the database test exceeded its bound");
}

/// A database name no other test uses.
fn environment_name() -> DatabaseName {
    DatabaseName::parse(format!("env_{}", uuid::Uuid::new_v4().simple())).unwrap()
}

/// The server's process: a pool on its registration's login and its kernel instance.
async fn process(registration: &Registration, server: &str) -> (PlatformStore, Instance) {
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
    let instance = Instance::start(&store, &ServerSlug::parse(server).unwrap())
        .await
        .unwrap();
    (store, instance)
}

/// An owner pool on another database of the fixture's cluster.
async fn owner_store(db: &TestDb, database: &DatabaseName) -> PlatformStore {
    let config = db.config();
    let mut url = config.url().clone();
    url.set_path(database.as_str());
    PlatformStore::connect(
        StoreConfig::builder(url.as_str(), config.password().clone())
            .username(config.username())
            .max_connections(2)
            .build()
            .unwrap(),
    )
    .await
    .unwrap()
}

/// Start `relay_later` through the Hub and return the Task's public identifier.
async fn later(client: &Client) -> String {
    match client
        .call_tool_once(
            CallToolRequestParams::new("relay__relay_later").with_arguments(Default::default()),
        )
        .await
        .unwrap()
    {
        CallToolResponse::Task(created) => created.task.task_id,
        other => panic!("relay_later did not start a Task: {other:?}"),
    }
}

/// The result of the Task `id` once it completed.
async fn completed(client: &Client, id: &str) -> serde_json::Map<String, serde_json::Value> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match client
                .get_task(GetTaskParams::new(id))
                .await
                .unwrap()
                .task
                .payload
            {
                TaskPayload::Completed { result } => return result,
                TaskPayload::Failed { error } => panic!("relay_later failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    })
    .await
    .expect("relay_later finished")
}

/// The identifier the server issued for the Task behind a public identifier, and the
/// deployment its route names.
async fn source_task(db: &TestDb, id: &str) -> (String, DeploymentId) {
    let row = sqlx::query_file!("tests/data_environments/source_task.sql", id)
        .fetch_one(db.a.pool())
        .await
        .unwrap();
    (
        row.source_task_id,
        DeploymentId::try_from(row.deployment).unwrap(),
    )
}

/// The status of a Task in core's Task runtime of `store`'s database.
async fn task_status(store: &PlatformStore, id: &str) -> Option<String> {
    sqlx::query_file_scalar!(
        "tests/data_environments/task_status.sql",
        uuid::Uuid::parse_str(id).unwrap()
    )
    .fetch_optional(store.pool())
    .await
    .unwrap()
}

async fn schema_exists(store: &PlatformStore, schema: &str) -> bool {
    sqlx::query_file_scalar!("tests/data_environments/schema_exists.sql", schema)
        .fetch_one(store.pool())
        .await
        .unwrap()
}

async fn database_exists(store: &PlatformStore, database: &DatabaseName) -> bool {
    sqlx::query_file_scalar!(
        "tests/data_environments/database_exists.sql",
        database.as_str()
    )
    .fetch_one(store.pool())
    .await
    .unwrap()
}

fn branch_name(name: &str) -> veoveo_hub::execution::BranchName {
    veoveo_hub::execution::BranchName::parse(name).unwrap()
}
