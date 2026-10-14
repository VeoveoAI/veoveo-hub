//! A private Keycloak server for one test process, on HTTPS at a loopback port with a
//! certificate authority generated for it. The dev image builds the distribution at
//! `/opt/keycloak` for an in-memory database; `KEYCLOAK_HOME` names another build. Each
//! server runs from its own home directory, which links the distribution and holds a
//! fresh `data` directory, so servers in parallel test processes share no state.
#![allow(dead_code, reason = "Each integration test uses part of the fixture")]

use std::{
    net::{Ipv4Addr, TcpListener},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use serde_json::Value;

const ADMIN: &str = "bootstrap-admin";

/// One Keycloak process, its home and the certificate authority its server certificate
/// chains to. Dropping it kills the server's process group and removes its home.
pub struct TestKeycloak {
    server: ProcessGroup,
    dir: tempfile::TempDir,
    base: String,
    admin_password: String,
    http: reqwest::Client,
}

impl TestKeycloak {
    /// Start Keycloak and wait until its master realm answers, for at most two minutes.
    /// The port is free when chosen, but another process can take it before Keycloak
    /// binds it; Keycloak then exits, and the fixture starts it again on another port,
    /// at most three times.
    pub async fn start() -> Self {
        let distribution =
            PathBuf::from(std::env::var_os("KEYCLOAK_HOME").unwrap_or("/opt/keycloak".into()));
        let dir = tempfile::tempdir().expect("Keycloak home directory");
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join("data")).expect("Keycloak data directory");
        for part in ["bin", "conf", "lib", "providers", "themes"] {
            std::os::unix::fs::symlink(distribution.join(part), home.join(part))
                .expect("link the Keycloak distribution");
        }
        let (ca, certificate, key) = server_certificate();
        std::fs::write(dir.path().join("ca.pem"), &ca).unwrap();
        std::fs::write(dir.path().join("tls.crt"), certificate).unwrap();
        std::fs::write(dir.path().join("tls.key"), key).unwrap();
        let http = reqwest::Client::builder()
            .add_root_certificate(reqwest::Certificate::from_pem(ca.as_bytes()).unwrap())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let admin_password = random_text();
        let deadline = Instant::now() + Duration::from_secs(120);
        for _ in 0..3 {
            let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .and_then(|listener| listener.local_addr())
                .expect("a free loopback port")
                .port();
            let base = format!("https://127.0.0.1:{port}");
            let log = std::fs::File::create(dir.path().join("keycloak.log")).unwrap();
            let spawned = Command::new(distribution.join("bin/kc.sh"))
                .arg("start")
                .arg("--optimized")
                .arg(format!("-Dkc.home.dir={}", home.display()))
                .arg("--http-enabled=false")
                .arg("--http-host=127.0.0.1")
                .arg(format!("--https-port={port}"))
                .arg(format!(
                    "--https-certificate-file={}",
                    dir.path().join("tls.crt").display()
                ))
                .arg(format!(
                    "--https-certificate-key-file={}",
                    dir.path().join("tls.key").display()
                ))
                .arg(format!("--hostname={base}"))
                // A single-node test server needs no cluster, and it binds IPv4 only.
                .arg("--cache=local")
                .env("JAVA_OPTS_APPEND", "-Djava.net.preferIPv4Stack=true")
                .env("KC_BOOTSTRAP_ADMIN_USERNAME", ADMIN)
                .env("KC_BOOTSTRAP_ADMIN_PASSWORD", &admin_password)
                .stdin(Stdio::null())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                // kc.sh starts Java as its own child, so the fixture kills the group.
                .process_group(0)
                .spawn()
                .expect("start kc.sh; the dev image builds Keycloak at /opt/keycloak");
            let mut server = ProcessGroup(spawned);
            if answers(&mut server.0, &http, &base, dir.path(), deadline).await {
                return Self {
                    server,
                    dir,
                    base,
                    admin_password,
                    http,
                };
            }
            // Keycloak exited, most likely because another process took the port.
        }
        panic!(
            "Keycloak exited three times without answering:\n{}",
            std::fs::read_to_string(dir.path().join("keycloak.log")).unwrap_or_default()
        );
    }

    /// The server's own output.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("keycloak.log")).unwrap_or_default()
    }

    /// `https://127.0.0.1:<port>`.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The issuer of `realm`.
    pub fn issuer(&self, realm: &str) -> String {
        format!("{}/realms/{realm}", self.base)
    }

    /// The PEM certificate authority the server's certificate chains to.
    pub fn certificate_authority(&self) -> PathBuf {
        self.dir.path().join("ca.pem")
    }

    /// A client that trusts the server's certificate and follows no redirect.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Create a realm from its representation through the admin API, as the bootstrap
    /// administrator.
    pub async fn create_realm(&self, realm: &Value) {
        let token: Value = form(
            self.http.post(format!(
                "{}/realms/master/protocol/openid-connect/token",
                self.base
            )),
            &[
                ("grant_type", "password"),
                ("client_id", "admin-cli"),
                ("username", ADMIN),
                ("password", self.admin_password.as_str()),
            ],
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        let access = token["access_token"]
            .as_str()
            .unwrap_or_else(|| panic!("the bootstrap administrator signs in: {token}"));
        let created = self
            .http
            .post(format!("{}/admin/realms", self.base))
            .bearer_auth(access)
            .json(realm)
            .send()
            .await
            .unwrap();
        let status = created.status();
        assert_eq!(
            status,
            reqwest::StatusCode::CREATED,
            "creating the realm: {}",
            created.text().await.unwrap_or_default()
        );
    }
}

