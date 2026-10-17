//! The SDK's fixture servers registered with the Hub binary on loopback ports, the
//! MCP clients that call them through it, and the RFC 8693 exchanges a server sends.
#![allow(dead_code, reason = "Each integration test uses part of the fixture")]

use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, ProtocolVersion,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use veoveo_hub_sdk::{
    hosting::HostedServer,
    registration::{
        AuthorizationDeclaration, ClientMetadata, RegistrationDeclaration, ServerEndpoint,
        ServerVersion,
        client::{Registration, RegistrationClient},
        statement::ServerKeyPair,
    },
};
use veoveo_types::{ResourceScheme, ServerSlug};

use super::hub::Hub;

/// The bearer tokens a fixture server received.
#[derive(Clone, Default)]
pub struct Bearers(Arc<Mutex<Vec<String>>>);

impl Bearers {
    pub fn last(&self) -> String {
        self.0
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a forwarded token arrived")
    }
}

async fn record(State(bearers): State<Bearers>, request: Request, next: Next) -> Response {
    if let Some(bearer) = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        bearers.0.lock().unwrap().push(bearer.to_owned());
    }
    next.run(request).await
}

/// One fixture server: its key, registration and the bearers it received.
pub struct Served {
    pub key: ServerKeyPair,
    pub metadata: ClientMetadata,
    pub client: RegistrationClient,
    pub registration: Registration,
    pub bearers: Bearers,
}

/// Register `declaration` under `slug` with a server listening on a loopback port, and
/// serve what `build` makes of its registration.
pub async fn serve(
    hub: &Hub,
    http: &reqwest::Client,
    slug: &str,
    declaration: AuthorizationDeclaration,
    build: impl AsyncFnOnce(&Registration, &ClientMetadata, &ServerKeyPair) -> HostedServer,
) -> Served {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let key = ServerKeyPair::generate().unwrap();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            slug: ServerSlug::parse(slug).unwrap(),
            uri_scheme: ResourceScheme::parse(slug).unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(
                url::Url::parse(&format!("http://{address}/{slug}/mcp")).unwrap(),
            )
            .unwrap(),
        },
        declaration,
    )
    .unwrap();
    let issuer = url::Url::parse(&hub.base).unwrap();
    let endpoint = RegistrationClient::discover(http, &issuer).await.unwrap();
    let client = RegistrationClient::new(
        http.clone(),
        endpoint,
        SecretString::from(hub.secret("VEOVEO_REGISTRATION_SECRET").to_owned()),
        ServerKeyPair::from_pkcs8_der(key.pkcs8_der()).unwrap(),
        metadata.clone(),
    )
    .unwrap();
    let registration = client.register().await.unwrap();
    let bearers = Bearers::default();
    let router: Router = build(&registration, &metadata, &key)
        .await
        .into_router()
        .layer(axum::middleware::from_fn_with_state(
            bearers.clone(),
            record,
        ));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Served {
        key,
        metadata,
        client,
        registration,
        bearers,
    }
}

/// A hosted server for `D` that admits the Hub's forwarded tokens at its
/// registered endpoint.
pub fn hosted<D: veoveo_hub_sdk::hosting::DomainServer>(
    registration: &Registration,
    http: &reqwest::Client,
) -> veoveo_hub_sdk::hosting::HostedServerBuilder<
    D,
    veoveo_hub_sdk::hosting::Provided<veoveo_hub_sdk::hosting::Deployment>,
    veoveo_hub_sdk::hosting::Provided<Arc<veoveo_hub_sdk::ForwardedTokenVerifier>>,
    veoveo_hub_sdk::hosting::Missing,
> {
    let endpoint = registration
        .information()
        .metadata
        .registration
        .endpoint
        .url()
        .clone();
    let authority = format!(
        "{}:{}",
        endpoint.host_str().unwrap(),
        endpoint.port().unwrap()
    );
    HostedServer::for_domain::<D>()
        .internal([authority])
        .internal_trust(registration.access_tokens(), http.clone())
        .unwrap()
}

/// An MCP client of the local profile that presents `token`.
pub async fn mcp_client(hub: &Hub, token: String) -> RunningService<RoleClient, ClientConfig> {
    ClientConfig::new(
        ClientCapabilities::builder().enable_tasks().build(),
        Implementation::new("hub-qualification", "0.1.0"),
    )
    .serve_with_lifecycle(
        StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(hub.installation.resource.clone())
                .auth_header(token),
        ),
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await
    .unwrap()
}

/// Relay's answer to `tool`: its structured result, or the error it reported.
pub async fn relay_call(
    client: &RunningService<RoleClient, ClientConfig>,
    tool: &str,
    arguments: Value,
) -> Result<Value, String> {
    let request = CallToolRequestParams::new(format!("relay__{tool}"))
        .with_arguments(arguments.as_object().cloned().unwrap());
    let result = client
        .call_tool(request)
        .await
        .map_err(|error| error.to_string())?;
    if result.is_error == Some(true) {
        return Err(result
            .content
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.clone()))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    result.structured_content.ok_or_else(|| "no result".into())
}

/// An RFC 8693 exchange `server` sends itself, with its own key.
pub async fn exchange(
    hub: &Hub,
    http: &reqwest::Client,
    server: &Served,
    subject: (&str, &str),
    details: Option<Value>,
    resource: Option<&str>,
) -> (reqwest::StatusCode, Value) {
    let token_endpoint = hub.token_endpoint();
    let assertion = server
        .key
        .client_assertion(
            server.registration.client_id(),
            &token_endpoint,
            chrono::Utc::now(),
        )
        .unwrap();
    let mut form = vec![
        (
            "grant_type",
            veoveo_hub_sdk::calls::TOKEN_EXCHANGE_GRANT_TYPE.to_owned(),
        ),
        ("client_id", server.registration.client_id().to_string()),
        (
            "client_assertion_type",
            veoveo_hub_sdk::registration::assertion::CLIENT_ASSERTION_TYPE.to_owned(),
        ),
        ("client_assertion", assertion.expose_secret().to_owned()),
        ("subject_token", subject.0.to_owned()),
        ("subject_token_type", subject.1.to_owned()),
        (
            "resource",
            resource.unwrap_or(&hub.installation.resource).to_owned(),
        ),
    ];
    if let Some(details) = details {
        form.push(("authorization_details", details.to_string()));
    }
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in &form {
        body.append_pair(name, value);
    }
    let response = http
        .post(token_endpoint)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body.finish())
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
}

/// A JWT's claims, without verification.
pub fn decoded_claims(token: &str) -> Value {
    use base64::Engine as _;
    serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap()
}
