//! A fixture database with the Gateway's schema and audit declarations applied over the
//! kernel template of core's store fixture.
pub(crate) struct TestDb(veoveo_fixtures::store::TestDb);

impl TestDb {
    pub(crate) async fn new() -> Self {
        let db = veoveo_fixtures::store::TestDb::new().await;
        crate::store::GatewayStore::new(db.a.clone())
            .migrate()
            .await
            .expect("apply the Gateway migrations to the fixture database");
        veoveo_audit::AuditStore::new(db.a.clone(), crate::audit_registry())
            .declare()
            .await
            .expect("declare the Gateway's audit classes in the fixture database");
        Self(db)
    }
}

impl std::ops::Deref for TestDb {
    type Target = veoveo_fixtures::store::TestDb;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
