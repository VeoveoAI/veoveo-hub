//! A local installation: the configuration and secrets one Hub needs on a single
//! workstation. It names no server. Two client-credentials callers hold the `member`
//! and `admin` roles, so acceptance checks can exercise both sides of every risk.

use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use crate::control_plane::HubControlPlane;
use anyhow::{Context, Result};
use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use jsonwebtoken::{
    Algorithm, EncodingKey,
    jwk::{Jwk, JwkSet, ThumbprintHash},
};
use serde::Serialize;
use serde_json::json;
use veoveo_hub_sdk::PublicDeployment;
use veoveo_hub_sdk::registration::{Ed25519Jwk, Ed25519JwkSet, statement::ServerKeyPair};
use zeroize::Zeroizing;

/// The profile every local caller uses.
pub const LOCAL_PROFILE: &str = "local";
/// The coarse OAuth scope of the local profile's MCP endpoint.
pub const LOCAL_SCOPE: &str = "mcp";

/// What [`generate`] wrote. It holds paths and public identifiers only.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalInstallation {
    /// The installation configuration, for `--config`.
    pub config: PathBuf,
    /// `VEOVEO_*` secrets, one `NAME=value` per line, readable only by its owner.
    pub secrets: PathBuf,
    /// The OAuth token endpoint callers present client assertions to.
    pub token_endpoint: String,
    /// The protected resource callers request tokens for: the profile's MCP endpoint.
    pub resource: String,
    /// The scope callers request.
    pub scope: &'static str,
    pub callers: Vec<LocalCaller>,
}

/// One client-credentials caller and the role the installation grants it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCaller {
    pub client_id: String,
    pub role: String,
    /// RSA private key, PKCS#8 PEM, readable only by its owner.
    pub key: PathBuf,
    /// The key's RFC 7638 thumbprint, the `kid` of its client assertions.
    pub key_id: String,
}

/// Write a local installation for the Hub at `issuer` into `directory`, which
/// must exist. Existing files are never overwritten.
pub fn generate(directory: &Path, issuer: &str) -> Result<LocalInstallation> {
    let deployment = PublicDeployment::new(issuer)?;
    let base = deployment.base_url().trim_end_matches('/').to_owned();
    let directory = directory
        .canonicalize()
        .with_context(|| format!("{} must be an existing directory", directory.display()))?;
    let path = |name: &str| directory.join(name);

    let access_token_key = ServerKeyPair::generate()?;
    let forwarded_token_key = ServerKeyPair::generate()?;
    let access_token_jwk = Ed25519Jwk::new(*access_token_key.public_key());
    write_json(
        &path("authorization-server.jwks.json"),
        &Ed25519JwkSet::new(vec![access_token_jwk])?,
    )?;

    let mut callers = Vec::new();
    for (client_id, role) in [
        ("acceptance-member", "member"),
        ("acceptance-admin", "admin"),
    ] {
        let (key, key_id) = client_key(&directory, client_id)?;
        callers.push(LocalCaller {
            client_id: client_id.to_owned(),
            role: role.to_owned(),
            key,
            key_id,
        });
    }
    let config = configuration(&base, &directory, access_token_jwk, &callers)?;
    let config_path = path("hub.json");
    write_json(&config_path, &config)?;

    let secrets = Zeroizing::new(format!(
        "VEOVEO_ACCESS_TOKEN_SIGNING_KEY_B64={}\n\
         VEOVEO_FORWARDED_TOKEN_KEY_B64={}\n\
         VEOVEO_REFRESH_DELIVERY_KEY_B64={}\n\
         VEOVEO_AUDIT_SIGNING_KEY_B64={}\n\
         VEOVEO_REGISTRATION_SECRET={}\n",
        STANDARD.encode(access_token_key.pkcs8_der()),
        STANDARD.encode(forwarded_token_key.pkcs8_der()),
        STANDARD.encode(random::<32>()?),
        STANDARD.encode(random::<32>()?),
        URL_SAFE_NO_PAD.encode(random::<32>()?),
    ));
    let secrets_path = path("hub.env");
    write_private(&secrets_path, secrets.as_bytes())?;

    Ok(LocalInstallation {
        config: config_path,
        secrets: secrets_path,
        token_endpoint: format!("{base}/oauth/token"),
        resource: format!("{base}/mcp/{LOCAL_PROFILE}"),
        scope: LOCAL_SCOPE,
        callers,
    })
}

