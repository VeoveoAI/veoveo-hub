//! MCP requests the Apps host sends for the person to the Hub's own profile endpoint, in
//! process, so authentication, selection, policy, routing and audit apply unchanged.
use axum::{
    Router,
    body::Body,
    http::{HeaderValue, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::model::Tool;
use serde_json::{Map, Value, json};
use tower::ServiceExt as _;
use veoveo_hub_sdk::{HubProfileId, apps::host_extension_capability};

/// The MCP protocol version the Hub serves.
const PROTOCOL_VERSION: &str = "2026-07-28";
const PROTOCOL_VERSION_META: &str = "io.modelcontextprotocol/protocolVersion";
const CLIENT_INFO_META: &str = "io.modelcontextprotocol/clientInfo";
const CLIENT_CAPABILITIES_META: &str = "io.modelcontextprotocol/clientCapabilities";
/// The client the Apps host names in every request.
pub(super) const CLIENT_NAME: &str = "veoveo-hub-apps";
/// The most list pages the host reads to find an App or a tool.
const MAX_PAGES: usize = 100;

/// What the Hub's MCP endpoint answered.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Answer {
    /// The JSON-RPC result.
    Result(Value),
    /// The JSON-RPC error object.
    Error(Value),
}

/// Why a request has no answer.
#[derive(Debug, thiserror::Error)]
pub(super) enum Failure {
    /// The person's credential no longer authenticates.
    #[error("the browser session no longer authenticates")]
    Unauthenticated,
    #[error("the Hub's MCP endpoint did not answer: {0}")]
    Unavailable(String),
}

/// The person's MCP client of one profile, with a bearer that never leaves the process.
pub(super) struct PersonMcp {
    pub(super) router: Router,
    pub(super) host: HeaderValue,
    pub(super) profile: HubProfileId,
    pub(super) bearer: HeaderValue,
    /// The browser request's correlation, which every record of the in-process request names.
    pub(super) observation: crate::request_observation::RequestObservation,
}

impl PersonMcp {
    /// Send one request. `meta` is the request's own `_meta`, whose protocol version and
    /// client information the host sets; its client capabilities stay, with the Apps and
    /// Tasks extensions declared, so a Task-based tool answers an App with its Task.
    /// `tool` is the called tool, whose annotated arguments travel as `Mcp-Param-*`
    /// headers too.
    pub(super) async fn request(
        &self,
        method: &str,
        mut params: Map<String, Value>,
        tool: Option<&Tool>,
    ) -> Result<Answer, Failure> {
        let mut meta = match params.remove("_meta") {
            Some(Value::Object(meta)) => meta,
            _ => Map::new(),
        };
        let mut capabilities = match meta.remove(CLIENT_CAPABILITIES_META) {
            Some(Value::Object(capabilities)) => capabilities,
            _ => Map::new(),
        };
        let (extension, declaration) = host_extension_capability();
        let extensions = capabilities
            .entry("extensions")
            .or_insert_with(|| Value::Object(Map::new()));
        if !extensions.is_object() {
            *extensions = Value::Object(Map::new());
        }
        if let Value::Object(extensions) = extensions {
            extensions.insert(extension, Value::Object(declaration));
            extensions
                .entry(rmcp::model::TASKS_EXTENSION_ID)
                .or_insert_with(|| Value::Object(Map::new()));
        }
        meta.insert(PROTOCOL_VERSION_META.to_owned(), json!(PROTOCOL_VERSION));
        meta.insert(
            CLIENT_INFO_META.to_owned(),
            json!({"name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION")}),
        );
        meta.insert(
            CLIENT_CAPABILITIES_META.to_owned(),
            Value::Object(capabilities),
        );
        params.insert("_meta".to_owned(), Value::Object(meta));
        let params = Value::Object(params);
        let mut request = Request::post(format!("/mcp/{}", self.profile))
            .header(header::HOST, self.host.clone())
            .header(header::AUTHORIZATION, self.bearer.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .header("Mcp-Method", method);
        for (name, value) in standard_headers(method, &params, tool) {
            request = request.header(name, value);
        }
        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": 1, "method": method, "params": params
        }))
        .map_err(|error| Failure::Unavailable(error.to_string()))?;
        let mut request = request
            .body(Body::from(body))
            .map_err(|error| Failure::Unavailable(error.to_string()))?;
        request.extensions_mut().insert(self.observation.clone());
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .map_err(|error| Failure::Unavailable(error.to_string()))?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(Failure::Unauthenticated);
        }
        let bytes = axum::body::to_bytes(
            response.into_body(),
            veoveo_hub_sdk::MAX_MCP_RESPONSE_BYTES + 64 * 1024,
        )
        .await
        .map_err(|error| Failure::Unavailable(error.to_string()))?;
        let mut message: Map<String, Value> = serde_json::from_slice(&bytes).map_err(|_| {
            Failure::Unavailable(format!("HTTP {status} without a JSON-RPC response"))
        })?;
        if let Some(Value::Object(error)) = message.remove("error") {
            return Ok(Answer::Error(Value::Object(error)));
        }
        match message.remove("result") {
            Some(result) if status.is_success() => Ok(Answer::Result(result)),
            _ => Err(Failure::Unavailable(format!(
                "HTTP {status} without a JSON-RPC result"
            ))),
        }
    }

    /// Every item of a paged list, `resources/list` or `tools/list`, under `key`.
    pub(super) async fn list<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        key: &str,
    ) -> Result<Vec<T>, Failure> {
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut params = Map::new();
            if let Some(cursor) = cursor.take() {
                params.insert("cursor".to_owned(), Value::String(cursor));
            }
            let mut page = match self.request(method, params, None).await? {
                Answer::Result(Value::Object(page)) => page,
                Answer::Result(_) => {
                    return Err(Failure::Unavailable(format!("{method} answered no page")));
                }
                Answer::Error(error) => {
                    return Err(Failure::Unavailable(format!("{method} failed: {error}")));
                }
            };
            let listed = page.remove(key).unwrap_or(Value::Array(Vec::new()));
            let listed: Vec<T> = serde_json::from_value(listed)
                .map_err(|error| Failure::Unavailable(format!("{method} page: {error}")))?;
            items.extend(listed);
            match page.remove("nextCursor") {
                Some(Value::String(next)) if !next.is_empty() => cursor = Some(next),
                _ => return Ok(items),
            }
        }
        Err(Failure::Unavailable(format!(
            "{method} exceeds {MAX_PAGES} pages"
        )))
    }
}

