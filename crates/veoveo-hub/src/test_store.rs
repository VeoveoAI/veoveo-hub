//! A fixture database with the Hub's schema and audit declarations applied over the
//! kernel template of core's store fixture.
pub(crate) struct TestDb(veoveo_fixtures::store::TestDb);

impl TestDb {
    pub(crate) async fn new() -> Self {
        let db = veoveo_fixtures::store::TestDb::new().await;
        crate::store::HubStore::new(db.a.clone())
            .migrate()
            .await
            .expect("apply the Hub migrations to the fixture database");
        veoveo_audit::AuditStore::new(db.a.clone(), crate::audit_registry())
            .declare()
            .await
            .expect("declare the Hub's audit classes in the fixture database");
        Self(db)
    }
}

impl std::ops::Deref for TestDb {
    type Target = veoveo_fixtures::store::TestDb;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
