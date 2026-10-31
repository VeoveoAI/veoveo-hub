//! An App's Content Security Policy, built from its validated `_meta.ui.csp` as MCP Apps
//! `2026-01-26` constructs it.
use rmcp::model::Resource;
use veoveo_hub_sdk::{
    ServerSlug,
    apps::{UI_META_KEY, UiCsp, resource_ui_meta},
};

use super::origins::AppOrigins;

/// The most sources one directive takes.
const MAX_SOURCES: usize = 32;

/// Why the Hub does not admit an App's declared policy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("the App's `_meta.ui` does not parse")]
    Metadata,
    #[error("`{list}` declares more than {MAX_SOURCES} origins")]
    TooMany { list: &'static str },
    #[error("`{list}` names `{origin}`, which is not one exact origin it admits")]
    NotAnOrigin { list: &'static str, origin: String },
    #[error("`{list}` names `{origin}`, an origin of this installation's host or Apps")]
    Installation { list: &'static str, origin: String },
    #[error("`frameDomains` names `{origin}`; an App frames only its own server's app origin")]
    Frame { origin: String },
    #[error("the sandbox query is malformed")]
    Query,
}

/// One directive's declared sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum List {
    Connect,
    Resource,
    Frame,
    Base,
}

impl List {
    const ALL: [Self; 4] = [Self::Connect, Self::Resource, Self::Frame, Self::Base];

    /// The `_meta.ui.csp` member.
    fn member(self) -> &'static str {
        match self {
            Self::Connect => "connectDomains",
            Self::Resource => "resourceDomains",
            Self::Frame => "frameDomains",
            Self::Base => "baseUriDomains",
        }
    }

    /// The sandbox query parameter.
    fn parameter(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Resource => "resource",
            Self::Frame => "frame",
            Self::Base => "base",
        }
    }

    fn schemes(self) -> &'static [&'static str] {
        match self {
            Self::Connect => &["https", "http", "wss", "ws"],
            Self::Resource | Self::Frame | Self::Base => &["https", "http"],
        }
    }
}

/// Which app origins a `frame` source may name.
enum Frames<'a> {
    /// Only this server's: an App's declaration.
    Own(&'a ServerSlug),
    /// Any server's: the sandbox query, which the host page built from a declaration.
    AnyServer,
}

/// An App's admitted sources, each one exact origin, sorted and without duplicates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppPolicy {
    connect: Vec<String>,
    resource: Vec<String>,
    frame: Vec<String>,
    base: Vec<String>,
}

impl AppPolicy {
    /// The policy of `server`'s listed App `resource`. Without `_meta.ui.csp` every list
    /// is empty, the restrictive default.
    pub fn admit(
        resource: &Resource,
        server: &ServerSlug,
        origins: &AppOrigins,
    ) -> Result<Self, PolicyError> {
        let declared = resource
            .meta
            .as_ref()
            .is_some_and(|meta| meta.0.contains_key(UI_META_KEY));
        let csp = match resource_ui_meta(resource) {
            Some(meta) => meta.csp.unwrap_or_default(),
            None if declared => return Err(PolicyError::Metadata),
            None => UiCsp::default(),
        };
        let mut policy = Self::default();
        for list in List::ALL {
            let values = match list {
                List::Connect => &csp.connect_domains,
                List::Resource => &csp.resource_domains,
                List::Frame => &csp.frame_domains,
                List::Base => &csp.base_uri_domains,
            };
            *policy.list_mut(list) = admit_list(list, values, Frames::Own(server), origins)?;
        }
        Ok(policy)
    }

