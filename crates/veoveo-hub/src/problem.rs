//! RFC 9457 problem details for the Hub's HTTP APIs. OAuth endpoints keep the
//! error format of the RFC that defines them.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::collections::BTreeMap;

pub const PROBLEM_JSON: &str = "application/problem+json";

/// One problem: its `type` is `https://veoveo.ai/problems/<code>`.
#[derive(Debug, Clone)]
pub struct Problem {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: Option<String>,
    extensions: BTreeMap<&'static str, serde_json::Value>,
}

#[derive(Serialize)]
struct ProblemBody<'a> {
    #[serde(rename = "type")]
    problem_type: String,
    title: &'a str,
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
    #[serde(flatten)]
    extensions: &'a BTreeMap<&'static str, serde_json::Value>,
}

impl Problem {
    pub fn new(status: StatusCode, code: &'static str, title: &'static str) -> Self {
        Self {
            status,
            code,
            title,
            detail: None,
            extensions: BTreeMap::new(),
        }
    }

    /// Occurrence-specific copy that names no secret and no request content.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// An extension member (RFC 9457 §3.2), such as the identifier of what the request
    /// created before it failed. It names no secret and no request content.
    pub fn extension(mut self, name: &'static str, value: impl Serialize) -> Self {
        self.extensions.insert(
            name,
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal-error",
            "Internal error",
        )
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not-found", "Not found")
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let body = ProblemBody {
            problem_type: format!("https://veoveo.ai/problems/{}", self.code),
            title: self.title,
            status: self.status.as_u16(),
            detail: self.detail.as_deref(),
            extensions: &self.extensions,
        };
        (
            self.status,
            [(header::CONTENT_TYPE, PROBLEM_JSON)],
            Json(body),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn problems_name_a_veoveo_type_and_their_status() {
        let response = Problem::new(StatusCode::SERVICE_UNAVAILABLE, "not-ready", "Not ready")
            .detail("the audit service is starting")
            .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CONTENT_TYPE], PROBLEM_JSON);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "type": "https://veoveo.ai/problems/not-ready",
                "title": "Not ready",
                "status": 503,
                "detail": "the audit service is starting"
            })
        );
    }
}
