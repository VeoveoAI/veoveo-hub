//! Per-user selection against the real Hub binary and SDK fixture. Two instances of
//! Ledger register with their instance tokens: the base branch `dev` serves the
//! fixture's own descriptors and branch `dev/jose/x` those of synthetic revision A. The
//! local installation's `member` caller is user A and its `admin` caller user B. No
//! sandbox, Lore branch or application inference is involved; the instances' branches
//! are what the Hub recorded when it created them.
#[path = "support/hub.rs"]
mod hub;
#[allow(dead_code, reason = "Selection uses one revision of the fixture")]
#[path = "support/ledger_revisions.rs"]
mod revisions;
#[path = "support/servers.rs"]
mod servers;

use std::time::Duration;

use hub::Hub;
use revisions::FirstRevision;
use rmcp::{
    RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientConfig, ClientRequest, GetTaskParams,
        ListToolsRequest, ServerNotification, ServerResult, SubscriptionFilter, TaskPayload,
    },
    service::{RunningService, Subscription},
};
use serde_json::{Value, json};
use servers::{Served, decoded_claims, hosted, mcp_client, serve_instance};
use veoveo_fixtures::store::TestDb;
use veoveo_hub::{
    data_environment::DatabaseName, execution::BranchName, registration::create_instance,
};
use veoveo_hub_sdk::hosting::{DomainServer, Hosted};
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};

/// How long one of these tests may take, Hub start included.
const BOUND: Duration = Duration::from_secs(120);
const BASE: &str = "dev";
const FEATURE: &str = "dev/jose/x";
const BASE_DESCRIPTION: &str = "Inspect caller identity";
const FEATURE_DESCRIPTION: &str = "Inspect identity from source revision A";
const INVOCATION: &str = "https://veoveo.ai/claims/invocation";

type Client = RunningService<RoleClient, ClientConfig>;

/// The Hub with its base branch set, and one instance of Ledger on each branch.
struct Fixture {
    hub: Hub,
    http: reqwest::Client,
    base: Served,
    feature: Served,
    _db: TestDb,
    _directory: tempfile::TempDir,
}

impl Fixture {
    async fn start(feature_tasks: LedgerTasks) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        let db = TestDb::new().await;
        let hub = Hub::start_with(
            directory.path(),
            &db,
            Duration::from_secs(60),
            &[],
            |config| config["execution"]["baseBranch"] = json!(BASE),
        )
        .await;
        let http = reqwest::Client::new();
        let instance = create_instance(&db.a, &installation(&db.a), &branch(BASE))
            .await
            .unwrap();
        let base = serve_instance(
            &hub,
            &http,
            "ledger",
            ledger::authorization().unwrap(),
            &instance,
            async |registration, _, _| {
                hosted::<LedgerDomain>(registration, &http)
                    .handler(|| ledger::hosted(LedgerTasks::default()))
                    .build()
            },
        )
        .await;
        let instance = create_instance(&db.a, &installation(&db.a), &branch(FEATURE))
            .await
            .unwrap();
        let feature = serve_instance(
            &hub,
            &http,
            "ledger",
            FirstRevision::routes().authorization(),
            &instance,
            async |registration, _, _| {
                hosted::<FirstRevision>(registration, &http)
                    .handler(move || Hosted::new(FirstRevision).with_tasks(feature_tasks.clone()))
                    .build()
            },
        )
        .await;
        Self {
            hub,
            http,
            base,
            feature,
            _db: db,
            _directory: directory,
        }
    }

    async fn token(&self, user: &str) -> String {
        self.hub
            .access_token(&self.http, self.hub.caller(user))
            .await
    }

    async fn client(&self, user: &str) -> Client {
        mcp_client(&self.hub, self.token(user).await).await
    }

    /// `user` selects `branch` of Ledger, or clears its selection without one.
    async fn select(&self, user: &str, branch: Option<&str>) {
        let url = format!("{}/selection/local/ledger", self.hub.base);
        let request = match branch {
            Some(branch) => self.http.put(url).json(&json!({ "branch": branch })),
            None => self.http.delete(url),
        };
        let response = request
            .bearer_auth(self.token(user).await)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::NO_CONTENT,
            "{}",
            response.text().await.unwrap()
        );
    }
}

fn branch(name: &str) -> BranchName {
    BranchName::parse(name).unwrap()
}

/// The description of `ledger__inspect` as the Hub lists it now, if it lists it. The
/// request reaches the Hub instead of the client's private catalog cache.
async fn inspect_description(client: &Client) -> Option<String> {
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
    let mut inspect = listed
        .tools
        .into_iter()
        .filter(|tool| tool.name == "ledger__inspect");
    let tool = inspect.next()?;
    assert!(inspect.next().is_none(), "one instance serves one server");
    tool.description.map(|description| description.into_owned())
}

/// The deployment and the captured composition the last forwarded token `served`
/// received names.
fn capture(served: &Served) -> (Value, Value) {
    let invocation = &decoded_claims(&served.bearers.last())[INVOCATION];
    (
        invocation["deployment"].clone(),
        invocation["request_context"]["execution"].clone(),
    )
}