/// Write a new RSA client key and its public JWK Set for `client_id`, and return the
/// key's path and `kid`.
fn client_key(directory: &Path, client_id: &str) -> Result<(PathBuf, String)> {
    let (pem, jwk) = rsa_client_key()?;
    let key_id = jwk
        .common
        .key_id
        .clone()
        .context("client key has a key id")?;
    let key = directory.join(format!("{client_id}.pem"));
    write_private(&key, pem.as_bytes())?;
    write_json(
        &directory.join(format!("{client_id}.jwks.json")),
        &JwkSet { keys: vec![jwk] },
    )?;
    Ok((key, key_id))
}

fn configuration(
    base: &str,
    directory: &Path,
    access_token_jwk: Ed25519Jwk,
    callers: &[LocalCaller],
) -> Result<HubControlPlane> {
    let file = |name: &str| directory.join(name).display().to_string();
    let issuer = format!("{base}/oauth");
    let resource = format!("{base}/mcp/{LOCAL_PROFILE}");
    let clients: Vec<_> = callers
        .iter()
        .map(|caller| {
            json!({
                "id": caller.client_id,
                "authorizationServer": "local",
                "defaultWorkContext": "local",
                "invocationMode": "automated",
                "displayName": format!("Acceptance caller with the {} role", caller.role),
                "allowedResources": [resource],
                "grantTypes": ["client_credentials"],
                "authMethods": ["private_key_jwt"],
                "allowedScopes": [LOCAL_SCOPE],
                "jwks": { "source": "file", "path": file(&format!("{}.jwks.json", caller.client_id)) },
                "tenant": "local",
                "metadata": {}
            })
        })
        .collect();
    let grants: Vec<_> = callers
        .iter()
        .map(|caller| {
            json!({
                "role": caller.role,
                "subject": { "principal": format!("{issuer}#{}", caller.client_id) }
            })
        })
        .collect();
    let document = json!({
        "identityProviders": [{
            "id": "local",
            "issuer": format!("{base}/identity"),
            "jwks": { "source": "file", "path": file("authorization-server.jwks.json") },
            "metadata": {}
        }],
        "authorizationServers": [{
            "id": "local",
            "issuer": issuer,
            "jwks": { "source": "file", "path": file("authorization-server.jwks.json") },
            "accessTokenKeyId": access_token_jwk.kid().as_str(),
            "accessTokenSigningKey": "access_token_signing_key",
            "identityProvider": "local",
            "tokenEndpoint": format!("{base}/oauth/token"),
            "metadata": {}
        }],
        "profiles": [{
            "id": LOCAL_PROFILE,
            "identityProvider": "local",
            "authorizationServer": "local",
            "protectedResource": resource,
            "policyVersion": "local",
            "authModes": ["oauth_client_credentials"],
            "requiredScopes": [LOCAL_SCOPE],
            "metadata": {}
        }],
        "authorization": {
            "roles": [
                { "name": "member", "access": ["read", "write"] },
                {
                    "name": "admin",
                    "access": ["read", "write"],
                    "risks": ["sensitive", "destructive", "billed", "physical", "administer"]
                }
            ],
            "grants": grants
        },
        "registration": { "secret": "registration_secret" },
        "tenants": [{ "id": "local", "title": "Local installation" }],
        "workContexts": [{
            "id": "local",
            "tenant": "local",
            "title": "Local",
            "policyRevision": "local",
            "outputPolicy": { "owner": { "kind": "principal", "id": format!("{issuer}#acceptance-admin") } },
            "memberships": [{
                "level": "contributor",
                "oauthClients": callers.iter().map(|caller| caller.client_id.as_str()).collect::<Vec<_>>()
            }]
        }],
        "policies": [{ "version": "local", "rules": [] }],
        "dataLabels": [],
        "oauthClients": clients,
        "oidcClients": [],
        "secrets": [
            {
                "id": "access_token_signing_key",
                "source": "env",
                "purpose": "jwks_private_key",
                "locator": "VEOVEO_ACCESS_TOKEN_SIGNING_KEY_B64",
                "owner": { "kind": "hub" },
                "metadata": {}
            },
            {
                "id": "registration_secret",
                "source": "env",
                "purpose": "registration_secret",
                "locator": "VEOVEO_REGISTRATION_SECRET",
                "owner": { "kind": "hub" },
                "metadata": {}
            }
        ],
        "metadata": {}
    });
    Ok(serde_json::from_value(document)?)
}

