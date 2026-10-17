//! Trusted endpoint placement; registration still owns each service's logical URI.
//! Captured at Hub startup. Catalog reloads must retain this configuration exactly.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::registration::ServerEndpoint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeUpstreams {
    /// OpenShell's authenticated control-plane origin.
    pub gateway: url::Url,
    pub certificate_authority: PathBuf,
    /// Variable holding a native opaque credential reference. No issuer credentials.
    pub bearer_environment: String,
    pub bindings: Vec<NativeUpstreamBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeUpstreamBinding {
    pub endpoint: ServerEndpoint,
    pub workspace: String,
    pub sandbox: String,
    pub port: u16,
}
