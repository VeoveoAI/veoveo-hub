//! An OAuth client of the Hub's own whose secret the OpenShell gateway holds in refresh
//! custody. The gateway refreshes one provider credential with it through its
//! `oauth2-client-credentials` strategy, posting the secret to the Hub's token endpoint;
//! the Hub keeps only the secret's digest, which tells the endpoint whom the client acts
//! for. The secret never enters a sandbox. A factory instance and a Computer each have
//! one, for their Lore token.
use std::collections::HashMap;

use anyhow::Result;
use openshell_sdk::raw::proto as p;
use secrecy::{ExposeSecret, SecretString};
use veoveo_types::Sha256Digest;

use crate::registration;

pub(crate) struct CustodyClient {
    id: String,
    secret: SecretString,
}

impl CustodyClient {
    /// The client `id` with a new secret.
    pub(crate) fn new(id: String) -> Result<Self> {
        Ok(Self {
            id,
            secret: registration::random_token()?,
        })
    }

    /// What the Hub keeps of the secret.
    pub(crate) fn digest(&self) -> Sha256Digest {
        registration::digest(self.secret.expose_secret())
    }

    /// The request that gives the gateway the client as the refresh of `credential_key`
    /// of `provider`.
    pub(crate) fn refresh(
        &self,
        workspace_scope: Option<p::WorkspaceSelector>,
        provider: String,
        credential_key: String,
    ) -> p::ConfigureProviderRefreshRequest {
        p::ConfigureProviderRefreshRequest {
            workspace_scope,
            provider,
            credential_key,
            strategy: p::ProviderCredentialRefreshStrategy::Oauth2ClientCredentials as i32,
            material: HashMap::from([
                ("client_id".into(), self.id.clone()),
                ("client_secret".into(), self.secret.expose_secret().into()),
            ]),
            secret_material_keys: vec!["client_secret".into()],
            ..Default::default()
        }
    }
}
