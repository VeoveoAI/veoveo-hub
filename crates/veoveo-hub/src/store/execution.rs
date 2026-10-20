//! Immutable composition persistence. Authority is resolved against current control state.
use anyhow::{Context, Result, ensure};
use veoveo_hub_sdk::{CompositionRevision, ResolvedComposition};

impl crate::store::HubStore {
    /// Publication admits references, never transient target health. These retained
    /// records are immutable; withdrawing a live placement does not erase provenance.
    pub async fn check_execution_configuration(
        &self,
        configuration: Option<&crate::execution::ExecutionConfiguration>,
    ) -> Result<()> {
        let Some(configuration) = configuration else {
            return Ok(());
        };
        configuration.validate()?;
        for record in configuration.environments.values() {
            if let Some(crate::execution::EnvironmentBase::Pin {
                environment,
                revision,
            }) = &record.base
            {
                let composition = self
                    .composition(revision)
                    .await?
                    .context("pinned execution composition is not retained")?;
                ensure!(
                    composition.context().environment == *environment,
                    "pinned composition belongs to another environment"
                );
            }
            for (server, selection) in &record.components {
                if let veoveo_hub_sdk::ResolvedComponent::Deployment { deployment } = selection {
                    let exists = sqlx::query_file_scalar!(
                        "src/store/execution/deployment.sql",
                        deployment.as_uuid(),
                        server.as_str()
                    )
                    .fetch_one(self.pool())
                    .await?;
                    ensure!(exists, "selected deployment has not been admitted");
                }
            }
        }
        Ok(())
    }

    pub async fn retain_composition(&self, composition: &ResolvedComposition) -> Result<()> {
        let canonical = composition.canonical_bytes();
        let environment = composition.context().environment.as_uuid();
        sqlx::query_file!(
            "src/store/execution/retain.sql",
            composition.revision().digest().as_str(),
            environment,
            &canonical,
        )
        .execute(self.pool())
        .await
        .context("retain execution composition")?;
        let retained = self
            .composition(composition.revision())
            .await?
            .context("retained execution composition is missing")?;
        ensure!(
            retained == *composition,
            "execution composition digest conflicts with retained record"
        );
        Ok(())
    }

    pub async fn composition(
        &self,
        revision: &CompositionRevision,
    ) -> Result<Option<ResolvedComposition>> {
        let bytes =
            sqlx::query_file_scalar!("src/store/execution/read.sql", revision.digest().as_str(),)
                .fetch_optional(self.pool())
                .await
                .context("read retained execution composition")?;
        bytes
            .map(|bytes| {
                let composition = ResolvedComposition::from_canonical_bytes(&bytes)
                    .context("stored execution composition is not canonical")?;
                ensure!(
                    composition.revision() == revision,
                    "stored execution composition revision differs"
                );
                Ok(composition)
            })
            .transpose()
    }
}
