# Gateway HTTP Admission

## Standards And Protocols

| Standard | Profile |
|---|---|
| HTTP, RFC 9110 | Axum request admission for every profile-scoped route |
| Bearer tokens, RFC 6750 | Profile JWT authentication from the `Authorization` header only, with current invocation authority, session and revocation checks, and the §3 `WWW-Authenticate: Bearer` challenge |
| Protected resource metadata, RFC 9728 | Every challenge names the profile's metadata URL in `resource_metadata` (§5.1) |
| Problem details, RFC 9457 | The body of every challenge and of an unavailable authorization server |

## Profile Authentication

`authenticate_profile` admits every route under `/mcp/{profile}`, `/admin/{profile}`,
`/artifacts/{profile}` and `/console-api/{profile}`. It reads the profile from the
registered `{profile}` segment of the matched route and its raw spelling in the
original URI, so an encoded alias fails before bearer verification. Catalog
replacement changes the next request's authentication and policy through the shared
catalog handle.

Authentication records credential denials before dispatch. A verified bearer resolves
the caller's OAuth client, Work Context and invocation authority, synchronizes the
principal into the platform directory and continues with an `AuthenticatedSubject`.
The middleware fails closed when the audit writer or the database is unavailable.

A denial answers with the profile's challenge, which names `resource_metadata` and
the profile's required scopes in `scope`:

| Request | Status | `error` |
|---|---|---|
| No `Authorization` header, or a scheme other than `Bearer` | 401 | none, as RFC 6750 §3.1 asks |
| A bearer credential that is not an RFC 6750 `b64token` | 400 | `invalid_request` |
| A token that fails verification: malformed, wrongly typed or signed, expired, for another issuer or audience, revoked, or with a revoked session or authority | 401 | `invalid_token` |
| A valid token without every scope the profile requires | 403 | `insufficient_scope` |

When the keys of the profile's authorization server cannot be loaded, the answer is
HTTP 503, because the token was never judged.
