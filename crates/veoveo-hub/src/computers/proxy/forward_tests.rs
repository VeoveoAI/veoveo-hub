//! A Computer's page loads and requests through `forward`, against the fake OpenShell
//! gateway, which relays the sandbox's forwarding stream to a local HTTP server standing
//! in for the app, and a disposable database holding the owner's browser session. A
//! working Computer answers. Once its sandbox is gone, a page load goes back to entry,
//! which signs in again because that repairs it, while the event stream and a fetch keep
//! answering unavailable. Every credential and answer is synthetic.
use std::{collections::BTreeSet, sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::Utc;
use http_body_util::BodyExt;
use openshell_sdk::raw::proto::{self as p, ProviderCredentialRefreshStrategy as Strategy};
use veoveo_hub_sdk::{Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{PrincipalId, TenantId};

use super::forward;
use crate::{
    HubCatalogHandle, HubRefreshIssueRequest, HubState,
    computers::{
        ComputerRuntime,
        oauth::{SESSION_COOKIE, digest, entry},
        test_fixture::{OWNER, PUBLIC, catalog, gateway, next_login},
    },
    contract::PrincipalDisplayName,
};

/// The browser's session cookie, a synthetic random selector.
const COOKIE: &str = "5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de";

/// A request a browser sends for `path` under the Computer: a page load, or an API
/// request with the destination a fetch or an event stream names.
fn request(
    id: &str,
    path: &str,
    mode: &str,
    destination: Option<&str>,
    accept: &str,
) -> Request<Body> {
    let mut request = Request::builder()
        .uri(format!("/computer/c/{id}{path}"))
        .header(header::COOKIE, format!("{SESSION_COOKIE}={COOKIE}"))
        .header(header::ACCEPT, accept)
        .header("sec-fetch-mode", mode);
    if let Some(destination) = destination {
        request = request.header("sec-fetch-dest", destination);
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn a_page_load_of_a_computer_without_its_sandbox_goes_back_to_entry() {
    tokio::time::timeout(Duration::from_secs(90), async {
        // The app inside the sandbox.
        let app = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = app.local_addr().unwrap();
        tokio::spawn(
            axum::serve(
                app,
                axum::Router::new().fallback(|| async { "synthetic app page" }),
            )
            .into_future(),
        );
        let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
        fake.forward_to(address);
        let db = crate::test_store::TestDb::new().await;
        let hub = HubState::new(db.a.clone())
            .bind_token_extensions(Default::default())
            .unwrap()
            .bind_oauth_client_resolver(Arc::new(crate::oauth_clients::CatalogOAuthClientResolver))
            .unwrap();
        let catalog = catalog();
        let config = provisioner.config.as_ref().clone();
        let profile = catalog.profile(&config.profile).unwrap();
        let tenant = TenantId::parse("tenant-a").unwrap();
        let principal = Principal {
            id: PrincipalId::parse(OWNER).unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://identity.example").unwrap(),
            subject: TokenSubject::parse("alice").unwrap(),
            tenant: Some(tenant.clone()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: profile.required_scopes.iter().cloned().collect(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let issued = hub
            .issue_refresh_token(HubRefreshIssueRequest {
                authorization_server: &profile.authorization_server,
                protected_resource: &profile.protected_resource,
                oauth_client_id: &config.oauth_client,
                work_context: &config.work_context,
                principal: &principal,
                principal_display_name: &PrincipalDisplayName::new("Owner").unwrap(),
                scopes: &principal.scopes,
                now: Utc::now(),
            })
            .await
            .unwrap();
        // The owner's sign-in: its custody handoff creates the sandbox, then the
        // session is bound to the browser.
        let store = hub.hub_store();
        let computer = store
            .ensure_computer(&tenant, &principal.id, &provisioner.spec)
            .await
            .unwrap();
        let (_, grant) = next_login(&computer);
        let handoff = store
            .begin_computer_handoff(&computer, &grant.lore.digest())
            .await
            .unwrap();
        let pending = store
            .computer(&tenant, &principal.id, &computer.id)
            .await
            .unwrap()
            .unwrap();
        provisioner
            .handoff(&catalog, &pending, grant)
            .await
            .unwrap();
        store
            .lock_computer_handoff(&pending, handoff)
            .await
            .unwrap()
            .unwrap()
            .complete(&issued.grant.family_id, &digest(COOKIE))
            .await
            .unwrap();
        let runtime = ComputerRuntime {
            onboarding: None,
            onboarding_jobs: Default::default(),
            catalog: HubCatalogHandle::new(Arc::new(catalog)),
            hub,
            config: Arc::new(config),
            public: url::Url::parse(&format!("{PUBLIC}/")).unwrap(),
            http: reqwest::Client::new(),
            provisioner,
        };
        let id = computer.id.to_string();
        let page = || {
            request(
                &id,
                "/session/old",
                "navigate",
                Some("document"),
                "text/html",
            )
        };
        let fetch = || {
            request(
                &id,
                "/api/session",
                "cors",
                Some("empty"),
                "application/json",
            )
        };
        let events = || {
            request(
                &id,
                "/api/event",
                "cors",
                Some("empty"),
                "text/event-stream",
            )
        };

        // A working Computer answers its page loads and its requests.
        let response = forward(runtime.clone(), page()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"synthetic app page");
        assert_eq!(
            forward(runtime.clone(), fetch()).await.status(),
            StatusCode::OK
        );

        // Its sandbox is deleted.
        let name = computer.native_name();
        let mut grpc = runtime
            .provisioner
            .gateway
            .client
            .raw_grpc_fresh()
            .await
            .unwrap();
        grpc.delete_sandbox(p::DeleteSandboxRequest {
            workspace_scope: Some(p::workspace_selector(name.clone())),
            name,
            ..Default::default()
        })
        .await
        .unwrap();

        // A page load goes back to entry, also one whose browser names no destination.
        for page in [
            page(),
            request(&id, "/session/old", "navigate", None, "text/html"),
        ] {
            let response = forward(runtime.clone(), page).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert_eq!(response.headers()[header::LOCATION], "/computer");
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "private, no-store"
            );
        }
        // The event stream, a fetch and a nested document keep answering unavailable.
        let frame = request(&id, "/session/old", "navigate", Some("iframe"), "text/html");
        for request in [events(), fetch(), frame] {
            let response = forward(runtime.clone(), request).await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!response.headers().contains_key(header::LOCATION));
        }

        // Entry, with the same session, signs in again instead of returning to the
        // Computer, since the new sign-in's handoff creates the sandbox.
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE}={COOKIE}").parse().unwrap(),
        );
        let response = entry(runtime.clone(), headers).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers()[header::LOCATION].to_str().unwrap();
        assert!(
            location.starts_with(&format!("{PUBLIC}/oauth/authorize?")),
            "{location}"
        );
    })
    .await
    .expect("the Computer forwarding test exceeded its bound");
}
