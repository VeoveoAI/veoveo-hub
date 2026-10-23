//! A tool description a server changes while it runs reaches the Hub's clients, through
//! the real Hub binary and the SDK's Bulletin fixture, whose `read_bulletin` description
//! the test changes in process. A listening client is told that the tool list changed
//! and lists the new description; a client that only lists sees it on its next list,
//! because the Hub's discovery cache follows the server's changes. Ledger, beside it,
//! advertises no tool-list change and keeps being listed. The descriptions are synthetic
//! text; no Computer, OpenShell or model is involved.
#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::time::Duration;

use hub::Hub;
use rmcp::{
    RoleClient,
    model::{
        ClientConfig, ClientRequest, ListToolsRequest, ServerNotification, ServerResult,
        SubscriptionFilter,
    },
    service::{RunningService, Subscription},
};
use servers::{hosted, mcp_client, serve};
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk_fixture::{
    bulletin::{self, Bulletin, ReadBulletin},
    ledger::{self, LedgerDomain, LedgerTasks},
};

type Client = RunningService<RoleClient, ClientConfig>;

const BOUND: Duration = Duration::from_secs(120);
/// How long a change may take to reach the Hub before the test changes it again: a
/// change made before the Hub's subscription to the server listens is never told.
const SETTLE: Duration = Duration::from_millis(500);

/// The description of `tool` as the Hub lists it now. The request reaches the Hub
/// instead of the client's private catalog cache.
async fn description(client: &Client, tool: &str) -> Option<String> {
    let ServerResult::ListToolsResult(listed) = client
        .send_request(ClientRequest::ListToolsRequest(ListToolsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        }))
        .await
        .unwrap()
    else {
        panic!("tools/list returned another result");
    };
    listed
        .tools
        .into_iter()
        .find(|listed| listed.name == tool)
        .and_then(|listed| listed.description)
        .map(|description| description.into_owned())
}

async fn listen(client: &Client) -> Subscription {
    client
        .listen(SubscriptionFilter::builder().tools_list_changed().build())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_changed_tool_description_reaches_the_hubs_clients() {
    tokio::time::timeout(BOUND, async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        let db = TestDb::new().await;
        let hub = Hub::start(directory.path(), &db, Duration::from_secs(60)).await;
        let http = reqwest::Client::new();
        let _bulletin = serve(
            &hub,
            &http,
            "bulletin",
            bulletin::authorization(),
            async |registration, _, _| {
                hosted::<Bulletin>(registration, &http)
                    .handler(bulletin::hosted)
                    .build()
            },
        )
        .await;
        let _ledger = serve(
            &hub,
            &http,
            "ledger",
            ledger::authorization().unwrap(),
            async |registration, _, _| {
                hosted::<LedgerDomain>(registration, &http)
                    .handler(|| ledger::hosted(LedgerTasks::default()))
                    .build()
            },
        )
        .await;
        let token = |role: &'static str| hub.access_token(&http, hub.caller(role));
        let listener = mcp_client(&hub, token("member").await).await;
        let lister = mcp_client(&hub, token("admin").await).await;
        for client in [&listener, &lister] {
            assert_eq!(
                description(client, "bulletin__read_bulletin")
                    .await
                    .as_deref(),
                Some(bulletin::DESCRIPTION)
            );
            assert!(description(client, "ledger__inspect").await.is_some());
        }

        // A listening client is told, and lists the new description.
        let mut changes = listen(&listener).await;
        let mut round = 0;
        let told = loop {
            round += 1;
            assert!(round <= 20, "no tool-list change reached the client");
            let text = format!("Read the bulletin, edition {round}");
            assert!(bulletin::DESCRIPTIONS.set::<ReadBulletin>(text.clone()));
            match tokio::time::timeout(SETTLE, changes.next()).await {
                Ok(change) => {
                    assert!(matches!(
                        change.unwrap(),
                        Some(ServerNotification::ToolListChangedNotification(_))
                    ));
                    break text;
                }
                Err(_) => continue,
            }
        };
        assert_eq!(
            description(&listener, "bulletin__read_bulletin")
                .await
                .as_deref(),
            Some(told.as_str())
        );

        // A client that only lists sees the change on a later list: the Hub's discovery
        // cache follows the server.
        drop(changes);
        let mut round = 0;
        loop {
            round += 1;
            assert!(round <= 20, "the Hub kept listing an old description");
            let text = format!("Read the bulletin, reprint {round}");
            bulletin::DESCRIPTIONS.set::<ReadBulletin>(text.clone());
            tokio::time::sleep(SETTLE).await;
            if description(&lister, "bulletin__read_bulletin")
                .await
                .as_deref()
                == Some(text.as_str())
            {
                break;
            }
        }
        assert!(description(&lister, "ledger__inspect").await.is_some());
        listener.cancel().await.unwrap();
        lister.cancel().await.unwrap();
    })
    .await
    .expect("the test exceeded its bound");
}
