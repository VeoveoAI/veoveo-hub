//! Refresh Lore credentials in trusted storage. Envoy SDS output lets the proxy
//! authenticate repository requests without giving credentials to development agents.

use std::{
    io::Write as _,
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};
use veoveo_oauth::{CLIENT_ASSERTION_TYPE, ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_sdk_rust::OAuthClientId;
use zeroize::Zeroizing;

/// How long a failed round waits before it tries again.
const RETRY: Duration = Duration::from_secs(15);
/// The longest one token request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize)]
struct SecretResponse<'a> {
    resources: [GenericSecret<'a>; 1],
}

#[derive(Serialize)]
struct GenericSecret<'a> {
    #[serde(rename = "@type")]
    type_url: &'static str,
    name: &'a str,
    generic_secret: InlineSecret<'a>,
}

#[derive(Serialize)]
struct InlineSecret<'a> {
    secret: InlineString<'a>,
}

#[derive(Serialize)]
struct InlineString<'a> {
    inline_string: &'a str,
}

fn encode_secret(client: &OAuthClientId, token: &str) -> Result<Zeroizing<Vec<u8>>> {
    let authorization = Zeroizing::new(format!("Bearer {token}"));
    let secret = SecretResponse {
        resources: [GenericSecret {
            type_url: "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.Secret",
            name: client.as_str(),
            generic_secret: InlineSecret {
                secret: InlineString {
                    inline_string: &authorization,
                },
            },
        }],
    };
    Ok(Zeroizing::new(serde_json::to_vec(&secret)?))
}

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
            // Do not log a response body: even a failing peer may echo credentials.
            return Err(anyhow!("the token endpoint answered {status}"));
        }
        let token: TokenResponse = response.json().await?;
        let access_token = Zeroizing::new(token.access_token);
        let encoded = encode_secret(&target.client, &access_token)?;
        replace_token_file(&target.file, &encoded, self.group)?;
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
    let mut staged_name = file
        .file_name()
        .ok_or_else(|| anyhow!("{} has no file name", file.display()))?
        .to_os_string();
    staged_name.push(".next");
    let staged = directory.join(staged_name);
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
    std::fs::File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_names_its_client_and_token_file() {
        let target: LoreTokenTarget = "lore-observatory=/tokens/observatory/token"
            .parse()
            .unwrap();
        assert_eq!(target.client.as_str(), "lore-observatory");
        assert_eq!(target.file, Path::new("/tokens/observatory/token"));
        assert!("lore-observatory".parse::<LoreTokenTarget>().is_err());
    }

    #[test]
    fn envoy_receives_a_named_bearer_secret() {
        let client = OAuthClientId::parse("lore-observatory").unwrap();
        let token = "fixture-token";
        let encoded = encode_secret(&client, token).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(value["resources"].as_array().unwrap().len(), 1);
        let secret = &value["resources"][0];
        assert_eq!(
            secret["@type"],
            "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.Secret"
        );
        assert_eq!(secret["name"], "lore-observatory");
        assert_eq!(
            secret["generic_secret"]["secret"]["inline_string"],
            "Bearer fixture-token"
        );
    }
}