async fn listen(client: &Client) -> Subscription {
    client
        .listen(SubscriptionFilter::builder().tools_list_changed().build())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_selection_serves_its_branch_to_its_principal_only() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(LedgerTasks::default()).await;
        let (a, b) = (
            fixture.client("member").await,
            fixture.client("admin").await,
        );
        assert_eq!(inspect_description(&a).await.unwrap(), BASE_DESCRIPTION);

        fixture.select("member", Some(FEATURE)).await;
        assert_eq!(inspect_description(&a).await.unwrap(), FEATURE_DESCRIPTION);
        assert_eq!(
            inspect_description(&b).await.unwrap(),
            BASE_DESCRIPTION,
            "a principal without a selection uses the base branch"
        );
        let call = CallToolRequestParams::new("ledger__inspect").with_arguments(Default::default());
        assert_ne!(
            a.call_tool(call.clone()).await.unwrap().is_error,
            Some(true)
        );
        assert_ne!(b.call_tool(call).await.unwrap().is_error, Some(true));
        let (a_target, a_composition) = capture(&fixture.feature);
        let (b_target, b_composition) = capture(&fixture.base);
        assert_eq!(
            a_target,
            json!(fixture.feature.metadata.registration.deployment)
        );
        assert_eq!(
            b_target,
            json!(fixture.base.metadata.registration.deployment)
        );
        assert_ne!(a_composition, b_composition);

        fixture.select("member", None).await;
        assert_eq!(inspect_description(&a).await.unwrap(), BASE_DESCRIPTION);
        a.cancel().await.unwrap();
        b.cancel().await.unwrap();
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn a_selected_branch_without_an_instance_is_unavailable_with_its_cause() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(LedgerTasks::default()).await;
        let (a, b) = (fixture.client("member").await, fixture.client("admin").await);
        fixture.select("member", Some("dev/maria/y")).await;
        assert_eq!(
            inspect_description(&a).await,
            None,
            "the base branch does not stand in for the selected one"
        );
        let refused = a
            .call_tool(CallToolRequestParams::new("ledger__inspect").with_arguments(Default::default()))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains(
                "server `ledger` is unavailable: the selected branch `dev/maria/y` has no registered instance"
            ),
            "{refused}"
        );
        assert_eq!(inspect_description(&b).await.unwrap(), BASE_DESCRIPTION);
        a.cancel().await.unwrap();
        b.cancel().await.unwrap();
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn a_selection_change_hints_only_the_principals_listens() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(LedgerTasks::default()).await;
        let (a, b) = (
            fixture.client("member").await,
            fixture.client("admin").await,
        );
        // Settle discovery first, so no listen sees a late catalog instead of a hint.
        assert_eq!(inspect_description(&a).await.unwrap(), BASE_DESCRIPTION);
        assert_eq!(inspect_description(&b).await.unwrap(), BASE_DESCRIPTION);
        let (mut a_listen, mut b_listen) = (listen(&a).await, listen(&b).await);

        fixture.select("member", Some(FEATURE)).await;
        assert!(matches!(
            a_listen.next().await.unwrap(),
            Some(ServerNotification::ToolListChangedNotification(_))
        ));
        assert!(
            a_listen.next().await.unwrap().is_none(),
            "the hint completes the listen, so the client listens again"
        );
        // B's listen woke with A's: had it taken the change for its own, it would hold
        // a notification or have ended by now.
        assert!(
            tokio::time::timeout(Duration::from_secs(1), b_listen.next())
                .await
                .is_err(),
            "another principal's selection change reached B"
        );
        assert_eq!(inspect_description(&b).await.unwrap(), BASE_DESCRIPTION);

        fixture.select("admin", Some(FEATURE)).await;
        assert!(matches!(
            b_listen.next().await.unwrap(),
            Some(ServerNotification::ToolListChangedNotification(_))
        ));
        assert_eq!(inspect_description(&b).await.unwrap(), FEATURE_DESCRIPTION);
        a.cancel().await.unwrap();
        b.cancel().await.unwrap();
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn a_task_keeps_the_instance_it_started_on() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(LedgerTasks::new(Duration::from_secs(5))).await;
        let a = fixture.client("member").await;
        fixture.select("member", Some(FEATURE)).await;
        let CallToolResponse::Task(created) = a
            .call_tool_once(
                CallToolRequestParams::new("ledger__tally")
                    .with_arguments(json!({ "entries": 7 }).as_object().unwrap().clone()),
            )
            .await
            .unwrap()
        else {
            panic!("the feature instance did not create a Task");
        };
        let task = created.task.task_id;
        let started = capture(&fixture.feature);
        assert_eq!(
            started.0,
            json!(fixture.feature.metadata.registration.deployment)
        );

        fixture.select("member", None).await;
        assert_eq!(inspect_description(&a).await.unwrap(), BASE_DESCRIPTION);
        let result = loop {
            match a
                .get_task(GetTaskParams::new(&task))
                .await
                .unwrap()
                .task
                .payload
            {
                TaskPayload::Completed { result } => break result,
                TaskPayload::Failed { error } => panic!("the Task failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        };
        assert_eq!(result["structuredContent"]["entries"], 7);
        // Only the feature instance holds the Task, and every read of it reached that
        // instance under the composition the Task started with.
        assert_eq!(capture(&fixture.feature), started);
        let call = CallToolRequestParams::new("ledger__inspect").with_arguments(Default::default());
        assert_ne!(a.call_tool(call).await.unwrap().is_error, Some(true));
        assert_ne!(
            capture(&fixture.base).1,
            started.1,
            "the cleared selection routes new calls under another composition"
        );
        a.cancel().await.unwrap();
    })
    .await
    .expect("the test exceeded its bound");
}

/// The installation database, where these instances register.
fn installation(store: &veoveo_store::PlatformStore) -> DatabaseName {
    DatabaseName::installation(store).unwrap()
}
