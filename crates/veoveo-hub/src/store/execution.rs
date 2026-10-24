//! Immutable composition persistence: each request's routing, retained for its Tasks and calls.
use anyhow::{Context, Result, ensure};
use veoveo_hub_sdk::{CompositionRevision, ResolvedComposition};

impl crate::store::HubStore {
    pub async fn retain_composition(&self, composition: &ResolvedComposition) -> Result<()> {
        let canonical = composition.canonical_bytes();
        sqlx::query_file!(
            "src/store/execution/retain.sql",
            composition.revision().digest().as_str(),
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
