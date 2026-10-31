//! The Apps host's documents: each App's host page, the sandbox proxy page, the host
//! script, and the plain pages that say why no App opens.
use std::sync::LazyLock;

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_hub_sdk::apps::UiCsp;

use super::{csp::AppPolicy, origins::AppOrigins, route::AppView};

/// The host page's script, served at [`host_script_path`].
const HOST_JS: &str = include_str!("host.js");
/// The sandbox proxy's page, whose host origin the Hub fills in.
const PROXY_HTML: &str = include_str!("proxy.html");
const HOST_ORIGIN_SLOT: &str = "\"{{HOST_ORIGIN}}\"";
/// Where the host page loads its script: a path named by the script's own digest, so a
/// cache anywhere between the Hub and the browser may keep it for good, and a new Hub's
/// host pages, which no cache keeps, name its new script.
static HOST_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let digest = Sha256::digest(HOST_JS.as_bytes());
    format!("/apps/host-{}.js", hex::encode(&digest[..16]))
});

pub fn host_script_path() -> &'static str {
    &HOST_SCRIPT
}

/// What the host page tells its script, as JSON data.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostConfig<'a> {
    uri: &'a str,
    title: &'a str,
    proxy: String,
    sandbox: &'a str,
    csp: UiCsp,
    host: HostInfo,
}

#[derive(Serialize)]
struct HostInfo {
    name: &'static str,
    version: &'static str,
}

/// JSON a `<script type="application/json">` element can hold: no `<`, `>` or `&` that
/// could end it.
fn script_json(value: &impl Serialize) -> String {
    serde_json::to_string(value)
        .expect("host data serializes")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn private(mut response: Response, policy: &str) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    match HeaderValue::from_str(policy) {
        Ok(policy) => {
            headers.insert(header::CONTENT_SECURITY_POLICY, policy);
            response
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn html(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

/// The host page of `view`: its script and the App's data, under the host's policy.
pub(super) fn host_page(
    view: &AppView,
    title: &str,
    policy: &AppPolicy,
    origins: &AppOrigins,
) -> Response {
    let query = policy.query();
    let config = HostConfig {
        uri: view.uri(),
        title,
        proxy: if query.is_empty() {
            format!("{}/", origins.sandbox())
        } else {
            format!("{}/?{query}", origins.sandbox())
        },
        sandbox: origins.sandbox(),
        csp: policy.csp(),
        host: HostInfo {
            name: "veoveo-hub",
            version: env!("CARGO_PKG_VERSION"),
        },
    };
    let body = format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title>\
         <script type=\"application/json\" id=\"app\">{}</script>\
         <script src=\"{}\" defer></script></head><body></body></html>\n",
        escaped(title),
        script_json(&config),
        host_script_path(),
    );
    let policy = format!(
        "default-src 'none'; script-src 'self'; connect-src 'self'; frame-src {}; \
         img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        origins.sandbox()
    );
    private(html(StatusCode::OK, body), &policy)
}

/// The host page's script, immutable at its path.
pub(super) fn host_script() -> Response {
    let mut response = (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        HOST_JS,
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// The sandbox proxy's page under `policy`, the App's, for the host at `origins`.
pub(super) fn proxy_page(policy: &AppPolicy, origins: &AppOrigins) -> Response {
    let body = PROXY_HTML.replace(HOST_ORIGIN_SLOT, &script_json(&origins.host()));
    private(html(StatusCode::OK, body), &policy.header(origins))
}

/// A plain page that says why no App opens.
pub(super) fn message(status: StatusCode, title: &str, text: &str) -> Response {
    let body = format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>{0}</title></head>\
         <body><h1>{0}</h1><p>{1}</p></body></html>\n",
        escaped(title),
        escaped(text),
    );
    private(
        html(status, body),
        "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_data_cannot_end_its_script_element() {
        let data =
            script_json(&serde_json::json!({"title": "</script><script>alert(1)</script>&"}));
        assert!(!data.contains('<') && !data.contains('>') && !data.contains('&'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&data).unwrap()["title"],
            "</script><script>alert(1)</script>&"
        );
    }

    #[test]
    fn the_proxy_page_names_its_host_once_and_runs_no_markup_from_it() {
        assert_eq!(PROXY_HTML.matches(HOST_ORIGIN_SLOT).count(), 1);
        let origins = AppOrigins::new(
            "https://veoveo.example",
            "https://sandbox.apps.veoveo.example".parse().unwrap(),
        )
        .unwrap();
        let response = proxy_page(&AppPolicy::default(), &origins);
        assert_eq!(
            response.headers()[header::CONTENT_SECURITY_POLICY],
            AppPolicy::default().header(&origins).as_str()
        );
    }
}
