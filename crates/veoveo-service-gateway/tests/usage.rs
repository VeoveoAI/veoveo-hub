//! The Gateway's daily usage views over core's usage ledger, through the Gateway binary:
//! the Gateway keeps the ledger's partitions three months ahead, a caller reads its own
//! totals page by page, an administrator the tenant's, a member asking for the tenant's
//! is refused and recorded, and a cursor continues only its own view.

#[path = "support/gateway.rs"]
mod gateway;

use std::time::Duration;

use chrono::{TimeDelta, Utc};
use gateway::Gateway;
use serde_json::{Value, json};
use veoveo_fixtures::store::TestDb;
use veoveo_types::{PrincipalId, TenantId, WorkContextId};
use veoveo_usage::UsageStore;
use veoveo_usage_contract::{
    ChargePeriod, Currency, Money, Quantity, ServiceCategory, UsageActor, UsageDraft, UsageKind,
    UsageKindDeclaration, UsagePhase, UsageSourceId, UsageUnit,
};

/// The principal, tenant and Work Context an access token names.
fn actor(token: &str) -> UsageActor {
    use base64::Engine as _;
    let claims: Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    UsageActor {
        principal: PrincipalId::parse(format!(
            "{}#{}",
            claims["iss"].as_str().unwrap(),
            claims["sub"].as_str().unwrap()
        ))
        .unwrap(),
        tenant: TenantId::parse(claims["https://veoveo.ai/claims/tenant"].as_str().unwrap())
            .unwrap(),
        work_context: WorkContextId::parse(
            claims["https://veoveo.ai/claims/work-context"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
    }
}

async fn daily(
    gateway: &Gateway,
    http: &reqwest::Client,
    token: &str,
    query: &[(&str, String)],
) -> (reqwest::StatusCode, Value) {
    let mut url = url::Url::parse(&format!("{}/admin/local/usage/daily", gateway.base)).unwrap();
    url.query_pairs_mut()
        .extend_pairs(query.iter().map(|(name, value)| (*name, value.as_str())));
    let response = http.get(url).bearer_auth(token).send().await.unwrap();
    let status = response.status();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn usage_views_read_the_ledger_for_the_caller_or_its_tenant() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("the usage qualification finished");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    // The administrator may make administrative reads; the member may not.
    let gateway = Gateway::start_with(
        directory.path(),
        &db,
        Duration::from_secs(5),
        &[],
        |config| {
            let issuer = config["authorizationServers"][0]["issuer"]
                .as_str()
                .unwrap()
                .to_owned();
            config["policies"][0]["rules"] = json!([{
                "id": "administrative-reads",
                "effect": "allow",
                "actions": ["admin_read"],
                "principalIds": [format!("{issuer}#acceptance-admin")]
            }]);
        },
    )
    .await;
    let http = reqwest::Client::new();
    let member_token = gateway.access_token(&http, gateway.caller("member")).await;
    let admin_token = gateway.access_token(&http, gateway.caller("admin")).await;
    let member = actor(&member_token);
    let admin = actor(&admin_token);

    // The Gateway keeps the ledger's monthly partitions three months ahead.
    let today = Utc::now().date_naive();
    let horizon = today
        .checked_add_months(chrono::Months::new(3))
        .unwrap()
        .format("record_%Y_%m")
        .to_string();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let prepared: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT FROM pg_class AS c JOIN pg_namespace AS n \
                 ON n.oid = c.relnamespace WHERE n.nspname = 'usage' AND c.relname = $1)",
            )
            .bind(&horizon)
            .fetch_one(db.a.pool())
            .await
            .unwrap();
            if prepared {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the Gateway prepared the usage partitions");

    // Two servers' kinds, written straight to the ledger as the installation owner.
    let ledger = UsageStore::new(db.a.clone());
    ledger
        .declare(&[
            UsageKindDeclaration {
                kind: UsageKind::parse("media.generation").unwrap(),
                service_category: ServiceCategory::AiAndMachineLearning,
                unit: UsageUnit::units(),
                monetary: true,
            },
            UsageKindDeclaration {
                kind: UsageKind::parse("time.lookups").unwrap(),
                service_category: ServiceCategory::Analytics,
                unit: UsageUnit::requests(),
                monetary: false,
            },
        ])
        .await
        .unwrap();
    let usd = |amount: &str| Money {
        amount: veoveo_usage_contract::Amount::parse(amount).unwrap(),
        currency: Currency::parse("USD").unwrap(),
    };
    let now = Utc::now();
    let drafts = [
        UsageDraft::builder(
            UsageKind::parse("media.generation").unwrap(),
            UsagePhase::Estimate,
            member.clone(),
            Quantity::whole(1),
            UsageSourceId::parse("member-estimate").unwrap(),
            ChargePeriod::at(now),
        )
        .money(usd("0.04"))
        .build()
        .unwrap(),
        UsageDraft::builder(
            UsageKind::parse("media.generation").unwrap(),
            UsagePhase::Actual,
            member.clone(),
            Quantity::whole(1),
            UsageSourceId::parse("member-actual").unwrap(),
            ChargePeriod::at(now),
        )
        .money(usd("0.05"))
        .build()
        .unwrap(),
        UsageDraft::builder(
            UsageKind::parse("time.lookups").unwrap(),
            UsagePhase::Actual,
            admin.clone(),
            Quantity::whole(5),
            UsageSourceId::parse("admin-lookups").unwrap(),
            ChargePeriod::at(now),
        )
        .build()
        .unwrap(),
    ];
    ledger.append(&drafts).await.unwrap();

    let range = |scope: &str| {
        vec![
            ("scope", scope.to_owned()),
            ("from", today.to_string()),
            ("to", (today + TimeDelta::days(1)).to_string()),
        ]
    };

    // The member reads its own totals, one row per page.
    let mut query = range("own");
    query.push(("limit", "1".into()));
    let (status, first) = daily(&gateway, &http, &member_token, &query).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{first}");
    assert_eq!(first["rows"].as_array().unwrap().len(), 1);
    let cursor = first["nextCursor"].as_str().unwrap().to_owned();
    let mut next = query.clone();
    next.push(("cursor", cursor.clone()));
    let (status, second) = daily(&gateway, &http, &member_token, &next).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{second}");
    assert_eq!(second["rows"].as_array().unwrap().len(), 1);
    assert!(second.get("nextCursor").is_none(), "{second}");
    let own: Vec<&Value> = first["rows"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["rows"].as_array().unwrap())
        .collect();
    for row in &own {
        assert_eq!(row["principal"], member.principal.as_str());
        assert_eq!(row["server"], "media");
        assert_eq!(row["money"]["currency"], "USD");
    }
    let phases: Vec<&str> = own
        .iter()
        .map(|row| row["phase"].as_str().unwrap())
        .collect();
    assert_eq!(phases, ["actual", "estimate"]);

    // The member may not read the tenant's totals; the refusal is in the audit log.
    let (status, _) = daily(&gateway, &http, &member_token, &range("tenant")).await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
    let refused: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.record WHERE class = 'gateway.admin_admission' \
         AND outcome = 'denied' AND draft #>> '{detail,operation}' = 'usage_read'",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(refused, 1);

    // The administrator reads every principal's totals across servers, without money
    // for usage that carries none.
    let (status, tenant) = daily(&gateway, &http, &admin_token, &range("tenant")).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{tenant}");
    let rows = tenant["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{tenant}");
    let lookups = rows.iter().find(|row| row["server"] == "time").unwrap();
    assert_eq!(lookups["principal"], admin.principal.as_str());
    assert_eq!(lookups["quantity"], "5");
    assert!(lookups.get("money").is_none(), "{lookups}");

    // A cursor continues only the view it came from, and a range stays bounded.
    let mut foreign = range("tenant");
    foreign.push(("cursor", cursor));
    let (status, _) = daily(&gateway, &http, &admin_token, &foreign).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    let backwards = vec![("from", today.to_string()), ("to", today.to_string())];
    let (status, _) = daily(&gateway, &http, &member_token, &backwards).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    drop(gateway);
}
