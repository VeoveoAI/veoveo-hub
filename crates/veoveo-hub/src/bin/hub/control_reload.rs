//! Follow the authoritative active revision using the kernel's native notification
//! and reconnect reconciliation stream. No contribution-specific cache is retained.
use crate::runtime::{AdminState, build_http_client, replace_catalog, replace_http_client};
use futures::StreamExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use veoveo_hub::HubCatalog;
use veoveo_store::ChangeSignal;

pub(super) async fn reconcile(
    control_store: &veoveo_hub::HubControlStore,
    catalog_handle: &crate::runtime::SharedCatalog,
    http_handle: &crate::runtime::SharedHttpClient,
    publication: &tokio::sync::Mutex<()>,
    public: &str,
) -> anyhow::Result<()> {
    let _publication = publication.lock().await;
    let active = control_store
        .load_active_revision()
        .await?
        .ok_or_else(|| anyhow::anyhow!("active control-plane revision is missing"))?;
    if active.sha256 == hex::encode(catalog_handle.current().configuration_sha256()) {
        return Ok(());
    }
    let catalog =
        HubCatalog::from_control_plane(active.control_plane, catalog_handle.current().admission())?;
    catalog_handle.validate_replacement(&catalog)?;
    let http = build_http_client(&catalog)?;
    let catalog = catalog
        .prepare_runtime(Some(&catalog_handle.current()), public, http.clone())
        .await?;
    replace_http_client(http_handle, http);
    replace_catalog(catalog_handle, Arc::new(catalog))?;
    tracing::info!(revision = %active.revision_id, "adopted active control-plane revision");
    Ok(())
}
pub(super) fn spawn(state: AdminState, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = state
            .control_store
            .platform_store()
            .listen(vec![veoveo_hub::store::channel()]);
        loop {
            let signal = tokio::select! { () = stop.cancelled() => return, signal = changes.next() => signal };
            let reconcile_needed = match signal {
                Some(Ok(ChangeSignal::Changed(change))) => change.table == "control_active",
                Some(Ok(ChangeSignal::Reconcile)) => true,
                Some(Err(error)) => {
                    tracing::warn!(%error, "control-plane notifications interrupted; native listener will reconnect");
                    false
                }
                None => {
                    tracing::error!("control-plane notification stream ended");
                    stop.cancel();
                    return;
                }
            };
            if !reconcile_needed {
                continue;
            }
            if let Err(error) = reconcile(
                &state.control_store,
                &state.catalog,
                &state.http,
                &state.publication,
                &state.public_base_url,
            )
            .await
            {
                // A failed adoption must not leave an issuer serving indefinitely
                // with rights older than the revision it was told became active.
                tracing::error!(%error, "control-plane adoption failed; stopping hosted service");
                stop.cancel();
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use veoveo_hub::control_plane::HubControlPlane;
    use veoveo_hub::{HubCatalogHandle, HubControlStore, store::HubStore};

    #[tokio::test]
    async fn native_notifications_reconcile_two_handles_and_restart_ignores_stale_seed() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = veoveo_fixtures::store::TestDb::new().await;
            HubStore::new(db.a.clone()).migrate().await.unwrap();
            veoveo_audit::AuditStore::new(db.a.clone(), veoveo_hub::audit_registry())
                .declare()
                .await
                .unwrap();
            let store = HubControlStore::from_platform_store(
                db.a.clone(),
                crate::catalog_admission().unwrap(),
            );
            let mut plane: HubControlPlane =
                serde_json::from_str(include_str!("../../../testdata/hub.json")).unwrap();
            let initial = HubCatalog::from_control_plane(plane.clone(), store.admission()).unwrap();
            crate::startup::import(&store, &initial).await.unwrap();
            let left = HubCatalogHandle::new(Arc::new(initial.clone()));
            let right = HubCatalogHandle::new(Arc::new(initial));
            let http = Arc::new(parking_lot::RwLock::new(
                build_http_client(&right.current()).unwrap(),
            ));
            let publication = tokio::sync::Mutex::new(());
            let mut changes = db.b.listen(vec![veoveo_hub::store::channel()]);
            assert!(matches!(
                changes.next().await.unwrap().unwrap(),
                ChangeSignal::Reconcile
            ));
            plane.metadata = serde_json::json!({"published":"new"});
            let next = HubCatalog::from_control_plane(plane, store.admission()).unwrap();
            crate::startup::import(&store, &next).await.unwrap();
            left.replace(Arc::new(next)).unwrap();
            let signal = changes.next().await.unwrap().unwrap();
            assert!(
                matches!(signal, ChangeSignal::Changed(change) if change.table == "control_active")
            );
            assert_ne!(
                left.current().configuration_sha256(),
                right.current().configuration_sha256()
            );
            reconcile(&store, &right, &http, &publication, "https://hub.test")
                .await
                .unwrap();
            assert_eq!(
                left.current().configuration_sha256(),
                right.current().configuration_sha256()
            );
            let generation = right.snapshot().generation();
            reconcile(&store, &right, &http, &publication, "https://hub.test")
                .await
                .unwrap();
            assert_eq!(
                right.snapshot().generation(),
                generation,
                "same digest must not create a reload loop"
            );
            // No file read is needed once an authoritative revision exists.
            let restarted = crate::startup::load_catalog(
                &store,
                std::path::Path::new("/absent/stale-seed.json"),
            )
            .await
            .unwrap();
            assert_eq!(
                restarted.configuration_sha256(),
                left.current().configuration_sha256()
            );
        })
        .await
        .expect("control revision notification fixture exceeded 90 seconds");
    }
}
