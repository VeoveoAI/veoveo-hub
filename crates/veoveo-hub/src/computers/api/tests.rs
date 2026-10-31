//! The Computer's actions on one path for both callers, against the fake OpenShell
//! gateway and a disposable database holding the owner's ready Computer: a bearer gets
//! JSON, `202` for a restart and `409` while it waits to begin; the owner's browser
//! session gets the status page and, from this origin only, a redirect to `/computer`
//! after an action; a browser without a session goes to `/computer` to sign in. The
//! bearer's subject is a synthetic fixture with the owner's tenant and principal; the
//! Hub's bearer admission itself is not involved.
use std::time::Duration;

use axum::{
    Extension,
    http::{HeaderMap, HeaderValue, StatusCode, header},
};
use http_body_util::BodyExt;
use openshell_sdk::raw::proto::ProviderCredentialRefreshStrategy as Strategy;
use veoveo_types::{PrincipalId, TenantId};

use super::{keep_on, restart, status, stop, version};
use crate::computers::{
    oauth::{SESSION_COOKIE, digest},
    test_fixture::{
        OWNER, PUBLIC, catalog, gateway, hub, installation, installation_template, ready_computer,
        runtime,
    },
};

const COOKIE: &str = "c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ff";

async fn json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn a_bearer_gets_json_and_a_browser_gets_pages_on_the_same_paths() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
        let db = crate::test_store::TestDb::new().await;
        let hub = hub(&db);
        let catalog = catalog();
        let computer = ready_computer(&hub, &catalog, &provisioner, &digest(COOKIE)).await;
        provisioner
            .replace(&computer, uuid::Uuid::now_v7())
            .await
            .unwrap();
        let runtime = runtime(hub, catalog, installation(&provisioner));
        let mut subject = crate::mcp::task_ownership_tests::subject();
        subject.principal.id = PrincipalId::parse(OWNER).unwrap();
        subject.principal.tenant = Some(TenantId::parse("tenant-a").unwrap());
        let bearer = || Some(Extension(subject.clone()));

        let answer = status(runtime.clone(), bearer(), HeaderMap::new()).await;
        assert_eq!(answer.status(), StatusCode::OK);
        let answer = json(answer).await;
        assert_eq!(answer["computer"], computer.id.to_string());
        assert_eq!(answer["sandbox"], "ready");
        assert_eq!(answer["updateAvailable"], false);

        // A restart answers at once and waits before it begins, so a second one is
        // refused while the first runs.
        let answer = restart(runtime.clone(), bearer(), HeaderMap::new()).await;
        assert_eq!(answer.status(), StatusCode::ACCEPTED);
        assert_eq!(
            json(answer).await["restart"]["version"]["version"],
            computer.provisioning.version().unwrap().as_str()
        );
        let again = restart(runtime.clone(), bearer(), HeaderMap::new()).await;
        assert_eq!(again.status(), StatusCode::CONFLICT);

        // Another person with no Computer has none to restart.
        let mut stranger = subject.clone();
        stranger.principal.id = PrincipalId::parse("https://identity.example#bob").unwrap();
        let none = restart(runtime.clone(), Some(Extension(stranger)), HeaderMap::new()).await;
        assert_eq!(none.status(), StatusCode::NOT_FOUND);

        // A browser without a session signs in at /computer; with one, it gets the page.
        let page = status(runtime.clone(), None, HeaderMap::new()).await;
        assert_eq!(page.status(), StatusCode::SEE_OTHER);
        assert_eq!(page.headers()[header::LOCATION], "/computer");
        let mut session = HeaderMap::new();
        session.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE}={COOKIE}")).unwrap(),
        );
        let page = status(runtime.clone(), None, session.clone()).await;
        assert_eq!(page.status(), StatusCode::OK);
        let text = page.into_body().collect().await.unwrap().to_bytes();
        assert!(
            std::str::from_utf8(&text)
                .unwrap()
                .contains("Your Computer is restarting")
        );

        // A browser action needs this origin, then follows the restart at /computer.
        let refused = restart(runtime.clone(), None, session.clone()).await;
        assert_eq!(refused.status(), StatusCode::FORBIDDEN);
        session.insert(header::ORIGIN, HeaderValue::from_static(PUBLIC));
        let followed = restart(runtime.clone(), None, session).await;
        assert_eq!(followed.status(), StatusCode::SEE_OTHER);
        assert_eq!(followed.headers()[header::LOCATION], "/computer");

        // The public version names only the digest and the release note's first line.
        fake.remove_template("installation", "computer");
        fake.set_template(
            "installation",
            installation_template(
                "registry.internal/computer@sha256:9",
                Some("Faster builds\nInternal detail of the build host"),
            ),
        );
        let current = version(runtime.clone()).await;
        assert_eq!(current.status(), StatusCode::OK);
        let current = json(current).await;
        let members: Vec<_> = current.as_object().unwrap().keys().cloned().collect();
        assert_eq!(members, ["summary", "version"]);
        assert_eq!(current["summary"], "Faster builds");
    })
    .await
    .expect("the action test exceeded its bound");
}

