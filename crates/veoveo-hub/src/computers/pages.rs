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

/// The way in, where the start page reloads until the Computer is ready.
pub(super) const ENTRY: &str = "/computer";
/// The status page.
pub(super) const STATUS: &str = "/computer/status";

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

/// `text` as the end of a sentence: with a period, unless it ends with its own
/// punctuation, as a release note's summary may.
fn ended(text: &str) -> String {
    if text.ends_with(['.', '!', '?']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

/// Where a network endpoint, `host:port` and any path, lets the Computer connect, in
/// words: the host, its port unless it is HTTPS's or HTTP's, and the path.
fn place(endpoint: &str) -> String {
    let (address, path) = match endpoint.find('/') {
        Some(index) => endpoint.split_at(index),
        None => (endpoint, ""),
    };
    let place = match address.rsplit_once(':') {
        Some((host, "443" | "80")) => format!("{host}{path}"),
        Some((host, port)) => format!("{host}{path} on port {port}"),
        None => format!("{address}{path}"),
    };
    escaped(&place)
}

/// `places` as a sentence's list: "a", "a and b", "a, b and c".
fn listed(places: &[String]) -> String {
    match places {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What a version brings, for people: its release note, that the software was
/// updated, and where the Computer can now connect or no longer. Image references and
/// rule names stay in the JSON status.
fn whats_new(changes: &WhatsNew) -> String {
    let mut items = String::new();
    if let Some(note) = &changes.note {
        let _ = write!(items, "<li>{}</li>", escaped(note).replace('\n', "<br>"));
    }
    if changes.image.is_some() {
        items.push_str("<li>The Computer's software was updated.</li>");
    }
    for (rules, sentence) in [
        (
            &changes.network_rules_added,
            "Your Computer can now connect to",
        ),
        (
            &changes.network_rules_removed,
            "Your Computer can no longer connect to",
        ),
    ] {
        let places: std::collections::BTreeSet<String> = rules
            .iter()
            .flat_map(|rule| &rule.endpoints)
            .map(|endpoint| place(endpoint))
            .collect();
        if !places.is_empty() {
            let places: Vec<String> = places.into_iter().collect();
            let _ = write!(items, "<li>{sentence} {}.</li>", listed(&places));
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
    page(
        StatusCode::OK,
        "Starting your Computer",
        "<meta http-equiv=refresh content=2>",
        &starting_body(target, changes),
    )
}

fn starting_body(target: Option<&VersionInfo>, changes: Option<&WhatsNew>) -> String {
    let mut body = String::new();
    match target {
        Some(target) => {
            let _ = write!(
                body,
                "<p>Your Computer is starting, at {}</p>",
                ended(&version(target))
            );
        }
        None => body.push_str("<p>Your Computer is starting.</p>"),
    }
    body.push_str("<p>This page continues to your Computer when it is ready.</p>");
    if let Some(changes) = changes {
        body.push_str(&whats_new(changes));
    }
    let _ = write!(body, "<p><a href=\"{STATUS}\">Status</a></p>");
    body
}

/// The Computer's status, with Restart, and after a failure Start the previous version.
pub(super) fn status(status: &ComputerStatus) -> Response {
    page(StatusCode::OK, "Your Computer", "", &status_body(status))
}

const FILES_KEPT: &str = "with your files as you left them";

/// What the person's Computer is doing, in plain words, and what they can do: open
/// it, wait for it, or restart it. The JSON status keeps the sandbox's phase.
fn state(status: &ComputerStatus) -> String {
    let open = format!("<a href=\"{ENTRY}\">Open your Computer</a>");
    if let Some(restart) = &status.restart {
        return format!(
            "<p>Your Computer is restarting, since {}, to {} <a href=\"{ENTRY}\">Follow it</a></p>",
            escaped(&restart.since.to_rfc3339()),
            ended(&version(&restart.version))
        );
    }
    match status.sandbox {
        "ready" if status.blocked.is_some() => String::new(),
        "ready" => format!("<p>Your Computer is running. {open}</p>"),
        "none" if status.failure.is_some() => String::new(),
        "none" if status.collected.is_some() => format!(
            "<p>Your Computer is asleep while you're away; opening it starts it again, {FILES_KEPT}. {open}</p>"
        ),
        "none" | "stopped" | "completed" => format!(
            "<p>Your Computer is not running; opening it starts it, {FILES_KEPT}. {open}</p>"
        ),
        "stopping" | "deleting" => format!(
            "<p>Your Computer is stopping. Opening it starts it again once it has stopped, {FILES_KEPT}. {open}</p>"
        ),
        "error" => format!(
            "<p>Your Computer stopped with an error. Restart starts it again, {FILES_KEPT}.</p>"
        ),
        _ => format!(
            "<p>Your Computer is starting. <a href=\"{ENTRY}\">Open it</a> to wait until it is ready.</p>"
        ),
    }
}

fn status_body(status: &ComputerStatus) -> String {
    let mut body = String::new();
    if let Some(blocked) = &status.blocked {
        let _ = write!(
            body,
            "<p><strong>Your Computer needs a restart:</strong> the Hub can no longer serve it safely, because {} Restart starts it again, {FILES_KEPT}.</p>",
            ended(&escaped(blocked))
        );
    }
    if let Some(failure) = &status.failure {
        let _ = write!(
            body,
            "<p><strong>Your Computer could not start version <code>{}</code></strong> at {}: {} Restart tries again; Start the previous version starts the version you had.</p>",
            escaped(failure.version.as_str()),
            escaped(&failure.at.to_rfc3339()),
            ended(&escaped(&failure.cause))
        );
    }
    body.push_str(&state(status));
    let _ = write!(
        body,
        "<p>Your Computer is at {}</p>",
        ended(&version(&status.version))
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
            let _ = write!(
                body,
                "<p>An update is available: {}</p>",
                ended(&version(current))
            );
            if let Some(changes) = &status.whats_new {
                body.push_str(&whats_new(changes));
            }
        }
        (_, Some(unavailable)) => {
            let _ = write!(
                body,
                "<p>No Computer can start now: {}</p>",
                ended(&escaped(unavailable))
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
    body
}

/// No Computer can start now, and why.
pub(super) fn unavailable(cause: &str) -> Response {
    // The Hub logs the cause when Computers become unavailable: this page is an
    // expected server error, not a failure of the request.
    crate::http::expected(page(
        StatusCode::SERVICE_UNAVAILABLE,
        "Computers are unavailable",
        "",
        &format!(
            "<p>{}</p><p>A Computer that runs is still served. Try again later.</p>",
            ended(&escaped(cause))
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computers::{ComputerFailure, ComputerId, ImageChange, NetworkRule, RestartStatus};
    use chrono::{TimeZone, Utc};

    fn info(version: &str, summary: Option<&str>) -> VersionInfo {
        serde_json::from_value(serde_json::json!({"version": version, "summary": summary})).unwrap()
    }

    fn computer(sandbox: &'static str) -> ComputerStatus {
        ComputerStatus {
            computer: ComputerId::new(),
            version: info("223df6dc9a5c", Some("Restarts and updates.")),
            sandbox,
            current: None,
            current_unavailable: None,
            update_available: false,
            whats_new: None,
            restart: None,
            failure: None,
            blocked: None,
            updated: None,
            collected: None,
        }
    }

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 23, 23, 48, 29).unwrap()
    }

    #[test]
    fn each_state_says_what_happens_and_what_to_do() {
        let mut collected = computer("none");
        collected.collected = Some(at());
        let asleep = status_body(&collected);
        assert!(
            asleep.contains(
                "Your Computer is asleep while you're away; opening it starts it again, with your files as you left them."
            ),
            "{asleep}"
        );
        assert!(asleep.contains("Open your Computer"));

        for (sandbox, says) in [
            ("ready", "Your Computer is running."),
            (
                "stopping",
                "Your Computer is stopping. Opening it starts it again once it has stopped",
            ),
            ("deleting", "Your Computer is stopping."),
            ("provisioning", "Your Computer is starting."),
            ("starting", "Your Computer is starting."),
            ("none", "Your Computer is not running; opening it starts it"),
            (
                "error",
                "Your Computer stopped with an error. Restart starts it again",
            ),
        ] {
            let body = status_body(&computer(sandbox));
            assert!(body.contains(says), "{sandbox}: {body}");
            assert!(!body.contains("sandbox"), "{sandbox}: {body}");
        }

        let mut failed = computer("none");
        failed.failure = Some(ComputerFailure {
            cause: "creating the sandbox: the previous Computer is still stopping; try again"
                .into(),
            at: at(),
            version: serde_json::from_value(serde_json::json!("9a1b2c3d4e5f")).unwrap(),
        });
        let body = status_body(&failed);
        assert!(
            body.contains("Your Computer could not start version <code>9a1b2c3d4e5f</code>"),
            "{body}"
        );
        assert!(
            body.contains(
                "the previous Computer is still stopping; try again. Restart tries again"
            )
        );
        assert!(body.contains("Start the previous version"));
        assert!(
            !body.contains("Open your Computer"),
            "a failure is restarted: {body}"
        );

        let mut restarting = computer("ready");
        restarting.restart = Some(RestartStatus {
            version: info("9a1b2c3d4e5f", None),
            since: at(),
        });
        let body = status_body(&restarting);
        assert!(body.contains("Your Computer is restarting"), "{body}");
        assert!(body.contains("Follow it"));
    }

    #[test]
    fn whats_new_is_told_in_words_without_image_references() {
        let changes = WhatsNew {
            note: Some("Faster builds".into()),
            image: Some(ImageChange {
                from: "registry.veoveo-system.svc/computer@sha256:0a1b".into(),
                to: "registry.veoveo-system.svc/computer@sha256:2c3d".into(),
            }),
            network_rules_added: vec![
                NetworkRule {
                    name: "model".into(),
                    endpoints: vec!["api.deepinfra.com:443".into()],
                },
                NetworkRule {
                    name: "lore".into(),
                    endpoints: vec![
                        "lore.veoveo-system.svc:41337".into(),
                        "api.deepinfra.com:443".into(),
                    ],
                },
            ],
            network_rules_removed: vec![NetworkRule {
                name: "old".into(),
                endpoints: vec!["example.com:80/v1".into()],
            }],
        };
        let text = whats_new(&changes);
        assert!(text.contains("<li>Faster builds</li>"), "{text}");
        assert!(
            text.contains("<li>The Computer's software was updated.</li>"),
            "{text}"
        );
        assert!(
            text.contains(
                "<li>Your Computer can now connect to api.deepinfra.com and lore.veoveo-system.svc on port 41337.</li>"
            ),
            "{text}"
        );
        assert!(
            text.contains("<li>Your Computer can no longer connect to example.com/v1.</li>"),
            "{text}"
        );
        for raw in ["sha256", "registry.", "model", "Network rule"] {
            assert!(!text.contains(raw), "{raw} in {text}");
        }
        for page in [
            starting_body(Some(&info("9a1b2c3d4e5f", None)), Some(&changes)),
            status_body(&ComputerStatus {
                update_available: true,
                current: Some(info("9a1b2c3d4e5f", None)),
                whats_new: Some(changes.clone()),
                ..computer("ready")
            }),
        ] {
            assert!(!page.contains("sha256"), "{page}");
            assert!(
                page.contains("The Computer's software was updated."),
                "{page}"
            );
        }
    }

    #[test]
    fn a_summary_with_its_own_period_gets_no_second_one_on_any_page() {
        let summary = info("223df6dc9a5c", Some("Restarts and updates."));
        let mut computer = computer("ready");
        computer.current = Some(summary.clone());
        computer.update_available = true;
        computer.whats_new = Some(WhatsNew::default());
        computer.restart = Some(RestartStatus {
            version: summary.clone(),
            since: at(),
        });
        computer.blocked = Some("its command changed.".into());
        computer.current_unavailable = Some("template `computer` does not exist.".into());
        for page in [starting_body(Some(&summary), None), status_body(&computer)] {
            assert!(page.contains("Restarts and updates."), "{page}");
            assert!(!page.contains(".."), "{page}");
        }
    }

    #[test]
    fn a_summary_ends_its_sentence_once() {
        assert_eq!(ended("Faster builds"), "Faster builds.");
        assert_eq!(ended("Your home is kept."), "Your home is kept.");
        assert_eq!(ended("Try it now!"), "Try it now!");
    }

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
