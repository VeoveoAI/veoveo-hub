//! The origins of an installation's Apps: the issuer's for host pages, the sandbox
//! proxy's, and each server's own app origin beside it under one apps domain. All come
//! from configuration; no request's `Host` names them.
use std::{fmt, str::FromStr};

use anyhow::{Context, ensure};
use veoveo_hub_sdk::ServerSlug;

use super::route::SANDBOX_SLUG;

/// The public origin of the sandbox proxy, `sandbox.{apps domain}`: an `https` or `http`
/// origin whose host's first label is `sandbox` and whose apps domain is a lowercase DNS
/// name of at least two labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxOrigin {
    scheme: String,
    domain: String,
    port: Option<u16>,
}

impl FromStr for SandboxOrigin {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        let url = url::Url::parse(value).context("the sandbox origin is not a URL")?;
        ensure!(
            matches!(url.scheme(), "https" | "http") && url.origin().ascii_serialization() == value,
            "the sandbox origin `{value}` must be an https or http origin, spelled as one"
        );
        let host = url.host_str().context("the sandbox origin names no host")?;
        let domain = host
            .strip_prefix(&format!("{SANDBOX_SLUG}."))
            .with_context(|| {
                format!("the sandbox origin's host `{host}` must start with `{SANDBOX_SLUG}.`")
            })?;
        let labels: Vec<_> = domain.split('.').collect();
        ensure!(
            labels.len() >= 2
                && domain.len() <= 200
                && labels.iter().all(|label| {
                    (1..=63).contains(&label.len())
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                        })
                }),
            "the apps domain `{domain}` must be a lowercase DNS name of at least two labels"
        );
        Ok(Self {
            scheme: url.scheme().to_owned(),
            domain: domain.to_owned(),
            port: url.port(),
        })
    }
}

impl SandboxOrigin {
    /// The origin of `label` under the apps domain, with the sandbox's scheme and port.
    fn under(&self, label: &str) -> String {
        match self.port {
            Some(port) => format!("{}://{label}.{}:{port}", self.scheme, self.domain),
            None => format!("{}://{label}.{}", self.scheme, self.domain),
        }
    }
}

impl fmt::Display for SandboxOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.under(SANDBOX_SLUG))
    }
}

/// Every origin of the installation's Apps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppOrigins {
    host: String,
    sandbox: SandboxOrigin,
    sandbox_origin: String,
}

impl AppOrigins {
    /// The origins of the Hub at `issuer` with its proxy at `sandbox`. Refuses an apps
    /// domain that is the issuer's host or a parent of it, since a server whose slug is
    /// the issuer's first label would then share the host page's origin.
    pub fn new(issuer: &str, sandbox: SandboxOrigin) -> anyhow::Result<Self> {
        let issuer = url::Url::parse(issuer).context("the issuer is not a URL")?;
        let host = issuer.host_str().context("the issuer names no host")?;
        let domain = &sandbox.domain;
        ensure!(
            host != domain && !host.ends_with(&format!(".{domain}")),
            "the apps domain `{domain}` must not be the issuer's host `{host}` or a parent of it"
        );
        Ok(Self {
            host: issuer.origin().ascii_serialization(),
            sandbox_origin: sandbox.to_string(),
            sandbox,
        })
    }

    /// The issuer's origin, where host pages live.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The sandbox proxy's public origin.
    pub fn sandbox(&self) -> &str {
        &self.sandbox_origin
    }

    /// `server`'s own app origin.
    pub fn app_origin(&self, server: &ServerSlug) -> String {
        self.sandbox.under(server.as_str())
    }

    /// The server whose app origin `origin` is, a normalized `scheme://host[:port]`.
    pub fn app_server(&self, origin: &str) -> Option<ServerSlug> {
        let url = url::Url::parse(origin).ok()?;
        let label = url
            .host_str()?
            .strip_suffix(&format!(".{}", self.sandbox.domain))?;
        let server = ServerSlug::parse(label).ok()?;
        (server.as_str() != SANDBOX_SLUG
            && !server.is_reserved()
            && self.app_origin(&server) == origin)
            .then_some(server)
    }

    /// Whether `host` is the apps domain or a name under it.
    pub fn under_domain(&self, host: &str) -> bool {
        host == self.sandbox.domain || host.ends_with(&format!(".{}", self.sandbox.domain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origins(issuer: &str, sandbox: &str) -> AppOrigins {
        AppOrigins::new(issuer, sandbox.parse().unwrap()).unwrap()
    }

    #[test]
    fn app_origins_sit_beside_the_configured_sandbox_origin() {
        let public = origins(
            "https://veoveo.example",
            "https://sandbox.apps.veoveo.example",
        );
        assert_eq!(public.host(), "https://veoveo.example");
        assert_eq!(public.sandbox(), "https://sandbox.apps.veoveo.example");
        let fixture = ServerSlug::parse("fixture").unwrap();
        assert_eq!(
            public.app_origin(&fixture),
            "https://fixture.apps.veoveo.example"
        );
        assert_eq!(
            public.app_server("https://fixture.apps.veoveo.example"),
            Some(fixture.clone())
        );
        let local = origins(
            "http://127.0.0.1:8080",
            "http://sandbox.apps.veoveo.localhost:8081",
        );
        assert_eq!(local.host(), "http://127.0.0.1:8080");
        assert_eq!(local.sandbox(), "http://sandbox.apps.veoveo.localhost:8081");
        assert_eq!(
            local.app_origin(&fixture),
            "http://fixture.apps.veoveo.localhost:8081"
        );
    }

    #[test]
    fn only_a_servers_exact_app_origin_names_it() {
        let public = origins(
            "https://veoveo.example",
            "https://sandbox.apps.veoveo.example",
        );
        for origin in [
            "https://sandbox.apps.veoveo.example",
            "https://veoveo.apps.veoveo.example",
            "http://fixture.apps.veoveo.example",
            "https://fixture.apps.veoveo.example:8443",
            "https://a.fixture.apps.veoveo.example",
            "https://apps.veoveo.example",
            "https://fixture.veoveo.example",
        ] {
            assert_eq!(public.app_server(origin), None, "{origin}");
        }
        assert!(public.under_domain("apps.veoveo.example"));
        assert!(public.under_domain("x.y.apps.veoveo.example"));
        assert!(!public.under_domain("veoveo.example"));
        assert!(!public.under_domain("xapps.veoveo.example"));
    }

    #[test]
    fn a_sandbox_origin_is_one_origin_apart_from_the_issuer() {
        for sandbox in [
            "",
            "sandbox.apps.veoveo.example",
            "https://apps.veoveo.example",
            "https://proxy.apps.veoveo.example",
            "https://sandbox.apps",
            "https://sandbox.Apps.example",
            "https://sandbox.apps_x.example",
            "https://sandbox.apps.veoveo.example/",
            "https://sandbox.apps.veoveo.example/proxy",
            "https://sandbox.apps.veoveo.example?x",
            "https://user@sandbox.apps.veoveo.example",
            "wss://sandbox.apps.veoveo.example",
        ] {
            assert!(sandbox.parse::<SandboxOrigin>().is_err(), "{sandbox}");
        }
        for (issuer, sandbox) in [
            (
                "https://apps.veoveo.example",
                "https://sandbox.apps.veoveo.example",
            ),
            (
                "https://hub.apps.veoveo.example",
                "https://sandbox.apps.veoveo.example",
            ),
            ("https://veoveo.example", "https://sandbox.veoveo.example"),
        ] {
            assert!(
                AppOrigins::new(issuer, sandbox.parse().unwrap()).is_err(),
                "{issuer} {sandbox}"
            );
        }
    }
}
