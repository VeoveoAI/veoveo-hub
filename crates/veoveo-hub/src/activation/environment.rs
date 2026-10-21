use super::ActivationError;
use crate::{
    control_plane::HubControlPlane,
    execution::{EnvironmentBase, EnvironmentOwner, EnvironmentRecord},
};
use anyhow::{Result, ensure};
use sqlx::PgConnection;
use std::collections::BTreeSet;
use veoveo_hub_sdk::{EnvironmentId, ResolvedComponent, ServerSlug};

/// Narrow authenticated owner mutation. This cannot change trusted runtime eligibility,
/// another environment, registration authority, readers or installation defaults.
#[derive(Clone, Debug)]
pub struct EnvironmentPublication {
    pub owner: EnvironmentOwner,
    pub environment: EnvironmentId,
    pub display_name: String,
    pub base: Option<EnvironmentBase>,
    pub components: BTreeSet<ServerSlug>,
}
impl EnvironmentPublication {
    pub fn project(&self, current: &HubControlPlane) -> Result<HubControlPlane> {
        ensure!(
            !self.display_name.trim().is_empty() && self.display_name.len() <= 256,
            ActivationError::InvalidDefinition
        );
        let mut proposed = current.clone();
        let execution = proposed
            .execution
            .as_mut()
            .ok_or(ActivationError::UnknownEnvironment)?;
        let previous = execution.environments.get(&self.environment);
        if let Some(previous) = previous {
            ensure!(previous.owner == self.owner, ActivationError::Forbidden);
        }
        if let Some(base) = &self.base {
            let parent = execution
                .environments
                .get(&base.environment())
                .ok_or(ActivationError::UnknownEnvironment)?;
            ensure!(
                parent.allows(&self.owner.tenant, &self.owner.principal),
                ActivationError::Forbidden
            );
        }
        let record = EnvironmentRecord {
            owner: self.owner.clone(),
            display_name: self.display_name.clone(),
            base: self.base.clone(),
            readers: previous.map_or_else(Default::default, |p| p.readers.clone()),
            components: self
                .components
                .iter()
                .map(|component| {
                    let selection = previous
                        .and_then(|p| p.components.get(component))
                        .cloned()
                        .unwrap_or(ResolvedComponent::Unavailable {});
                    (component.clone(), selection)
                })
                .collect(),
        };
        execution.environments.insert(self.environment, record);
        execution.validate()?;
        Ok(proposed)
    }
    pub(crate) async fn publish_on(
        &self,
        connection: &mut PgConnection,
        current: &HubControlPlane,
        proposed: &HubControlPlane,
    ) -> Result<()> {
        ensure!(
            self.project(current)? == *proposed,
            ActivationError::InvalidDefinition
        );
        // Withdrawal invalidates pending work atomically with resuming inheritance.
        // Retained native workers/inputs are not erased or replayed on a new worker.
        let components = self
            .components
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect::<Vec<_>>();
        let mut after = String::new();
        loop {
            let page = sqlx::query_file!(
                "src/activation/withdraw_list.sql",
                self.owner.tenant.as_str(),
                self.owner.principal.as_str(),
                self.environment.as_uuid(),
                &after,
                &components,
                super::ACTIVATION_PAGE_SIZE
            )
            .fetch_all(&mut *connection)
            .await?;
            if page.is_empty() {
                break;
            }
            for row in page {
                sqlx::query_file!(
                    "src/activation/withdraw.sql",
                    self.owner.tenant.as_str(),
                    self.owner.principal.as_str(),
                    self.environment.as_uuid(),
                    &row.component
                )
                .execute(&mut *connection)
                .await?;
                after = row.component;
            }
        }
        Ok(())
    }
}

/// Closed typed guards only; ordinary administrative/contribution publication is unchanged.
pub(crate) enum PublicationGuard<'a> {
    Place(&'a super::ActivationPlacement),
    Select(&'a super::ActivationPublication),
    Environment(&'a EnvironmentPublication),
}
impl PublicationGuard<'_> {
    pub(crate) async fn publish_on(
        &self,
        connection: &mut PgConnection,
        current: &HubControlPlane,
        proposed: &HubControlPlane,
    ) -> Result<()> {
        match self {
            Self::Place(placement) => placement.place_on(connection, current, proposed).await,
            Self::Select(select) => select.select_on(connection, current, proposed).await,
            Self::Environment(environment) => {
                environment.publish_on(connection, current, proposed).await
            }
        }
    }
}
