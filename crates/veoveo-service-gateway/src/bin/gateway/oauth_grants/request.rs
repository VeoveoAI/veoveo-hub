use serde::Deserialize;

/// An RFC 6749 §4 token request. `client_id` is empty when an RFC 7523 client
/// assertion identifies the client instead (RFC 7521 §4.2); the token endpoint fills it
/// from the assertion before a grant reads it.
#[derive(Deserialize)]
pub(crate) struct TokenRequest {
    pub(crate) grant_type: String,
    #[serde(default)]
    pub(crate) client_id: String,
    #[serde(default)]
    pub(crate) scope: Option<String>,
    #[serde(default)]
    pub(crate) resource: Option<String>,
    #[serde(default)]
    pub(crate) work_context: Option<String>,
    #[serde(default)]
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) redirect_uri: Option<String>,
    #[serde(default)]
    pub(crate) code_verifier: Option<String>,
    #[serde(default)]
    pub(crate) refresh_token: Option<String>,
    #[serde(default)]
    pub(crate) client_assertion_type: Option<String>,
    #[serde(default)]
    pub(crate) client_assertion: Option<String>,
    #[serde(default)]
    pub(crate) assertion: Option<String>,
    #[serde(default)]
    pub(crate) subject_token: Option<String>,
    #[serde(default)]
    pub(crate) subject_token_type: Option<String>,
    #[serde(default)]
    pub(crate) authorization_details: Option<String>,
    #[serde(default)]
    pub(crate) requested_token_type: Option<String>,
    #[serde(default)]
    pub(crate) actor_token: Option<String>,
    #[serde(default)]
    pub(crate) audience: Option<String>,
}
