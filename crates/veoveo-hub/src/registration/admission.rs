//! Trusted deployment admission is separate from namespace bootstrap and environment edits.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_hub_sdk::registration::{ClientMetadata, RegistrationAccessToken, StorageBinding};
use veoveo_hub_sdk::{DeploymentId, EnvironmentId};

use crate::control_plane::NativeUpstreamBinding;
use crate::execution::EnvironmentOwner;

/// Input to the trusted AdminWrite/native-provisioning path, never a source-manifest grant.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentAdmissionRequest {
    pub environment: EnvironmentId,
    pub metadata: ClientMetadata,
    pub storage: StorageBinding,
    pub runtime: NativeUpstreamBinding,
    pub expires_at: DateTime<Utc>,
}

/// A single-use RFC 7591 initial access credential, issued only after trusted admission.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAdmissionGrant {
    pub deployment: DeploymentId,
    pub initial_access_token: RegistrationAccessToken,
    pub expires_at: DateTime<Utc>,
}

pub(crate) struct StoredDeploymentAdmission {
    pub request: DeploymentAdmissionRequest,
    pub owner: EnvironmentOwner,
    pub token_digest: veoveo_types::Sha256Digest,
    pub consumed: bool,
}

pub(crate) enum RegistrationAuthority {
    Namespace,
    Deployment(Box<StoredDeploymentAdmission>),
}
