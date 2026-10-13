//! The Gateway binary on a disposable PostgreSQL database with a generated local
//! installation, and the OAuth requests its acceptance callers send.
#![allow(dead_code, reason = "Each integration test uses part of the fixture")]

use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use secrecy::ExposeSecret;
use tokio::process::{Child, Command};
use veoveo_oauth_client::{CLIENT_ASSERTION_TYPE, ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_service_gateway::local::{LocalCaller, LocalInstallation};
use veoveo_test_fixtures::store::TestDb;
use veoveo_types::OAuthClientId;

/// The Gateway binary, its local installation and its log.
pub struct Gateway {
    /// The Gateway's issuer, `http://127.0.0.1:<port>`.
    pub base: String,
    pub installation: LocalInstallation,
    secrets: Vec<(String, String)>,
    log: PathBuf,
    child: Child,
}

impl Gateway {
    /// Generate a local installation in `directory` and start the Gateway on `db`. A
    /// registration without a live process lasts `registration_grace`.
    pub async fn start(directory: &Path, db: &TestDb, registration_grace: Duration) -> Self {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let base = format!("http://127.0.0.1:{port}");
        let installation = veoveo_service_gateway::local::generate(directory, &base).unwrap();
        let secrets: Vec<(String, String)> = std::fs::read_to_string(&installation.secrets)
            .unwrap()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        let log = directory.join("gateway.log");
        let output = std::fs::File::create(&log).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_veoveo-service-gateway"))
            .arg("serve")
            .current_dir(directory)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("RUST_LOG", "warn,veoveo_service_gateway=info")
            .env("NO_COLOR", "1")
            .env(
                "VEOVEO_LISTEN_ADDRESS",
                SocketAddr::from((Ipv4Addr::LOCALHOST, port)).to_string(),
            )
            .env("VEOVEO_ISSUER", &base)
            .env("VEOVEO_CONFIG", &installation.config)
            .env("VEOVEO_DATABASE_URL", db.url())
            .env("VEOVEO_DATABASE_PASSWORD", db.password().expose_secret())
            .env(
                "VEOVEO_REGISTRATION_GRACE_SECONDS",
                registration_grace.as_secs().to_string(),
            )
            .envs(secrets.iter().cloned())
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let gateway = Self {
            base,
            installation,
            secrets,
            log,
            child,
        };
        gateway.ready().await;
        gateway
    }

    async fn ready(&self) {
        let http = reqwest::Client::new();
        for _ in 0..300 {
            if let Ok(response) = http.get(format!("{}/readyz", self.base)).send().await
                && response.status() == reqwest::StatusCode::OK
            {
                let live = http
                    .get(format!("{}/livez", self.base))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(live.status(), reqwest::StatusCode::NO_CONTENT);
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!(
            "the Gateway did not become ready:\n{}",
            std::fs::read_to_string(&self.log).unwrap_or_default()
        );
    }

    /// A secret the installation generated, by its environment variable.
    pub fn secret(&self, name: &str) -> &str {
        self.secrets
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("the installation generates {name}"))
    }

    pub fn caller(&self, role: &str) -> &LocalCaller {
        self.installation
            .callers
            .iter()
            .find(|caller| caller.role == role)
            .unwrap()
    }

    pub fn client(caller: &LocalCaller) -> OAuthClientId {
        OAuthClientId::parse(caller.client_id.clone()).unwrap()
    }

    /// The caller's RFC 7523 assertion signer, under its key's thumbprint.
    pub fn signer(&self, caller: &LocalCaller) -> ClientAssertionSigner {
        ClientAssertionSigner::from_rsa_pem(
            ClientAssertionKeyId::parse(caller.key_id.clone()).unwrap(),
            &std::fs::read(&caller.key).unwrap(),
        )
        .unwrap()
    }

    pub fn token_endpoint(&self) -> url::Url {
        url::Url::parse(&self.installation.token_endpoint).unwrap()
    }

    /// A client-credentials token request of `client`, authenticated by `assertion`,
    /// for the local profile's MCP resource and scope.
    pub fn client_credentials(&self, client: &OAuthClientId, assertion: &str) -> TokenForm {
        TokenForm(vec![
            ("grant_type", "client_credentials".to_owned()),
            ("client_id", client.to_string()),
            ("client_assertion_type", CLIENT_ASSERTION_TYPE.to_owned()),
            ("client_assertion", assertion.to_owned()),
            ("resource", self.installation.resource.clone()),
            ("scope", self.installation.scope.to_owned()),
        ])
    }

    /// Send a form to the token endpoint and return the status, headers and JSON body.
    pub async fn request_token(
        &self,
        http: &reqwest::Client,
        form: &TokenForm,
    ) -> (
        reqwest::StatusCode,
        reqwest::header::HeaderMap,
        serde_json::Value,
    ) {
        let mut body = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in &form.0 {
            body.append_pair(name, value);
        }
        let response = http
            .post(self.token_endpoint())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body.finish())
            .send()
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.json().await.unwrap();
        (status, headers, body)
    }

    /// An access token for `caller` from an RFC 7523 client-credentials exchange.
    pub async fn access_token(&self, http: &reqwest::Client, caller: &LocalCaller) -> String {
        let client = Self::client(caller);
        let assertion = self
            .signer(caller)
            .assertion(&client, &self.token_endpoint())
            .unwrap();
        let form = self.client_credentials(&client, assertion.expose_secret());
        let (status, _, body) = self.request_token(http, &form).await;
        assert_eq!(status, reqwest::StatusCode::OK, "token response: {body}");
        body["access_token"].as_str().unwrap().to_owned()
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        if std::thread::panicking() {
            eprintln!(
                "Gateway log:\n{}",
                std::fs::read_to_string(&self.log).unwrap_or_default()
            );
        }
    }
}

/// The parameters of one token request, in the order they are sent.
#[derive(Clone)]
pub struct TokenForm(Vec<(&'static str, String)>);

impl TokenForm {
    /// The same request with `name` set to `value`.
    pub fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.0.retain(|(key, _)| *key != name);
        self.0.push((name, value.into()));
        self
    }

    /// The same request without `name`.
    pub fn without(mut self, name: &str) -> Self {
        self.0.retain(|(key, _)| *key != name);
        self
    }
}
