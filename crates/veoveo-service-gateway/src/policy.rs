//! Gateway composition over the shared current-policy evaluator.
use crate::GatewayCatalog;
use veoveo_mcp_contract::{
    DataLabelDefinition, GatewayProfile, GatewayProfileId, InstallationAuthorization,
    PolicyDecision, PolicySet, ServerSlug, TenantDefinition,
};
use veoveo_mcp_ext_registration::AuthorizationDeclaration;
pub(crate) use veoveo_policy::resource_scheme_from_uri as resource_scheme;
pub use veoveo_policy::{PolicyRequest, mcp_method_name, resource_scheme_from_uri};
use veoveo_types::{DataLabelId, PolicyVersion, TenantId};

impl veoveo_policy::PolicyCatalogView for GatewayCatalog {
    fn registry(&self) -> &veoveo_gateway_contract::CatalogRegistry {
        self.registry()
    }
    fn sections(&self) -> &veoveo_gateway_contract::AdmittedCatalogSections {
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
        veoveo_policy::decide(self, request)
    }
}
