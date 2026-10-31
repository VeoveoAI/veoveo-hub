//! What an App's view may ask of the Hub, decided against the catalogs the person sees
//! now and never against anything the frame supplies besides its request.
use rmcp::model::{Resource, Tool};
use serde::Deserialize;
use serde_json::{Map, Value};
use veoveo_hub_sdk::apps::{is_app_resource, resolve_app_tool};

use super::route::AppView;

/// JSON-RPC 2.0 `Method not found`.
pub(super) const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC 2.0 `Invalid params`.
pub(super) const INVALID_PARAMS: i64 = -32602;
/// JSON-RPC 2.0 `Internal error`.
pub(super) const INTERNAL_ERROR: i64 = -32603;

/// One JSON-RPC request of a view.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewRequest {
    #[serde(rename = "jsonrpc")]
    pub(super) _version: JsonRpcVersion,
    pub(super) id: RequestId,
    pub(super) method: String,
    #[serde(default)]
    pub(super) params: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
pub(super) enum JsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}

/// A request identifier: a string or a number, as JSON-RPC 2.0 allows of a request
/// that expects a response.
#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq)]
#[serde(untagged)]
pub(super) enum RequestId {
    Number(serde_json::Number),
    Text(String),
}

/// The view's method, when the bridge serves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Method {
    Ping,
    ToolsCall,
    ResourcesRead,
    TasksGet,
    TasksUpdate,
    TasksCancel,
    SubscriptionsListen,
}

impl Method {
    pub(super) fn parse(method: &str) -> Option<Self> {
        Some(match method {
            "ping" => Self::Ping,
            "tools/call" => Self::ToolsCall,
            "resources/read" => Self::ResourcesRead,
            "tasks/get" => Self::TasksGet,
            "tasks/update" => Self::TasksUpdate,
            "tasks/cancel" => Self::TasksCancel,
            "subscriptions/listen" => Self::SubscriptionsListen,
            _ => return None,
        })
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::ToolsCall => "tools/call",
            Self::ResourcesRead => "resources/read",
            Self::TasksGet => "tasks/get",
            Self::TasksUpdate => "tasks/update",
            Self::TasksCancel => "tasks/cancel",
            Self::SubscriptionsListen => "subscriptions/listen",
        }
    }
}

/// Why the bridge refuses a view's request before it reaches a server.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum Refusal {
    #[error("`{0}` is not a method an App may send to the Hub")]
    Method(String),
    #[error("the request names no `{0}`")]
    Missing(&'static str),
    #[error("the App `{0}` is not available to you")]
    App(String),
    #[error("the tool `{tool}` is not linked to the App `{app}`")]
    Tool { tool: String, app: String },
    #[error("the resource `{resource}` belongs to another server than the App `{app}`'s")]
    Resource { resource: String, app: String },
    #[error(
        "an App's listen names 1 to {MAX_LISTEN_URIS} `resourceSubscriptions` and no other notifications"
    )]
    Listen,
}

impl Refusal {
    pub(super) fn code(&self) -> i64 {
        match self {
            Self::Method(_) => METHOD_NOT_FOUND,
            _ => INVALID_PARAMS,
        }
    }
}

/// The view's App in the catalog the person sees: that exact resource, an App by its
/// MIME type.
pub(super) fn find_app<'a>(
    view: &AppView,
    resources: &'a [Resource],
) -> Result<&'a Resource, Refusal> {
    resources
        .iter()
        .find(|resource| resource.uri == view.uri() && is_app_resource(resource))
        .ok_or_else(|| Refusal::App(view.uri().to_owned()))
}

/// The tool a view's `tools/call` names, by its name on the App's server: a tool linked
/// to this exact App and visible to Apps, which the SDK's shared admission resolves.
/// The request's name becomes the tool's name in the Hub's catalog.
pub(super) fn admit_tool(
    app: &Resource,
    tools: &[Tool],
    params: &mut Map<String, Value>,
) -> Result<Tool, Refusal> {
    let Some(Value::String(name)) = params.get("name") else {
        return Err(Refusal::Missing("name"));
    };
    let tool = resolve_app_tool(app, tools, name).ok_or_else(|| Refusal::Tool {
        tool: name.clone(),
        app: app.uri.clone(),
    })?;
    params.insert("name".to_owned(), Value::String(tool.name.to_string()));
    Ok(tool.clone())
}

/// A view reads only its own server's resources.
pub(super) fn admit_read(view: &AppView, params: &Map<String, Value>) -> Result<(), Refusal> {
    let Some(Value::String(uri)) = params.get("uri") else {
        return Err(Refusal::Missing("uri"));
    };
    if view.owns_resource(uri) {
        Ok(())
    } else {
        Err(Refusal::Resource {
            resource: uri.clone(),
            app: view.uri().to_owned(),
        })
    }
}