    /// The policy a sandbox query names, validated again.
    pub fn from_query(query: &str, origins: &AppOrigins) -> Result<Self, PolicyError> {
        if query.len() > 16 * 1024 {
            return Err(PolicyError::Query);
        }
        let mut values: [Vec<String>; 4] = Default::default();
        for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
            let index = List::ALL
                .iter()
                .position(|list| list.parameter() == name)
                .ok_or(PolicyError::Query)?;
            values[index].push(value.into_owned());
        }
        let mut policy = Self::default();
        for (list, values) in List::ALL.into_iter().zip(&values) {
            *policy.list_mut(list) = admit_list(list, values, Frames::AnyServer, origins)?;
        }
        Ok(policy)
    }

    fn list_mut(&mut self, list: List) -> &mut Vec<String> {
        match list {
            List::Connect => &mut self.connect,
            List::Resource => &mut self.resource,
            List::Frame => &mut self.frame,
            List::Base => &mut self.base,
        }
    }

    fn list(&self, list: List) -> &[String] {
        match list {
            List::Connect => &self.connect,
            List::Resource => &self.resource,
            List::Frame => &self.frame,
            List::Base => &self.base,
        }
    }

    /// The sandbox query that names this policy.
    pub fn query(&self) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for list in List::ALL {
            for source in self.list(list) {
                query.append_pair(list.parameter(), source);
            }
        }
        query.finish()
    }

    /// The policy as `_meta.ui.csp` declares it, normalized, for the sandbox proxy.
    pub fn csp(&self) -> UiCsp {
        UiCsp {
            connect_domains: self.connect.clone(),
            resource_domains: self.resource.clone(),
            frame_domains: self.frame.clone(),
            base_uri_domains: self.base.clone(),
        }
    }

    /// The sandbox proxy's `Content-Security-Policy`: the App's policy, which its
    /// `about:srcdoc` frame inherits, and the one host origin that may frame the proxy.
    pub fn header(&self, origins: &AppOrigins) -> String {
        let sources = |list: &[String], always: &str, empty: &str| {
            let mut sources: Vec<&str> = Vec::new();
            if !always.is_empty() {
                sources.push(always);
            }
            sources.extend(list.iter().map(String::as_str));
            if sources.is_empty() {
                sources.push(empty);
            }
            sources.join(" ")
        };
        let resource = &self.resource;
        [
            "default-src 'none'".to_owned(),
            format!(
                "script-src {}",
                sources(resource, "'self' 'unsafe-inline'", "")
            ),
            format!(
                "style-src {}",
                sources(resource, "'self' 'unsafe-inline'", "")
            ),
            format!("img-src {}", sources(resource, "'self' data:", "")),
            format!("font-src {}", sources(resource, "'self'", "")),
            format!("media-src {}", sources(resource, "'self' data:", "")),
            format!("connect-src {}", sources(&self.connect, "", "'none'")),
            format!("frame-src {}", sources(&self.frame, "", "'none'")),
            format!("base-uri {}", sources(&self.base, "", "'self'")),
            "object-src 'none'".to_owned(),
            format!("frame-ancestors {}", origins.host()),
        ]
        .join("; ")
    }
}

fn admit_list(
    list: List,
    values: &[String],
    frames: Frames<'_>,
    origins: &AppOrigins,
) -> Result<Vec<String>, PolicyError> {
    if values.len() > MAX_SOURCES {
        return Err(PolicyError::TooMany {
            list: list.member(),
        });
    }
    let mut admitted = Vec::with_capacity(values.len());
    for value in values {
        let source =
            exact_origin(value, list.schemes()).ok_or_else(|| PolicyError::NotAnOrigin {
                list: list.member(),
                origin: value.clone(),
            })?;
        let host = url::Url::parse(&source)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        if list == List::Frame {
            let own = match frames {
                Frames::Own(server) => origins.app_origin(server) == source,
                Frames::AnyServer => origins.app_server(&source).is_some(),
            };
            if !own {
                return Err(PolicyError::Frame { origin: source });
            }
        } else if source == origins.host()
            || source == origins.sandbox()
            || origins.under_domain(&host)
        {
            return Err(PolicyError::Installation {
                list: list.member(),
                origin: source,
            });
        }
        admitted.push(source);
    }
    admitted.sort();
    admitted.dedup();
    Ok(admitted)
}

