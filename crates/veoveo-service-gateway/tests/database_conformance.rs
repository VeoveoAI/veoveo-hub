//! The Gateway's own queries against the database rules of core's `docs/DATABASE.md`:
//! tenant predicates, row bounds and index use on the schemas they read and write, and
//! the schema rules of those schemas. The Gateway owns no server schema, so its queries
//! are planned as the installation owner it connects as.

use veoveo_test_fixtures::database::Conformance;

#[tokio::test]
#[ignore = "core's Conformance fixture applies only the kernel migrations; checking the \
            owner-applied `gateway` schema needs an owner-migration option in core"]
async fn gateway_queries_follow_the_database_rules() {
    Conformance::new()
        .schema("gateway")
        .schema("identity")
        .schema("kernel")
        .queries_from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.sqlx"))
        .sources(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .check()
        .await;
}
