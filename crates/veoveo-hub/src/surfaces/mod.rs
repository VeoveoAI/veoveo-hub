//! The HTTP a registered server serves besides MCP, which it declares in its
//! registration's surfaces and the Hub routes: its public routes, which carry no Hub
//! authentication, on the issuer's origin under `/instances/{deployment}/`.
use axum::http::{HeaderMap, HeaderName, header};

mod public;

pub use public::PublicRoutes;

/// Remove the hop-by-hop fields, those `Connection` nominates included. Content
/// encoding and length stay: the entity's bytes pass unchanged and hyper frames them.
pub(crate) fn strip_hop(headers: &mut HeaderMap) {
    let nominated = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| HeaderName::from_bytes(value.trim().as_bytes()).ok())
        .collect::<Vec<_>>();
    for name in nominated {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

/// Whether `path` is a path of nonempty segments with no dot segment and no encoded
/// slash, dot or backslash, so a server's router reads it as the Hub matched it.
fn plain_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    path.starts_with('/')
        && !lower.contains("%2f")
        && !lower.contains("%2e")
        && !lower.contains("%5c")
        && !path.contains('\\')
        && path[1..].split('/').enumerate().all(|(index, segment)| {
            let last = index == path[1..].split('/').count() - 1;
            (!segment.is_empty() || last) && segment != "." && segment != ".."
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_path_is_matched_against_a_prefix() {
        for path in ["/n8n/hooks/a", "/n8n/hooks/", "/n8n/hooks/a/b"] {
            assert!(plain_path(path), "{path}");
        }
        for path in [
            "n8n/hooks/a",
            "/n8n/hooks/../mcp",
            "/n8n/hooks/./a",
            "/n8n//hooks/a",
            "/n8n/hooks/%2e%2e/mcp",
            "/n8n/hooks%2Fa",
            "/n8n/hooks/%5c",
            "/n8n/hooks\\a",
        ] {
            assert!(!plain_path(path), "{path}");
        }
    }
}
