use anyhow::{Context, Result, ensure};
use chrono::Utc;
use sqlx::{Connection as _, PgConnection, types::Json};
use veoveo_hub_sdk::{
    EnvironmentId,
    registration::{ClientMetadata, StorageBinding},
};
use veoveo_store::{TenantId as DatabaseTenantId, deterministic_tenant_id};
use veoveo_types::{PrincipalId, Sha256Digest, TenantId};

use super::super::admission::{DeploymentAdmissionRequest, StoredDeploymentAdmission};
use super::RegistrationStore;
use crate::{
    control_plane::{HubControlPlane, NativeUpstreamBinding},
    execution::EnvironmentOwner,
};

pub(super) fn current_owner(
    control: &HubControlPlane,
    request: &DeploymentAdmissionRequest,
) -> Result<EnvironmentOwner> {
    request
        .metadata
        .check()
        .context("invalid deployment declaration")?;
    let StorageBinding::SharedLogicalSchema { server } = &request.storage;
    ensure!(
        server == request.metadata.slug(),
        "deployment storage binding differs from logical server"
    );
    ensure!(
        request.runtime.endpoint == request.metadata.registration.endpoint,
        "deployment runtime differs from declared endpoint"
    );
    ensure!(
        control
            .native_upstreams
            .as_ref()
            .is_some_and(|native| native.bindings.contains(&request.runtime)),
        "deployment runtime is not currently admitted"
    );
    let execution = control
        .execution
        .as_ref()
        .context("execution is not configured")?;
    let environment = execution
        .environments
        .get(&request.environment)
        .context("deployment environment is not configured")?;
    Ok(environment.owner.clone())
}

#[derive(Clone, Copy)]
pub(in crate::registration) enum WorkerAdmission<'a> {
    Current(&'a crate::activation::AcceptedApply),
    Retained(&'a crate::activation::AcceptedApply),
}

impl RegistrationStore {
    pub(in crate::registration) async fn admit_deployment(
        &self,
        request: &DeploymentAdmissionRequest,
        digest: &Sha256Digest,
    ) -> Result<()> {
        let mut connection = self.store.pool().acquire().await?;
        Self::admit_deployment_on(&mut connection, request, digest).await
    }
    pub(in crate::registration) async fn admit_deployment_on(
        connection: &mut PgConnection,
        request: &DeploymentAdmissionRequest,
        digest: &Sha256Digest,
    ) -> Result<()> {
        Self::admit_deployment_guarded_on(connection, request, digest, None).await
    }
    pub(in crate::registration) async fn admit_deployment_guarded_on(
        connection: &mut PgConnection,
        request: &DeploymentAdmissionRequest,
        digest: &Sha256Digest,
        worker: Option<WorkerAdmission<'_>>,
    ) -> Result<()> {
        ensure!(
            request.expires_at > Utc::now(),
            "deployment admission must expire in the future"
        );
        let mut transaction = connection.begin().await?;
        let control = sqlx::query_file!("src/registration/store/admission_control.sql")
            .fetch_optional(&mut *transaction)
            .await?
            .context("no authoritative control revision")?
            .control_plane
            .0;
        if let Some(worker) = worker {
            let accepted = match &worker {
                WorkerAdmission::Current(accepted) | WorkerAdmission::Retained(accepted) => {
                    *accepted
                }
            };
            match worker {
                WorkerAdmission::Current(_) => {
                    crate::activation::ActivationStore::admit_worker_on(
                        &mut transaction,
                        &control,
                        accepted,
                    )
                    .await?
                }
                WorkerAdmission::Retained(_) => accepted.admit_retained(&control)?,
            }
            ensure!(
                request.environment == accepted.key.environment
                    && request.metadata.registration.deployment == accepted.deployment
                    && request.metadata.slug() == &accepted.key.component
                    && request.storage == accepted.runtime.storage,
                "retained admission identity differs from accepted worker"
            );
            if matches!(worker, WorkerAdmission::Retained(_)) {
                let tenant = deterministic_tenant_id(accepted.key.owner.tenant.as_str())?;
                ensure!(
                    sqlx::query_file!(
                        "src/registration/store/retained_task.sql",
                        tenant as DatabaseTenantId,
                        accepted.deployment.as_uuid(),
                        Utc::now()
                    )
                    .fetch_one(&mut *transaction)
                    .await?
                    .retained,
                    "worker has no unexpired canonical Task route"
                );
            }
        }
        let owner = current_owner(&control, request)?;
        let deployment = request.metadata.registration.deployment.as_uuid();
        let environment = request.environment.as_uuid();
        sqlx::query_file!(
            "src/registration/store/create_deployment.sql",
            deployment,
            request.metadata.slug().as_str(),
            Json(&request.metadata) as _,
            Json(&request.storage) as _,
            Utc::now(),
            environment,
            owner.tenant.as_str(),
            owner.principal.as_str(),
            Json(&request.runtime) as _,
        )
        .execute(&mut *transaction)
        .await?;
        let admitted = sqlx::query_file!("src/registration/store/deployment.sql", deployment)
            .fetch_one(&mut *transaction)
            .await?;
        ensure!(
            admitted.metadata.0 == request.metadata
                && admitted.storage.0 == request.storage
                && admitted.environment == Some(environment)
                && admitted.owner_tenant.as_deref() == Some(owner.tenant.as_str())
                && admitted.owner_principal.as_deref() == Some(owner.principal.as_str())
                && admitted.runtime.map(|runtime| runtime.0).as_ref() == Some(&request.runtime),
            "deployment admission cannot rebind an existing deployment"
        );
        let inserted = sqlx::query_file!(
            "src/registration/store/admit_deployment.sql",
            deployment,
            environment,
            owner.tenant.as_str(),
            owner.principal.as_str(),
            request.metadata.slug().as_str(),
            Json(&request.metadata) as _,
            Json(&request.storage) as _,
            Json(&request.runtime) as _,
            digest.as_str(),
            request.expires_at,
        )
        .execute(&mut *transaction)
        .await
        .context("admit deployment registration")?;
        ensure!(
            inserted.rows_affected() == 1,
            "deployment admission cannot rebind an existing deployment"
        );
        transaction.commit().await?;
        Ok(())
    }

    pub(in crate::registration) async fn deployment_admission(
        &self,
        digest: &Sha256Digest,
    ) -> Result<Option<StoredDeploymentAdmission>> {
        let row = sqlx::query_file!("src/registration/store/admission.sql", digest.as_str())
            .fetch_optional(self.store.pool())
            .await?;
        row.map(|row| {
            Ok(StoredDeploymentAdmission {
                owner: EnvironmentOwner {
                    tenant: TenantId::parse(row.owner_tenant)?,
                    principal: PrincipalId::parse(row.owner_principal)?,
                },
                request: DeploymentAdmissionRequest {
                    environment: EnvironmentId::parse(row.environment.to_string())?,
                    metadata: row.metadata.0,
                    storage: row.storage.0,
                    runtime: row.runtime.0,
                    expires_at: row.expires_at,
                },
                token_digest: digest.clone(),
                consumed: row.consumed_at.is_some(),
            })
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests;