/// A 2048-bit RSA client key as PKCS#8 PEM, and its public JWK whose `kid` is the RFC
/// 7638 thumbprint.
fn rsa_client_key() -> Result<(Zeroizing<String>, Jwk)> {
    let key = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048)
        .map_err(|_| anyhow::anyhow!("RSA key generation failed"))?;
    let der: Pkcs8V1Der<'static> = key
        .as_der()
        .map_err(|_| anyhow::anyhow!("RSA key encoding failed"))?;
    let body = Zeroizing::new(STANDARD.encode(der.as_ref()));
    let lines = body
        .as_bytes()
        .chunks(64)
        .map(|line| std::str::from_utf8(line).expect("base64 is ASCII"))
        .collect::<Vec<_>>()
        .join("\n");
    let pem = Zeroizing::new(format!(
        "-----BEGIN PRIVATE KEY-----\n{lines}\n-----END PRIVATE KEY-----\n"
    ));
    let mut jwk = Jwk::from_encoding_key(
        &EncodingKey::from_rsa_pem(pem.as_bytes())?,
        Algorithm::RS256,
    )?;
    jwk.common.key_id = Some(jwk.thumbprint(ThumbprintHash::SHA256)?);
    Ok((pem, jwk))
}

fn random<const N: usize>() -> Result<Zeroizing<[u8; N]>> {
    let mut bytes = Zeroizing::new([0_u8; N]);
    getrandom::fill(bytes.as_mut_slice())
        .map_err(|error| anyhow::anyhow!("random source: {error}"))?;
    Ok(bytes)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    create(path, 0o644)?.write_all(text.as_bytes())?;
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = create(path, 0o600)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn create(path: &Path, mode: u32) -> Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .with_context(|| format!("create {}; it must not exist yet", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_installation_is_a_valid_configuration_without_servers() {
        let directory = tempfile::tempdir().unwrap();
        let installation = generate(directory.path(), "http://vv-hub.internal:8080").unwrap();
        let catalog =
            crate::HubCatalog::load_json(&installation.config, crate::test_catalog::binding())
                .unwrap();
        assert_eq!(catalog.server_count(), 0);
        assert!(catalog.control_plane().registration.is_some());
        assert_eq!(
            installation.token_endpoint,
            "http://vv-hub.internal:8080/oauth/token"
        );
        use std::os::unix::fs::PermissionsExt;
        for private in [&installation.secrets, &installation.callers[0].key] {
            let mode = std::fs::metadata(private).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let secrets = std::fs::read_to_string(&installation.secrets).unwrap();
        crate::http::auth_support::AccessTokenSigningKey::from_base64(
            secrets
                .lines()
                .find_map(|line| line.strip_prefix("VEOVEO_ACCESS_TOKEN_SIGNING_KEY_B64="))
                .unwrap(),
        )
        .unwrap();
        assert!(generate(directory.path(), "http://vv-hub.internal:8080").is_err());
    }
}
