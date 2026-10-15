//! The Gateway's own queries against the database rules of core's `docs/DATABASE.md`:
//! tenant predicates, row bounds and index use on the schemas they read and write, and
//! the schema rules of those schemas. The fixture applies the Gateway's own migrations as
//! the installation owner after the kernel migrations, as the Gateway does at startup,
//! and plans its queries as that owner.

use veoveo_fixtures::database::Conformance;

#[tokio::test]
async fn gateway_queries_follow_the_database_rules() {
    Conformance::new()
        .owner("gateway", veoveo_service_gateway::store::migrator())
        .schema("identity")
        .schema("kernel")
        .all_tenants(
            "gateway.refresh_family",
            "a refresh family is addressed by its identifier or a token digest, and its \
             tenant travels in the tokens it issues",
        )
        .all_tenants(
            "gateway.control_revision",
            "a control-plane revision configures the whole installation; its tenant names \
             the one tenant a revision may belong to",
        )
        .queries_from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.sqlx"))
        .sources(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .check()
        .await;
}
