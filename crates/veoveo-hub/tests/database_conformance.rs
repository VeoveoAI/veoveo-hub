//! The Hub's own queries against the database rules of core's `docs/DATABASE.md`:
//! tenant predicates, row bounds and index use on the schemas they read and write, and
//! the schema rules of those schemas. The fixture applies the Hub's own migrations as
//! the installation owner after the kernel migrations, as the Hub does at startup,
//! and plans its queries as that owner.

use veoveo_fixtures::database::Conformance;

#[tokio::test]
async fn hub_queries_follow_the_database_rules() {
    Conformance::new()
        .owner("hub", veoveo_hub::store::migrator())
        .schema("identity")
        .schema("kernel")
        .all_tenants(
            "hub.refresh_family",
            "a refresh family is addressed by its identifier or a token digest, and its \
             tenant travels in the tokens it issues",
        )
        .all_tenants(
            "hub.control_revision",
            "a control-plane revision configures the whole installation; its tenant names \
             the one tenant a revision may belong to",
        )
        .queries_from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.sqlx"))
        .sources(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .check()
        .await;
}
