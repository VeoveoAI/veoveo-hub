# Veoveo Hub

The Hub is the MCP entry point of a Veoveo installation. It authenticates
callers, admits MCP servers that register themselves, authorizes every request with
the installation's roles over the permissions each server declares, and forwards the
request with a fresh access token that names the caller and the permissions granted.
Neither its code nor its configuration names a server.

## Standards And Protocols

| Standard | Profile |
|---|---|
| MCP `2026-07-28`, Streamable HTTP | One endpoint per profile at `/mcp/{profile}`, aggregating tools, resources, resource templates, prompts, completions, Tasks and resource subscriptions of registered servers. `server/discover` lists `2026-07-28` as the only version, and another version is refused with `-32022`. The `resources.subscribe` capability declares `resourceSubscriptions` on `subscriptions/listen`; `resources/subscribe` and `resources/unsubscribe` are no methods of that version and answer `-32601` |
| MCP Apps, extension `io.modelcontextprotocol/ui` | A server's Apps live under `ui://{slug}/`; `_meta.ui.resourceUri` keeps its value |
| [Server registration protocol](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/523c7ace42660a20b213bef7acb31398813fec97/crates/veoveo-hub-sdk/src/registration/DESIGN.md) | RFC 7591 registration, RFC 7592 read, update and delete, RFC 8414 discovery, software statements under RFC 7523 claim rules, and the `veoveo_*` client metadata members |
| JWT access tokens, RFC 9068, with RFC 9396 and RFC 8693 `act` | The identity forwarded to a server, issued by the SDK's `HubInternalTokenIssuer`; a call another server made names that server as the prior actor |
| OAuth 2.0 Token Exchange, RFC 8693, with RFC 9396 token requests | `/oauth/token` with the `urn:ietf:params:oauth:grant-type:token-exchange` grant, for registered servers that declare uses; profile in [calls between servers](#calls-between-servers) |
| OAuth 2.0, RFC 6749, and RFC 7523 | The hosted authorization server; see [token and session authority](src/auth/DESIGN.md) |
| Authorization server metadata, RFC 8414 | `/.well-known/oauth-authorization-server` for the Hub's issuer and `/.well-known/oauth-authorization-server/oauth` for the hosted authorization server; each names its `jwks_uri`, and `response_types_supported`, `grant_types_supported` and `token_endpoint_auth_methods_supported` are always present because §2 gives an absent member a default |
| OpenID Connect Discovery 1.0 metadata location, by RFC 8414 §5 | The hosted authorization server's RFC 8414 metadata also at `/oauth/.well-known/openid-configuration`, for discovery clients that read the issuer and `jwks_uri` there. The Hub issues no ID tokens at that issuer and claims no OpenID Provider conformance |
| JWK Set, RFC 7517, and JWK Thumbprint, RFC 7638 | `/jwks.json` and `/oauth/jwks.json`, served as `application/jwk-set+json`: public Ed25519 keys whose `kid` is the RFC 7638 thumbprint |
| Protected resource metadata, RFC 9728 | `/.well-known/oauth-protected-resource/mcp/{profile}` |
| Problem details, RFC 9457 | HTTP API errors outside OAuth endpoints, typed `https://veoveo.ai/problems/<code>` |
| PostgreSQL 18 through sqlx 0.9.0 | The installation database: core's kernel schemas, then the Hub's own `hub` schema, applied at startup; `LISTEN` on the `hub` channel for registration changes; one login role per registration. A database per data environment in the same instance, with core's kernel schemas |
| OpenTelemetry | `service.name` is `veoveo-hub` |
| Veoveo installation configuration | `HubControlPlane` in `src/control_plane.rs`, read as JSON |
| [Veoveo audit envelope](https://github.com/VeoveoAI/veoveo-core/blob/9a6b3ff95469ff19f9ccb513421a57ba6751aa9e/platform/audit/contract/DESIGN.md), after NIST SP 800-53 Rev. 5 AU-3 and mapped to OCSF 1.9 | Records name owner-declared classes, each with its OCSF class and activity: the SDK's `mcp.*` classes, core's `task.*` and `audit.*` classes, and the Hub's `hub.*` classes. The Hub's records carry its own reason codes and name targets by URI |
| [Veoveo usage ledger](https://github.com/VeoveoAI/veoveo-core/blob/9a6b3ff95469ff19f9ccb513421a57ba6751aa9e/platform/usage/DESIGN.md), after FOCUS 1.4 with ISO 4217 currencies | The Hub prepares the ledger's monthly partitions and serves daily totals across servers |

## Execution Composition And Deployment Identity

The live control plane's `execution` section is an internal routing output. It
declares an explicit default, owned environments, optional pinned or following
bases, and owned logical-component selections. A missing component inherits; an
explicit `unavailable` selection does not fall back. Display names convey no
authority. Fresh selection checks tenant, reader/owner and base-sharing access;
cycles and missing references are rejected. A pin shares one retained readable
composition, rather than recomposing its current ancestors. Deployment references
and pins are admitted before publication; health is checked at use, so temporary
target failure cannot block an unrelated revision.

RFC 7591 metadata carries a deployment ID. A deployment registers either with the
installation's registration secret under the key its logical namespace is pinned
to, or as an instance with the registration token the Hub created for it, and
receives the namespace's explicit `shared_logical_schema` storage binding. Shared
schema is an explicit data binding, not owner isolation. A registration of a
namespace deployment with new metadata, under the pinned key, replaces its contract,
endpoint and storage, and an instance's every registration replaces them, key
included. Registrations and health remain independent per deployment; a namespace
registration alone never selects a serving component, and an instance serves through
selection.

Each authenticated stateless MCP operation admits at most one `veoveo-environment`
header, an untrusted typed selector, or uses the configured default. The resource
URL stays `/mcp/{profile}`. The Hub resolves and retains the SDK's canonical immutable
composition once, then selects exact registered deployments for all surfaces.
Signed downstream context keeps this same capture while the target deployment
changes per hop. Descriptor caches and catalog continuation are scoped to that
composition; a continuation from another composition is refused. Domain collection
cursors remain owned by their servers. HTTP pools use deployment plus endpoint.
Publication hints complete existing listens normally, requiring native re-listen
and current admission instead of silently keeping a stale selection alive.

Each authenticated principal's selection, `hub.selection`, names a branch per
server. A fresh admission starts from the environment's composition; each server
that has registered instances, or that the selection names, then routes to the
instance of the selected branch, or else of the configured `execution.baseBranch`.
When that instance is missing, when several instances serve that branch, or when no
base branch is configured, the component is `unavailable`, never another branch or
the configured component, and a tool call to that server answers with the cause.
`PUT /selection/{profile}/{server}` with `{"branch": "…"}` and `DELETE` of the same
path change the caller's own selection and nothing else, so authentication is their
only check; a delegated caller changes the selection of the principal it carries.
Each change notifies the `hub` channel with the principal, and every replica
completes only that principal's open listens with a list-changed hint.

Tasks persist exact deployment and composition before the projected response.
A native retry of the same Task preserves its first capture. Later reads, changes,
cancellation, delegated work and re-listened Task filters use that capture while
checking current selected-environment access and component/data policy. Editing
current selection or base ancestry cannot retarget retained work. Revoking access
to the captured environment denies it even if another readable environment selects
the same deployment. Current native destination bindings remain authoritative: old
bindings must be retained while work drains; explicit withdrawal makes that work
unavailable and is never undone from historical provenance. Core's independent
Task deployment affinity prevents same-namespace workers sharing a schema from
claiming or recovering one another's work.

This boundary does not build or run deployments; routing outputs enter through
control-plane publication, instance registrations and selection. Public provider callbacks need their own
deployment-qualified ingress proof; MCP routing alone does not establish it.

## Process

`veoveo-hub serve` starts in this order:

1. Connect to PostgreSQL with `VEOVEO_DATABASE_URL` and `VEOVEO_DATABASE_PASSWORD`,
   waiting up to `VEOVEO_DATABASE_WAIT_SECONDS` for the database to accept the login.
2. Apply pending kernel migrations, then the Hub's own migrations of the `hub`
   schema, whose history is `hub._sqlx_migrations`. Replicas starting together
   wait on the migrators' locks.
3. Record the audit classes the Hub writes in `audit.detail_type`, before its first
   audit record. Repeating a declaration changes nothing.
4. Load and admit the active database revision. `VEOVEO_CONFIG` seeds only an empty
   installation; restarting never republishes a stale seed over live changes.
   A concurrent bootstrap adopts the winning active revision. To deliberately
   replace installation configuration, use trusted `import-control-plane` while
   the hosted process is stopped; routine changes use the live administrative API.
5. Load registrations and start the native registration and active-control-revision
   listeners, retention and housekeeping loops, then listen on `VEOVEO_LISTEN_ADDRESS`.
   Active revisions emit kernel notifications only after commit; each replica also
   reconciles on initial subscription and reconnection.

While it serves, the Hub holds a kernel instance of its own on the owner's login,
and it claims no Task work. When a lost connection ends the Hub's database sessions, its pools open new
connections, the registration listener reconnects and reloads every registration, and
core's instance takes its lock back on a new connection, so the Hub keeps serving
without a restart.

The Hub connects as the installation owner, a role that owns the databases and has
`CREATEROLE` and `CREATEDB`, because it applies the kernel and Hub migrations, creates
each registration's login and creates each data environment's database. The other commands are `validate`, `resolve-secret`, `audit` and
`generate-local-installation`; `audit` reads the same database settings.

| Variable | Meaning |
|---|---|
| `VEOVEO_LISTEN_ADDRESS` | Socket address, `127.0.0.1:8080` by default |
| `VEOVEO_ISSUER` | The Hub's public origin: its RFC 8414 issuer, the `iss` of forwarded tokens and the base of every URL it announces |
| `VEOVEO_CONFIG` | Installation configuration file |
| `VEOVEO_DATABASE_URL`, `VEOVEO_DATABASE_PASSWORD` | The installation database as a `postgres://` URL naming host, database and user, and the user's password |
| `VEOVEO_DATABASE_WAIT_SECONDS` | How long startup waits for the database, 60 seconds by default |
| `VEOVEO_SERVER_DATABASE_URL` | The database URL registration grants name, when servers reach it at another address; by default the Hub's own URL without its user |
| `VEOVEO_REGISTRATION_GRACE_SECONDS` | How long a registration may go without a live process before the Hub removes it, 60 seconds by default |
| `VEOVEO_FORWARDED_TOKEN_KEY_B64` | Ed25519 PKCS#8 key that signs forwarded tokens |
| `VEOVEO_FORWARDED_TOKEN_VERIFICATION_KEY_B64` | Optional Ed25519 PKCS#8 key whose public half is published beside the signing key's and which never signs; the Hub keeps only that public half |
| `VEOVEO_REFRESH_DELIVERY_KEY_B64` | 32-byte key for refresh successor envelopes |
| `VEOVEO_AUDIT_SIGNING_KEY_B64`, `VEOVEO_AUDIT_RETENTION_DAYS`, `VEOVEO_AUDIT_EXPORT_CONFIG` | Audit sealing, retention and export |
| `VEOVEO_ALLOW_LOOPBACK_HOSTS` | Accept loopback `Host` headers beside the issuer's host |

The configuration's `secrets` section names the remaining secrets, such as the
registration secret and the hosted authorization server's signing key, by
environment variable. The process also reads a `.env` file in its working directory.

## Configuration

The installation configuration holds identity providers, the hosted authorization
server, profiles, OAuth clients, tenants, Work Contexts, policy rules for the Hub's
own actions, and two sections for registered servers:

- `authorization`: roles that select permissions by access kind and risk, grants of
  roles to groups or principals, and overrides that add a risk to or deny a permission;
- `registration`: the secret reference of the registration secret.

`generate-local-installation` writes a complete local installation: the
configuration, its secrets, and two client-credentials callers, `acceptance-member`
and `acceptance-admin`, that hold the `member` and `admin` roles.

## Server Registration

`src/registration` implements the [server registration
protocol](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/523c7ace42660a20b213bef7acb31398813fec97/crates/veoveo-hub-sdk/src/registration/DESIGN.md)
at `/oauth/register` and `/oauth/register/{client_id}`. Admission follows the
protocol's order: the initial access token, the request body, `ClientMetadata::check`,
then the software statement against the pinned key and the endpoint URL. The slug
`hub` names the Hub's own schema and is refused with `invalid_client_metadata`. An
initial access token whose digest names an instance registers that instance; any
other token must be the installation's registration secret.

| Table in the `hub` schema | Holds |
|---|---|
| `namespace_claim` | One row per slug: the RFC 7638 thumbprint of the pinned key |
| `instance` | One row per instance: its deployment ID, the Lore repository and branch it serves, one instance per pair, the SHA-256 digest of its registration token, the database its server registers into, and what the Hub records about its factory sandbox: the latest pusher, the revision it last applied, when its server last registered or resumed, its last failed step and the capacity warning of its creation |
| `deployment` | One row per deployment: its slug, metadata and storage binding |
| `registration` | One row per registration: client identifier, deployment, metadata, software statement, SHA-256 digest of the registration access token, its login, and `orphaned_at`, when housekeeping found that login without a live process, which is NULL while it holds one |

One transaction registers a server: it claims the namespace on the slug's first
registration, or finds it pinned to the same key, calls `kernel.ensure_server` for the
server's group role `server_<schema>` and its schema, creates the registration's login
with `kernel.create_server_login`, and inserts the registration row. A namespace pinned
to another key is refused with `unapproved_software_statement`. A registration naming a
deployment registered before with other metadata replaces it in the same transaction,
as an instance's registration does: it writes the deployment's slug, metadata and
storage binding, and removes the deployment's registrations with their logins. When
the deployment belonged to another slug, that slug's namespace must be pinned to the
same key. A registration with the deployment's own metadata adds a replica beside
the others. The login is
`<schema>_<suffix>`, where the suffix is the last 14 hexadecimal digits of the client
identifier, and its password is 256 random bits. The response grants the login as
`veoveo_database`: `VEOVEO_SERVER_DATABASE_URL`, or the Hub's own URL without its
user, with the login and its password. The Hub keeps only the login's name.

An instance is one branch of a server repository, served from its own sandbox.
`registration::create_instance` gives it a new deployment ID and a random 256-bit
registration token, of which the Hub stores only the digest; the token stays valid
while the instance exists, until `registration::reissue_token` replaces it. Its one check is that the metadata names the instance's
deployment, otherwise `invalid_client_metadata`; the software statement is verified
against the key the metadata presents. It claims no namespace, pins no key, does not
expire and is not used up, so several instances of one slug register at once under
their own keys. Every registration with the token replaces the instance's previous
one in one transaction: it writes the deployment's slug, metadata and storage
binding, removes the deployment's registrations and drops their logins, then calls
`kernel.ensure_server`, creates the new login and inserts its row.

### Data Environments

A data environment is the PostgreSQL database the servers of one branch name use, in
the installation's one PostgreSQL instance; `src/data_environment.rs` implements it.
Each instance names its database. Instances of the base branch name the installation
database, which also keeps all of the Hub's own state; there is no environment table,
template or control database. A database name is 1 to 63 bytes of `[a-z0-9_]`, and
any other name is refused.

`ensure_database` connects to the database, creates it as the installation owner,
which then owns it, when the connection reports it missing, and applies core's kernel
migrations to it. A creation that finds the
database created meanwhile, `duplicate_database` or a unique violation of
`pg_database`, counts as done, and the migrator's lock applies the migrations once, so
concurrent callers all succeed. `drop_database_if_unused` drops a database no
instance names with `DROP DATABASE … WITH (FORCE)`, and never the installation
database; the caller deletes the instances' sandboxes first. `FORCE` ends other
logins' sessions only when the owner has the privileges of `pg_signal_backend`.

Registering an instance outside the installation database first calls
`kernel.ensure_server` in its database, on a connection of its own, then runs the
registration transaction above. The grant names that database at the address and
with the settings of the server database URL. Roles belong to the instance, so the
group role and login serve both databases. A server writes the identity rows its
Tasks need itself. The Hub keeps no connection and no `LISTEN` in an environment's
database: Task get, update, cancel and subscriptions go to the instance, and the one
read of Task state, whether a Task still runs before it acts for its caller, opens a
pool of one connection to the database of the Task's deployment for that read.
Registration health reads liveness in the installation database, which does not see
an instance's server in a data environment, so it reports no instance; an instance's
health comes from its factory sandbox.

A server takes its group role, applies its own migrations in its schema and holds a
kernel instance on its login while it serves. An RFC 7592 update keeps the
registration's deployment and slug, and with them its login; another deployment or
slug is refused with `invalid_client_metadata`. Its software statement is verified
against the key the slug's namespace is pinned to, or for an instance against the key
the metadata presents. Changed metadata replaces the registration's and the
deployment's in one transaction, an update audit record follows, and an unchanged
update writes nothing; it never extends anything.
Reads return the stored statement. A `DELETE` removes the row and drops the login in
one transaction, and the registration access token then answers HTTP 401.

A process can end without deregistering. Every replica's housekeeping loop runs every 5
seconds and writes only when a login's liveness changed. It sets `orphaned_at` on a
registration whose login it first finds without a live kernel instance, clears it when
an instance appears again, and removes the registrations orphaned for longer than
`VEOVEO_REGISTRATION_GRACE_SECONDS`, dropping their logins in the same transaction. It
never marks or removes an instance's registration: OpenShell restarts the instance's
process, and its next registration replaces the previous one. An
installation whose servers keep their instances writes nothing while idle. Each claim,
registration, declaration update, deregistration, removal and replacement writes an
audit record.

Every replica listens on the `hub` channel. A row inserted or deleted, or a
metadata change, wakes it to reload the one registration the notification names by
its client identifier. Every reconnection of the listener reloads all of them, because
PostgreSQL does not replay notifications. A change of `orphaned_at` wakes no one. A
replica routes each slug to its newest registration and replaces the catalog only when
the routed set or a declaration changes, so discovery caches stay intact otherwise.
Requests in flight finish on the catalog snapshot they started with. The health of a
server that is not an instance comes from liveness: healthy while the routed
registration's login holds a live instance, degraded from `orphaned_at` until
housekeeping removes it.

### Factory Sandboxes

An instance runs in its factory sandbox, one OpenShell sandbox in the workspace the
`factory` section names; `src/factory` implements it. The Hub creates, restarts,
recreates and deletes the sandbox, records what OpenShell cannot know, and reads the
rest from OpenShell. It runs no command inside a sandbox.

`Factory::apply(repository, branch, push, prepared)` applies one push: the Lore
repository ID, the branch, the revision with its pusher, and the revision's
`openshell/` as the caller read it, a checkout of `openshell/image/` and the text of
`openshell/policy.yaml`. It finds the instance of that repository and branch or
creates it in the branch's data environment: the installation database for the
current `execution.baseBranch`, otherwise `DatabaseName::of_branch`, the branch name's
ASCII letters and digits in lowercase with each other run as one underscore, cut to
fit, an underscore and the first eight hexadecimal digits of the name's SHA-256, at
most 63 bytes. It ensures that database, builds the image through the configured
`image::Build`, BuildKit's `ImageBuilder` unless a test supplies its own, named by the
sandbox, then creates, restarts or recreates the sandbox and records the revision.
Building changes nothing, so a failed build leaves the sandbox and what it serves as
they were. The caller applies the pushes of one instance one at a time.
`Factory::remove(repository, branch)` deletes the instance, if any.

The sandbox is named by the last two groups of the instance's deployment ID, 17
characters within OpenShell's 19 for a routable name. Every creation of an instance's
sandbox first gives the instance a new registration token and replaces its
registration provider, `<sandbox>-registration`, of the configured
`registrationProfile` with that token as `VEOVEO_REGISTRATION_SECRET`: a provider that
could not be created, or the provider of a sandbox that was replaced, leaves no token
behind that no sandbox holds. The sandbox then gets the pushed image by digest, the
configured CPU and memory as Kubernetes limits, restart policy `always`, that provider,
and the installation's base policy with the network rules of the revision's
`openshell/policy.yaml` added; a repository policy that sets anything else or reuses a
base rule's name is refused. Its environment names `VEOVEO_DEPLOYMENT_ID`,
`VEOVEO_PUBLIC_BASE_URL` (`http://<sandbox>.internal:<port>`), `VEOVEO_PORT` and
`VEOVEO_HUB_ISSUER`. Its fixed start command clones `<loreServer>/<repository>` into
`/sandbox/src` when no checkout exists, runs `lore sync --reset <branch>@LATEST` and
then `exec ./openshell/run`; the repository URL and branch are its arguments.

A push applies to an existing instance by the image its sandbox runs. With the same
image the Hub sets the policy through `UpdateConfig`, which OpenShell keeps when it is
unchanged, then stops and starts the sandbox; the start command fetches the branch's
latest revision. OpenShell's stop answers once the sandbox stopped, from ready, from a
failed program and from a crash loop's automatic restart, and resets the restart
count; its start runs the program at once. A sandbox still provisioning, or starting
for the first time, has not run its start command, which fetches the latest revision
when it does, and OpenShell would refuse to stop it: the Hub only sets its policy.
With another image, or without a sandbox, the Hub deletes the sandbox, waits until
OpenShell no longer has it, and creates it from the new image with an empty disk.
Deleting an instance deletes its sandbox and providers, then removes the instance and
its registrations with their logins in one transaction, then drops its database when
no other instance names it.

Every push that built records its pusher as the instance's latest pusher and clears
when its server last registered or resumed. A registration with the instance's token,
and an RFC 7592 read of an instance's registration, which is how the SDK resumes,
record that time. A step that succeeds records the pushed revision and clears the last
failure; a failed step records `failed{stage, message}` with stage `database`,
`build`, `capacity`, `create`, `restart` or `delete`, and the cause, such as
BuildKit's output or OpenShell's refusal, as its message. Status combines these with
OpenShell's phase, the main program's last exit code, the restart count and the
conditions, which include a scheduler's reason such as `Insufficient cpu`, and names
the sandbox and its workspace, which its output in the log store carries. The applied
revision serves while the sandbox is ready and its server registered or resumed since
the push. An instance is healthy while its sandbox is ready and its last step
succeeded, degraded while the sandbox is on its way to ready or still serves after a
failed step, and offline otherwise. With `caps.instancesPerUser`, creating a sandbox
for an instance whose latest pusher is the latest pusher of more instances records a
warning naming the count and the cap; with `caps.enforce` the Hub refuses it as
`failed{stage: capacity}` instead.

The Hub forwards to an instance through `nativeUpstreams`, which must name the
factory's gateway: an instance's deployment binds to its sandbox in the factory
workspace, at the port of the endpoint it registered, instead of to a control-plane
binding of its endpoint. When the registrations no longer name an instance's
deployment, every replica closes its forwarding listener and forgets its clients.

## Routing And Catalog

The catalog combines the installation configuration with the registration snapshot.
A server's MCP surfaces follow its authorization declaration: tools when it declares
tool requirements, resources and completions when it declares resources or templates,
prompts when it declares prompts. A server's URI scheme equals its slug, so resource
URIs pass through unchanged and the Hub routes them by scheme or `ui://{slug}/`.
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
access kind and every one of its risks. An explicitly declared `Authenticated`
route needs current principal/profile admission and no additional permission; a target
the server did not declare is denied. Server-wide and Task targets retain their
requirement for at least one granted permission in that server's namespace.
The Hub's own administrative actions are decided by the configuration's policy
rules.

## Forwarded Identity

`mcp/upstream_authorized_http.rs` issues a fresh RFC 9068 token for each upstream
HTTP request. Its `aud` is the server's registered MCP endpoint, `client_id` the
caller's OAuth client, `act` the Hub's issuer, and `authorization_details` the
permissions granted in the server's namespace. Ordinary tokens live at most 60
seconds and a `subscriptions/listen` token at most 15 minutes, never longer than the
caller's own token. The server verifies them with the keys its registration response
names in `veoveo_access_tokens`.

The issuer's RFC 8414 metadata names `{issuer}/jwks.json` as its `jwks_uri`. That JWK
Set lists the same keys: the public half of each forwarded-token key, with its RFC
7638 thumbprint as `kid`, `alg` `EdDSA` and `use` `sig`, and no private member. A
server that meets a `kid` outside its registered keys reads the set again from there.
The set is served with `Cache-Control: no-cache`, so every read reaches the Hub.

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
servers](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/f369804dd2bda5c69103f480169769c80fac2859/crates/veoveo-hub-sdk/src/registration/DESIGN.md#calls-between-servers)
and the admission of the tokens it issues. A registered server authenticates at
`/oauth/token` with an RFC 7523 assertion signed by the key its registration pinned;
the Hub records the assertion's `jti` with the other replay identifiers. The
registration must hold the token exchange grant, which a server registers exactly when
it declares uses. The subject is a forwarded token addressed to that server, including
a previously delegated request, or one of the server's own delegating Tasks. The
SDK retains up to eight registered actors in standard RFC 8693 nested `act` claims.
The Hub preserves the original caller/client, actor, invocation authority and signed
request context independently of the current HTTP request's audit correlation.
Neither forwarding nor another exchange can extend the inherited signed deadline.

A Task is a subject while it runs. When the Hub records the route of a Task that
a server with declared uses created for a caller's own call, the route keeps that
caller in `delegated_caller`: the OAuth client, actor, invocation authority and request
context the Hub authenticated. A Task another server's call created keeps none and
cannot acquire further Task delegation. Request composition can continue without
creating Tasks. The signed context retains the originating Task's server and ID,
independently of the immediate server actor. Every exchange and admission of such a
chain reads its unexpired native route, compares the original caller/client, authority,
request context and profile, and requires core's `tasks.task` in the database of the
Task's deployment to hold the Task as queued, running or waiting in the caller's
tenant.
A Task stops acting for a caller
whose browser session the Hub revoked because one of its refresh tokens was
replayed: the exchange is refused with `invalid_grant` and a Task-bound token is no
longer admitted. A logout, a session that expired or a caller without a session
leaves the delegation in place until the Task ends.

The token carries the requested permissions that the server declared and the caller
holds now: the installation's roles and overrides are evaluated again at each
exchange, over each provider's current declaration, for the caller as authenticated.
Admission retains the existing boundary: the signed grant/deadline and current
caller/target policy apply; an already issued short-lived grant is not additionally
revoked merely because the calling server's registration or declared uses changed.
Every subsequent exchange checks its registered caller's current declaration again.
A request permission no registered server declares refuses the exchange with
`invalid_target`, whose description names each such permission. A token for a request
subject ends with the subject token; one for a Task lives at most five minutes and
until the route expires. Each exchange, and each refusal after client authentication,
writes a `hub.token_exchange` record whose target is the calling server and whose
actor is the caller once the subject is known.

At `/mcp/{profile}` the Hub first reads a bearer's unverified `iss`. A token it
issued itself is an exchanged token and is verified with the forwarded-token keys for
the profile's protected resource, whatever authorization servers the profile names;
for a Task subject the Task must still run. The admitted subject is the caller, with
its delegation: the acting server, the token's permissions and, for a Task subject, the
Task. Every policy decision for that subject, for tool calls, resource reads and
subscriptions alike, narrows the role-granted permissions to the token's. Administrative
routes refuse such a subject outright: their decision does not narrow to the token's
permissions, and no server declares them. An
explicit declared `Authenticated` route may carry zero permissions after current
principal/profile and managed-client admission; undeclared targets remain denied,
and server/Task targets retain the nonempty narrowed-grant requirement. A route whose
required permission the token does not carry is denied as a missing permission.
The forwarded token names the immediate calling server as prior actor and retains
earlier actors in its SDK delegation context. For a direct Task subject it names the
calling server's Task as `acting_task`; a later request retains that origin in the
signed context instead of assigning it to the immediate server. Every record of a
delegated request names the calling server, and its Task when the identifier is a core
Task identifier, in the envelope's `via`.


## Audit Records

The Hub records with the installation's registry: the SDK's MCP classes, core's
platform classes and its own `hub.*` classes, each declared with its OCSF class.
`hub.authentication` maps to OCSF Authentication (3002), `hub.token_exchange`
to Authentication's Service Ticket Request (3002, 4), `hub.account_change` to
Account Change (3001), `hub.registration` to Entity Management (3004),
`hub.task_status` to API Activity Read (6003, 2), and the administrative classes to
API Activity (6003).

A record's reason is a code the Hub owns. A policy decision records its policy
reason code, such as `missing_permission` or `unknown_tool`, and an authentication its
authentication reason code, such as `invalid_bearer_token` or `refresh_token_replay`.
A failed administrative operation records the failed step, such as
`persist_jwt_revocation`, and a refused token exchange its OAuth error. Other outcomes
use `HubReason`: `invalid_request`, `internal_failure`, `not_found`,
`upstream_failure`, `upstream_denied`, `missing_scope`, `partition_denied`, `revoked`
and `unavailable`. A successful action records no reason.

Targets are URIs. The Hub names what it owns in the `veoveo` scheme:
`veoveo://hub`, `veoveo://servers/{server}`, `veoveo://profiles/{profile}`,
`veoveo://clients/{client}`, `veoveo://tenants/{tenant}/work-contexts/{context}` and
`veoveo://audit/installation` or `veoveo://audit/tenants/{tenant}` for an audit
partition. A resource is named by its own URI, a Task by core's UUID URN, and tools,
resource templates, prompts, discovery and Task routes by the SDK's MCP targets. The
details of caller records carry the invocation facts: the OAuth client, the principal
that delegated to the actor, the policy revision, and the actor's scopes and data
labels.

## Usage

The Hub keeps core's usage ledger writable and readable across servers. At
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
      "principal": "https://hub.example/oauth#acceptance-member",
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
Hub reads the table directly as the installation owner; servers write the ledger
through core's confined functions.

## Upstream Transport

The workspace pins RMCP 3.5.0 from `VeoveoAI/rust-sdk` at
`917e7914c93975fc1eddbff8792f1e2de933bc17`, the revision `veoveo-hub-sdk` pins. The fork
exposes HTTP rejection status and body through `StreamableHttpError::HttpResponse` and
keeps exact Task subscription filters. Each registered server has its own pooled HTTP
client. A server can restart behind its port forward while the forward keeps the
Hub's side of every pooled connection open, so those connections fail only when the
next request goes out. A request that fails in its connection, unsent or with its
response broken off, therefore discards that server's client, and later requests open
new connections through a new one. Other servers keep their pools. Upstream discovery
has a ten-second deadline, which covers both attempts, and one retry after a transport
failure, 200 ms later on the server's new client. Authorization challenges, malformed
responses and MCP errors fail at once, and no mutation is retried.

Structured results, resource contents and unknown metadata keep their upstream
values. Upstream MCP errors keep their code, message and data. A non-success HTTP
response without an MCP error keeps its status, and an unexpected redirect becomes
HTTP 502 without its location.

The Hub checks resource policy before it reads. The read handler commits one
`mcp.read` audit record before it returns a result; a failed commit prevents delivery.

## Hosted Resource Issuance

An OAuth protected resource is admitted by the current catalog: either an MCP
profile or a typed `CatalogSection` contribution. Authorization requests, codes,
refresh families, revocation and current-session checks bind the actual resource
ID. Contributed resources have no fabricated MCP profile. Their broker client is
selected by resource, authorization server and identity provider. MCP retains its
profile-specific grant selection and Work Context membership checks; attribution
of a Work Context alone neither grants nor denies a contributed owner's data.

The common `issuance` module owns signing, issuer, subject, audience, client,
family, timestamps and token purpose. A typed `TokenProfile` authorizes against its
section in the request's admitted catalog revision. The resulting private
`AuthorizedToken` binds that owner grant to the common resource/client/identity
context. Its encoded owner claims must have declared codecs and cannot replace
common claims. Signed purposes distinguish a login credential from a resource
credential and are parsed as a closed type. Existing MCP tokens retain their
existing claim shape without a purpose field. Protocol handlers remain responsible
for authenticating callers and admitting current session authority before issuance.

Native integration belongs only to a binary composition contribution, outside the
generic OAuth library; it supplies typed catalog records and claim encoding, not
an additional issuer process or a parallel grant database. Revision publication and
its existing transaction/audit boundary persist contributed records and a generic
revision-identity/digest success audit. A shared publication guard orders durable
publication and local catalog/HTTP-client adoption. Other replicas follow the
native active-pointer notification and initial/reconnect reconciliation stream,
using the same guard and admission. Failed adoption logs the error and stops the
hosted process rather than retaining an indefinitely stale issuer. Propagation is
asynchronous: in-flight requests retain their admitted revision, and native tokens
already issued retain their rights until expiry. There is no synchronous global
revocation claim.

Migration `0003_oauth_resource.sql` is an explicit development hard cut. It discards
hosted OAuth requests/codes, refresh families/tokens, browser sessions and pending
Computer login state. It clears only Computer authentication/readiness pointers,
retaining owner mappings, native workspace IDs and working storage. Tasks,
Artifacts, trusted configuration and unrelated storage are untouched. A fresh
ordinary login establishes new custody; there is no legacy binding or backfill.

## HTTP Surface

| Route | Purpose |
|---|---|
| `GET /livez`, `GET /readyz` | Process liveness, and readiness once the audit service runs; both skip `Host` validation |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 metadata of the Hub's issuer, with `jwks_uri` and, when registration is configured, `registration_endpoint` |
| `GET /jwks.json` | The keys that verify forwarded tokens |
| `POST /oauth/register`, `GET`/`PUT`/`DELETE /oauth/register/{client_id}` | Server registration |
| `/oauth/authorize`, `/oauth/callback`, `/oauth/token`, `/oauth/revoke`, `/oauth/jwks.json` | Hosted authorization server |
| `POST /oauth/device_authorization`, `GET`/`POST /oauth/device` | RFC 8628 native public-client login and explicit browser approval through the existing identity broker |
| `/.well-known/oauth-authorization-server/oauth`, `/oauth/.well-known/openid-configuration`, `/.well-known/oauth-protected-resource/mcp/{profile}` | Hosted authorization server metadata at its RFC 8414 and OpenID discovery locations, and protected resource metadata |
| `/mcp/{profile}` | The profile's MCP endpoint |
| `/console-api/{profile}/session` | Console session bootstrap |
| `/admin/{profile}/...` | Configuration, audit, JWT revocation, Task cancellation, server health and usage |
| `PUT`/`DELETE /selection/{profile}/{server}` | The caller's branch for a server, or back to the base branch; 204 |

Every route except the probes requires a `Host` header naming the issuer's host.

`POST /admin/{profile}/tasks/{server}/{task_id}/cancel` cancels the caller's Task by
the identifier its server issued. The Hub reads no Task state for it: it finds the
caller's unexpired route of that Task, applies the checks and the policy decision of
`tasks/cancel` on the Task's captured composition, and forwards `tasks/cancel` to the
deployment that holds the Task, without an MCP session of its own. It answers 204, 404
when no route of the caller's names the Task or the route refuses the caller, 403 when
policy refuses, and 502 when the server fails the request; each outcome is audited as
the administrative operation.

## Persistence

The Hub owns the `hub` schema. Its migrations in `migrations/` run after core's
kernel migrations and record their history in `hub._sqlx_migrations`, as
`sqlx.toml` declares. `src/store` reads and writes the schema through `HubStore`:
OAuth requests and codes, refresh families, JWT revocations, replay identifiers,
control-plane revisions and principals' selections. A publication also writes the identity rows of the Work
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

The Hub's own statements, for registrations and Task routes, are `.sql` files
beside the modules that run them, checked at compile time through sqlx's `query_file!`
macros against the metadata in `.sqlx`; tests keep theirs under `tests/`. No Rust code
builds SQL text, except `CREATE DATABASE` and `DROP DATABASE`, which take no
parameter and run only outside transactions and functions; their text names a
validated database name. `.sqlx` also holds the metadata of the core crates' queries, because
the check stage compiles them offline from an empty target directory.

`tests/database_conformance.rs` holds the Hub's own statements to core's
[database rules](https://github.com/VeoveoAI/veoveo-core/blob/9a6b3ff95469ff19f9ccb513421a57ba6751aa9e/docs/DATABASE.md#server-schema-rules)
over the `hub`, `identity` and `kernel` schemas. The fixture applies the Hub's
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
SDK's registration client and calls the Hub as both acceptance callers. It checks the
pinned namespace, the refusal of the slug `hub`, the initial access token, the
database grant, which migrates the
server's schema as its group role and holds the server's kernel instance, role
decisions by risk, refusal of undeclared resources, the permissions each forwarded
token carries, that three housekeeping rounds leave a live registration's row version
unchanged, that deregistration drops a registration's login, and that a process that
ends without deregistering loses its registration and login after the grace period.
The fixture server verifies every forwarded token with the keys it reads from the
issuer's `jwks_uri`, which equal those of its registration response. The Hub runs
with a verification key, so the test also checks that the set lists it after the
signing key and that every forwarded token carries the signing key's `kid`.

`tests/instance_registration.rs` starts the binary the same way and registers the
Observatory fixture's declaration as instances with tokens from `create_instance`.
It checks that two instances of one slug register under different keys as distinct
deployments that both route, that registering an instance again with a new key,
version, endpoint and declaration keeps its deployment, removes its previous
registration and drops that login, that an instance token whose metadata names
another deployment is refused, that any other token is checked as the registration
secret, whose namespace pin binds the namespace path only, and that housekeeping
neither orphans nor removes an instance registration whose login holds no live
kernel instance while it removes such a namespace registration. An RFC 7592 update
with a new key, version, endpoint and declaration replaces the instance registration's
metadata and its deployment's under the same registration and login, and one naming
another deployment is refused. A namespace registration of an existing deployment with
other declarations under the pinned key replaces the previous registration and drops
its login, an update under that key applies, and another key neither registers the
slug, for that deployment or another, nor updates the registration. The test sends
the updates itself, because the SDK's client updates only to the metadata it
registered.

`tests/data_environments.rs` starts the binary the same way and runs the SDK's Relay
fixture as two instances of one slug, one in the installation database and one in a
database `ensure_database` created, beside a Ledger namespace server. Each grant names
its instance's database at the installation database's address, and the server's
schema exists in each. For each instance, the administrative cancel of a queued Task
changes that Task's row in the instance's database, a Task created through the Hub
completes after its Ledger call from the Task, which the Hub admits only once it finds
the Task running in that database, the Task's rows are absent from the installation
database when the instance is elsewhere, and once the Task ended an exchange for it is
refused with `invalid_grant`. A second test, without the binary, runs four concurrent
`ensure_database` calls for one name, checks the kernel migrations applied once, and
checks that `drop_database_if_unused` drops that database, but neither one an instance
names nor the installation database.

`tests/selection.rs` starts the binary with base branch `dev`, registers instances of
the Ledger fixture on `dev` and on another branch, and calls them as the `member` and
`admin` callers. It checks that a selection serves its branch's tools to its principal while
the other principal keeps the base branch, that clearing returns to the base, that a
selected branch without an instance lists no tools of that server and refuses a call
with its cause, that a change completes only the changed principal's listens with a
tool-list hint, and that a Task started before a change completes on the instance it
started on, under its captured composition.

`tests/factory.rs` applies pushes against an in-process fake OpenShell gateway
(`tests/factory/gateway.rs`), which answers the methods the factory sends with the
gateway's stop and start rules, and a fake image builder whose digest is the SHA-256 of
a `Dockerfile`. It checks a first push's data environment, provider token, image,
size, restart policy, environment, start command and composed policy; that a push of
the same image only sets the policy of a provisioning sandbox and otherwise stops and
starts it, a failed or crash-looping one included, and that a push of another image
recreates it with a new token; that a failed build is recorded and changes nothing
else; that a refused provider creation is followed by a new token; that status reports
phase, exit code, restart count and a scheduling condition; and that caps warn above
the cap and, enforced, refuse. Against the binary, removing an instance whose Ledger
fixture registered into a data environment removes its sandbox, provider,
registration, login, instance and database, and the applied revision serves once the
fixture registered. Images and revisions are synthetic values; no OpenShell, Lore,
BuildKit or image is involved.

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
`via`, and the refusal naming the missing permission once Ledger deregisters.

`tests/browser_session.rs` logs a user in at a private Keycloak 26.8.1 server through
the authorization code flow with PKCE: the Hub's authorization endpoint, Keycloak's
login form, the Hub's callback and the code exchange. It registers the Ledger and
Relay fixtures and leaves Relay's `relay_later` Tasks unclaimed, so each stays live
while the test acts for it. A Task created for the user's call exchanges and calls
Ledger, and its route keeps the user's session family. After the user logs out, the
Hub refuses the browser's access and refresh tokens, while a fresh exchange for the
Task succeeds and the Task-bound tokens are admitted. In a second session, presenting a
consumed refresh token again after the delivery window revokes the session: the Task's
exchange is refused with `invalid_grant`, and the Task-bound token issued before is
refused at the MCP endpoint. The test checks each step's audit record by class,
outcome, reason and actor, including `refresh_token_revoked` for the logout,
`refresh_token_replay` for the replay, `invalid_grant` for the refused exchange and
`invalid_bearer_token` for the refused token. `tests/session_family.rs` checks on real
families that only a replay, and not a logout, an expiry or a removed family, ends a
Task's delegation.

`tests/database_reconnect.rs` registers the Ledger fixture, which holds a kernel instance
on its own login, then ends every session of the installation owner's login, as a lost
connection between the Hub and PostgreSQL does. The Hub keeps running, its
kernel instance holds its lock again, readiness answers, a new token and a call to
Ledger through the Hub succeed, and past the grace period Ledger's registration
carries no orphan mark.

`tests/usage.rs` starts the binary with an action policy that lets the administrator
make administrative reads, waits for the partitions three months ahead, writes two
servers' usage to the ledger and checks the member's own totals over two pages, the
refused and recorded tenant read of the member, the administrator's tenant totals
across both servers, and the refusal of a cursor from another view and of an empty
range.

`tests/mcp_lifecycle.rs` checks the advertised version and resource capabilities, the
answers to `resources/subscribe` under `2026-07-28` and under `2025-11-25`, and the
acknowledgement of a `subscriptions/listen` filter. The upstream connection tests put a
port forward in front of an upstream that restarts on the same port: the forward keeps
the Hub's pooled connections open and closes each without an answer. Every
discovery and tool call after the restart succeeds, only the first discovery's first
attempt reaches a stale connection, and another server's pool keeps its connections. Unit and database tests cover the
catalog, policy evaluator, discovery cache, Task routes, refresh families, session
binding, the audit vocabulary and readers, and the control store.

## Contract Compliance

- Several versions of one slug may hold registrations; the newest routes until
  session version selection exists.
- A namespace server stays registered only while its registration's login holds a
  live kernel instance; a server that never starts one is removed after the grace
  period. An instance's registration stays until the instance registers again.
- Servers are reached at their registered endpoint; dial-in is not implemented.
- Concrete resources a server adds at run time appear in `resources/list` after the
  next catalog generation; templates and reads are always current.
- Readiness and registration internals answer with RFC 9457 problem details. Other administrative handlers still answer some errors with a
  status and no body.


### Live native destinations and Computer defaults

The hosted catalog carries one prepared immutable routing/provisioning generation.
The administrative publication mutex covers preparation, durable revision/audit
publication and local adoption. Native gateway origin, CA path and management
credential environment are process bootstrap. Configure native transport at startup
(with an empty binding list if necessary); destination changes do not toggle the
installation between native-only transport and direct TCP. MCP request destinations (workspace,
sandbox and port), destination additions/removals, and Computer workload defaults
are live data. Prepare listeners, policy, native template, provider-profile shape,
secret locators and browser-client binding before committing; failed preparation
rejects the update without changing the active revision or working runtime.

Unchanged destination listeners and HTTP clients are shared with the next revision.
Changed destinations get a new private socket/client; requests retain their old
snapshot until completion or ordinary cancellation. Removal prevents new routing,
without replaying a mutation or forcibly restarting unrelated services. Shared
client invalidation remains visible across unchanged generations. The existing
control-revision notification listener prepares/adopts on each replica and
reconciles on connection/reconnection; convergence is asynchronous. An unrecoverable
replica adoption failure follows normal process shutdown instead of serving stale
rights indefinitely. There is no zero-downtime or automatic Task migration claim.

Each Computer request selects a prepared default snapshot. Pending PKCE state
retains its initiating client/profile/context and admitted defaults with the
verifier, so a default update between entry and callback cannot exchange an old
code with a new client. Consumption is still single-use and browser-bound. Current
catalog grant/family authority is checked before custody; this does not grandfather
removed permissions. Initial owner mapping
creation stores a typed immutable provision snapshot: workload defaults, native
policy/template protobufs, selected persistence backend and model secret locator,
never access/refresh tokens or actual model credentials. An existing owner keeps
that snapshot on reauthorization and resume. The current catalog still controls
browser/session/client/grant authority. Changes to template, command, policy,
provider defaults or port affect new or explicitly replaced Computers only; they
do not restart or rewrite existing instances. The supplied refresh grant's actual
client ID accompanies custody; it is not inferred from an old provision snapshot.
Use immutable versioned native template names and policy file paths when changing
defaults, and publish the new reference through the live control plane. Editing a
native template under the same name is not watched and does not automatically
refresh cached defaults for new Computers. The selected profile's resource URL and
model secret locator also participate in prepared-default reuse. Forwarding
capacity remains the current process-wide budget, not a recorded per-user quota;
old instance snapshots reuse that gateway budget. Requests already using an older
budget generation drain naturally when the installation changes the limit.
Unchanged runtime data reuses prepared state without a new native RPC on
a grant-only catalog update. The stable router can enable/disable Computer routes
from a prepared catalog without rebuilding the server router.

Migration `0006` defines the mandatory per-instance snapshot for a reset/reseeded
development application database. It deliberately has no backfill from current
defaults and does not attempt to migrate old Computer state. Native workload or
storage deletion is not performed by this schema change. Installation must add the
exact `GetGatewayInfo` POST and `config:read` to the dedicated control authority;
no native authorization implementation changes. SDK/policy `9724fb1` already
provide this API. Comparing them with maintained fork `846de5fa` found no production
SDK/policy changes and no changes to the public management/ForwardTcp/policy fields
used here; internal gateway/supervisor protocol additions still require matching
maintained runtime images. Kubernetes deployment qualification remains separate
from the Rust component tests.

### Private Computer browser ingress

The optional live `computers` installation selects one public PKCE client,
profile/Work Context, authenticated native OpenShell gateway, trusted workload
template, native policy file, fixed service command and global provider profiles.
The gateway authority has explicit workspace/provider/sandbox provisioning rights;
it never enters the Computer. The trusted bearer environment must hold a native
mediated reference whose management credential renews under OpenShell custody.
`raw_grpc_fresh` opens an authenticated call; it does not refresh a copied expiring
JWT. Reference mediation for that dedicated authority needs deployment qualification.
Browser input selects no image, policy, secret,
provider profile or forwarding target. Both credential keys name native runtime
**environment aliases**, rather than logical profile credential names.

`GET /computer` admits the existing opaque host-only session cookie or starts an
ordinary PKCE authorization against this Hub's existing OAuth endpoints. A bounded
single-use database state binds the verifier to a separate initiating-browser
cookie. The callback exchanges the native code, verifies the actual signed access
JWT with the shared verifier, resolves current native subject authority, and binds
the browser only to its actual refresh-family ID and exact tenant/principal owner.
No expired or fabricated JWT establishes a browser session. Cookies contain random
selectors; only their hashes are stored. Grant material is never stored in Hub
Computer rows, cookies or UI bundles.

A precommitted pending handoff invalidates old browser admission before native
mutation. An owned worker locks that exact handoff, clones the trusted native
workload into a distinct owner workspace with its nonsecret external base path,
creates workspace-local providers, and stops an existing app before replacing its
Hub refresh material. ConfigureProviderRefresh **and** RotateProviderCredential
must succeed before Start/ready publication. The existing managed runtime access
value is never replaced with UpdateProvider; no operator credential export occurs.
The model seed resolves through the existing trusted Env ProviderApiKey locator.
Native global profiles and exact refresh recipient are checked before custody.
The selected backend is read with native `GetGatewayInfo` (requiring `config:read`
and `platform_admin`). Docker must opt into its persistent workspace. Kubernetes
uses its native automatic per-sandbox `/sandbox` PVC. Reusable Computer defaults
must not contain explicit mounts at/below `/sandbox`: a static claim could share
private owner data or disable whole-workspace persistence. Read-only configuration
PVCs outside that workspace remain supported by native admission. This is the
current automatic-persistence contract, not a new authorization rule. Unknown
backends fail closed. Create also sets native persistent retention, the admitted
policy/command and `Always` restart policy.
Native gateway lifecycle owns missing-runtime recovery and runtime identities.
Provisioning and browser entry also wait for a successful native ForwardTcp target
connection under the existing 120-second startup bound. The gateway must return
ForwardTcp success after the native relay claim, not before target connection.
Document navigation can resume native lifecycle and wait on its actual forwarding
connection. Warm asset/API requests open only their real forwarding connection;
there is no extra health endpoint or per-request probe. Retry classification uses
native status codes, never error text; authorization/invalid-target failures stop
immediately. A successful TCP connection does not prove application protocol health.

The worker survives HTTP disconnection. Native operations and readiness waits are
bounded; a crash or ambiguous failure leaves the durable handoff pending and
requires reauthorization. The row lock serializes cooperating Hub workers, while
native request IDs preserve each admitted native mutation's semantics. This is
not exactly-once grant custody across a crash after provider consumption. No
provider health/status assertion substitutes for proof of which family was handed
off. No alternate token store, provisioning controller or reconciliation daemon is
introduced.

Mounted requests admit the exact Computer owner before opening authenticated
native ForwardTcp to its fixed loopback port. HTTP entities remain encoded streams;
hop-by-hop and Connection-nominated headers, incoming credentials/cookies/forwarded
identity, backend cookies and CORS headers do not cross this boundary. Unsafe HTTP
and WebSocket requests require exactly the public origin; Origin is then removed
before the trusted loopback backend. WebSocket upgrades preserve the byte stream.
Each active connection has an independent family-expiry deadline and fresh native
family/client/profile/membership checks every five seconds, with a five-second DB
read bound. Errors fail closed; revocation observation is at most ten seconds under
normally scheduled checks. Dropping a response or upgrade cancels its native stream
and releases a bounded connection permit. Proxy responses are private/no-store.
The online-only native app must be used; mounted UI caches are convenience, not a
same-origin browser security boundary or a guarantee about previously loaded data.

An initial authorization refusal on a browser document navigation redirects to
the fixed `/computer` entry. Entry resolves the current Computer from a still-valid
cookie or starts ordinary sign-in; it never replays the stale mount or chat URL.
API/fetch and WebSocket requests retain `401`, malformed routes retain `404`, and
operational failures retain `503` rather than redirecting into a recovery loop.

`POST /computer/logout` is parameterless and same-origin. Only a fresh admitted
BrowserSession selects its owner and actual native family. A conditional transaction
locks the current ready/no-handoff Computer, rejects stale owner/family bindings,
revokes using ClientRevocation, clears successor delivery envelopes, and commits a
native `hub.authentication` Revoke record with BrowserSession method. A newer
handoff cannot be revoked by the old cookie. The cookie is cleared only after that
operation succeeds (or when there is no current admitted session).

The native audit declaration flow updates a changed class's documenting schema
in place before startup records are written. The added `browser_session` method
uses that existing flow; it does not require a Core/SDK pin, database reset or
record rewrite. Other owners' declarations and stored evidence remain intact.
Generic stored-record reading and sealing do not deserialize this owner enum.
Coordinate Hub versions: an older binary can re-declare older documentation, and
an older consumer explicitly decoding this enum may reject the new method.

This source path still needs native image/gateway, browser OAuth/owner-isolation,
revocation/streaming and user-agent acceptance on the selected installation. Local
protocol fixtures and database keepers do not establish live identity/provider,
private source transport or model-inference qualification.

### Native contributed credential lifecycle

The executable binds one typed contribution and its claim codecs at composition.
Its admitted section contains deployment endpoint/audience, native recipient,
finite login scopes, repository scope and resource-ID templates, token lifetime,
repository registrations and exact tenant/principal/native-action assignments.
None of these records is compiled into OAuth routing or captured by the adapter.
The common signer fixes identity, issuer, audience, client, family, timestamps and
signed purpose; owners cannot override those claims. Encoding/signing unavailability
is a server failure, while a refused owner target is an authorization denial.

A public device client selects one resource and its finite login scopes. Device,
user and initiating-browser capabilities are stored only as digests. Explicit
browser approval uses the existing enterprise broker, nonce and upstream PKCE,
with a typed device destination instead of a fabricated downstream code redirect.
Polling is persisted and row-locked, enforces the five-second initial interval
and `slow_down`, and consumes approval once. Expired records are removed in
bounded batches. Discovery advertises the actual device endpoint.

Native public RFC 8693 exchange is separate from registered-server delegation.
It verifies a common-signer Login token, current family/client/resource binding
and finite login scopes, then dispatches the target to the current typed owner.
Only the owner interprets its configured repository scope template and exact
assignment. Resource-purpose tokens cannot become login issuance authority.
Refresh retains the original identity/family binding and dispatches current owner
admission; rights are not copied into the family. Explicitly configured
`client_secret_post` factory clients authenticate through the existing secret
resolver and request the same owner target, with no human session or inferred
service-account/administrator repository permission. Existing private-key clients
and registered-server delegation keep their original proof contracts.

The native HTTP fixture approves a synthetic identity through trusted fixture
state, checks actual hosted HTTP/JWT/persistence/live-publication behavior, and
restarts against a stale seed. It does not establish Entra interaction or actual
native CLI/server acceptance; those remain deployment qualification.
