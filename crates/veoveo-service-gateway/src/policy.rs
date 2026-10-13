//! Authorization decisions over the Gateway's catalog: installation roles and grants
//! over registered servers' declared permissions, policy rules and session freshness.
//! Authentication and durable freshness are callers' responsibilities.
mod catalog;
mod evaluation;
mod grants;
pub mod session;

pub use catalog::{PolicyCatalog, PolicyCatalogView};
pub use evaluation::{
    PolicyRequest, RuleMatchDetail, RuleOutcome, assemble_rule_outcome, decide, intersects,
    mcp_method_name, principal_rule_conditions, remember_strongest_missing_requirement,
    resource_scheme_from_uri, strongest_missing_rule_detail,
};
pub use grants::{granted_permissions, held_roles};

use crate::GatewayCatalog;
use crate::control_plane::{
    DataLabelDefinition, GatewayProfile, InstallationAuthorization, PolicyDecision, PolicySet,
    TenantDefinition,
};
pub(crate) use evaluation::resource_scheme_from_uri as resource_scheme;
use veoveo_sdk_rust::registration::AuthorizationDeclaration;
use veoveo_sdk_rust::{GatewayProfileId, ServerSlug};
use veoveo_types::{DataLabelId, PolicyVersion, TenantId};

impl PolicyCatalogView for GatewayCatalog {
    fn registry(&self) -> &crate::contract::CatalogRegistry {
        self.registry()
    }
    fn sections(&self) -> &crate::contract::AdmittedCatalogSections {
        self.sections()
    }
    fn profile(&self, id: &GatewayProfileId) -> Option<&GatewayProfile> {
        self.profile(id)
    }
    fn policy(&self, id: &PolicyVersion) -> Option<&PolicySet> {
        self.policy(id)
    }
    fn data_label(&self, id: &DataLabelId) -> Option<&DataLabelDefinition> {
        self.data_label(id)
    }
    fn tenant(&self, id: &TenantId) -> Option<&TenantDefinition> {
        self.tenant(id)
    }
    fn installation_authorization(&self) -> &InstallationAuthorization {
        &self.control_plane().authorization
    }
    fn server_authorization(&self, server: &ServerSlug) -> Option<&AuthorizationDeclaration> {
        self.server(server)
            .map(|server| server.authorization.as_ref())
    }
}
impl GatewayCatalog {
    pub fn decide(&self, request: PolicyRequest<'_>) -> PolicyDecision {
        let _timing = crate::request_observation::StageTimer::start(
            crate::request_observation::RequestStage::Policy,
        );
        decide(self, request)
    }
}
