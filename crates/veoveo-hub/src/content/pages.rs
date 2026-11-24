//! The content domain's two documents: the Hub's page, which frames a file, and the page on the
//! content origin that opens a file with `file-viewer`.
use std::sync::LazyLock;

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::apps::pages::{escaped, html, private, script_json};

const VIEW_JS: &str = include_str!("view.js");
/// Where the page loads its script: a path named by the script's own digest, as the App host
/// script's is.
static VIEW_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let digest = Sha256::digest(VIEW_JS.as_bytes());
    format!("/apps/view-{}.js", hex::encode(&digest[..16]))
});

pub(super) fn view_script_path() -> &'static str {
    &VIEW_SCRIPT
}

/// The page's script, immutable at its path.
pub(super) fn view_script() -> Response {
    let mut response = (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        VIEW_JS,
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

/// What the page tells its script.
#[derive(Serialize)]
struct ViewConfig<'a> {
    origin: &'a str,
}

/// The Hub's page around `src` on the content origin `origin`, whose `frame-src` is `frames`.
pub(super) fn view_page(title: &str, origin: &str, src: &str, frames: &str) -> Response {
    let body = format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title>\
         <script type=\"application/json\" id=\"view\">{}</script>\
         <script src=\"{}\" defer></script></head><body>\
         <iframe id=\"content\" title=\"{0}\" src=\"{}\" \
         sandbox=\"allow-scripts allow-same-origin allow-forms allow-popups allow-downloads\" \
         allow=\"webgpu; autoplay\"></iframe></body></html>\n",
        escaped(title),
        script_json(&ViewConfig { origin }),
        view_script_path(),
        escaped(src),
    );
    let policy = format!(
        "default-src 'none'; script-src 'self'; connect-src 'self'; frame-src {frames}; \
         img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    );
    let mut response = private(html(StatusCode::OK, body), &policy);
    // Nothing between here and the browser adds to the page, as a CDN's analytics beacon would.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, no-transform"),
    );
    response
}

/// What the viewer page tells `file-viewer`.
#[derive(Serialize)]
struct ViewerConfig<'a> {
    path: &'a str,
    filename: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<&'static str>,
}

/// `file-viewer` needs an absolute asset URL and a declared charset, or it renders blank.
const MOUNT_JS: &str = r#"
const file = JSON.parse(document.getElementById("file").textContent);
const viewer = window.FlyfishFileViewerWebFull;
viewer.setDefaultFullAssetBaseUrl(location.origin + "/_veoveo/viewer/");
const options = {
  url: location.origin + file.path,
  filename: file.filename,
};
if (file.type) options.type = file.type;
viewer.mountViewer(document.getElementById("viewer"), options);
"#;

/// The page on the content origin that opens the file at `path`, raw from its URL, with
/// `file-viewer`, which reaches nothing but this origin.
pub(super) fn viewer_page(path: &str, filename: &str) -> Response {
    let body = format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title><style>html,body,#viewer{{margin:0;height:100%}}</style></head>\
         <body><div id=\"viewer\"></div>\
         <script type=\"application/json\" id=\"file\">{}</script>\
         <script src=\"/_veoveo/viewer/flyfish-file-viewer-web-full.iife.js\"></script>\
         <script>{MOUNT_JS}</script></body></html>\n",
        escaped(filename),
        script_json(&ViewerConfig {
            path,
            filename,
            r#type: filename
                .rsplit_once('.')
                .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("rhai"))
                .then_some("txt"),
        }),
    );
    let mut response = html(StatusCode::OK, body);
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self' blob: data:; \
             script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' blob:; \
             style-src 'self' 'unsafe-inline'; connect-src 'self' blob: data:; \
             worker-src 'self' blob:; img-src 'self' blob: data:; media-src 'self' blob: data:; \
             font-src 'self' data:",
        ),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn viewer_body(path: &str, filename: &str) -> (serde_json::Value, String) {
        let response = viewer_page(path, filename);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        let json = body
            .split_once("id=\"file\">")
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0;
        (serde_json::from_str(json).unwrap(), body)
    }

    #[tokio::test]
    async fn rhai_uses_native_plain_text_hint_without_renaming_the_source() {
        for filename in ["rules.rhai", "RULES.RHAI", "rules.RhAi"] {
            let (file, body) = viewer_body("/exact/source/rules.rhai", filename).await;
            assert_eq!(
                file,
                serde_json::json!({"path":"/exact/source/rules.rhai", "filename":filename, "type":"txt"})
            );
            assert!(body.contains("if (file.type) options.type = file.type;"));
            assert!(body.contains("filename: file.filename"));
        }
        for filename in [
            "rules.txt",
            "rules.html",
            "rules.usda",
            "rules.docx",
            "rules.rhai.txt",
            "rhai",
        ] {
            let (file, _) = viewer_body("/exact/source", filename).await;
            assert_eq!(
                file,
                serde_json::json!({"path":"/exact/source", "filename":filename})
            );
        }
    }

    #[tokio::test]
    async fn malicious_source_name_is_data_in_the_plain_text_viewer_page() {
        let filename = "</script><img src=x onerror=alert(1)>.rhai";
        let path = "/exact/%3Cscript%3E.rhai";
        let (file, body) = viewer_body(path, filename).await;
        assert_eq!(file["filename"], filename);
        assert_eq!(file["path"], path);
        assert_eq!(file["type"], "txt");
        assert!(!body.contains("<img"));
        assert!(!body.contains("</script><img"));
        assert!(body.contains("\\u003c/script\\u003e"));
    }
}
