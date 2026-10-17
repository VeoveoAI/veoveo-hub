//! Explicit owner admission bound before any catalog is admitted or published.
use crate::control_plane::HubControlPlane;
use anyhow::{Context, Result, ensure};

#[derive(Debug, Clone, Default)]
pub struct HubCatalogAdmission {
    registry: Option<crate::contract::CatalogRegistry>,
}
impl HubCatalogAdmission {
    pub fn unbound() -> Self {
        Self::default()
    }
    pub fn bind(mut self, registry: crate::contract::CatalogRegistry) -> Result<Self> {
        ensure!(
            self.registry.is_none(),
            "hub catalog admission already bound"
        );
        self.registry = Some(registry);
        Ok(self)
    }
    pub fn ensure_bound(&self) -> Result<()> {
        self.registry().map(|_| ())
    }
    pub fn registry(&self) -> Result<&crate::contract::CatalogRegistry> {
        self.registry
            .as_ref()
            .context("hub catalog admission is unbound; bind the installation catalog registry")
    }
    pub fn validate(
        &self,
        control_plane: &HubControlPlane,
    ) -> Result<crate::contract::AdmittedCatalogSections> {
        Ok(control_plane.validate(self.registry()?)?)
    }
    pub(crate) fn same_binding(&self, other: &Self) -> bool {
        matches!((&self.registry,&other.registry),(Some(a),Some(b)) if a.same_binding(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HubCatalog, HubCatalogHandle};
    use std::sync::Arc;
    #[test]
    fn admission_is_required_unique_and_retained_on_reload() {
        let plane: HubControlPlane =
            serde_json::from_str(include_str!("../testdata/hub.json")).unwrap();
        assert!(
            HubCatalog::from_control_plane(plane.clone(), HubCatalogAdmission::unbound()).is_err()
        );
        let registry = crate::catalog_fixture::registry();
        let admission = HubCatalogAdmission::unbound()
            .bind(registry.clone())
            .unwrap();
        assert!(admission.clone().bind(registry).is_err());
        let catalog = HubCatalog::from_control_plane(plane.clone(), admission.clone()).unwrap();
        let handle = HubCatalogHandle::new(Arc::new(catalog));
        let mut changes = handle.subscribe();
        handle
            .replace(Arc::new(
                HubCatalog::from_control_plane(plane.clone(), admission).unwrap(),
            ))
            .unwrap();
        assert_eq!(handle.snapshot().generation(), 1);
        assert_eq!(*changes.borrow_and_update(), 1);
        let foreign = HubCatalogAdmission::unbound()
            .bind(crate::catalog_fixture::fresh_registry())
            .unwrap();
        assert!(
            handle
                .replace(Arc::new(
                    HubCatalog::from_control_plane(plane, foreign).unwrap()
                ))
                .is_err()
        );
        assert_eq!(handle.snapshot().generation(), 1);
        assert!(!changes.has_changed().unwrap());
    }
}
