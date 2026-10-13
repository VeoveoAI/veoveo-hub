//! The Gateway reads another owner's audit records with a registry built from the classes
//! stored in `audit.detail_type`, such as those a registered server declared through its
//! own login while the Gateway ran.
#[path = "support/database.rs"]
mod database;

use std::{sync::Arc, time::Duration};

use database::TestDb;
use veoveo_audit::AuditStore;
use veoveo_audit_contract::{
    AuditDetailType, AuditDraft, AuditOutcome, AuditPartition, AuditQuery, AuditReadScope,
    AuditReason, AuditRegistry, AuditRequest, AuditTarget, AuditTransactionWrite,
};
use veoveo_service_gateway::GatewayState;
use veoveo_types::ServerSlug;

/// A registered server's own detail, class `inventory.moved`.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
struct Moved {
    count: u32,
}

impl AuditDetailType for Moved {
    const CLASS: &'static str = "inventory.moved";
}

#[tokio::test]
async fn the_reader_reads_records_of_classes_a_server_declared() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = TestDb::new().await;
        let state = GatewayState::new(db.a.clone());
        let server = ServerSlug::parse("inventory").unwrap();
        let mut builder = AuditRegistry::builder();
        builder.declare::<Moved>().unwrap();
        let registry = Arc::new(builder.build());
        let inventory = AuditStore::new(db.server_store(&server).await, registry.clone());
        inventory.declare_as_server(&server).await.unwrap();

        let record = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Installation,
            registry.detail(&Moved { count: 1 }).unwrap(),
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .build()
        .unwrap();
        let mut transaction = inventory.platform().pool().begin().await.unwrap();
        AuditTransactionWrite::new(&registry, record.clone())
            .unwrap()
            .append_as_server(&mut transaction)
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        let scope = AuditReadScope::new(None, true);
        let query = AuditQuery::new(AuditPartition::Installation);
        let page = state
            .audit_reader()
            .await
            .unwrap()
            .page(&scope, &query)
            .await
            .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].draft, record);
        // The Gateway's own registry, which it writes with, does not know the class.
        assert!(state.audit_store().page(&scope, &query).await.is_err());
    })
    .await
    .expect("audit reader test exceeded two minutes");
}
