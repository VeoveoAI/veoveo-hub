# Veoveo Gateway

The Gateway is the MCP entry point of a Veoveo installation. It authenticates
callers, admits MCP servers that register themselves, authorizes every request with
the installation's roles over the permissions each server declares, and forwards the
request with a fresh access token that names the caller and the permissions granted.
Neither its code nor its configuration names a server.

## Standards And Protocols

| Standard | Profile |
|---|---|
| MCP `2026-07-28`, Streamable HTTP | One endpoint per profile at `/mcp/{profile}`, aggregating tools, resources, resource templates, prompts, completions, Tasks and resource subscriptions of registered servers |
| MCP Apps, extension `io.modelcontextprotocol/ui` | A server's Apps live under `ui://{slug}/`; `_meta.ui.resourceUri` keeps its value |
| [Server registration protocol](https://github.com/VeoveoAI/veoveo-core/blob/main/mcp/ext-registration/DESIGN.md) | RFC 7591 registration, RFC 7592 read, update and delete, RFC 8414 discovery, software statements under RFC 7523 claim rules, and the `veoveo_*` client metadata members |
| JWT access tokens, RFC 9068, with RFC 9396 and RFC 8693 `act` | The identity forwarded to a server, issued by core's `GatewayInternalTokenIssuer` |
| OAuth 2.0, RFC 6749, and RFC 7523 | The hosted authorization server; see [token and session authority](src/auth/DESIGN.md) |
| Authorization server metadata, RFC 8414 | `/.well-known/oauth-authorization-server` for the Gateway's issuer and `/.well-known/oauth-authorization-server/oauth` for the hosted authorization server; each names its `jwks_uri`, and `response_types_supported`, `grant_types_supported` and `token_endpoint_auth_methods_supported` are always present because §2 gives an absent member a default |
| JWK Set, RFC 7517, and JWK Thumbprint, RFC 7638 | `/jwks.json` and `/oauth/jwks.json`, served as `application/jwk-set+json`: public Ed25519 keys whose `kid` is the RFC 7638 thumbprint |
| Protected resource metadata, RFC 9728 | `/.well-known/oauth-protected-resource/mcp/{profile}` |
| Problem details, RFC 9457 | HTTP API errors outside OAuth endpoints, typed `https://veoveo.ai/problems/<code>` |
| PostgreSQL 18 through sqlx 0.9.0 | The installation database: core's kernel schemas, including `gateway`, applied at startup; `LISTEN` on the `gateway` channel for registration changes; one login role per registration |
| OpenTelemetry | `service.name` is `veoveo-service-gateway` |
| Veoveo installation configuration | `GatewayControlPlane` from `veoveo-mcp-contract`, read as JSON |

## Process

`veoveo-service-gateway serve` starts in this order:

1. Connect to PostgreSQL with `VEOVEO_DATABASE_URL` and `VEOVEO_DATABASE_PASSWORD`,
   waiting up to `VEOVEO_DATABASE_WAIT_SECONDS` for the database to accept the login.
2. Apply pending kernel migrations with `PlatformStore::migrate`. Replicas starting
   together wait on the migrator's lock.
3. Load the installation configuration from `VEOVEO_CONFIG` and publish it as the
   active revision when its digest differs from the active one. A replica that loses
   the publication to another one with the same file continues.
4. Load the registrations, start the registration listener and the housekeeping loop,
   and listen on `VEOVEO_LISTEN_ADDRESS`.

The Gateway connects as the installation owner, a role that owns the database and has
`CREATEROLE`, because it applies the kernel migrations and creates each registration's
login. The other commands are `validate`, `resolve-secret`, `audit` and
`generate-local-installation`; `audit` reads the same database settings.

| Variable | Meaning |
|---|---|
| `VEOVEO_LISTEN_ADDRESS` | Socket address, `127.0.0.1:8080` by default |
| `VEOVEO_ISSUER` | The Gateway's public origin: its RFC 8414 issuer, the `iss` of forwarded tokens and the base of every URL it announces |
| `VEOVEO_CONFIG` | Installation configuration file |
| `VEOVEO_DATABASE_URL`, `VEOVEO_DATABASE_PASSWORD` | The installation database as a `postgres://` URL naming host, database and user, and the user's password |
| `VEOVEO_DATABASE_WAIT_SECONDS` | How long startup waits for the database, 60 seconds by default |
| `VEOVEO_SERVER_DATABASE_URL` | The database URL registration grants name, when servers reach it at another address; by default the Gateway's own URL without its user |
| `VEOVEO_REGISTRATION_GRACE_SECONDS` | How long a registration may go without a live process before the Gateway removes it, 60 seconds by default |
| `VEOVEO_FORWARDED_TOKEN_KEY_B64` | Ed25519 PKCS#8 key that signs forwarded tokens |
| `VEOVEO_REFRESH_DELIVERY_KEY_B64` | 32-byte key for refresh successor envelopes |
| `VEOVEO_AUDIT_SIGNING_KEY_B64`, `VEOVEO_AUDIT_RETENTION_DAYS`, `VEOVEO_AUDIT_EXPORT_CONFIG` | Audit sealing, retention and export |
| `VEOVEO_ARTIFACT_SERVICE_URL` | The Artifact service; without it the Gateway serves no Artifact routes |
| `VEOVEO_ALLOW_LOOPBACK_HOSTS` | Accept loopback `Host` headers beside the issuer's host |

The configuration's `secrets` section names the remaining secrets, such as the
registration secret and the hosted authorization server's signing key, by
environment variable. The process also reads a `.env` file in its working directory.

## Configuration

The installation configuration holds identity providers, the hosted authorization
server, profiles, OAuth clients, tenants, Work Contexts, policy rules for the Gateway's
own actions, and two sections for registered servers:

- `authorization`: roles that select permissions by access kind and risk, grants of
  roles to groups or principals, and overrides that add a risk to or deny a permission;
- `registration`: the secret reference of the registration secret.

`generate-local-installation` writes a complete local installation: the
configuration, its secrets, and two client-credentials callers, `acceptance-member`
and `acceptance-admin`, that hold the `member` and `admin` roles.

## Server Registration

`src/registration` implements the [server registration
protocol](https://github.com/VeoveoAI/veoveo-core/blob/main/mcp/ext-registration/DESIGN.md)
at `/oauth/register` and `/oauth/register/{client_id}`. Admission follows the
protocol's order: the initial access token, the request body, `ClientMetadata::check`,
then the software statement against the pinned key and the endpoint URL.

| Table in core's `gateway` schema | Holds |
|---|---|
| `namespace_claim` | One row per slug: the RFC 7638 thumbprint of the pinned key |
| `registration` | One row per registration: client identifier, metadata, software statement, SHA-256 digest of the registration access token, its login and when the Gateway last saw that login hold a live process |

One transaction registers a server: it claims the namespace on the slug's first
registration, or finds it pinned to the same key, calls `kernel.ensure_server` for the
server's group role `server_<schema>` and its schema, creates the registration's login
with `kernel.create_server_login`, and inserts the registration row. A namespace pinned
to another key is refused with `unapproved_software_statement`. The login is
`<schema>_<suffix>`, where the suffix is the last 14 hexadecimal digits of the client
identifier, and its password is 256 random bits. The response grants the login as
`veoveo_database`: `VEOVEO_SERVER_DATABASE_URL`, or the Gateway's own URL without its
user, with the login and its password. The Gateway keeps only the login's name.

A server takes its group role, applies its own migrations in its schema and holds a
kernel instance on its login while it serves. An RFC 7592 update replaces the stored
metadata when it changed and writes nothing otherwise; it never extends anything.
Reads return the stored statement. A `DELETE` removes the row and drops the login in
one transaction, and the registration access token then answers HTTP 401.

A process can end without deregistering. Every replica's housekeeping loop runs every 5
seconds: it stamps the registrations whose logins hold a live kernel instance, then
removes the registrations whose logins held none for `VEOVEO_REGISTRATION_GRACE_SECONDS`
and drops their logins in the same transaction. Each claim, registration, declaration
update, deregistration and removal writes an audit record.

Every replica listens on the `gateway` channel. A row inserted or deleted, or a
metadata change, wakes it to reload the registrations, and so does every reconnection
of the listener, because PostgreSQL does not replay notifications. Liveness stamps wake
no one. A replica routes each slug to its newest registration and replaces the catalog
only when the routed set or a declaration changes, so discovery caches stay intact
otherwise. Requests in flight finish on the catalog snapshot they started with. Server
health comes from liveness: healthy while the routed registration's login holds a live
instance, degraded until housekeeping removes it.

## Routing And Catalog

The catalog combines the installation configuration with the registration snapshot.
A server's MCP surfaces follow its authorization declaration: tools when it declares
tool requirements, resources and completions when it declares resources or templates,
prompts when it declares prompts. A server's URI scheme equals its slug, so resource
URIs pass through unchanged and the Gateway routes them by scheme or `ui://{slug}/`.
Tools are listed as `{slug}__{tool}`.

Discovery caches the admitted descriptors per catalog generation, caller authority and
OAuth client. A registered server's tools, resources, templates and prompts change
only with its declarations, which start a new generation. List pages use an opaque
cursor of at most 16 KiB that carries the surface and the last returned identity, so
each page uses the caller's current admission on any replica.

## Authorization

`veoveo_policy::decide` makes every decision. A registered server's tools, resources,
templates, prompts and Tasks are decided by installation roles over that server's
declared permissions, following the [policy
design](https://github.com/VeoveoAI/veoveo-core/blob/main/platform/policy/DESIGN.md#registered-server-decisions).
A role grants a permission only when it allows its access kind and every one of its
risks. A target the server did not declare is denied. The Gateway's own actions,
Artifacts and usage are decided by the configuration's policy rules.

## Forwarded Identity

`mcp/upstream_authorized_http.rs` issues a fresh RFC 9068 token for each upstream
HTTP request. Its `aud` is the server's registered MCP endpoint, `client_id` the
caller's OAuth client, `act` the Gateway's issuer, and `authorization_details` the
permissions granted in the server's namespace. Ordinary tokens live at most 60
seconds and a `subscriptions/listen` token at most 15 minutes, never longer than the
caller's own token. The server verifies them with the keys its registration response
names in `veoveo_access_tokens`.

The issuer's RFC 8414 metadata names `{issuer}/jwks.json` as its `jwks_uri`. That JWK
Set lists the same keys: the public half of each forwarded-token key, with its RFC
7638 thumbprint as `kid`, `alg` `EdDSA` and `use` `sig`, and no private member. A
server that meets a `kid` outside its registered keys reads the set again from there.
The set is served with `Cache-Control: no-cache`, so every read reaches the Gateway.

`VEOVEO_FORWARDED_TOKEN_KEY_B64` names one key, so the set holds one key and a rotation
drains: every replica stops, and every replica then starts with the new key. Tokens
signed by the old key are verified when they arrive, and none remains in flight once
the old replicas have stopped.

## Upstream Transport

The workspace pins RMCP 3.5.0 from `VeoveoAI/rust-sdk` at
`917e7914c93975fc1eddbff8792f1e2de933bc17`. The fork exposes HTTP rejection status and
body through `StreamableHttpError::HttpResponse` and keeps exact Task subscription
filters. Upstream discovery has a ten-second deadline and one retry after a transport
failure; authorization challenges, malformed responses and MCP errors fail at once,
and no mutation is retried.

Structured results, resource contents and unknown metadata keep their upstream
values. Upstream MCP errors keep their code, message and data. A non-success HTTP
response without an MCP error keeps its status, and an unexpected redirect becomes
HTTP 502 without its location.

The Gateway checks resource policy before it reads. A read that declares the
`ai.veoveo/knowledge-source` extension carries its observation, which the Gateway
verifies against the member URI, the returned bytes and the server. The handler commits
one audit record before it returns a result; a failed commit prevents delivery.

## HTTP Surface

| Route | Purpose |
|---|---|
| `GET /livez`, `GET /readyz` | Process liveness, and readiness once the audit service runs; both skip `Host` validation |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 metadata of the Gateway's issuer, with `jwks_uri` and, when registration is configured, `registration_endpoint` |
| `GET /jwks.json` | The keys that verify forwarded tokens |
| `POST /oauth/register`, `GET`/`PUT`/`DELETE /oauth/register/{client_id}` | Server registration |
| `/oauth/authorize`, `/oauth/callback`, `/oauth/token`, `/oauth/revoke`, `/oauth/jwks.json` | Hosted authorization server |
| `/.well-known/oauth-authorization-server/oauth`, `/.well-known/oauth-protected-resource/mcp/{profile}` | Hosted authorization server and protected resource metadata |
| `/mcp/{profile}` | The profile's MCP endpoint |
| `/console-api/{profile}/session` | Console session bootstrap |
| `/admin/{profile}/...` | Configuration, audit, JWT revocation, Task cancellation, server health and, with an Artifact service, Artifact administration |
| `/artifacts/{profile}/...` | Artifact download and resumable upload, with an Artifact service |

Every route except the probes requires a `Host` header naming the issuer's host.

## Persistence

The `gateway` schema belongs to core: its migration ships with the kernel migrations in
`veoveo-platform-store`, which the Gateway applies at startup. With the store's
`gateway` feature the Gateway reads and writes its OAuth state, refresh families,
revocations, replay identifiers, control-plane revisions and Artifact upload authority
through the store's typed API. Refresh rotation and control-plane publication commit
their audit records in the same transaction through the Audit API's transactional
append; an audit record that cannot commit rolls the change back.

The Gateway's own statements, for registrations and Task routes, are `.sql` files
beside the modules that run them, checked at compile time through sqlx's `query_file!`
macros against the metadata in `.sqlx`; tests keep theirs under `tests/`. No Rust code
builds SQL text.

## Qualification

`tests/registration.rs` starts the binary against a fresh PostgreSQL database with a
generated local installation, registers core's modular fixture server with core's
registration client and calls the Gateway as both acceptance callers. It checks the
pinned namespace, the initial access token, the database grant, which migrates the
server's schema as its group role and holds the server's kernel instance, role
decisions by risk, refusal of undeclared resources, the permissions each forwarded
token carries, that deregistration drops a registration's login, and that a process
that ends without deregistering loses its registration and login after the grace
period. The fixture server verifies every forwarded token with the keys it reads from
the issuer's `jwks_uri`, which equal those of its registration response.

`tests/oauth.rs` starts the binary the same way and checks over HTTP the token
endpoint's error code for a wrongly signed, expired, replayed or misaddressed
assertion, an unknown client, resource or scope and an unsupported grant, and the MCP
endpoint's challenge for a missing, malformed, expired, misaddressed, mistyped, wrongly
signed, revoked or under-scoped token. It follows the protected resource's RFC 9728
metadata to its authorization server's RFC 8414 metadata, checks each issuer, endpoint
and member §2 requires, verifies an issued token with the key at `jwks_uri`, and
compares the forwarded-token JWK Set with the configured key and the RFC 7638
thumbprint of each `kid`. Unit and database tests cover the catalog,
policy adapter, discovery cache, Task routes, refresh families, session binding, audit
readers and the control store.

## Contract Compliance

- Several versions of one slug may hold registrations; the newest routes until
  session version selection exists.
- A server stays registered only while its registration's login holds a live kernel
  instance; a server that never starts one is removed after the grace period.
- Servers are reached at their registered endpoint; dial-in is not implemented.
- Concrete resources a server adds at run time appear in `resources/list` after the
  next catalog generation; templates and reads are always current.
- Readiness, registration internals and Artifact administration answer with RFC 9457
  problem details. Other administrative handlers still answer some errors with a
  status and no body.
- The forwarded-token key set holds the single configured key, so it cannot publish
  the outgoing key beside the incoming one, and a rotation drains every replica.
