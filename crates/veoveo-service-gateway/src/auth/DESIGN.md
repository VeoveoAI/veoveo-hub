# Gateway Token And Session Authority

## Standards And Protocols

| Standard | Profile |
|---|---|
| OAuth 2.0, RFC 6749 | Authorization code with PKCE, refresh, client credentials and the ID-JAG grant, for clients the installation configures |
| JWT assertions, RFC 7523 | `private_key_jwt` client authentication and identity assertion grants |
| JWT, RFC 7519, and JWS, RFC 7515 | Access tokens the hosted authorization server signs with Ed25519 (`EdDSA`, RFC 8037); identity provider and client tokens in RS, PS, ES or EdDSA algorithms |
| JWK Thumbprint, RFC 7638 | The `kid` of the hosted authorization server's key, which the configuration repeats as `accessTokenKeyId` |
| Token revocation, RFC 7009, and OAuth security BCP, RFC 9700 | Refresh-family rotation, replay detection and revocation |
| Veoveo access token claims | Collision-resistant names under `https://veoveo.ai/claims/`: `principal`, `principal-display-name`, `principal-kind`, `session-family`, `work-context`, `invocation-mode`, `initiator`, `delegation`, `tenant`, `data-labels` and `principal-assurances`; `managed-execution` is reserved and never accepted from a token |
| PostgreSQL 18 | Refresh families, replay guards, JWT revocations and audit records shared by every replica, in core's `gateway` and `audit` schemas |

The forwarded tokens the Gateway sends to registered servers are a separate profile,
defined by core's [server registration protocol](https://github.com/VeoveoAI/veoveo-core/blob/main/mcp/ext-registration/DESIGN.md#forwarded-identity).

## Access Tokens

The hosted authorization server's issuer is `{issuer}/oauth`. It signs access tokens
with the Ed25519 PKCS#8 key the configuration names as `accessTokenSigningKey`. The
token header carries the key's RFC 7638 thumbprint as `kid`, and startup refuses a
configuration whose `accessTokenKeyId` differs from it. `/oauth/jwks.json` publishes
the public key, and the Gateway verifies its own tokens with the same key.

Verification requires the signed `exp` to be in the future; clock-skew tolerance never
extends it. An expired bearer receives HTTP 401 with the profile's challenge before any
upstream discovery, and the denial is recorded as an authentication event.

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
tokens, as RFC 7009 §2.1 allows. The family predicate lives in core's
`veoveo_policy::session`.

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
