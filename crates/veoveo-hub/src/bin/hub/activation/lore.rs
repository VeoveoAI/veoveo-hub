//! Native protocol and token-profile adapter. No retained bearer, role/group
//! reconstruction, custom Lore authorization, or source revision types in Core.
use super::{
    native::{Runtime, protocol},
    source::{ControlDocument, SourceAuthority, SourceRecord},
    types::*,
};
use crate::{
    contributions::lore::{Deployment, Lore},
    runtime::AdminState,
};
use anyhow::{Context, Result, ensure};
use lore_proto::lore::revision::v1::{
    BranchGetRequest, branch_get_request, revision_service_client::RevisionServiceClient,
};
use sqlx::{Connection as _, PgConnection, types::Json};
use tonic::{
    Request,
    transport::{Channel, ClientTlsConfig, Endpoint},
};
use veoveo_hub::{
    activation::{AcceptedApply, ActivationError, MaterializedSource},
    token_profile::{ProfileRequest, TokenContext},
};

pub(super) async fn credential(
    state: &AdminState,
    authority: &SourceAuthority,
    id: &crate::native_source_ids::DeploymentId,
    repository: &crate::native_source_ids::RepositoryId,
) -> Result<(Deployment, String)> {
    let current = state
        .control_store
        .load_active_revision()
        .await?
        .context("authoritative control missing")?;
    let catalog = veoveo_hub::HubCatalog::from_control_plane(
        current.control_plane,
        state.catalog.current().admission(),
    )?;
    credential_in(&catalog, authority, id, repository).await
}
pub(super) async fn credential_in(
    catalog: &veoveo_hub::HubCatalog,
    authority: &SourceAuthority,
    id: &crate::native_source_ids::DeploymentId,
    repository: &crate::native_source_ids::RepositoryId,
) -> Result<(Deployment, String)> {
    let section: Lore = serde_json::from_value(
        catalog
            .control_plane()
            .extensions
            .get("lore")
            .cloned()
            .context("native source contribution missing")?,
    )?;
    let deployment = section
        .deployments
        .into_iter()
        .find(|d| &d.id == id)
        .context("native source deployment withdrawn")?;
    let principal = authority.native_principal();
    let context = TokenContext::admit(
        catalog,
        &deployment.audience,
        &authority.client,
        &principal,
        &authority.work_context,
        &deployment.login_scopes,
        None,
    )?;
    let scope = deployment.scope_template.render(repository);
    let token = crate::contributions::bindings()?
        .profiles
        .issue(
            context,
            ProfileRequest::Exchange {
                resource: None,
                scope: Some(&scope),
            },
            None,
        )
        .await?
        .access_token;
    Ok((deployment, token))
}
pub(super) async fn connect(deployment: &Deployment) -> Result<Channel> {
    let mut address = deployment.endpoint.clone();
    address
        .set_scheme("https")
        .map_err(|()| anyhow::anyhow!("invalid native source endpoint"))?;
    Ok(Endpoint::from_shared(address.to_string())?
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(30))
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?)
}
pub(super) async fn subscribe(
    deployment: &Deployment,
    repository: &crate::native_source_ids::RepositoryId,
    token: &str,
) -> Result<tonic::Streaming<lore_proto::lore::notification::Event>> {
    use lore_proto::lore::notification::{
        SubscribeRequest, notification_service_client::NotificationServiceClient,
    };
    let mut client = NotificationServiceClient::new(connect(deployment).await?);
    let bytes = hex::decode(repository.as_str())?;
    let mut request = Request::new(SubscribeRequest {
        repository: bytes.clone().into(),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    request.metadata_mut().insert_bin(
        "lore-partition-bin",
        tonic::metadata::MetadataValue::from_bytes(&bytes),
    );
    request.metadata_mut().insert_bin(
        "urc-repository-id-bin",
        tonic::metadata::MetadataValue::from_bytes(&bytes),
    );
    // Native server registers its broadcast receiver before this response. Await
    // it BEFORE authoritative initial read; healthy-stream gaps still need polling.
    Ok(client.subscribe(request).await?.into_inner())
}
pub(super) async fn resolve(
    deployment: &Deployment,
    repository: &crate::native_source_ids::RepositoryId,
    branch: &crate::native_source_ids::BranchId,
    token: &str,
) -> Result<NativeRevision> {
    let mut client = RevisionServiceClient::new(connect(deployment).await?);
    let mut request = Request::new(BranchGetRequest {
        query: Some(branch_get_request::Query::Id(
            hex::decode(branch.as_str())?.into(),
        )),
    });
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    let repository_bytes = hex::decode(repository.as_str())?;
    request.metadata_mut().insert_bin(
        "lore-partition-bin",
        tonic::metadata::MetadataValue::from_bytes(&repository_bytes),
    );
    request.metadata_mut().insert_bin(
        "urc-repository-id-bin",
        tonic::metadata::MetadataValue::from_bytes(&repository_bytes),
    );
    let branch_record = client
        .branch_get(request)
        .await?
        .into_inner()
        .branch
        .context("native branch response missing")?;
    ensure!(
        hex::encode(&branch_record.id) == branch.as_str() && !branch_record.deleted,
        "native branch removed or response identity changed"
    );
    Ok(NativeRevision::parse(hex::encode(&branch_record.latest))?)
}
/// Bind the first native response only if the same source selector and admitted
/// desired generation remain current. Slow RPCs never hold these row locks.
async fn remember(
    state: &AdminState,
    connection: &mut PgConnection,
    accepted: &AcceptedApply,
    mut record: SourceRecord,
    revision: NativeRevision,
) -> Result<SourceRecord> {
    let mut tx = connection.begin().await?;
    let current = sqlx::query_file!("src/bin/hub/activation/source_control_lock.sql")
        .fetch_one(&mut *tx)
        .await?
        .control_plane
        .0;
    // Same lock order as reserve/publication: control, then owned head.
    let guarded = sqlx::query_file!(
        "src/bin/hub/activation/source_guard.sql",
        accepted.build.as_uuid()
    )
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        guarded.current && guarded.accepted.0 == *accepted,
        ActivationError::Superseded
    );
    accepted.admit(&current)?;
    let catalog =
        veoveo_hub::HubCatalog::from_control_plane(current, state.catalog.current().admission())?;
    let SourceSelector::Lore {
        deployment,
        repository,
        ..
    } = &record.component.source
    else {
        anyhow::bail!(ActivationError::InvalidDefinition)
    };
    // Re-admit the initiating client's native resource and explicit repository
    // assignment against exactly the locked control revision, without old roles.
    let _ = credential_in(&catalog, &record.authority, deployment, repository).await?;
    let stored = super::source::read_on(&mut tx, accepted.build).await?;
    ensure!(
        stored.component == record.component && stored.authority == record.authority,
        ActivationError::Superseded
    );
    if let Some(previous) = stored.resolved_revision {
        ensure!(previous == revision, ActivationError::SourceMismatch);
    }
    record.resolved_revision = Some(revision);
    sqlx::query_file!(
        "src/bin/hub/activation/source_resolve.sql",
        accepted.build.as_uuid(),
        Json(&record) as _
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(record)
}
pub(super) async fn materialize(
    state: &AdminState,
    connection: &mut PgConnection,
    accepted: &AcceptedApply,
    record: SourceRecord,
    runtime: &Runtime,
) -> Result<MaterializedSource> {
    let SourceSelector::Lore {
        deployment: id,
        repository,
        selection,
    } = &record.component.source
    else {
        anyhow::bail!(ActivationError::InvalidDefinition)
    };
    let repository = repository.clone();
    let (deployment, token) = credential(state, &record.authority, id, &repository).await?;
    let revision = match &record.resolved_revision {
        Some(revision) => revision.clone(),
        None => match selection {
            LoreSelection::Pin { revision } => revision.clone(),
            LoreSelection::Follow { branch } => {
                let _subscription = subscribe(&deployment, &repository, &token).await?;
                resolve(&deployment, &repository, branch, &token).await?
            }
        },
    };
    let mut remote = deployment.endpoint.clone();
    remote.set_path(&format!("/{repository}"));
    // Publication validates the response against CURRENT selector/head, so a
    // late native response cannot displace an explicit local apply or pin.
    let record = remember(state, connection, accepted, record, revision.clone()).await?;
    let (_, access_token) =
        credential(state, &record.authority, &deployment.id, &repository).await?;
    drop(token);
    runtime
        .invoke(
            "materialize-lore",
            &protocol::MaterializeLoreRequest {
                build: accepted.build,
                source: protocol::LoreSource {
                    remote,
                    repository,
                    revision,
                },
                access_token,
            },
        )
        .await
}