/// The most resources one listen of an App names.
pub(super) const MAX_LISTEN_URIS: usize = 64;

/// A view listens only for updates of its own server's resources: its
/// `params.notifications` names `resourceSubscriptions` and nothing else, 1 to
/// [`MAX_LISTEN_URIS`] of them, each as `resources/read` admits it.
pub(super) fn admit_listen(view: &AppView, params: &Map<String, Value>) -> Result<(), Refusal> {
    let Some(Value::Object(notifications)) = params.get("notifications") else {
        return Err(Refusal::Missing("notifications"));
    };
    let uris = match (
        notifications.len(),
        notifications.get("resourceSubscriptions"),
    ) {
        (1, Some(Value::Array(uris))) if (1..=MAX_LISTEN_URIS).contains(&uris.len()) => uris,
        _ => return Err(Refusal::Listen),
    };
    for uri in uris {
        let Value::String(uri) = uri else {
            return Err(Refusal::Listen);
        };
        if !view.owns_resource(uri) {
            return Err(Refusal::Resource {
                resource: uri.clone(),
                app: view.uri().to_owned(),
            });
        }
    }
    Ok(())
}

/// A Task request names its Task.
pub(super) fn admit_task(params: &Map<String, Value>) -> Result<(), Refusal> {
    match params.get("taskId") {
        Some(Value::String(task)) if !task.is_empty() => Ok(()),
        _ => Err(Refusal::Missing("taskId")),
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::JsonObject;
    use serde_json::json;
    use veoveo_hub_sdk::apps::{UiVisibility, app_resource, link_tool_to_app};

    use super::*;

    fn view() -> AppView {
        AppView::from_path("/apps/fixture/board.html").unwrap()
    }

    /// The catalog the person sees: the App, another of its server's Apps and another
    /// server's App, with tools linked to each and tools linked to none.
    fn catalog() -> (Vec<Resource>, Vec<Tool>) {
        let board = app_resource("ui://fixture/board.html", "board");
        let other = app_resource("ui://fixture/other.html", "other");
        let foreign = app_resource("ui://neighbor/board.html", "board");
        let tool = |name: &str| Tool::new(name.to_owned(), "fixture tool", JsonObject::new());
        let tools = vec![
            link_tool_to_app(
                tool("fixture__move"),
                &board.uri,
                &[UiVisibility::Model, UiVisibility::App],
            ),
            link_tool_to_app(tool("fixture__reset"), &board.uri, &[]),
            link_tool_to_app(
                tool("fixture__summarize"),
                &board.uri,
                &[UiVisibility::Model],
            ),
            link_tool_to_app(tool("fixture__other"), &other.uri, &[UiVisibility::App]),
            tool("fixture__unlinked"),
            link_tool_to_app(tool("neighbor__move"), &board.uri, &[UiVisibility::App]),
            link_tool_to_app(tool("neighbor__play"), &foreign.uri, &[UiVisibility::App]),
        ];
        let mut plain = Resource::new("ui://fixture/plain.html", "plain");
        plain.mime_type = Some("text/html".to_owned());
        (vec![board, other, foreign, plain], tools)
    }

    fn call(name: &str) -> Map<String, Value> {
        json!({"name": name, "arguments": {"to": 3}})
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn a_view_calls_only_tools_linked_to_its_own_app_on_its_own_server() {
        let (resources, tools) = catalog();
        let app = find_app(&view(), &resources).unwrap();
        for name in ["move", "reset"] {
            let mut params = call(name);
            let tool = admit_tool(app, &tools, &mut params).unwrap();
            assert_eq!(tool.name, format!("fixture__{name}"));
            assert_eq!(params["name"], json!(format!("fixture__{name}")));
            assert_eq!(params["arguments"], json!({"to": 3}));
        }
        for (name, why) in [
            ("unlinked", "a tool linked to no App"),
            ("summarize", "a tool visible only to the model"),
            ("other", "a tool linked to another App of the server"),
            ("play", "another server's tool"),
            ("fixture__move", "the Hub's own name of a linked tool"),
            (
                "neighbor__move",
                "another server's tool that claims the App",
            ),
            ("missing", "no tool"),
        ] {
            let mut params = call(name);
            assert_eq!(
                admit_tool(app, &tools, &mut params),
                Err(Refusal::Tool {
                    tool: name.to_owned(),
                    app: "ui://fixture/board.html".to_owned()
                }),
                "{why}"
            );
            assert_eq!(
                params["name"],
                json!(name),
                "a refused request is unchanged"
            );
        }
        assert_eq!(
            admit_tool(app, &tools, &mut Map::new()),
            Err(Refusal::Missing("name"))
        );
    }

    #[test]
    fn a_view_reads_only_its_own_servers_resources() {
        let read = |uri: &str| json!({"uri": uri}).as_object().unwrap().clone();
        assert_eq!(admit_read(&view(), &read("fixture://items/1")), Ok(()));
        assert_eq!(
            admit_read(&view(), &read("ui://fixture/board.html")),
            Ok(())
        );
        for uri in [
            "neighbor://items/1",
            "ui://neighbor/board.html",
            "fixtures://items/1",
        ] {
            assert_eq!(
                admit_read(&view(), &read(uri)),
                Err(Refusal::Resource {
                    resource: uri.to_owned(),
                    app: "ui://fixture/board.html".to_owned()
                })
            );
        }
        assert_eq!(
            admit_read(&view(), &Map::new()),
            Err(Refusal::Missing("uri"))
        );
    }

    #[test]
    fn the_app_must_be_in_the_persons_catalog_as_an_app() {
        let (resources, _) = catalog();
        assert!(find_app(&view(), &resources).is_ok());
        for path in ["/apps/fixture/plain.html", "/apps/fixture/absent.html"] {
            let view = AppView::from_path(path).unwrap();
            assert_eq!(
                find_app(&view, &resources),
                Err(Refusal::App(view.uri().to_owned()))
            );
        }
    }

    #[test]
    fn a_view_listens_only_for_its_own_servers_resources() {
        let listen = |notifications: Value| {
            json!({"notifications": notifications})
                .as_object()
                .unwrap()
                .clone()
        };
        assert_eq!(
            admit_listen(
                &view(),
                &listen(json!({"resourceSubscriptions": [
                    "fixture://events", "fixture://events?cursor=eyJhIjoxfQ%3D%3D", "ui://fixture/board.html"
                ]}))
            ),
            Ok(())
        );
        for uri in [
            "neighbor://events",
            "ui://neighbor/board.html",
            "fixtures://events",
        ] {
            assert_eq!(
                admit_listen(
                    &view(),
                    &listen(json!({"resourceSubscriptions": ["fixture://events", uri]}))
                ),
                Err(Refusal::Resource {
                    resource: uri.to_owned(),
                    app: "ui://fixture/board.html".to_owned()
                }),
                "{uri}"
            );
        }
        let too_many: Vec<String> = (0..=MAX_LISTEN_URIS)
            .map(|n| format!("fixture://items/{n}"))
            .collect();
        for notifications in [
            json!({}),
            json!({"resourceSubscriptions": []}),
            json!({"resourceSubscriptions": too_many}),
            json!({"resourceSubscriptions": [7]}),
            json!({"resourceSubscriptions": "fixture://events"}),
            json!({"resourceSubscriptions": ["fixture://events"], "resourcesListChanged": true}),
            json!({"resourceSubscriptions": ["fixture://events"], "toolsListChanged": true}),
            json!({"taskIds": ["task-1"]}),
        ] {
            assert_eq!(
                admit_listen(&view(), &listen(notifications.clone())),
                Err(Refusal::Listen),
                "{notifications}"
            );
        }
        assert_eq!(
            admit_listen(&view(), &Map::new()),
            Err(Refusal::Missing("notifications"))
        );
    }

    #[test]
    fn requests_are_json_rpc_with_a_method_the_bridge_serves() {
        let request: ViewRequest = serde_json::from_value(
            json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {}}),
        )
        .unwrap();
        assert_eq!(request.id, RequestId::Number(7.into()));
        assert_eq!(Method::parse(&request.method), Some(Method::ToolsCall));
        for refused in [
            json!({"jsonrpc": "1.0", "id": 1, "method": "ping"}),
            json!({"jsonrpc": "2.0", "method": "ping"}),
            json!({"jsonrpc": "2.0", "id": null, "method": "ping"}),
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "extra": true}),
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": []}),
        ] {
            assert!(
                serde_json::from_value::<ViewRequest>(refused.clone()).is_err(),
                "{refused}"
            );
        }
        for method in [
            "tools/list",
            "resources/list",
            "prompts/get",
            "ui/initialize",
            "ui/notifications/resource-updated",
            "notifications/cancelled",
        ] {
            assert_eq!(Method::parse(method), None, "{method}");
        }
        assert_eq!(
            Method::parse("subscriptions/listen"),
            Some(Method::SubscriptionsListen)
        );
        assert_eq!(
            admit_task(json!({"taskId": "t"}).as_object().unwrap()),
            Ok(())
        );
        assert_eq!(
            admit_task(json!({"taskId": ""}).as_object().unwrap()),
            Err(Refusal::Missing("taskId"))
        );
    }
}