/// `value` as `scheme://host[:port]` when it is one exact origin of `schemes`: a host,
/// and no user information, path, query, fragment or wildcard.
fn exact_origin(value: &str, schemes: &[&str]) -> Option<String> {
    if value.contains('*')
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b';' || byte == b',' || byte == b'\'')
    {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    let host = url.host_str()?;
    if !schemes.contains(&url.scheme())
        || host.is_empty()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
        || value.ends_with('?')
        || value.ends_with('#')
    {
        return None;
    }
    Some(match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    })
}

#[cfg(test)]
mod tests {
    use veoveo_hub_sdk::apps::{ResourceUiMeta, app_resource, app_resource_with_meta};

    use super::*;

    fn origins() -> AppOrigins {
        AppOrigins::new(
            "https://veoveo.example",
            "https://sandbox.apps.veoveo.example".parse().unwrap(),
        )
        .unwrap()
    }

    fn fixture() -> ServerSlug {
        ServerSlug::parse("fixture").unwrap()
    }

    fn declared(csp: UiCsp) -> Resource {
        app_resource_with_meta(
            "ui://fixture/board.html",
            "board",
            ResourceUiMeta {
                csp: Some(csp),
                prefers_border: None,
            },
        )
    }

    #[test]
    fn an_app_without_a_declaration_gets_the_restrictive_default() {
        let origins = origins();
        let policy = AppPolicy::admit(
            &app_resource("ui://fixture/board.html", "board"),
            &fixture(),
            &origins,
        )
        .unwrap();
        assert_eq!(policy, AppPolicy::default());
        assert_eq!(
            policy.header(&origins),
            "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; font-src 'self'; media-src 'self' data:; connect-src 'none'; \
             frame-src 'none'; base-uri 'self'; object-src 'none'; frame-ancestors https://veoveo.example"
        );
        let mut plain = Resource::new("ui://fixture/board.html", "board");
        plain.mime_type = Some(veoveo_hub_sdk::apps::APP_MIME_TYPE.to_owned());
        assert_eq!(
            AppPolicy::admit(&plain, &fixture(), &origins).unwrap(),
            AppPolicy::default()
        );
    }

    #[test]
    fn declared_origins_reach_exactly_their_directives() {
        let origins = origins();
        let policy = AppPolicy::admit(
            &declared(UiCsp {
                connect_domains: vec![
                    "wss://stream.example.net".into(),
                    "https://api.example.net/".into(),
                ],
                resource_domains: vec![
                    "https://cdn.example.net:8443".into(),
                    "https://cdn.example.net:8443".into(),
                ],
                frame_domains: vec!["https://fixture.apps.veoveo.example".into()],
                base_uri_domains: vec!["https://base.example.net".into()],
            }),
            &fixture(),
            &origins,
        )
        .unwrap();
        assert_eq!(
            policy.header(&origins),
            "default-src 'none'; script-src 'self' 'unsafe-inline' https://cdn.example.net:8443; \
             style-src 'self' 'unsafe-inline' https://cdn.example.net:8443; \
             img-src 'self' data: https://cdn.example.net:8443; font-src 'self' https://cdn.example.net:8443; \
             media-src 'self' data: https://cdn.example.net:8443; \
             connect-src https://api.example.net wss://stream.example.net; \
             frame-src https://fixture.apps.veoveo.example; base-uri https://base.example.net; \
             object-src 'none'; frame-ancestors https://veoveo.example"
        );
        assert_eq!(
            AppPolicy::from_query(&policy.query(), &origins),
            Ok(policy.clone())
        );
        assert_eq!(
            policy.csp().connect_domains,
            ["https://api.example.net", "wss://stream.example.net"]
        );
    }

