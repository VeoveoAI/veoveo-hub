use std::{
    convert::Infallible, fmt, fmt::Write as _, num::NonZeroU32, path::PathBuf, str::FromStr,
};
use veoveo_hub::contract::{SecretPurpose, SecretReferenceId, SecretSource};

#[path = "hub/bindings.rs"]
mod bindings;
use bindings::catalog_admission;
#[path = "hub/contributions/mod.rs"]
mod contributions;

#[path = "hub/admin.rs"]
mod admin;
#[path = "hub/audit.rs"]
mod audit;
#[path = "hub/audit_cli.rs"]
mod audit_cli;
#[path = "hub/auth.rs"]
mod auth;
#[path = "hub/console/mod.rs"]
mod console;
#[path = "hub/control_reload.rs"]
mod control_reload;
#[path = "hub/host.rs"]
mod host;
#[path = "hub/http_util.rs"]
mod http_util;
#[path = "hub/oauth.rs"]
mod oauth;
#[path = "hub/oauth_client_credentials.rs"]
mod oauth_client_credentials;
#[path = "hub/oauth_grants.rs"]
mod oauth_grants;
#[path = "hub/runtime.rs"]
mod runtime;
#[path = "hub/server.rs"]
mod server;
#[path = "hub/startup.rs"]
mod startup;
use veoveo_hub::issuance as tokens;

use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_hub::{
    HubCatalog, HubControlStore, HubRefreshDeliveryWindow, HubSecretResolver,
    RefreshTokenDeliveryCipher,
};
use veoveo_hub_sdk::{TelemetryGuard, init_server_telemetry};
use veoveo_store::StoreConfig;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default();
}

#[derive(Parser, Debug)]
#[command(name = "veoveo-hub", about = "Veoveo MCP hub")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, ClapArgs, Debug)]
struct DatabaseArgs {
    /// The installation database as a `postgres://` URL that names the user and no
    /// password.
    #[arg(long = "database-url", env = "VEOVEO_DATABASE_URL")]
    url: String,
    /// The password of the URL's user.
    #[arg(
        long = "database-password",
        env = "VEOVEO_DATABASE_PASSWORD",
        hide_env_values = true
    )]
    password: RedactedSecret,
}

impl DatabaseArgs {
    fn into_config(self) -> anyhow::Result<StoreConfig> {
        Ok(StoreConfig::builder(self.url, self.password.0)
            .application_name("veoveo-hub")
            .build()?)
    }
}

#[derive(Clone)]
struct RedactedSecret(SecretString);

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl FromStr for RedactedSecret {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(SecretString::from(value)))
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Explicitly replace the authoritative control plane while the service is stopped.
    ImportControlPlane {
        #[command(flatten)]
        database: DatabaseArgs,
        #[arg(long, env = "VEOVEO_CONFIG")]
        config: PathBuf,
    },
    /// Validate an installation configuration file and exit.
    Validate {
        /// Installation configuration (JSON).
        #[arg(long, env = "VEOVEO_CONFIG")]
        config: PathBuf,
    },
    /// Resolve one configured secret and print redacted evidence as JSON.
    ResolveSecret {
        /// Installation configuration (JSON).
        #[arg(long, env = "VEOVEO_CONFIG")]
        config: PathBuf,
        /// Secret reference id.
        #[arg(long)]
        secret_id: String,
        /// Expected secret purpose, as the configuration spells it.
        #[arg(long)]
        purpose: String,
    },
    /// Write a local installation's configuration, secrets and two acceptance callers
    /// into an existing directory, and print where they are. Prints no secret.
    GenerateLocalInstallation {
        /// Directory that receives the files; existing files are never overwritten.
        #[arg(long)]
        directory: PathBuf,
        /// The Hub's public origin, as `serve --issuer` will receive it.
        #[arg(long, env = "VEOVEO_ISSUER")]
        issuer: String,
    },
    Audit {
        #[command(subcommand)]
        command: Box<audit_cli::AuditCommand>,
    },
    /// Start the Hub.
    Serve(Box<ServeArgs>),
}

