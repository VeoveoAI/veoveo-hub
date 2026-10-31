//! `/apps/{slug}/{page}`: the one canonical path of the App `ui://{slug}/{page}`.
use veoveo_hub_sdk::ServerSlug;
use veoveo_types::ResourceUri;

/// The slug whose name is the sandbox proxy's origin. No server registers it, and no App
/// path names it.
pub const SANDBOX_SLUG: &str = "sandbox";

/// The longest page, in bytes.
const MAX_PAGE_BYTES: usize = 512;

/// One App: its server and its `ui://` resource, from a canonical path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppView {
    server: ServerSlug,
    page: String,
    uri: ResourceUri,
}

impl AppView {
    /// The App a raw request path names, or None for any other spelling: the slug a
    /// server slug other than `sandbox`, and the page one or more segments of RFC 3986
    /// unreserved characters, none empty, `.` or `..`. Percent-encoding is another
    /// spelling, so one App has one path.
    pub fn from_path(path: &str) -> Option<Self> {
        let (slug, page) = path.strip_prefix("/apps/")?.split_once('/')?;
        let server = ServerSlug::parse(slug).ok()?;
        if server.as_str() != slug || server.as_str() == SANDBOX_SLUG || server.is_reserved() {
            return None;
        }
        if page.is_empty()
            || page.len() > MAX_PAGE_BYTES
            || page.split('/').any(|segment| {
                segment.is_empty()
                    || segment == "."
                    || segment == ".."
                    || !segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-._~".contains(&byte))
            })
        {
            return None;
        }
        let uri = ResourceUri::new(format!("ui://{slug}/{page}")).ok()?;
        Some(Self {
            server,
            page: page.to_owned(),
            uri,
        })
    }

    /// The App's canonical path.
    pub fn path(&self) -> String {
        format!("/apps/{}/{}", self.server, self.page)
    }

    /// `ui://{slug}/{page}`.
    pub fn uri(&self) -> &str {
        self.uri.as_str()
    }

    /// The server whose App this is.
    pub fn server(&self) -> &ServerSlug {
        &self.server
    }

    /// Whether `uri` is a resource of the App's own server: a server's URI scheme is its
    /// slug, and its Apps live under `ui://{slug}/`.
    pub fn owns_resource(&self, uri: &str) -> bool {
        if ResourceUri::new(uri.to_owned()).is_err() {
            return false;
        }
        uri.starts_with(&format!("ui://{}/", self.server))
            || crate::resource_scheme_from_uri(uri)
                .is_some_and(|scheme| scheme.as_str() == self.server.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_canonical_path_names_one_app() {
        let view = AppView::from_path("/apps/fixture/board.html").unwrap();
        assert_eq!(view.uri(), "ui://fixture/board.html");
        assert_eq!(view.server().as_str(), "fixture");
        assert_eq!(view.path(), "/apps/fixture/board.html");
        let nested = AppView::from_path("/apps/fixture/editor/v2~draft_1.html").unwrap();
        assert_eq!(nested.uri(), "ui://fixture/editor/v2~draft_1.html");
        assert_eq!(AppView::from_path(&nested.path()), Some(nested));
    }

    #[test]
    fn other_spellings_and_reserved_slugs_name_no_app() {
        for path in [
            "/apps/fixture",
            "/apps/fixture/",
            "/apps//board.html",
            "/apps/fixture//board.html",
            "/apps/fixture/board.html/",
            "/apps/fixture/./board.html",
            "/apps/fixture/../other/board.html",
            "/apps/fixture/%62oard.html",
            "/apps/fi%78ture/board.html",
            "/apps/Fixture/board.html",
            "/apps/fixture/board html",
            "/apps/fixture/board.html?x=1",
            "/apps/fixture/board:html",
            "/apps/sandbox/proxy.html",
            "/apps/veoveo/board.html",
            "/apps/https/board.html",
            "/apps/pg-catalog/board.html",
            "/app/fixture/board.html",
            "apps/fixture/board.html",
        ] {
            assert!(AppView::from_path(path).is_none(), "{path} names no App");
        }
        let long = format!("/apps/fixture/{}", "a".repeat(MAX_PAGE_BYTES + 1));
        assert!(AppView::from_path(&long).is_none());
        let longest = format!("/apps/fixture/{}", "a".repeat(MAX_PAGE_BYTES));
        assert!(AppView::from_path(&longest).is_some());
    }

    #[test]
    fn an_app_owns_only_its_own_servers_resources() {
        let view = AppView::from_path("/apps/fixture/board.html").unwrap();
        for uri in [
            "fixture://items/1",
            "fixture://docs",
            "fixture://events?cursor=eyJhZnRlciI6MX0%3D",
            "ui://fixture/board.html",
            "ui://fixture/other.html",
        ] {
            assert!(view.owns_resource(uri), "{uri} is the App's server's");
        }
        for uri in [
            "other://items/1",
            "ui://other/board.html",
            "ui://fixtures/board.html",
            "fixtures://items/1",
            "ui://fixture",
            "fixture",
            "",
        ] {
            assert!(!view.owns_resource(uri), "{uri} is not the App's server's");
        }
    }
}