impl Drop for TestKeycloak {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("Keycloak log:\n{}", self.log());
        }
    }
}

/// kc.sh and the Java process it starts as its own child, which share the group kc.sh
/// leads. Dropping it stops kc.sh, which passes the signal to Java and reaps it, and
/// kills the whole group if that takes longer than ten seconds.
struct ProcessGroup(Child);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let signal = |signal: &str, target: String| {
            let _ = Command::new("kill")
                .args([signal, "--", &target])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        };
        signal("-TERM", self.0.id().to_string());
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        signal("-KILL", format!("-{}", self.0.id()));
        let _ = self.0.wait();
    }
}

/// True once the master realm's discovery document answers at `base`, false when
/// Keycloak exits first. Panics at `deadline` with Keycloak's log.
async fn answers(
    child: &mut Child,
    http: &reqwest::Client,
    base: &str,
    dir: &Path,
    deadline: Instant,
) -> bool {
    let discovery = format!("{base}/realms/master/.well-known/openid-configuration");
    loop {
        if let Ok(response) = http.get(&discovery).send().await
            && response.status() == reqwest::StatusCode::OK
        {
            return true;
        }
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        assert!(
            Instant::now() < deadline,
            "Keycloak did not answer within two minutes:\n{}",
            std::fs::read_to_string(dir.join("keycloak.log")).unwrap_or_default()
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A certificate authority, and a server certificate for `127.0.0.1` it signed, with
/// the server's PKCS#8 key, all PEM.
fn server_certificate() -> (String, String, String) {
    let mut authority = CertificateParams::new(Vec::<String>::new()).unwrap();
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    authority
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Gateway test authority");
    let authority_key = KeyPair::generate().unwrap();
    let authority_certificate = authority.self_signed(&authority_key).unwrap();
    let issuer = Issuer::new(authority, authority_key);
    let server = CertificateParams::new(vec!["127.0.0.1".to_owned()]).unwrap();
    let server_key = KeyPair::generate().unwrap();
    let server_certificate = server.signed_by(&server_key, &issuer).unwrap();
    (
        authority_certificate.pem(),
        server_certificate.pem(),
        server_key.serialize_pem(),
    )
}

/// `request` with `pairs` as its `application/x-www-form-urlencoded` body.
pub fn form(request: reqwest::RequestBuilder, pairs: &[(&str, &str)]) -> reqwest::RequestBuilder {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        body.append_pair(name, value);
    }
    request
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body.finish())
}

/// 32 random bytes as hexadecimal text.
pub fn random_text() -> String {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).expect("random source");
    hex::encode(bytes)
}