    #[test]
    fn frame_domains_admit_only_the_apps_own_servers_app_origin() {
        let origins = origins();
        for frame in [
            "https://other.apps.veoveo.example",
            "https://sandbox.apps.veoveo.example",
            "https://veoveo.example",
            "https://fixture.apps.veoveo.example:8443",
            "http://fixture.apps.veoveo.example",
            "https://www.example.net",
        ] {
            let resource = declared(UiCsp {
                frame_domains: vec![frame.into()],
                ..UiCsp::default()
            });
            assert!(
                matches!(
                    AppPolicy::admit(&resource, &fixture(), &origins),
                    Err(PolicyError::Frame { .. })
                ),
                "{frame}"
            );
        }
        // The sandbox query admits any server's app origin, and no other frame.
        assert!(
            AppPolicy::from_query("frame=https%3A%2F%2Fother.apps.veoveo.example", &origins)
                .is_ok()
        );
        for query in [
            "frame=https%3A%2F%2Fsandbox.apps.veoveo.example",
            "frame=https%3A%2F%2Fwww.example.net",
            "frame=https%3A%2F%2Fveoveo.example",
        ] {
            assert!(AppPolicy::from_query(query, &origins).is_err(), "{query}");
        }
    }

    #[test]
    fn a_source_is_one_exact_origin_outside_the_installation() {
        let origins = origins();
        for connect in [
            "*.example.net",
            "https://*.example.net",
            "https://example.net/path",
            "https://example.net/?q",
            "https://example.net?q=1",
            "https://example.net#f",
            "https://user@example.net",
            "https://user:pass@example.net",
            "ftp://example.net",
            "data:",
            "'self'",
            "https://example.net 'unsafe-eval'",
            "https://example.net;script-src *",
            "example.net",
            "",
        ] {
            let resource = declared(UiCsp {
                connect_domains: vec![connect.into()],
                ..UiCsp::default()
            });
            assert!(
                matches!(
                    AppPolicy::admit(&resource, &fixture(), &origins),
                    Err(PolicyError::NotAnOrigin { .. })
                ),
                "{connect}"
            );
        }
        for connect in [
            "https://veoveo.example",
            "https://sandbox.apps.veoveo.example",
            "https://fixture.apps.veoveo.example",
            "wss://other.apps.veoveo.example",
            "https://apps.veoveo.example",
        ] {
            let resource = declared(UiCsp {
                connect_domains: vec![connect.into()],
                ..UiCsp::default()
            });
            assert!(
                matches!(
                    AppPolicy::admit(&resource, &fixture(), &origins),
                    Err(PolicyError::Installation { .. })
                ),
                "{connect}"
            );
        }
        let resource = declared(UiCsp {
            resource_domains: vec!["wss://stream.example.net".into()],
            ..UiCsp::default()
        });
        assert!(
            AppPolicy::admit(&resource, &fixture(), &origins).is_err(),
            "resources load over HTTP only"
        );
        let many = declared(UiCsp {
            connect_domains: (0..=MAX_SOURCES)
                .map(|n| format!("https://h{n}.example.net"))
                .collect(),
            ..UiCsp::default()
        });
        assert!(matches!(
            AppPolicy::admit(&many, &fixture(), &origins),
            Err(PolicyError::TooMany { .. })
        ));
    }

    #[test]
    fn a_metadata_that_does_not_parse_is_refused() {
        let mut resource = app_resource("ui://fixture/board.html", "board");
        resource.meta.as_mut().unwrap().0.insert(
            UI_META_KEY.to_owned(),
            serde_json::json!({"csp": {"connectDomains": "https://api.example.net"}}),
        );
        assert_eq!(
            AppPolicy::admit(&resource, &fixture(), &origins()),
            Err(PolicyError::Metadata)
        );
    }

    #[test]
    fn a_query_names_only_known_lists() {
        let origins = origins();
        assert_eq!(
            AppPolicy::from_query("", &origins),
            Ok(AppPolicy::default())
        );
        assert_eq!(
            AppPolicy::from_query("script=https%3A%2F%2Fa.example.net", &origins),
            Err(PolicyError::Query)
        );
        assert!(AppPolicy::from_query("connect=https%3A%2F%2Fveoveo.example", &origins).is_err());
    }
}
