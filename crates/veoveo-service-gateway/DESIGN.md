# Veoveo Gateway

The Gateway is the MCP entry point of a Veoveo installation. It authenticates
callers, admits MCP servers that register themselves, authorizes every request with
the installation's roles over the permissions each server declares, and forwards the
request with a fresh access token that names the caller and the permissions granted.
Neither its code nor its configuration names a server.

## Standards And Protocols

| Standard | Profile |
|---|---|
| MCP `2026-07-28`, Streamable HTTP | One endpoint per profile at `/mcp/{profile}`, aggregating tools, resources, resource templates, prompts, completions, Tasks and resource subscriptions of registered servers. `server/discover` lists `2026-07-28` as the only version, and another version is refused with `-32022`. The `resources.subscribe` capability declares `resourceSubscriptions` on `subscriptions/listen`; `resources/subscribe` and `resources/unsubscribe` are no methods of that version and answer `-32601` |
| MCP Apps, extension `io.modelcontextprotocol/ui` | A server's Apps live under `ui://{slug}/`; `_meta.ui.resourceUri` keeps its value |
| [Server registration protocol](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/5216efe847079c720282e934ab09417be1265266/crates/veoveo-sdk-rust/src/registration/DESIGN.md) | RFC 7591 registration, RFC 7592 read, update and delete, RFC 8414 discovery, software statements under RFC 7523 claim rules, and the `veoveo_*` client metadata members |
| JWT access tokens, RFC 9068, with RFC 9396 and RFC 8693 `act` | The identity forwarded to a server, issued by the SDK's `GatewayInternalTokenIssuer`; a call another server made names that server as the prior actor |
| OAuth 2.0 Token Exchange, RFC 8693, with RFC 9396 token requests | `/oauth/token` with the `urn:ietf:params:oauth:grant-type:token-exchange` grant, for registered servers that declare uses; profile in [calls between servers](#calls-between-servers) |
| OAuth 2.0, RFC 6749, and RFC 7523 | The hosted authorization server; see [token and session authority](src/auth/DESIGN.md) |
| Authorization server metadata, RFC 8414 | `/.well-known/oauth-authorization-server` for the Gateway's issuer and `/.well-known/oauth-authorization-server/oauth` for the hosted authorization server; each names its `jwks_uri`, and `response_types_supported`, `grant_types_supported` and `token_endpoint_auth_methods_supported` are always present because §2 gives an absent member a default |
| JWK Set, RFC 7517, and JWK Thumbprint, RFC 7638 | `/jwks.json` and `/oauth/jwks.json`, served as `application/jwk-set+json`: public Ed25519 keys whose `kid` is the RFC 7638 thumbprint |
| Protected resource metadata, RFC 9728 | `/.well-known/oauth-protected-resource/mcp/{profile}` |
| Problem details, RFC 9457 | HTTP API errors outside OAuth endpoints, typed `https://veoveo.ai/problems/<code>` |
| PostgreSQL 18 through sqlx 0.9.0 | The installation database: core's kernel schemas, then the Gateway's own `gateway` schema, applied at startup; `LISTEN` on the `gateway` channel for registration changes; one login role per registration |
| OpenTelemetry | `service.name` is `veoveo-service-gateway` |
| Veoveo installation configuration | `GatewayControlPlane` in `src/control_plane.rs`, read as JSON |
| [Veoveo audit envelope](https://github.com/VeoveoAI/veoveo-core/blob/50e72a3a1895d59b4f1eedd2b50cf947fa5d2e72/platform/audit/contract/DESIGN.md), after NIST SP 800-53 Rev. 5 AU-3 and mapped to OCSF 1.9 | Records name owner-declared classes, each with its OCSF class and activity: the SDK's `mcp.*` classes, core's `task.*` and `audit.*` classes, and the Gateway's `gateway.*` classes. The Gateway's records carry its own reason codes and name targets by URI |
| [Veoveo usage ledger](https://github.com/VeoveoAI/veoveo-core/blob/50e72a3a1895d59b4f1eedd2b50cf947fa5d2e72/platform/usage/DESIGN.md), after FOCUS 1.4 with ISO 4217 currencies | The Gateway prepares the ledger's monthly partitions and serves daily totals across servers |

## Process

`veoveo-service-gateway serve` starts in this order:

1. Connect to PostgreSQL with `VEOVEO_DATABASE_URL` and `VEOVEO_DATABASE_PASSWORD`,
   waiting up to `VEOVEO_DATABASE_WAIT_SECONDS` for the database to accept the login.
2. Apply pending kernel migrations, then the Gateway's own migrations of the `gateway`
   schema, whose history is `gateway._sqlx_migrations`. Replicas starting together
   wait on the migrators' locks.
3. Record the audit classes the Gateway writes in `audit.detail_type`, before its first
   audit record. Repeating a declaration changes nothing.
4. Load the installation configuration from `VEOVEO_CONFIG` and publish it as the
   active revision when its digest differs from the active one. A replica that loses
   the publication to another one with the same file continues.
5. Load the registrations, start the registration listener and the housekeeping loop,
   and listen on `VEOVEO_LISTEN_ADDRESS`.

The Gateway connects as the installation owner, a role that owns the database and has
`CREATEROLE`, because it applies the kernel and Gateway migrations and creates each
registration's login. The other commands are `validate`, `resolve-secret`, `audit` and
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
| `VEOVEO_FORWARDED_TOKEN_VERIFICATION_KEY_B64` | Optional Ed25519 PKCS#8 key whose public half is published beside the signing key's and which never signs; the Gateway keeps only that public half |
| `VEOVEO_REFRESH_DELIVERY_KEY_B64` | 32-byte key for refresh successor envelopes |
| `VEOVEO_AUDIT_SIGNING_KEY_B64`, `VEOVEO_AUDIT_RETENTION_DAYS`, `VEOVEO_AUDIT_EXPORT_CONFIG` | Audit sealing, retention and export |
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
protocol](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/5216efe847079c720282e934ab09417be1265266/crates/veoveo-sdk-rust/src/registration/DESIGN.md)
at `/oauth/register` and `/oauth/register/{client_id}`. Admission follows the
protocol's order: the initial access token, the request body, `ClientMetadata::check`,
then the software statement against the pinned key and the endpoint URL. The slug
`gateway` names the Gateway's own schema and is refused with `invalid_client_metadata`.

| Table in the `gateway` schema | Holds |
|---|---|
| `namespace_claim` | One row per slug: the RFC 7638 thumbprint of the pinned key |
| `registration` | One row per registration: client identifier, metadata, software statement, SHA-256 digest of the registration access token, its login, and `orphaned_at`, when housekeeping found that login without a live process, which is NULL while it holds one |

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
seconds and writes only when a login's liveness changed. It sets `orphaned_at` on a
registration whose login it first finds without a live kernel instance, clears it when
an instance appears again, and removes the registrations orphaned for longer than
`VEOVEO_REGISTRATION_GRACE_SECONDS`, dropping their logins in the same transaction. An
installation whose servers keep their instances writes nothing while idle. Each claim,
registration, declaration update, deregistration and removal writes an audit record.

Every replica listens on the `gateway` channel. A row inserted or deleted, or a
metadata change, wakes it to reload the one registration the notification names by
its client identifier. Every reconnection of the listener reloads all of them, because
PostgreSQL does not replay notifications. A change of `orphaned_at` wakes no one. A
replica routes each slug to its newest registration and replaces the catalog only when
the routed set or a declaration changes, so discovery caches stay intact otherwise.
Requests in flight finish on the catalog snapshot they started with. Server health
comes from liveness: healthy while the routed registration's login holds a live
instance, degraded from `orphaned_at` until housekeeping removes it.

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

`policy::decide` in `src/policy` makes every decision. A registered server's tools,
resources, templates, prompts and Tasks are decided by installation roles over that
server's declared permissions. A role grants a permission only when it allows its
access kind and every one of its risks. A target the server did not declare is denied.
The Gateway's own administrative actions are decided by the configuration's policy
rules.

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

The set lists the key `VEOVEO_FORWARDED_TOKEN_KEY_B64` names, which signs every
forwarded token, and then the public half of
`VEOVEO_FORWARDED_TOKEN_VERIFICATION_KEY_B64` when that is set. Startup refuses a
verification key equal to the signing key. A rolling rotation from key A to key B
takes three rollouts, each complete on every replica before the next begins:

1. Keep A as the signing key and set B as the verification key. Every replica then
   publishes A and B.
2. Make B the signing key and A the verification key. Replicas of either rollout
   publish both keys, so a server verifies a token from any of them, and a server that
   meets B's `kid` for the first time finds it at `jwks_uri`.
3. When 15 minutes, the longest lifetime of a forwarded token, have passed since the
   last replica stopped signing with A, remove the verification key.

## Calls Between Servers

`src/delegation.rs` implements the exchange of the SDK's [calls between
servers](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/5216efe847079c720282e934ab09417be1265266/crates/veoveo-sdk-rust/src/registration/DESIGN.md#calls-between-servers)
and the admission of the tokens it issues. A registered server authenticates at
`/oauth/token` with an RFC 7523 assertion signed by the key its registration pinned;
the Gateway records the assertion's `jti` with the other replay identifiers. The
registration must hold the token exchange grant, which a server registers exactly when
it declares uses. The subject is a forwarded token addressed to that server and not
itself delegated, or one of the server's Tasks.

A Task is a subject while it runs. When the Gateway records the route of a Task that
a server with declared uses created for a caller's own call, the route keeps that
caller in `delegated_caller`: the OAuth client, actor, invocation authority and request
context the Gateway authenticated. A Task another server's call created keeps none, so
a delegation never extends past one calling server. The exchange reads the route by
server and Task identifier, and core's `tasks.task` for that server must hold the Task
as queued, running or waiting in the caller's tenant. A Task stops acting for a caller
whose browser session the Gateway revoked because one of its refresh tokens was
replayed: the exchange is refused with `invalid_grant` and a Task-bound token is no
longer admitted. A logout, a session that expired or a caller without a session
leaves the delegation in place until the Task ends.

The token carries the requested permissions that the server declared and the caller
holds now: the installation's roles and overrides are evaluated again at each
exchange, over each provider's current declaration, for the caller as authenticated.
A request permission no registered server declares refuses the exchange with
`invalid_target`, whose description names each such permission. A token for a request
subject ends with the subject token; one for a Task lives at most five minutes and
until the route expires. Each exchange, and each refusal after client authentication,
writes a `gateway.token_exchange` record whose target is the calling server and whose
actor is the caller once the subject is known.

At `/mcp/{profile}` the Gateway first reads a bearer's unverified `iss`. A token it
issued itself is an exchanged token and is verified with the forwarded-token keys for
the profile's protected resource, whatever authorization servers the profile names;
for a Task subject the Task must still run. The admitted subject is the caller, with
its delegation: the acting server, the token's permissions and, for a Task subject, the
Task. Every policy decision for that subject, for tool calls, resource reads and
subscriptions alike, narrows the role-granted permissions to the token's, so a target
the token does not carry is denied as a missing permission. The forwarded token names
the calling server as the prior actor and, for a Task subject, the calling server's
Task as `acting_task`, by the identifier that server issued. Every record of a
delegated request names the calling server, and its Task when the identifier is a core
Task identifier, in the envelope's `via`.

## Audit Records

The Gateway records with the installation's registry: the SDK's MCP classes, core's
platform classes and its own `gateway.*` classes, each declared with its OCSF class.
`gateway.authentication` maps to OCSF Authentication (3002), `gateway.token_exchange`
to Authentication's Service Ticket Request (3002, 4), `gateway.account_change` to
Account Change (3001), `gateway.registration` to Entity Management (3004),
`gateway.task_status` to API Activity Read (6003, 2), and the administrative classes to
API Activity (6003).

A record's reason is a code the Gateway owns. A policy decision records its policy
reason code, such as `missing_permission` or `unknown_tool`, and an authentication its
authentication reason code, such as `invalid_bearer_token` or `refresh_token_replay`.
A failed administrative operation records the failed step, such as
`persist_jwt_revocation`, and a refused token exchange its OAuth error. Other outcomes
use `GatewayReason`: `invalid_request`, `internal_failure`, `not_found`,
`upstream_failure`, `upstream_denied`, `missing_scope`, `partition_denied`, `revoked`
and `unavailable`. A successful action records no reason.

Targets are URIs. The Gateway names what it owns in the `veoveo` scheme:
`veoveo://gateway`, `veoveo://servers/{server}`, `veoveo://profiles/{profile}`,
`veoveo://clients/{client}`, `veoveo://tenants/{tenant}/work-contexts/{context}` and
`veoveo://audit/installation` or `veoveo://audit/tenants/{tenant}` for an audit
partition. A resource is named by its own URI, a Task by core's UUID URN, and tools,
resource templates, prompts, discovery and Task routes by the SDK's MCP targets. The
details of caller records carry the invocation facts: the OAuth client, the principal
that delegated to the actor, the policy revision, and the actor's scopes and data
labels.

## Usage

The Gateway keeps core's usage ledger writable and readable across servers. At
startup and then once a day it creates the ledger's monthly record partitions through
three months ahead, retrying a failed attempt after a minute.

`GET /admin/{profile}/usage/daily?scope=own|tenant&from=YYYY-MM-DD&to=YYYY-MM-DD`
returns daily totals from `usage.daily` for the days from `from` up to `to`, at most
366 days, by day, principal, server, kind and phase, with the money of usage that
carries it:

```json
{
  "rows": [
    {
      "day": "2026-10-13",
      "principal": "https://gateway.example/oauth#acceptance-member",
      "server": "media",
      "kind": "media.generation",
      "phase": "actual",
      "quantity": "1",
      "money": { "amount": "0.05", "currency": "USD" },
      "records": 1
    }
  ],
  "nextCursor": "eyJzY29wZSI6Im93biIs..."
}
```

`scope=own`, the default, reads the caller's own totals in its tenant. `scope=tenant`
reads every principal's in the caller's tenant and is an administrative read: the
action policy decides `admin_read`, and the admission is recorded as the `usage_read`
operation. A page holds up to `limit` rows, 100 by default and at most 1000, and
`cursor` continues a page only for the scope, principal and days it came from. The
Gateway reads the table directly as the installation owner; servers write the ledger
through core's confined functions.

## Upstream Transport

The workspace pins RMCP 3.5.0 from `VeoveoAI/rust-sdk` at
`917e7914c93975fc1eddbff8792f1e2de933bc17`, the revision `veoveo-sdk-rust` pins. The fork
exposes HTTP rejection status and body through `StreamableHttpError::HttpResponse` and
keeps exact Task subscription filters. Upstream discovery has a ten-second deadline and one retry after a transport
failure; authorization challenges, malformed responses and MCP errors fail at once,
and no mutation is retried.

Structured results, resource contents and unknown metadata keep their upstream
values. Upstream MCP errors keep their code, message and data. A non-success HTTP
response without an MCP error keeps its status, and an unexpected redirect becomes
HTTP 502 without its location.

The Gateway checks resource policy before it reads. The read handler commits one
`mcp.read` audit record before it returns a result; a failed commit prevents delivery.

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
| `/admin/{profile}/...` | Configuration, audit, JWT revocation, Task cancellation, server health and usage |

Every route except the probes requires a `Host` header naming the issuer's host.

## Persistence

The Gateway owns the `gateway` schema. Its migrations in `migrations/` run after core's
kernel migrations and record their history in `gateway._sqlx_migrations`, as
`sqlx.toml` declares. `src/store` reads and writes the schema through `GatewayStore`:
OAuth requests and codes, refresh families, JWT revocations, replay identifiers and
control-plane revisions. A publication also writes the identity rows of the Work
Contexts it defines in core's `identity` schema. Refresh rotation and control-plane
publication commit their audit records in the same transaction through the audit
envelope's transactional append; an audit record that cannot commit rolls the change
back.

Every replica runs an hourly retention pass that deletes expired authorization
requests and codes, JWT revocations, replay identifiers, refresh tokens and families
and Task routes, and a pass every minute that clears ended refresh delivery
envelopes. Each prune goes through core's batched maintenance: one statement changes at
most 1,000 rows in its own transaction, skipping rows another replica holds, and the
prune repeats it until a statement changes fewer.

The Gateway's own statements, for registrations and Task routes, are `.sql` files
beside the modules that run them, checked at compile time through sqlx's `query_file!`
macros against the metadata in `.sqlx`; tests keep theirs under `tests/`. No Rust code
builds SQL text. `.sqlx` also holds the metadata of the core crates' queries, because
the check stage compiles them offline from an empty target directory.

`tests/database_conformance.rs` holds the Gateway's own statements to core's
[database rules](https://github.com/VeoveoAI/veoveo-core/blob/50e72a3a1895d59b4f1eedd2b50cf947fa5d2e72/docs/DATABASE.md#server-schema-rules)
over the `gateway`, `identity` and `kernel` schemas. The fixture applies the Gateway's
migrations as the installation owner after the kernel migrations, as startup does, and
plans every statement as that owner. Refresh families and control-plane revisions are
read without a tenant predicate: a family is addressed by its identifier or a token
digest, and a revision configures the whole installation.
A Task route lookup names the caller's tenant, except the exchange's lookup of a
server's own Task by the identifier it issued, which finds the caller there.
Registration housekeeping and
reconciliation read every registration and every process instance of the installation
by design, and each such statement carries a `-- veoveo:` marker with its reason.

## Qualification

`tests/registration.rs` starts the binary against a fresh PostgreSQL database with a
generated local installation, registers the SDK's Observatory fixture server with the
SDK's registration client and calls the Gateway as both acceptance callers. It checks the
pinned namespace, the refusal of the slug `gateway`, the initial access token, the
database grant, which migrates the
server's schema as its group role and holds the server's kernel instance, role
decisions by risk, refusal of undeclared resources, the permissions each forwarded
token carries, that three housekeeping rounds leave a live registration's row version
unchanged, that deregistration drops a registration's login, and that a process that
ends without deregistering loses its registration and login after the grace period.
The fixture server verifies every forwarded token with the keys it reads from the
issuer's `jwks_uri`, which equal those of its registration response. The Gateway runs
with a verification key, so the test also checks that the set lists it after the
signing key and that every forwarded token carries the signing key's `kid`.

`tests/oauth.rs` starts the binary the same way and checks over HTTP the token
endpoint's error code for a wrongly signed, expired, replayed or misaddressed
assertion, an unknown client, resource or scope and an unsupported grant, and the MCP
endpoint's challenge for a missing, malformed, expired, misaddressed, mistyped, wrongly
signed, revoked or under-scoped token. It follows the protected resource's RFC 9728
metadata to its authorization server's RFC 8414 metadata, checks each issuer, endpoint
and member §2 requires, verifies an issued token with the key at `jwks_uri`, and
compares the forwarded-token JWK Set with the configured key and the RFC 7638
thumbprint of each `kid`.

`tests/calls.rs` registers the SDK's Ledger and Relay fixture servers, Relay with a
durable Task on its own login, and checks calls between them as both acceptance
callers: the caller, client and acting server Ledger receives, the act chain of the
delegated token, narrowing of a sensitive permission to the admin, a Ledger Task
resolved through Relay's Task handle, a Relay Task that calls Ledger after the call
that created it returned and is refused once it ended, the token endpoint's refusal of
an undeclared use, another profile, another server's subject and a delegated subject,
the audit records of exchanges and delegated admissions with the calling server in
`via`, the refusal of a Task whose caller's session was revoked for a replayed refresh
token, and the refusal naming the missing permission once Ledger deregisters.
`tests/session_family.rs` checks on real families that only a replay, and not a
logout, an expiry or a removed family, ends a Task's delegation.

`tests/usage.rs` starts the binary with an action policy that lets the administrator
make administrative reads, waits for the partitions three months ahead, writes two
servers' usage to the ledger and checks the member's own totals over two pages, the
refused and recorded tenant read of the member, the administrator's tenant totals
across both servers, and the refusal of a cursor from another view and of an empty
range.

`tests/mcp_lifecycle.rs` checks the advertised version and resource capabilities, the
answers to `resources/subscribe` under `2026-07-28` and under `2025-11-25`, and the
acknowledgement of a `subscriptions/listen` filter. Unit and database tests cover the
catalog, policy evaluator, discovery cache, Task routes, refresh families, session
binding, the audit vocabulary and readers, and the control store.

## Contract Compliance

- Several versions of one slug may hold registrations; the newest routes until
  session version selection exists.
- A server stays registered only while its registration's login holds a live kernel
  instance; a server that never starts one is removed after the grace period.
- Servers are reached at their registered endpoint; dial-in is not implemented.
- Concrete resources a server adds at run time appear in `resources/list` after the
  next catalog generation; templates and reads are always current.
- Readiness and registration internals answer with RFC 9457 problem details. Other administrative handlers still answer some errors with a
  status and no body.