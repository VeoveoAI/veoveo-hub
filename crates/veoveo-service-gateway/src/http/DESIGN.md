# Gateway HTTP Admission

## Standards And Protocols

| Standard | Profile |
|---|---|
| HTTP, RFC 9110 | Axum request admission for every profile-scoped route |
| Bearer tokens, RFC 6750 | Profile JWT authentication with current invocation authority, session and revocation checks; a 401 carries the profile's `WWW-Authenticate: Bearer` challenge with its RFC 9728 resource metadata URL |

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