/// The SEP-2243 headers of a request: `Mcp-Name` from the parameter its method names, and
/// for `tools/call` an `Mcp-Param-*` header for each argument the tool's input schema
/// annotates with `x-mcp-header`.
fn standard_headers(method: &str, params: &Value, tool: Option<&Tool>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    let name = match method {
        "tools/call" | "prompts/get" => Some("name"),
        "resources/read" => Some("uri"),
        "tasks/get" | "tasks/update" | "tasks/cancel" => Some("taskId"),
        _ => None,
    };
    if let Some(value) = name.and_then(|key| params.get(key)).and_then(Value::as_str) {
        headers.push(("Mcp-Name".to_owned(), header_value(value)));
    }
    if method == "tools/call"
        && let (Some(tool), Some(arguments)) = (tool, params.get("arguments"))
        && let Some(Value::Object(properties)) = tool.input_schema.get("properties")
    {
        for (property, schema) in properties {
            let Some(Value::String(header)) = schema.get("x-mcp-header") else {
                continue;
            };
            let value = match arguments.get(property) {
                Some(Value::String(value)) => value.clone(),
                Some(Value::Bool(value)) => value.to_string(),
                Some(Value::Number(value)) => value.to_string(),
                _ => continue,
            };
            headers.push((format!("Mcp-Param-{header}"), header_value(&value)));
        }
    }
    headers
}

/// A header value as SEP-2243 carries it: as is, or Base64 inside `=?base64?…?=` when it
/// has controls, non-ASCII, edge whitespace, or the sentinel's own shape.
fn header_value(value: &str) -> String {
    let wrap = matches!(value.as_bytes().first(), Some(b' ' | b'\t'))
        || matches!(value.as_bytes().last(), Some(b' ' | b'\t'))
        || value.chars().any(|c| !(' '..='~').contains(&c))
        || (value.starts_with("=?base64?") && value.ends_with("?="));
    if wrap {
        format!("=?base64?{}?=", BASE64_STANDARD.encode(value))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_travel_in_headers_as_sep_2243_spells_them() {
        assert_eq!(header_value("fixture__run"), "fixture__run");
        assert_eq!(
            header_value("ui://fixture/board.html"),
            "ui://fixture/board.html"
        );
        assert_eq!(header_value("é"), "=?base64?w6k=?=");
        assert_eq!(header_value(" x"), "=?base64?IHg=?=");
        let tool = Tool::new(
            "fixture__run",
            "Run",
            serde_json::from_value::<rmcp::model::JsonObject>(json!({
                "type": "object",
                "properties": {"region": {"type": "string", "x-mcp-header": "Region"}, "n": {"type": "integer"}}
            }))
            .unwrap(),
        );
        let params = json!({"name": "fixture__run", "arguments": {"region": "eu", "n": 3}});
        assert_eq!(
            standard_headers("tools/call", &params, Some(&tool)),
            [
                ("Mcp-Name".to_owned(), "fixture__run".to_owned()),
                ("Mcp-Param-Region".to_owned(), "eu".to_owned())
            ]
        );
        assert_eq!(
            standard_headers("tasks/get", &json!({"taskId": "t-1"}), None),
            [("Mcp-Name".to_owned(), "t-1".to_owned())]
        );
        assert!(standard_headers("resources/list", &json!({}), None).is_empty());
    }
}