#[tokio::test]
async fn an_owner_keeps_the_computer_on_or_stops_it() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
        let db = crate::test_store::TestDb::new().await;
        let hub = hub(&db);
        let catalog = catalog();
        let computer = ready_computer(&hub, &catalog, &provisioner, &digest(COOKIE)).await;
        provisioner
            .replace(&computer, uuid::Uuid::now_v7())
            .await
            .unwrap();
        let store = hub.hub_store();
        let runtime = runtime(hub.clone(), catalog, installation(&provisioner));
        let mut subject = crate::mcp::task_ownership_tests::subject();
        subject.principal.id = PrincipalId::parse(OWNER).unwrap();
        subject.principal.tenant = Some(TenantId::parse("tenant-a").unwrap());
        let bearer = || Some(Extension(subject.clone()));
        let json_body = |keep: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            (
                headers,
                axum::body::Bytes::from(format!("{{\"keepOn\":{keep}}}")),
            )
        };
        let collectable = || async {
            store
                .collectable_computers(chrono::Utc::now())
                .await
                .unwrap()
                .iter()
                .any(|collectable| collectable.id == computer.id)
        };
        assert!(collectable().await);

        // Kept on, collection never considers it, and its status says so.
        let (headers, body) = json_body("true");
        let kept = keep_on(runtime.clone(), bearer(), headers, body).await;
        assert_eq!(kept.status(), StatusCode::NO_CONTENT);
        assert!(!collectable().await);
        let answer = json(status(runtime.clone(), bearer(), HeaderMap::new()).await).await;
        assert_eq!(answer["keepOn"], true);
        let (headers, _) = json_body("true");
        let refused = keep_on(runtime.clone(), bearer(), headers, "{\"keepOn\":1}".into()).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

        // The status page's form turns it off, from this origin only.
        let mut session = HeaderMap::new();
        session.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE}={COOKIE}")).unwrap(),
        );
        let form = || axum::body::Bytes::from_static(b"keepOn=false");
        let refused = keep_on(runtime.clone(), None, session.clone(), form()).await;
        assert_eq!(refused.status(), StatusCode::FORBIDDEN);
        session.insert(header::ORIGIN, HeaderValue::from_static(PUBLIC));
        let turned = keep_on(runtime.clone(), None, session.clone(), form()).await;
        assert_eq!(turned.status(), StatusCode::SEE_OTHER);
        assert_eq!(turned.headers()[header::LOCATION], "/computer/status");
        assert!(collectable().await);

        // Stopping deletes its sandbox and keeps its home, kept on or not; a second stop
        // has nothing to stop.
        let (headers, body) = json_body("true");
        keep_on(runtime.clone(), bearer(), headers, body).await;
        let name = computer.native_name();
        assert!(fake.sandbox(&name, &name).is_some());
        let stopped = stop(runtime.clone(), bearer(), HeaderMap::new()).await;
        assert_eq!(stopped.status(), StatusCode::NO_CONTENT);
        assert!(fake.sandbox(&name, &name).is_none());
        let answer = json(status(runtime.clone(), bearer(), HeaderMap::new()).await).await;
        assert_eq!(answer["sandbox"], "none");
        assert!(answer["collected"].is_string(), "{answer}");
        assert_eq!(answer["keepOn"], true, "stopping keeps the setting");
        assert_eq!(
            stop(runtime.clone(), bearer(), HeaderMap::new())
                .await
                .status(),
            StatusCode::NO_CONTENT
        );
        // The owner's browser stops it from this origin, then reads the status page.
        let stopped = stop(runtime.clone(), None, session).await;
        assert_eq!(stopped.status(), StatusCode::SEE_OTHER);
        assert_eq!(stopped.headers()[header::LOCATION], "/computer/status");

        // While a restart holds its running sandbox, a stop is refused.
        provisioner
            .replace(&computer, uuid::Uuid::now_v7())
            .await
            .unwrap();
        assert_eq!(
            restart(runtime.clone(), bearer(), HeaderMap::new())
                .await
                .status(),
            StatusCode::ACCEPTED
        );
        let busy = stop(runtime.clone(), bearer(), HeaderMap::new()).await;
        assert_eq!(busy.status(), StatusCode::CONFLICT);
    })
    .await
    .expect("the keep-on test exceeded its bound");
}
