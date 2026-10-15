//! `lore-tokens`: keeps each environment's Lore token file current. For every Lore
//! client it requests a client-credentials token with the client's own key and replaces
//! the environment's token file, owned by root and readable only by the group the
//! setgid Lore client runs as. The environments never hold the keys.

use std::{
    io::Write as _,
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use serde::Deserialize;
use veoveo_oauth::{CLIENT_ASSERTION_TYPE, ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_sdk_rust::OAuthClientId;
use zeroize::Zeroizing;

/// How long a failed round waits before it tries again.
const RETRY: Duration = Duration::from_secs(15);
/// The longest one token request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// One Lore client and the token file it keeps current.
#[derive(Debug, Clone)]
pub(super) struct LoreTokenTarget {
    pub client: OAuthClientId,
    pub file: PathBuf,
}

impl std::str::FromStr for LoreTokenTarget {
    type Err = anyhow::Error;

    /// `<client-id>=<token file>`.
    fn from_str(value: &str) -> Result<Self> {
        let (client, file) = value
            .split_once('=')
            .ok_or_else(|| anyhow!("expected <client-id>=<token file>"))?;
        Ok(Self {
            client: OAuthClientId::parse(client)?,
            file: PathBuf::from(file),
        })
    }
}

pub(super) struct LoreTokenRefresher {
    /// The directory `generate-local-installation` wrote, holding each client's key.
    pub directory: PathBuf,
    pub token_endpoint: url::Url,
    pub resource: String,
    pub targets: Vec<LoreTokenTarget>,
    /// The group that reads the token files.
    pub group: u32,
    pub interval: Duration,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

impl LoreTokenRefresher {
    /// Refresh every token, then again every `interval`, until the process is stopped.
    /// A round that fails for any client retries after a short wait.
    pub async fn run(self) -> Result<()> {
        let signers = self
            .targets
            .iter()
            .map(|target| signer(&self.directory, &target.client))
            .collect::<Result<Vec<_>>>()?;
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        loop {
            let mut failed = false;
            for (target, signer) in self.targets.iter().zip(&signers) {
                match self.refresh(&http, target, signer).await {
                    Ok(expires_in) => {
                        tracing::info!(client = %target.client, expires_in, "refreshed Lore token");
                    }
                    Err(error) => {
                        failed = true;
                        tracing::warn!(client = %target.client, "Lore token refresh failed: {error:#}");
                    }
                }
            }
            let wait = if failed { RETRY } else { self.interval };
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                _ = tokio::signal::ctrl_c() => return Ok(()),
            }
        }
    }

    async fn refresh(
        &self,
        http: &reqwest::Client,
        target: &LoreTokenTarget,
        signer: &ClientAssertionSigner,
    ) -> Result<u64> {
        let assertion = signer.assertion(&target.client, &self.token_endpoint)?;
        let form = Zeroizing::new(
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs([
                    ("grant_type", "client_credentials"),
                    ("client_id", target.client.as_str()),
                    ("client_assertion_type", CLIENT_ASSERTION_TYPE),
                    ("client_assertion", assertion.expose_secret()),
                    ("resource", self.resource.as_str()),
                ])
                .finish(),
        );
        let response = http
            .post(self.token_endpoint.clone())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form.as_str().to_owned())
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            // An OAuth error body names the error and holds no token.
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("the token endpoint answered {status}: {body}"));
        }
        let token: TokenResponse = response.json().await?;
        let access_token = Zeroizing::new(token.access_token);
        replace_token_file(&target.file, access_token.as_bytes(), self.group)?;
        Ok(token.expires_in)
    }
}

fn signer(directory: &Path, client: &OAuthClientId) -> Result<ClientAssertionSigner> {
    let jwks: jsonwebtoken::jwk::JwkSet = serde_json::from_slice(
        &std::fs::read(directory.join(format!("{client}.jwks.json")))
            .with_context(|| format!("read the JWK Set of `{client}`"))?,
    )?;
    let key_id = jwks
        .keys
        .first()
        .and_then(|key| key.common.key_id.clone())
        .ok_or_else(|| anyhow!("the JWK Set of `{client}` names no key"))?;
    let pem = Zeroizing::new(
        std::fs::read(directory.join(format!("{client}.pem")))
            .with_context(|| format!("read the key of `{client}`"))?,
    );
    Ok(ClientAssertionSigner::from_rsa_pem(
        ClientAssertionKeyId::parse(key_id)?,
        &pem,
    )?)
}

/// Replace `file` with `token` in one rename: owned by root, readable by `group`, and
/// in a directory only root and `group` enter.
fn replace_token_file(file: &Path, token: &[u8], group: u32) -> Result<()> {
    let directory = file
        .parent()
        .ok_or_else(|| anyhow!("{} has no directory", file.display()))?;
    std::os::unix::fs::chown(directory, Some(0), Some(group))?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o750))?;
    let staged = directory.join(".token.next");
    let _ = std::fs::remove_file(&staged);
    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&staged)?;
    handle.write_all(token)?;
    handle.sync_all()?;
    std::os::unix::fs::chown(&staged, Some(0), Some(group))?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o640))?;
    std::fs::rename(&staged, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_names_its_client_and_token_file() {
        let target: LoreTokenTarget = "lore-vv-mcp-time=/tokens/vv-mcp-time/token"
            .parse()
            .unwrap();
        assert_eq!(target.client.as_str(), "lore-vv-mcp-time");
        assert_eq!(target.file, Path::new("/tokens/vv-mcp-time/token"));
        assert!("lore-vv-mcp-time".parse::<LoreTokenTarget>().is_err());
    }
}
