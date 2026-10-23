//! The Computer's plain pages: the start page shown while its sandbox is created or
//! started, the status page with Restart, and the page of Computers being unavailable.
//! No script or style; the start page reloads itself.
use std::fmt::Write as _;

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};

use super::{
    lifecycle::ComputerStatus,
    oauth::private,
    version::{VersionInfo, WhatsNew},
};

/// Where the start page goes again until the Computer is ready.
pub(super) const OPEN: &str = "/computer/open";

fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn page(status: StatusCode, title: &str, head: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><meta charset=utf-8>{head}<title>{title}</title><h1>{title}</h1>{body}",
        title = escaped(title),
    );
    let mut response = private((status, Html(html)).into_response());
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    );
    response
}

fn version(info: &VersionInfo) -> String {
    match &info.summary {
        Some(summary) => format!(
            "version <code>{}</code>: {}",
            escaped(info.version.as_str()),
            escaped(summary)
        ),
        None => format!("version <code>{}</code>", escaped(info.version.as_str())),
    }
}

fn whats_new(changes: &WhatsNew) -> String {
    let mut items = String::new();
    if let Some(note) = &changes.note {
        let _ = write!(items, "<li>{}</li>", escaped(note).replace('\n', "<br>"));
    }
    if let Some(image) = &changes.image {
        let _ = write!(
            items,
            "<li>New image <code>{}</code>, was <code>{}</code></li>",
            escaped(&image.to),
            escaped(&image.from)
        );
    }
    for (rules, verb) in [
        (&changes.network_rules_added, "now allows"),
        (&changes.network_rules_removed, "no longer allows"),
    ] {
        for rule in rules {
            let _ = write!(
                items,
                "<li>Network rule <code>{}</code> {verb} {}</li>",
                escaped(&rule.name),
                rule.endpoints
                    .iter()
                    .map(|endpoint| format!("<code>{}</code>", escaped(endpoint)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    if items.is_empty() {
        "<p>No changes are listed.</p>".to_owned()
    } else {
        format!("<h2>What's new</h2><ul>{items}</ul>")
    }
}

const KEPT: &str = "<p>A restart keeps your home, <code>~</code>. It ends running programs and empties <code>/tmp</code>.</p>";

/// Shown while the sandbox is created or started; it reloads itself and continues to
/// the Computer once it is ready.
pub(super) fn starting(target: Option<&VersionInfo>, changes: Option<&WhatsNew>) -> Response {
    let mut body = String::new();
    match target {
        Some(target) => {
            let _ = write!(body, "<p>Starting {}.</p>", version(target));
        }
        None => body.push_str("<p>Your Computer is starting.</p>"),
    }
    body.push_str("<p>This page continues to your Computer when it is ready.</p>");
    if let Some(changes) = changes {
        body.push_str(&whats_new(changes));
    }
    body.push_str("<p><a href=\"/computer\">Status</a></p>");
    page(
        StatusCode::OK,
        "Starting your Computer",
        "<meta http-equiv=refresh content=2>",
        &body,
    )
}

/// The Computer's status, with Restart, and after a failure Start the previous version.
pub(super) fn status(status: &ComputerStatus) -> Response {
    let mut body = String::new();
    if let Some(blocked) = &status.blocked {
        let _ = write!(
            body,
            "<p><strong>The Hub can no longer serve this Computer safely:</strong> {}. Restart creates it again.</p>",
            escaped(blocked)
        );
    }
    if let Some(failure) = &status.failure {
        let _ = write!(
            body,
            "<p><strong>Starting version <code>{}</code> failed at {}:</strong> {}</p>",
            escaped(failure.version.as_str()),
            escaped(&failure.at.to_rfc3339()),
            escaped(&failure.cause)
        );
    }
    if let Some(restart) = &status.restart {
        let _ = write!(
            body,
            "<p>Restarting to {} since {}. <a href=\"{OPEN}\">Follow it</a></p>",
            version(&restart.version),
            escaped(&restart.since.to_rfc3339())
        );
    } else if status.sandbox == "ready" && status.blocked.is_none() {
        let _ = write!(body, "<p><a href=\"{OPEN}\">Open your Computer</a></p>");
    } else if status.failure.is_none() {
        let _ = write!(body, "<p><a href=\"{OPEN}\">Start your Computer</a></p>");
    }
    let _ = write!(
        body,
        "<p>Your Computer is at {}. Its sandbox is {}.</p>",
        version(&status.version),
        status.sandbox
    );
    if let Some(updated) = &status.updated {
        let _ = write!(
            body,
            "<p>Updated from version <code>{}</code> to <code>{}</code>.</p>{}",
            escaped(updated.from.as_str()),
            escaped(updated.to.as_str()),
            whats_new(&updated.whats_new)
        );
    }
    match (&status.current, &status.current_unavailable) {
        (Some(current), _) if status.update_available => {
            let _ = write!(body, "<p>An update is available: {}.</p>", version(current));
            if let Some(changes) = &status.whats_new {
                body.push_str(&whats_new(changes));
            }
        }
        (_, Some(unavailable)) => {
            let _ = write!(
                body,
                "<p>No Computer can start now: {}</p>",
                escaped(unavailable)
            );
        }
        _ => {}
    }
    if status.restart.is_none() {
        let label = if status.failure.is_some() {
            "Restart (try again)"
        } else {
            "Restart"
        };
        let _ = write!(
            body,
            "<form method=post action=\"/computer/restart\"><button>{label}</button></form>"
        );
        if status.failure.is_some() {
            let _ = write!(
                body,
                "<form method=post action=\"/computer/previous\"><button>Start the previous version, <code>{}</code></button></form>",
                escaped(status.version.version.as_str())
            );
        }
        body.push_str(KEPT);
    }
    page(StatusCode::OK, "Your Computer", "", &body)
}

/// No Computer can start now, and why.
pub(super) fn unavailable(cause: &str) -> Response {
    page(
        StatusCode::SERVICE_UNAVAILABLE,
        "Computers are unavailable",
        "",
        &format!(
            "<p>{}.</p><p>A Computer that runs is still served. Try again later.</p>",
            escaped(cause)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_text_is_escaped() {
        assert_eq!(
            escaped("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        let response = unavailable("template `<computer>` does not exist");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert!(
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .starts_with("default-src 'none'")
        );
    }
}
