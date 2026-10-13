# Gateway Token And Session Authority

## Standards And Protocols

| Standard | Profile |
|---|---|
| OAuth 2.0, RFC 6749 | Authorization code with PKCE, refresh, client credentials and the ID-JAG grant, for clients the installation configures |
| JWT assertions, RFC 7521 and RFC 7523 | `private_key_jwt` client authentication and identity assertion grants |
| Resource indicators, RFC 8707 | `resource` names the protected resource a token is for |
| JWT access tokens, RFC 9068 | Every caller access token: the hosted authorization server's carry `typ` `at+jwt`, and profile authentication accepts no other type |
| JWT, RFC 7519, and JWS, RFC 7515 | Access tokens the hosted authorization server signs with Ed25519 (`EdDSA`, RFC 8037); identity provider and client tokens in RS, PS, ES or EdDSA algorithms |
| JWK Thumbprint, RFC 7638 | The `kid` of the hosted authorization server's key, which the configuration repeats as `accessTokenKeyId` |
| Token revocation, RFC 7009, and OAuth security BCP, RFC 9700 | Refresh-family rotation, replay detection and revocation |
| Veoveo access token claims | Collision-resistant names under `https://veoveo.ai/claims/`: `principal`, `principal-display-name`, `principal-kind`, `session-family`, `work-context`, `invocation-mode`, `initiator`, `delegation`, `tenant`, `data-labels` and `principal-assurances`; `managed-execution` is reserved and never accepted from a token |
| PostgreSQL 18 | Refresh families, replay guards, JWT revocations and audit records shared by every replica, in the Gateway's `gateway` schema and core's `audit` schema |

The forwarded tokens the Gateway sends to registered servers are a separate profile,
defined by the SDK's [server registration protocol](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/5c6e1bafc4acd64df117c7ed92149b969619093a/crates/veoveo-sdk-rust/src/registration/DESIGN.md#forwarded-identity).

## Access Tokens

The hosted authorization server's issuer is `{issuer}/oauth`. It signs access tokens
with the Ed25519 PKCS#8 key the configuration names as `accessTokenSigningKey`. The
token header carries `typ` `at+jwt`, as RFC 9068 §2.1 requires, and the key's RFC 7638
thumbprint as `kid`; startup refuses a configuration whose `accessTokenKeyId` differs
from the thumbprint. `/oauth/jwks.json` publishes the public key, and the Gateway
verifies its own tokens with the same key.

Profile authentication follows RFC 9068 §4 for every authorization server a profile
names: a token whose `typ` is neither `at+jwt` nor `application/at+jwt` is refused
before its signature is checked. Verification requires the signed `exp` to be in the
future; clock-skew tolerance never extends it. An expired bearer receives HTTP 401
with the profile's challenge before any upstream discovery, and the denial is recorded
as an authentication event.

## Token Endpoint

`/oauth/token` reads one `application/x-www-form-urlencoded` form. A client that
authenticates with an RFC 7523 assertion may omit `client_id`, as RFC 7521 §4.2
allows: the assertion's `sub` names the client, and verification against that
client's keys requires `iss` and `sub` to name it. An assertion authenticates once:
its `jti` is recorded on every replica until the assertion expires. Its `aud` is the
token endpoint URL.

Errors carry the codes of RFC 6749 §5.2 and RFC 8707 §2, with HTTP 400 and
`Cache-Control: no-store`:

| Request | `error` |
|---|---|
| Not one form, a repeated parameter, no `grant_type`, or a `work_context` the caller may not act in | `invalid_request` |
| An unknown client, or an assertion that is malformed, wrongly signed, expired, replayed or addressed elsewhere | `invalid_client` |
| An authenticated client that may not use the grant, whose own authority is unavailable, or that may not act in its default Work Context | `unauthorized_client` |
| An authorization code or identity assertion whose principal may not act in the Work Context it binds | `invalid_grant` |
| A grant the resource does not support | `unsupported_grant_type` |
| A resource that is unknown or that the client may not request | `invalid_target` |
| A scope the client may not request | `invalid_scope` |

The token and revocation endpoints accept no HTTP authentication scheme, so
`invalid_client` keeps the 400 of RFC 6749 §5.2. A 401 would have to carry a challenge
for such a scheme.

## Sessions

An authorization-code exchange creates a refresh family before it signs the access
token. When the client supports refresh, the token carries the family identity in the
`session-family` claim, and each rotation keeps it. Client-credentials and ID-JAG
exchanges create no family.

After JWT validation and Work Context resolution, authentication reads a bound family
and checks its authorization server, profile, client, Work Context, principal, tenant,
scope and lifetime. A missing, expired or revoked family denies the request; a
database failure returns HTTP 503 within a five-second deadline. Logout and refresh
replay revoke the family on every replica, which also denies its outstanding access
tokens, as RFC 7009 §2.1 allows. The family predicate lives in `policy::session`.

Refresh tokens reference their family with `ON DELETE CASCADE`, so deleting a family
removes its tokens in the same transaction. Revocation keeps the family until expiry
so that replay stays detectable. The store keeps each token's SHA-256 digest, never the
token. Rotation locks the family's row, so concurrent presentations of one token
serialize across replicas: the first rotates it and commits its audit record in the same
transaction, a presentation inside the successor's delivery window receives the same
successor from its encrypted envelope, and a later one revokes the family.

## Request Context And Audit

The Gateway assigns a request identity before authentication and carries it in
`GatewayRequestContext.audit`. Forwarded tokens sign the request ID, W3C trace and span
identifiers and the peer address with the access-token subject. A server uses these
signed values and never trusts an unsigned request-ID header.

Authentication records identify the verified principal with its scopes and data
labels. API activity records identify the invocation actor: a delegated request names
the OAuth service as actor and the user as `delegating_principal`. Anonymous failures
never acquire a principal from unverified claims.

## Claim Admission

JWT JSON with duplicate keys is rejected before any value is built. Every fixed claim
name is reserved, and an extension registry that reserves one of them is refused. This
binary binds an empty extension registry, so a token that carries any reserved
extension claim fails admission, and unrelated unknown claims are ignored.
