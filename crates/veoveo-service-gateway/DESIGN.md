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
| Authorization server metadata, RFC 8414 | `/.well-known/oauth-authorization-server` for the Gateway's issuer and `/.well-known/oauth-authorization-server/oauth` for the hosted authorization server |
| Protected resource metadata, RFC 9728 | `/.well-known/oauth-protected-resource/mcp/{profile}` |
| Problem details, RFC 9457 | HTTP API errors outside OAuth endpoints, typed `https://veoveo.ai/problems/<code>` |
| SurrealDB 3.3.0 | Platform store, LIVE queries for registration changes, and migrations through core's module runner |
| OpenTelemetry | `service.name` is `veoveo-service-gateway` |
| Veoveo installation configuration | `GatewayControlPlane` from `veoveo-mcp-contract`, read as JSON |

## Process

`veoveo-service-gateway serve` starts in this order:

1. Connect to SurrealDB. With root credentials it first creates the namespace and the
   database. It waits up to `VEOVEO_DATABASE_WAIT_SECONDS` for the database.
2. Apply pending kernel migrations with core's module runner. The runner's registry
   also holds every module a namespace claim recorded, because the database holds
   their history.
3. Load the installation configuration from `VEOVEO_CONFIG` and publish it as the
   active revision when its digest differs from the active one.
4. Load the registrations, start the LIVE watcher and listen on
   `VEOVEO_LISTEN_ADDRESS`.

The Gateway's credentials must be able to apply migrations and define database users:
root, or a database user with the `OWNER` role. The other commands are `validate`,
`resolve-secret`, `audit` and `generate-local-installation`.

| Variable | Meaning |
|---|---|
| `VEOVEO_LISTEN_ADDRESS` | Socket address, `127.0.0.1:8080` by default |
| `VEOVEO_ISSUER` | The Gateway's public origin: its RFC 8414 issuer, the `iss` of forwarded tokens and the base of every URL it announces |
| `VEOVEO_CONFIG` | Installation configuration file |
| `VEOVEO_SURREAL_ENDPOINT`, `_NAMESPACE`, `_DATABASE`, `_AUTH_LEVEL`, `_USERNAME`, `_PASSWORD` | Platform database |
| `VEOVEO_SERVER_DATABASE_ENDPOINT` | The database endpoint registration responses name, when servers reach it at another address |
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
- `registration`: the secret reference of the registration secret and the lease
  timings, renewal every 10 seconds and expiry after 30 by default.

`generate-local-installation` writes a complete local installation: the
configuration, its secrets, and two client-credentials callers, `acceptance-member`
and `acceptance-admin`, that hold the `member` and `admin` roles.

## Server Registration

`src/registration` implements the [server registration
protocol](https://github.com/VeoveoAI/veoveo-core/blob/main/mcp/ext-registration/DESIGN.md)
at `/oauth/register` and `/oauth/register/{client_id}`. Admission follows the
protocol's order: the initial access token, the request body, `ClientMetadata::check`,
the software statement against the pinned key and the endpoint URL, then the module.

| Table | Holds |
|---|---|
| `gateway_namespace_claim` | One row per slug: the RFC 7638 thumbprint of the pinned key and the module manifest whose migrations the Gateway applied |
| `gateway_registration` | One row per registration: client identifier, metadata, software statement, SHA-256 digest of the registration access token, database user and lease expiry |

The first registration of a slug creates its claim. A later version may append
migrations to the recorded module; a manifest that changes an applied migration is
refused with `invalid_client_metadata`, and an older version whose migrations are a
prefix of the record registers without migrating. Each registration with a module
receives its own database user, `{slug}_{suffix}` with the `EDITOR` role over the whole
database, which the Gateway removes when the registration ends. Admissions on one
replica are serialized, so each migration sees the other claims.

A renewal is an update that repeats the metadata. The Gateway extends the lease and
returns the presented registration access token, so tokens do not rotate. An update
with changed declarations takes full admission. Reads return the stored statement.
A registration whose lease lapsed answers HTTP 401, and its row and database user are
removed. Every claim, migration, registration, declaration update, removal and expiry
writes an audit record; renewals do not.

Every replica follows `gateway_registration` through a LIVE query and checks leases
every second. It routes each slug to its newest current registration and replaces
the catalog only when the routed set or a declaration changes, so renewals leave the
discovery caches intact. Requests in flight finish on the catalog snapshot they
started with. Server health comes from the leases: healthy while renewals arrive on
schedule, degraded after a late renewal.

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
| `GET /.well-known/oauth-authorization-server` | RFC 8414 metadata of the Gateway's issuer, with `registration_endpoint` when registration is configured |
| `POST /oauth/register`, `GET`/`PUT`/`DELETE /oauth/register/{client_id}` | Server registration |
| `/oauth/authorize`, `/oauth/callback`, `/oauth/token`, `/oauth/revoke`, `/oauth/jwks.json` | Hosted authorization server |
| `/.well-known/oauth-authorization-server/oauth`, `/.well-known/oauth-protected-resource/mcp/{profile}` | Hosted authorization server and protected resource metadata |
| `/mcp/{profile}` | The profile's MCP endpoint |
| `/console-api/{profile}/session` | Console session bootstrap |
| `/admin/{profile}/...` | Configuration, audit, JWT revocation, Task cancellation, server health and, with an Artifact service, Artifact administration |
| `/artifacts/{profile}/...` | Artifact download and resumable upload, with an Artifact service |

Every route except the probes requires a `Host` header naming the issuer's host.

## Persistence

The library embeds its SurrealQL from `src/queries/`, one file per statement; tests
use `tests/queries/`. Callers bind every runtime value. The Gateway's tables belong
to the kernel `gateway` module in `veoveo-platform-store`.

## Qualification

`tests/registration.rs` starts the binary against a disposable SurrealDB with a
generated local installation, registers core's modular fixture server with core's
registration client and calls the Gateway as both acceptance callers. It checks the
pinned namespace, the initial access token, role decisions by risk, refusal of
undeclared resources, the permissions each forwarded token carries, and that an
expired lease removes the server. Unit and store tests cover the catalog, policy
adapter, discovery cache, Task routes, refresh families, audit readers and the
control store.

## Contract Compliance

- Several versions of one slug may hold registrations; the newest routes until
  session version selection exists.
- A server's database user has database-wide `EDITOR` access; limiting it to the
  server's table prefix is tracked separately.
- Servers are reached at their registered endpoint; dial-in is not implemented.
- Concrete resources a server adds at run time appear in `resources/list` after the
  next catalog generation; templates and reads are always current.
- Readiness, registration internals and Artifact administration answer with RFC 9457
  problem details. Other administrative handlers still answer some errors with a
  status and no body.
