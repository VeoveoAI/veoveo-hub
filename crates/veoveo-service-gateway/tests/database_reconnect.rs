//! The Gateway binary through a lost database connection. Every session of the
//! Gateway's login ends at once, as a network failure between the Gateway and
//! PostgreSQL ends them. The Gateway keeps running and serving, its kernel instance
//! holds its lock again, and the server it routes stays registered and reachable.

#[path = "support/gateway.rs"]
mod gateway;
#[path = "support/servers.rs"]
mod servers;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use gateway::Gateway;
use rmcp::{
    RoleClient,
    model::{CallToolRequestParams, ClientConfig},
    service::RunningService,
};
use servers::{hosted, mcp_client, serve};
use veoveo_sdk_rust_fixture::ledger::{self, LedgerDomain, LedgerTasks};
use veoveo_service_gateway::registration::HOUSEKEEPING_INTERVAL;
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_test_fixtures::store::TestDb;
use veoveo_types::ServerSlug;

/// The grace period the Gateway gives a registration without a live process.
const GRACE: Duration = Duration::from_secs(5);
/// How long the Gateway's kernel instance may take to hold its lock again.
const RECOVERY: Duration = Duration::from_secs(30);

#[tokio::test]
async fn the_gateway_keeps_serving_and_its_registrations_after_its_database_sessions_end() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("the reconnect qualification exceeded three minutes");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let mut gateway = Gateway::start(directory.path(), &db, GRACE).await;
    let http = reqwest::Client::new();

    // Ledger holds a kernel instance on the login its registration grants, which keeps
    // it registered, as a server process does.
    let slug = ServerSlug::parse("ledger").unwrap();
    let ledger_instance: Arc<Mutex<Option<Instance>>> = Arc::default();
    let ledger_tasks = LedgerTasks::default();
    let _ledger = serve(
        &gateway,
        &http,
        "ledger",
        ledger::authorization().unwrap(),
        async |registration, _, _| {
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
            let instance = Instance::start(&store, &slug).await.unwrap();
            *ledger_instance.lock().unwrap() = Some(instance);
            let tasks = ledger_tasks.clone();
            hosted::<LedgerDomain>(registration, &http)
                .handler(move || ledger::hosted(tasks.clone()))
                .build()
        },
    )
    .await;
    inspect(&member(&gateway, &http).await).await;
    assert_eq!(
        live_gateway_instances(&db.a).await.len(),
        1,
        "the Gateway holds one kernel instance"
    );

    // Every session of the installation owner's login ends: the Gateway's pools, its
    // listener and its instance lock, and the fixture's own owner pools.
    assert!(db.end_sessions(&db.a).await > 0, "the Gateway had sessions");
    let owner = db.connect().await;
    let deadline = tokio::time::Instant::now() + RECOVERY;
    while live_gateway_instances(&owner).await.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the Gateway's kernel instance held no lock within {RECOVERY:?}:\n{}",
            gateway.log()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(gateway.running(), "the Gateway kept running");
    let ready = http
        .get(format!("{}/readyz", gateway.base))
        .send()
        .await
        .unwrap();
    assert_eq!(ready.status(), reqwest::StatusCode::OK);

    // A new token and a call through the Gateway write its replay and audit rows on new
    // connections, and the route to Ledger is still there.
    inspect(&member(&gateway, &http).await).await;

    // Housekeeping keeps running and finds Ledger's process alive: the registration
    // stays, without an orphan mark, past the grace period.
    tokio::time::sleep(GRACE + HOUSEKEEPING_INTERVAL * 2).await;
    assert_eq!(
        registrations(&owner).await,
        [("ledger".to_owned(), None)],
        "Ledger stays registered and alive"
    );
    inspect(&member(&gateway, &http).await).await;
    assert!(gateway.running(), "the Gateway kept running");
    assert_eq!(live_gateway_instances(&owner).await.len(), 1);
    drop(ledger_instance);
    owner.close().await;
}

/// An MCP client of the local profile for the acceptance member.
async fn member(
    gateway: &Gateway,
    http: &reqwest::Client,
) -> RunningService<RoleClient, ClientConfig> {
    mcp_client(
        gateway,
        gateway.access_token(http, gateway.caller("member")).await,
    )
    .await
}

/// Call Ledger's `inspect` through the Gateway and expect an answer.
async fn inspect(client: &RunningService<RoleClient, ClientConfig>) {
    let result = client
        .call_tool(CallToolRequestParams::new("ledger__inspect"))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "inspect failed: {result:?}");
    assert!(result.structured_content.is_some());
}

/// The ids of the Gateway's kernel instances whose lock some session holds.
async fn live_gateway_instances(store: &PlatformStore) -> Vec<i32> {
    sqlx::query_file_scalar!("tests/database_reconnect/live_gateway_instances.sql")
        .fetch_all(store.pool())
        .await
        .unwrap()
}

/// Every registration's server and orphan mark.
async fn registrations(store: &PlatformStore) -> Vec<(String, Option<DateTime<Utc>>)> {
    sqlx::query_file!("tests/database_reconnect/registrations.sql")
        .fetch_all(store.pool())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.server, row.orphaned_at))
        .collect()
}
