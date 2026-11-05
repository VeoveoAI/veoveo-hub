//! The content domain: untrusted content and every viewer on `{token}.{content domain}`,
//! another registrable domain from the issuer's, so a browser sends it none of the Hub's or an
//! App's cookies. `/view/{owner}/{item}/{path}` on the issuer's origin mints a person's token
//! and frames the file; the Apps listener answers the token's origin, checking the person's
//! access on every request. Owners supply the items, and a server that declares content is one;
//! the Hub names no server. A server's link has an origin too, `{slug}-{token}.{content domain}`,
//! which the server checks on every request. `DESIGN.md`'s Content Domain section is the rule.
mod owned;
mod pages;
mod serve;
#[cfg(test)]
mod tests;
mod token;
mod view;

use std::{collections::BTreeMap, fmt, path::PathBuf, str::FromStr, sync::Arc};

use anyhow::{Context, ensure};
use axum::{
    http::{HeaderMap, Method},
    response::Response,
};
use futures::future::BoxFuture;
use veoveo_hub_sdk::ServerSlug;

pub use view::view_return_path;

use crate::{
    HubCatalog, HubCatalogHandle, HubState,
    apps::{AppOrigins, ServerOrigins},
    computers::BrowserSession,
    problem::Problem,
    surfaces::PublicRoutes,
};

/// The installation's content domain: an `https` or `http` origin, spelled as one, whose host is
/// a lowercase DNS name of at least two labels. `{token}.{domain}` is one item's origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentOrigin {
    scheme: String,
    domain: String,
    port: Option<u16>,
}

impl FromStr for ContentOrigin {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        let url = url::Url::parse(value).context("the content origin is not a URL")?;
        ensure!(
            matches!(url.scheme(), "https" | "http") && url.origin().ascii_serialization() == value,
            "the content origin `{value}` must be an https or http origin, spelled as one"
        );
        let domain = url.host_str().context("the content origin names no host")?;
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
            "the content domain `{domain}` must be a lowercase DNS name of at least two labels"
        );
        Ok(Self {
            scheme: url.scheme().to_owned(),
            domain: domain.to_owned(),
            port: url.port(),
        })
    }
}

impl fmt::Display for ContentOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.domain)?;
        match self.port {
            Some(port) => write!(f, ":{port}"),
            None => Ok(()),
        }
    }
}

impl ContentOrigin {
    /// `{scheme}://{label}.{domain}`, with the domain's port.
    fn under(&self, label: &str) -> String {
        match self.port {
            Some(port) => format!("{}://{label}.{}:{port}", self.scheme, self.domain),
            None => format!("{}://{label}.{}", self.scheme, self.domain),
        }
    }

    /// Whether `host` is the domain, or either lies under the other.
    fn overlaps(&self, host: &str) -> bool {
        self.domain == host
            || self.domain.ends_with(&format!(".{host}"))
            || host.ends_with(&format!(".{}", self.domain))
    }

    /// What stands before the domain in the one `X-Forwarded-Host` of a request, when its name
    /// lies under the domain.
    fn label(&self, headers: &HeaderMap) -> Option<String> {
        let mut values = headers.get_all("x-forwarded-host").iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return None;
        };
        let host = value.to_str().ok()?.to_ascii_lowercase();
        let suffix = match self.port {
            Some(port) => format!(".{}:{port}", self.domain),
            None => format!(".{}", self.domain),
        };
        host.strip_suffix(&suffix).map(str::to_owned)
    }
}

/// Refuse a content domain at or under the issuer's host or the Apps domain, or above either.
fn check_apart(origins: &AppOrigins, origin: &ContentOrigin) -> anyhow::Result<()> {
    for neighbour in [origins.host(), origins.sandbox()] {
        let url = url::Url::parse(neighbour).context("an App origin is not a URL")?;
        let host = url.host_str().context("an App origin names no host")?;
        let host = host.strip_prefix("sandbox.").unwrap_or(host);
        ensure!(
            !origin.overlaps(host),
            "the content domain {origin} must stand apart from {host}"
        );
    }
    Ok(())
}

/// What a `/view/{owner}/...` URL names, after the owner: an item, canonically spelled so that it
/// names one token and is the URL's own text, and a raw path in it, which may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub item: String,
    pub path: String,
}

/// What supplies items on the content domain, under its `{owner}` in `/view/`. The Lore
/// contribution is one; a server that declares content is another.
pub trait ContentOwner: Send + Sync + 'static {
    /// What `rest`, the canonical URL path after `/view/{owner}/`, names; `None` when it names
    /// nothing this owner holds.
    fn named(&self, rest: &str) -> Option<Named>;

    /// Whether the person of `session` may read `item` now, before the Hub answers anything of
    /// the item's origin itself, a viewer's file among them.
    fn admit<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
    ) -> BoxFuture<'a, Result<(), Problem>>;

    /// The person of `session` opens `item` on the Hub's page: [`Self::admit`], and whatever an
    /// owner keeps of a visit, once however many requests it makes.
    fn visit<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
    ) -> BoxFuture<'a, Result<(), Problem>> {
        self.admit(catalog, session, item)
    }

    /// `path`, raw from the URL, in `item`, as the person of `session`, to a `method` request of
    /// `headers`, refused as [`Self::admit`] would. A file of unknown type answers
    /// `application/octet-stream`, and the Hub names its type.
    fn read<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
        path: &'a str,
        method: &'a Method,
        headers: &'a HeaderMap,
    ) -> BoxFuture<'a, Result<Response, Problem>>;
}

/// The content domain's service: `/view/` on the issuer's listener, and token origins on the
/// Apps listener.
#[derive(Clone)]
pub struct ContentDomain {
    catalog: HubCatalogHandle,
    hub: HubState,
    origin: ContentOrigin,
    issuer: String,
    owners: Arc<BTreeMap<String, Arc<dyn ContentOwner>>>,
    viewer_files: PathBuf,
    public_routes: PublicRoutes,
    servers: ServerOrigins,
}

impl ContentDomain {
    /// The service of `origin`, which `origins`' issuer and Apps domain must stand apart from,
    /// with `file-viewer`'s files at `viewer_files`, the items of `owners` and those of the
    /// servers that declare content, which `servers` reaches as the person.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        catalog: HubCatalogHandle,
        hub: HubState,
        origins: &AppOrigins,
        origin: ContentOrigin,
        viewer_files: PathBuf,
        public_routes: PublicRoutes,
        servers: ServerOrigins,
        owners: impl IntoIterator<Item = (String, Arc<dyn ContentOwner>)>,
    ) -> anyhow::Result<Self> {
        check_apart(origins, &origin)?;
        Ok(Self {
            catalog,
            hub,
            origin,
            issuer: origins.host().to_owned(),
            owners: Arc::new(owners.into_iter().collect()),
            viewer_files,
            public_routes,
            servers,
        })
    }

    /// The owner named `name`: one of the installation's, or the server of that slug when it
    /// declares content.
    fn owner(&self, catalog: &HubCatalog, name: &str) -> Option<Arc<dyn ContentOwner>> {
        if let Some(owner) = self.owners.get(name) {
            return Some(owner.clone());
        }
        let server = ServerSlug::parse(name).ok()?;
        let route = self.routed(catalog, &server)?.surfaces.as_ref()?;
        let route = route.content.as_ref()?.route.clone();
        Some(Arc::new(owned::ServerContent::new(
            self.servers.clone(),
            server,
            route,
        )))
    }
}
