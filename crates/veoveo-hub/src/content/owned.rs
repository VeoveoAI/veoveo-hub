//! A server's items on the content domain: the Hub reads them from the route the server declares,
//! as the person, and the server decides each read. The Hub holds nothing about them.
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, Method, StatusCode, header},
    response::Response,
};
use futures::future::BoxFuture;
use veoveo_hub_sdk::{ServerSlug, registration::PublicRoutePrefix};

use super::{ContentOwner, Named};
use crate::{
    HubCatalog, apps::ServerOrigins, computers::BrowserSession, problem::Problem,
    surfaces::plain_path,
};

/// What of a browser's request a server's file answers by.
const READ_HEADERS: [header::HeaderName; 3] =
    [header::RANGE, header::IF_RANGE, header::IF_NONE_MATCH];

pub(super) struct ServerContent {
    servers: ServerOrigins,
    server: ServerSlug,
    route: PublicRoutePrefix,
}

impl ServerContent {
    pub(super) fn new(
        servers: ServerOrigins,
        server: ServerSlug,
        route: PublicRoutePrefix,
    ) -> Self {
        Self {
            servers,
            server,
            route,
        }
    }

    /// `{route}{item}`, whose answer says whether the person may read it.
    async fn decide(
        &self,
        catalog: &HubCatalog,
        session: &BrowserSession,
        item: &str,
        method: Method,
    ) -> Result<(), Problem> {
        let path = format!("{}{item}", self.route);
        if !plain_path(&path) {
            return Err(Problem::not_found());
        }
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .map_err(|_| Problem::not_found())?;
        let answer = self
            .servers
            .content_request(catalog, session, &self.server, request)
            .await;
        match answer.status() {
            status if status.is_success() => Ok(()),
            StatusCode::NOT_FOUND => Err(Problem::not_found()),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(Problem::new(
                StatusCode::FORBIDDEN,
                "content-refused",
                "You cannot read this item",
            )),
            status => {
                tracing::warn!(server = %self.server, %status, "a server did not answer for an item");
                Err(Problem::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "content-unavailable",
                    "This file is unavailable",
                ))
            }
        }
    }
}

impl ContentOwner for ServerContent {
    /// The first segment is the item, and the rest its path, which may be empty: the server knows
    /// what to do with the item's top.
    fn named(&self, rest: &str) -> Option<Named> {
        let (item, path) = rest.split_once('/').unwrap_or((rest, ""));
        (!item.is_empty()).then(|| Named {
            item: item.to_owned(),
            path: path.to_owned(),
        })
    }

    fn admit<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
    ) -> BoxFuture<'a, Result<(), Problem>> {
        Box::pin(self.decide(catalog, session, item, Method::HEAD))
    }

    fn visit<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
    ) -> BoxFuture<'a, Result<(), Problem>> {
        Box::pin(self.decide(catalog, session, item, Method::POST))
    }

    fn read<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
        path: &'a str,
        method: &'a Method,
        headers: &'a HeaderMap,
    ) -> BoxFuture<'a, Result<Response, Problem>> {
        Box::pin(async move {
            let target = format!("{}{item}/{path}", self.route);
            // Only a plain path stays under the route on its way to the server.
            if !plain_path(&target) {
                return Err(Problem::not_found());
            }
            let mut request = Request::builder().method(method.clone()).uri(target);
            for name in READ_HEADERS {
                if let Some(value) = headers.get(&name) {
                    request = request.header(name, value);
                }
            }
            let request = request
                .body(Body::empty())
                .map_err(|_| Problem::not_found())?;
            Ok(self
                .servers
                .content_request(catalog, session, &self.server, request)
                .await)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owned_path_names_an_item_and_a_path_in_it() {
        let named = |rest: &str| {
            let (item, path) = rest.split_once('/').unwrap_or((rest, ""));
            (!item.is_empty()).then(|| (item.to_owned(), path.to_owned()))
        };
        assert_eq!(named("a1/b.png"), Some(("a1".into(), "b.png".into())));
        assert_eq!(named("a1/"), Some(("a1".into(), String::new())));
        assert_eq!(named("a1"), Some(("a1".into(), String::new())));
        assert_eq!(named("/b.png"), None);
    }

    #[test]
    fn no_path_climbs_out_of_the_route() {
        for path in [
            "/studio/content/a1/../../mcp",
            "/studio/content/a1/%2e%2e/%2e%2e/mcp",
            "/studio/content/a1/x%2Fy",
            "/studio/content/a1//x",
        ] {
            assert!(!plain_path(path), "{path}");
        }
        assert!(plain_path("/studio/content/a1/a%20b.png"));
        assert!(plain_path("/studio/content/a1/"));
    }
}
