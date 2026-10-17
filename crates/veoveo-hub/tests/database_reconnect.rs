//! The Hub binary through a lost database connection. Every session of the
//! Hub's login ends at once, as a network failure between the Hub and
//! PostgreSQL ends them. The Hub keeps running and serving, its kernel instance
//! holds its lock again, and the server it routes stays registered and reachable.

#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use hub::Hub;
use rmcp::{
    RoleClient,
    model::{CallToolRequestParams, ClientConfig},
    service::RunningService,
};
use servers::{hosted, mcp_client, serve};
use veoveo_fixtures::store::TestDb;
use veoveo_hub::registration::HOUSEKEEPING_INTERVAL;
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_types::ServerSlug;

/// The grace period the Hub gives a registration without a live process.
const GRACE: Duration = Duration::from_secs(5);
/// How long the Hub's kernel instance may take to hold its lock again.
const RECOVERY: Duration = Duration::from_secs(30);

#[tokio::test]
async fn the_hub_keeps_serving_and_its_registrations_after_its_database_sessions_end() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("the reconnect qualification exceeded three minutes");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let mut hub = Hub::start(directory.path(), &db, GRACE).await;
    let http = reqwest::Client::new();

    // Ledger holds a kernel instance on the login its registration grants, which keeps
    // it registered, as a server process does.
    let slug = ServerSlug::parse("ledger").unwrap();
    let ledger_instance: Arc<Mutex<Option<Instance>>> = Arc::default();
    let ledger_tasks = LedgerTasks::default();
    let _ledger = serve(
        &hub,
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
    inspect(&member(&hub, &http).await).await;
    assert_eq!(
        live_hub_instances(&db.a).await.len(),
        1,
        "the Hub holds one kernel instance"
    );

    // Every session of the installation owner's login ends: the Hub's pools, its
    // listener and its instance lock, and the fixture's own owner pools.
    assert!(db.end_sessions(&db.a).await > 0, "the Hub had sessions");
    let owner = db.connect().await;
    let deadline = tokio::time::Instant::now() + RECOVERY;
    while live_hub_instances(&owner).await.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the Hub's kernel instance held no lock within {RECOVERY:?}:\n{}",
            hub.log()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(hub.running(), "the Hub kept running");
    let ready = http
        .get(format!("{}/readyz", hub.base))
        .send()
        .await
        .unwrap();
    assert_eq!(ready.status(), reqwest::StatusCode::OK);

    // A new token and a call through the Hub write its replay and audit rows on new
    // connections, and the route to Ledger is still there.
    inspect(&member(&hub, &http).await).await;

    // Housekeeping keeps running and finds Ledger's process alive: the registration
    // stays, without an orphan mark, past the grace period.
    tokio::time::sleep(GRACE + HOUSEKEEPING_INTERVAL * 2).await;
    assert_eq!(
        registrations(&owner).await,
        [("ledger".to_owned(), None)],
        "Ledger stays registered and alive"
    );
    inspect(&member(&hub, &http).await).await;
    assert!(hub.running(), "the Hub kept running");
    assert_eq!(live_hub_instances(&owner).await.len(), 1);
    drop(ledger_instance);
    owner.close().await;
}

/// An MCP client of the local profile for the acceptance member.
async fn member(hub: &Hub, http: &reqwest::Client) -> RunningService<RoleClient, ClientConfig> {
    mcp_client(hub, hub.access_token(http, hub.caller("member")).await).await
}

/// Call Ledger's `inspect` through the Hub and expect an answer.
async fn inspect(client: &RunningService<RoleClient, ClientConfig>) {
    let result = client
        .call_tool(CallToolRequestParams::new("ledger__inspect"))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "inspect failed: {result:?}");
    assert!(result.structured_content.is_some());
}

/// The ids of the Hub's kernel instances whose lock some session holds.
async fn live_hub_instances(store: &PlatformStore) -> Vec<i32> {
    sqlx::query_file_scalar!("tests/database_reconnect/live_hub_instances.sql")
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