#[derive(ClapArgs, Debug)]
struct ServeArgs {
    /// Socket address to listen on.
    #[arg(long, env = "VEOVEO_LISTEN_ADDRESS", default_value = "127.0.0.1:8080")]
    listen_address: std::net::SocketAddr,
    /// The Hub's public origin: its RFC 8414 issuer and the base of every URL it
    /// announces.
    #[arg(long, env = "VEOVEO_ISSUER")]
    issuer: String,
    /// Installation seed (JSON), used only when the database has no active revision.
    #[arg(long, env = "VEOVEO_CONFIG")]
    config: PathBuf,
    #[command(flatten)]
    database: DatabaseArgs,
    /// The database URL registration grants name, when servers reach the database at
    /// another address than the Hub. It carries no user or password.
    #[arg(long, env = "VEOVEO_SERVER_DATABASE_URL")]
    server_database_url: Option<url::Url>,
    /// How long a registration may go without a live process of its login before the
    /// Hub removes it and drops the login.
    #[arg(long, env = "VEOVEO_REGISTRATION_GRACE_SECONDS", default_value_t = 60)]
    registration_grace_seconds: u64,
    /// Base64 Ed25519 PKCS#8 key that signs the access tokens the Hub forwards
    /// to registered servers.
    #[arg(
        long = "forwarded-token-key-b64",
        env = "VEOVEO_FORWARDED_TOKEN_KEY_B64",
        hide_env_values = true
    )]
    forwarded_token_key_b64: RedactedSecret,
    /// Base64 Ed25519 PKCS#8 key whose public half the Hub publishes beside the
    /// signing key and never signs with: the next key before a rotation, the previous
    /// one after it. Only the public half is kept.
    #[arg(
        long = "forwarded-token-verification-key-b64",
        env = "VEOVEO_FORWARDED_TOKEN_VERIFICATION_KEY_B64",
        hide_env_values = true
    )]
    forwarded_token_verification_key_b64: Option<RedactedSecret>,
    /// Base64 32-byte key that encrypts refresh-token successor envelopes shared by
    /// Hub replicas.
    #[arg(long, env = "VEOVEO_REFRESH_DELIVERY_KEY_B64", hide_env_values = true)]
    refresh_delivery_key_b64: RedactedSecret,
    /// Time in which a concurrent presentation of a consumed refresh token receives
    /// the same successor. Later reuse revokes the family.
    #[arg(
        long,
        env = "VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS",
        default_value = "5",
        value_parser = clap::value_parser!(NonZeroU32)
    )]
    refresh_delivery_window_seconds: NonZeroU32,
    /// Accept loopback Host headers in addition to the issuer's host.
    #[arg(long, env = "VEOVEO_ALLOW_LOOPBACK_HOSTS", default_value_t = false)]
    allow_loopback_hosts: bool,
    /// The public origin of the Apps sandbox proxy, `sandbox.{apps domain}`, such as
    /// `https://sandbox.apps.veoveo.example`. Without it the Hub hosts no App.
    #[arg(long, env = "VEOVEO_APPS_SANDBOX_ORIGIN")]
    apps_sandbox_origin: Option<veoveo_hub::apps::SandboxOrigin>,
    /// Socket address of the Apps listener, which serves only the sandbox proxy; bound
    /// only with a sandbox origin.
    #[arg(
        long,
        env = "VEOVEO_APPS_LISTEN_ADDRESS",
        default_value = "127.0.0.1:8081"
    )]
    apps_listen_address: std::net::SocketAddr,
    /// Installation connectivity.
    #[arg(
        long,
        env = "VEOVEO_CONNECTIVITY_MODE",
        value_enum,
        default_value = "connected"
    )]
    connectivity_mode: ConnectivityMode,
    /// Retention window for audit records.
    #[arg(long, env = "VEOVEO_AUDIT_RETENTION_DAYS", default_value = "30", value_parser = clap::value_parser!(NonZeroU32))]
    audit_retention_days: NonZeroU32,
    /// Base64 audit block-signing seed. Never reuse a token signing key.
    #[arg(long, env = "VEOVEO_AUDIT_SIGNING_KEY_B64", hide_env_values = true)]
    audit_signing_key_b64: RedactedSecret,
    /// Audit export destinations (JSON); credentials stay in secrets.
    #[arg(long, env = "VEOVEO_AUDIT_EXPORT_CONFIG")]
    audit_export_config: Option<PathBuf>,
    /// How long startup waits for the database.
    #[arg(long, env = "VEOVEO_DATABASE_WAIT_SECONDS", default_value_t = 60)]
    database_wait_seconds: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ConnectivityMode {
    Connected,
    Offline,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let command = Args::parse().command;
    let _telemetry: Option<TelemetryGuard> = if matches!(&command, Command::Serve(_)) {
        Some(init_server_telemetry(
            "veoveo-hub",
            "info,veoveo_hub=debug",
        )?)
    } else {
        // Operator commands reserve stdout for their result, even inside a Pod
        // whose environment configures the hosted service's OTLP exporters.
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn".into()),
            )
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .try_init()
            .map_err(|error| anyhow::anyhow!("initialize CLI logging: {error}"))?;
        None
    };

    match command {
        Command::ImportControlPlane { database, config } => {
            let platform =
                startup::connect(database.into_config()?, std::time::Duration::from_secs(30))
                    .await?;
            startup::migrate(&platform).await?;
            let admission = crate::catalog_admission()?;
            let catalog = HubCatalog::load_json(&config, admission.clone())?;
            let store = HubControlStore::from_platform_store(platform, admission);
            startup::import(&store, &catalog).await?;
            println!("published control plane");
            Ok(())
        }
        Command::Validate { config } => {
            let catalog = HubCatalog::load_json(&config, crate::catalog_admission()?)?;
            println!("ok: {} profile(s)", catalog.profile_count());
            Ok(())
        }
        Command::ResolveSecret {
            config,
            secret_id,
            purpose,
        } => {
            let catalog = HubCatalog::load_json(&config, crate::catalog_admission()?)?;
            let secret_id = SecretReferenceId::parse(secret_id)?;
            let purpose = parse_secret_purpose(&purpose)?;
            let resolved = HubSecretResolver::new()
                .resolve_string(&catalog, &secret_id, purpose)
                .await?;
            println!(
                "{}",
                serde_json::to_string(&ResolvedSecretEvidence {
                    id: resolved.id().to_string(),
                    source: resolved.source(),
                    purpose: resolved.purpose(),
                    byte_length: resolved.expose_secret().len(),
                    sha256: sha256_hex(resolved.expose_secret().as_bytes()),
                })?
            );
            Ok(())
        }
        Command::GenerateLocalInstallation { directory, issuer } => {
            let installation = veoveo_hub::local::generate(&directory, &issuer)?;
            println!("{}", serde_json::to_string_pretty(&installation)?);
            Ok(())
        }
        Command::Audit { command } => command.run().await,
        Command::Serve(args) => serve(*args).await,
    }
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let ServeArgs {
        listen_address,
        issuer,
        config,
        database,
        server_database_url,
        registration_grace_seconds,
        forwarded_token_key_b64,
        forwarded_token_verification_key_b64,
        refresh_delivery_key_b64,
        refresh_delivery_window_seconds,
        allow_loopback_hosts,
        apps_sandbox_origin,
        apps_listen_address,
        connectivity_mode,
        audit_retention_days,
        audit_signing_key_b64,
        audit_export_config,
        database_wait_seconds,
    } = args;
    let store_config = database.into_config()?;
    let server_database = veoveo_hub::registration::ServerDatabase {
        url: veoveo_hub_sdk::registration::DatabaseUrl::new(
            server_database_url.unwrap_or_else(|| store_config.url().clone()),
        )?,
    };
    let platform = startup::connect(
        store_config,
        std::time::Duration::from_secs(database_wait_seconds),
    )
    .await?;
    startup::migrate(&platform).await?;
    let control_store =
        veoveo_hub::HubControlStore::from_platform_store(platform, crate::catalog_admission()?);
    let catalog = startup::load_catalog(&control_store, &config).await?;
    let audit_exports = audit_export_config
        .as_deref()
        .map(veoveo_audit::export::AuditExportConfig::load)
        .transpose()?
        .unwrap_or_default();
    let audit_signing_key =
        std::sync::Arc::new(veoveo_audit::integrity::AuditSigningKey::from_base64(
            audit_signing_key_b64.0.expose_secret(),
        )?);
    let refresh_delivery_cipher =
        RefreshTokenDeliveryCipher::from_base64(refresh_delivery_key_b64.0.expose_secret())?;
    let refresh_delivery_window =
        HubRefreshDeliveryWindow::from_seconds(refresh_delivery_window_seconds)?;
    server::serve(server::ServeConfig {
        listen_address,
        issuer,
        catalog,
        control_store,
        server_database,
        registration_grace: std::time::Duration::from_secs(registration_grace_seconds),
        forwarded_token_key_b64: forwarded_token_key_b64.0,
        forwarded_token_verification_key_b64: forwarded_token_verification_key_b64.map(|key| key.0),
        refresh_delivery_cipher,
        refresh_delivery_window,
        allow_loopback_hosts,
        apps: apps_sandbox_origin.map(|sandbox| (sandbox, apps_listen_address)),
        offline_mode: connectivity_mode == ConnectivityMode::Offline,
        audit_retention_days,
        audit_signing_key,
        audit_exports,
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedSecretEvidence {
    id: String,
    source: SecretSource,
    purpose: SecretPurpose,
    byte_length: usize,
    sha256: String,
}

fn parse_secret_purpose(value: &str) -> anyhow::Result<SecretPurpose> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|err| anyhow::anyhow!("invalid secret purpose `{value}`: {err}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("write to string");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANONICAL_STORE_ARGS: [&str; 4] = [
        "--database-url",
        "postgres://veoveo@127.0.0.1:5432/veoveo",
        "--database-password",
        "do-not-log",
    ];

    #[test]
    fn serve_cli_redacts_database_password_and_keys() {
        let mut arguments = vec![
            "hub",
            "serve",
            "--issuer",
            "https://veoveo.example",
            "--config",
            "hub.json",
            "--forwarded-token-key-b64",
            "forwarded-token-secret",
            "--forwarded-token-verification-key-b64",
            "verification-key-secret",
            "--refresh-delivery-key-b64",
            "refresh-delivery-secret",
            "--audit-signing-key-b64",
            "audit-signing-secret",
        ];
        arguments.extend(CANONICAL_STORE_ARGS);
        let parsed = Args::try_parse_from(arguments).unwrap();
        let debug = format!("{parsed:?}");
        assert!(debug.contains("[REDACTED]"));
        for secret in [
            "do-not-log",
            "forwarded-token-secret",
            "verification-key-secret",
            "refresh-delivery-secret",
            "audit-signing-secret",
        ] {
            assert!(!debug.contains(secret));
        }
    }
}
