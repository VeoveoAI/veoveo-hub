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
| gRPC over HTTP/2, Lore's `epic_urc.UrcAuthApi` | `LookupUserPermissions` only, on the Hub's own listener, for Lore's repository listing; see [repository listing](#repository-listing) |
| gRPC over HTTP/2, Lore's `ucs.auth.RebacApi` | `CreateResource` and `DeleteResource`, on the Hub's own listener, for the repositories Lore creates and deletes; see [repositories Lore creates and deletes](#repositories-lore-creates-and-deletes) |
| PostgreSQL 18 through sqlx 0.9.0 | The installation database: core's kernel schemas, then the Hub's own `hub` schema, applied at startup; `LISTEN` on the `hub` channel for registration changes; one login role per registration. A database per data environment in the same instance, with core's kernel schemas |
| OpenTelemetry | `service.name` is `veoveo-hub` |
| Veoveo installation configuration | `HubControlPlane` in `src/control_plane.rs`, read as JSON |
| [Veoveo audit envelope](https://github.com/VeoveoAI/veoveo-core/blob/9a6b3ff95469ff19f9ccb513421a57ba6751aa9e/platform/audit/contract/DESIGN.md), after NIST SP 800-53 Rev. 5 AU-3 and mapped to OCSF 1.9 | Records name owner-declared classes, each with its OCSF class and activity: the SDK's `mcp.*` classes, core's `task.*` and `audit.*` classes, and the Hub's `hub.*` classes. The Hub's records carry its own reason codes and name targets by URI |
| [Veoveo usage ledger](https://github.com/VeoveoAI/veoveo-core/blob/9a6b3ff95469ff19f9ccb513421a57ba6751aa9e/platform/usage/DESIGN.md), after FOCUS 1.4 with ISO 4217 currencies | The Hub prepares the ledger's monthly partitions and serves daily totals across servers |

## Execution Composition And Deployment Identity

There are no named environments. The live control plane's `execution` section holds
only `baseBranch`; routing comes from registrations and each principal's selection.

RFC 7591 metadata carries a deployment ID. A deployment registers either with the
installation's registration secret under the key its logical namespace is pinned
to, or as an instance with the registration token the Hub created for it, and
receives the namespace's explicit `shared_logical_schema` storage binding. Shared
schema is an explicit data binding, not owner isolation. A registration of a
namespace deployment with new metadata, under the pinned key, replaces its contract,
endpoint and storage, and an instance's every registration replaces them, key
included. Registrations and health remain independent per deployment.

Registering is serving: no enrollment step publishes a deployment. A slug with no
instances routes to its deployment that has registrations. When several deployments
of one slug have them, the newest registration whose process lives wins: the others
are set aside, not made unavailable, so moving to a new deployment needs no gap. A
registration is dead once housekeeping has set its `orphaned_at` (its login holds no
live kernel instance), so a slug whose newest deployment ended routes to a live older
one at once instead of failing for the grace period. Setting or clearing that mark
notifies the `hub` channel (migration `0027`), so every replica follows it; Hub
probes nothing else. When every deployment of a slug is orphaned, the newest still
routes, and its requests fail as before until the grace period removes its
registration or a process registers it again. Each replica logs one
WARN naming the slug, the deployment it routes to and the ones it set aside, and
logs again only when that changes, with an INFO once one deployment remains. Our
redeploys keep their deployment ID, so this happens only when a new deployment
replaces an old one.

Each authenticated stateless MCP operation routes on its own; a client selects
nothing, and the retired `veoveo-environment` header is ignored. The resource URL
stays `/mcp/{profile}`. The Hub resolves and retains the SDK's canonical immutable
composition (`formatVersion` 2, from slug to deployment) once, then selects exact
registered deployments for all surfaces. Signed downstream context keeps this same
capture while the target deployment changes per hop. Descriptor caches and catalog
continuation are scoped to that composition; a continuation from another composition
is refused. Domain collection cursors remain owned by their servers. HTTP pools use
deployment plus endpoint. Publication hints complete existing listens normally,
requiring native re-listen and current admission instead of silently keeping a stale
selection alive.

Each authenticated principal's selection, `hub.selection`, names a branch per
server. Each server that has registered instances, or that the selection names,
routes to the instance of the selected branch, or else of the configured
`execution.baseBranch`, instead of its registered deployment. When that instance is
missing, when several instances serve that branch, or when no base branch is
configured, the component is `unavailable`, never another branch or the server's
other deployment, and a tool call to that server answers with the cause.
`PUT /selection/{profile}/{server}` with `{"branch": "…"}` and `DELETE` of the same
path change the caller's own selection and nothing else, so authentication is their
only check; a delegated caller changes the selection of the principal it carries.
Each change notifies the `hub` channel with the principal, and every replica
completes only that principal's open listens with a list-changed hint.

Tasks persist exact deployment and composition before the projected response.
A native retry of the same Task preserves its first capture. Later reads, changes,
cancellation, delegated work and re-listened Task filters use that capture while
checking current component/data policy, so a newer registration or a selection change
cannot retarget retained work. Current native destination bindings remain
authoritative: old bindings must be retained while work drains; explicit withdrawal
makes that work unavailable and is never undone from historical provenance. Core's
independent Task deployment affinity prevents same-namespace workers sharing a schema
from claiming or recovering one another's work. Migration `0026` drops the
environment columns of `hub.execution_composition` and `hub.task_route` and removes
their rows, whose version 1 records no longer read.

This boundary does not build or run deployments; routing inputs enter through
registrations, instance registrations and selection. Public provider callbacks need their own
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
   listeners, retention and housekeeping loops, then listen: on the socket the process
   that started the Hub passed it, by sd_listen_fds(3)'s protocol, or else on
   `VEOVEO_LISTEN_ADDRESS`. The listener speaks HTTP/1.1 and cleartext HTTP/2 (prior
   knowledge), which gRPC needs.
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
| `LISTEN_FDS`, `LISTEN_PID` | A listening socket passed at file descriptor 3 (`LISTEN_FDS=1`, and `LISTEN_PID`, when set, naming the Hub's process), on which the Hub listens instead of `VEOVEO_LISTEN_ADDRESS` |
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

## Logging

An expected, planned unavailability logs at INFO, or at WARN where an operator should
notice it, and never at ERROR; ERROR is for failures. The HTTP trace layer logs every
server error response as a failure, except one the Hub answers on purpose and marks
expected (`http::expected`), whose cause the code that answers it logs once, at the
level the cause deserves.

- **The Computer proxy.** A request the proxy cannot forward gets its own `503`,
  marked expected, and one line, `Computer request answered unavailable`, with the
  Computer and the reason. It logs INFO when the owner's admission ended, or when the
  Hub's record shows a replacement running or the sandbox missing, starting, stopping,
  stopped or being deleted; WARN when forwarding capacity is exhausted, the Computer
  needs its owner (its last replacement failed, its sandbox is in error, or the Hub can
  no longer serve it safely) or its ready sandbox does not answer; ERROR when the Hub
  cannot read the Computer's state or the gateway refuses the Hub's authentication or
  configuration. A cause the forwarding error names decides first; otherwise the Hub
  observes the Computer after the failure.
- **Registered servers.** A server's connection that closes, as when its sandbox is
  replaced, is a reconnect: discovery and idempotent requests retry once on new
  connections ([Upstream Transport](#upstream-transport)), each logging one WARN that
  names the server and the transport error.

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
| `instance` | One row per instance: its deployment ID, the Lore repository and branch it serves, one instance per pair, the SHA-256 digest of its registration token, the database its server registers into, and what the Hub records about its factory sandbox: the latest pusher, the revision it last applied, when its server last registered or resumed, its last failed step, the capacity warning of its creation, and the SHA-256 digest of its client's secret |
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
their own keys. They are the branches of one repository: a slug names one server's
schema, permissions and tools, so an instance whose slug a registered instance of
another repository serves is refused with `invalid_client_metadata`, saying so, and
the server that already serves it keeps serving. Every registration with the token replaces the instance's previous
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

A data environment's database has core's audit and usage schemas but neither a sealer
nor prepared partitions: the Hub runs the audit sealer and prepares the usage ledger's
monthly partitions only in the installation database. Audit records a server writes
in a data environment therefore stay unsealed and are absent from the Hub's audit
console and exports, its usage records land in the ledger's default partition and are
absent from the Hub's daily totals, and both go with the database when it is dropped.

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
sandbox first gives the instance a new registration token and replaces its registration
provider, `<sandbox>-registration`, of the configured `registrationProfile` with that
token as `VEOVEO_REGISTRATION_SECRET`: a provider that could not be created, or the
provider of a sandbox that was replaced, leaves no token behind that no sandbox holds.
It likewise gives the instance's own client a new secret and replaces its Lore provider,
`<sandbox>-lore`, of the configured `loreProfile`. The Lore provider has no credential
of its own: the Hub configures the gateway's `oauth2_client_credentials` refresh of
`LORE_ACCESS_TOKEN` with the client, whose identifier is the deployment ID and whose
secret only the gateway keeps; the Hub stores the secret's digest. The sandbox then gets
the pushed image by digest, the factory template's size and driver configuration,
restart policy `always`, both providers, and the installation's base policy with the
network rules of the revision's `openshell/policy.yaml` added; a repository policy that
sets anything else or reuses a base rule's name is refused. Its environment names
`VEOVEO_DEPLOYMENT_ID`, `LORE_IDENTITY`, `VEOVEO_PUBLIC_BASE_URL`
(`http://<sandbox>.internal:<port>`), `VEOVEO_PORT` and `VEOVEO_HUB_ISSUER`. Its fixed
start command clones `<loreServer>/<repository>` into `/sandbox/src` when no checkout
exists, runs `lore sync --reset <branch>@LATEST` and then `exec ./openshell/run`; the
repository URL and branch are its arguments. Lore reads its access token from
`LORE_ACCESS_TOKEN`, the provider's placeholder, which the gateway replaces with the
real token in Lore's requests. Lore requires an identity with a token it cannot read,
which it reads from `LORE_IDENTITY`; that identity names only the checkout's local user,
and the Lore server authorizes the token alone, so the instance names itself with its
deployment ID. The global Lore profile is the one Computers use.

The factory template, which `factory.template` names by workspace and name, is an
OpenShell workload template the installation owns, like the Computer template. It gives
every instance's sandbox its size, as CPU and memory, and its driver configuration,
such as the installation's read-only mounts. The Hub reads it on every push by name
and creates the sandbox inline from it with the branch's image, because OpenShell does
not let a create that names a template set its own image: the template's CPU and
memory become Kubernetes limits and its driver configuration is passed unchanged. The
template sets nothing else; one that names an image, environment or a GPU is refused,
and a missing one fails the push, both as `failed{stage: create}`, leaving the sandbox
as it was. The template may appear before or after the Hub starts.

Lore's command line comes from the installation's Lore image, which the Hub's sandbox
and every factory sandbox mount read-only at `/opt/veoveo/lore`. The Hub runs it as
`/opt/veoveo/lore/usr/local/bin/lore`, and so does an instance's start command; that
path is the installation's contract, and no image the Hub builds or runs carries Lore.
A repository's `openshell/image/` holds only the toolchain that builds the branch, and
BuildKit builds it from that context alone.

Builds share no writable cache. Every build sets BuildKit's
`BUILDKIT_CACHE_MOUNT_NS` build option, which a `Dockerfile` cannot set, to
`image::CacheNamespace::of(repository, branch)`: the SHA-256, in 64 lowercase
hexadecimal digits, of the repository ID, a zero byte and the branch name. The
Dockerfile frontend prefixes it to every `RUN --mount=type=cache` id, so no branch
opens another's cache mount. BuildKit keys a cached step by its instruction and inputs
and not by what a cache mount held, so a cache mount's content must not outlive its
build: the Hub runs `buildctl prune --all --filter type==exec.cachemount` before every
build, whose failure fails the build as `failed{stage: build}`, and again after it,
whatever its outcome, logging a WARN when that prune fails. A prune spares a cache
mount a running build holds, and may empty one between two steps of a concurrent
build; layers stay cached, so an unchanged `openshell/image/` builds to the same
digest. The Hub's policy reaches BuildKit as plain TCP for `buildctl`, which carries
the prune as it carries the build.

The instance's client is no installed OAuth client. At `/oauth/token` a
`client_credentials` request whose client ID is an instance's deployment ID and whose
`client_secret` matches its digest receives the Lore token of the instance's latest
pusher for the instance's repository, issued by the one Lore deployment that registers
that repository ID: the token the deployment's one public token-exchange client would
obtain for that person, which the Lore contribution's profile admits against the
person's current [grant](#groups-and-sharing) on every issue. The client requests no scope or resource
and holds no assignment of its own; a pusher without the grant is refused with
`invalid_grant`. Before every sandbox creation and restart the Hub rotates the
credential, so the sandbox starts as the latest pusher.

A push applies to an existing instance by the image, the policy, the size and the
driver configuration its sandbox runs, which OpenShell reports as the sandbox's spec:
the policy in the canonical form OpenShell records at creation, which is the form the
Hub composes, and the size and driver configuration as the Hub passed them. The Hub
never updates a sandbox's configuration; OpenShell's `UpdateConfig` needs
`config:write`, which the Hub's identity does not hold. With the same image, policy,
size and driver configuration the Hub stops and starts the sandbox; the start command
fetches the branch's latest revision. OpenShell's stop answers once the sandbox
stopped, from ready, from a failed program and from a crash loop's automatic restart,
and resets the restart count; its start runs the program at once. A sandbox still
provisioning, or starting for the first time, has not run its start command, which
fetches the latest revision when it does, and OpenShell would refuse to stop it: the
Hub leaves it to start. With another image or another policy, a factory template whose
size or driver configuration changed, or without a sandbox, the Hub deletes the
sandbox, waits until OpenShell no longer has it, and creates it from the new image,
policy and template with an empty disk, so a changed template reaches each instance at
its next push. A repository policy the Hub refuses is recorded as
`failed{stage: create}` and, like a failed build, leaves the sandbox as it was.
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
the sandbox and its workspace, which its output in the log store carries. OpenShell
removes a sandbox that never becomes ready, so an instance whose sandbox OpenShell no
longer has, while no push or removal is being applied to it, reports
`failed{stage: create}` saying so; the branch's next push creates it again. The applied
revision serves while the sandbox is ready and its server registered or resumed since
the push. An instance is healthy while its sandbox is ready and its last step
succeeded, degraded while the sandbox is on its way to ready or still serves after a
failed step, and offline otherwise. With `caps.instancesPerUser`, creating a sandbox
for an instance whose latest pusher is the latest pusher of more instances records a
warning naming the count and the cap; with `caps.enforce` the Hub refuses it as
`failed{stage: capacity}` instead.

`GET /factory/{profile}/instance?repository=…&branch=…&wait=…`, in the Lore
contribution, answers an instance's status to a caller with a current Lore grant on the repository
in its tenant, own or through a group, the same grant Lore tokens are issued from,
whether it calls itself or through a server that calls the Hub for it; the profile only
authenticates. Without the grant it answers 403, without an instance 404. With
`wait`, at most 60 seconds, it answers as soon as the applied revision serves, the
Hub's last step failed or the sandbox's program exited since its start, which OpenShell
reports as an error or a restart. Its answer also carries `watch`, the repository's
watcher and why the last subscription as them failed, and `prepareFailure`, why the
branch's latest push could not be read; without an instance its detail names those
causes. The route exists when the installation configures a factory.

The Lore contribution turns pushes into instances (`src/bin/hub/contributions/lore/factory`).
`PUT /factory/{profile}/watch?repository=<id>` subscribes to the repository's Lore
notifications as the caller, answers 403 when Lore or the caller's grant refuses
them and 204 otherwise, and records the caller as the repository's watcher in
`hub.lore_watch`; a later watch replaces the watcher and its follower, which stops
between pushes. One follower per watched repository subscribes as its watcher, and
again five seconds after any subscription ends, including Lore ending a lagging
subscriber's stream with `ABORTED`; at startup the Hub resumes every recorded watch.
A subscription is idle between pushes, and connection tracking on the way to Lore
forgets an idle connection (Cilium's after 2 h 13 min), after which Lore's events are
dropped without either end noticing. So every connection to Lore sends HTTP/2
keepalive pings every 30 seconds and ends when one goes unanswered for 20 seconds:
the pings keep the connection known, and a dead one ends its subscription, which the
follower then subscribes again.
Every subscription starts with a re-check of every branch: each live branch whose
latest revision its instance has not applied is prepared as that revision's committer,
and the instance of a deleted branch whose name no live branch took is removed.
`BranchPushed` prepares the pushed revision as its pusher; `BranchCreated` and
`BranchDeleted` re-check. Preparing reads `openshell/` at the revision as that person:
the revision tree lists the files, proved by the revision's signature, and Lore's own
command line writes `run`, `policy.yaml` and the `image/` subtree from a bare clone of
that revision into a temporary directory, with the sizes and executable bits the tree
names. A revision with an executable `openshell/run` is applied with `image/` as its
image context and `policy.yaml` as its policy; one without it removes the branch's
instance. One push is applied to an instance at a time. A failed read leaves the
instance as it serves and is recorded in `hub.lore_prepare_failure` until the branch's
next push is prepared. Lore checks access only when a subscription starts, so a
watcher who lost access is noticed at the next subscription: the failure is recorded on
the watch and the follower stops until someone with access watches again. Every Lore
call issues a new token for its person, so each acts under that person's current grant.

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
with its declarations, which start a new generation, and its tool descriptions also
while it runs when it advertises `tools.listChanged`. For such a server the Hub keeps a
`subscriptions/listen` for tool-list changes open beside each cached tool list, empties
the server's cached tools on a change, and opens one for every client listen that asks
for tool-list changes, passing the notification on. A server whose discovered
capabilities do not advertise the change is remembered for the generation and never
watched. Known limit: the watch beside a cached tool list is per catalog generation,
caller and authority, so a server that advertises the change, such as console, holds
one listen per caller, and those listens count toward the replica's cap of 256
discovery watches; a discovery beyond it fails until watches end. List pages use an opaque
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
rules. A grant or rule that names a group matches the caller's groups: their identity
provider's, and their Lore groups by ID ([groups and sharing](#groups-and-sharing)).

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
`faa4e9d57dcb834740b6053c483ed2de7b8b8076`, the revision `veoveo-hub-sdk` pins. The fork
exposes HTTP rejection status and body through `StreamableHttpError::HttpResponse`,
keeps exact Task subscription filters, and ends a client worker whose first request
failed with that error at DEBUG once the caller has received it, since the caller
decides what it means; a worker fatal no caller received stays ERROR. Each registered server has its own pooled HTTP
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
| `PUT /factory/{profile}/watch?repository=<id>` | The Hub watches a Lore repository as the caller; 204, or 403 without the caller's grant |
| `POST /lore/{profile}/repositories` | The caller creates a Lore repository and owns it; 201 with its ID and clone URL |
| `GET`/`PUT /lore/{profile}/repositories/{id}/access` | Who may reach a repository; an owner shares it with a person or a group ([groups and sharing](#groups-and-sharing)) |
| `GET`/`POST /lore/{profile}/groups`, `PUT /lore/{profile}/groups/{name}/members`, `DELETE /lore/{profile}/groups/{name}` | The caller's groups; creating one; an owner sets a membership or deletes the group |
| `POST /epic_urc.UrcAuthApi/LookupUserPermissions` | gRPC: the Lore repositories the token's person holds a grant on, own or through a group, for Lore's repository listing |
| `POST /ucs.auth.RebacApi/CreateResource` | gRPC: registers the repository Lore is about to create, its creator its owner |
| `POST /ucs.auth.RebacApi/DeleteResource` | gRPC: removes the repository Lore is about to delete, when an owner deletes it |

Every route except the probes requires a `Host` header naming the issuer's host.

`GET /admin/{profile}/control-plane` answers the active control plane with its
revision ID, in the body as `revisionId` and as the strong entity tag
`ETag: "<revision id>"` (RFC 9110 §8.8.3), and `sha256`, the SHA-256 of the control
plane as Hub keeps and serves it: Hub's own serialization of `HubControlPlane`, with
every default written, which the revision records. `revisionId` and the entity tag are
present only when the served control plane is the active revision's.
`PUT /admin/{profile}/control-plane` publishes a complete control plane as the
successor of the revision its author read, which `If-Match: "<revision id>"` names
(RFC 9110 §13.1.1). A request without that precondition answers 428
`control-plane-precondition-required` (RFC 6585), and one whose revision is no longer
active answers 412 `control-plane-revision-changed`, naming the active revision in its
detail; nothing is published, and the request is audited as
rejected. The check and the commit are one: the revision is recorded as the successor
of the one `If-Match` names, so a publication that becomes active between them also
answers 412. A client reads the control plane again, applies its change to that
revision and publishes again, so what others published in between, such as a
repository someone created, is never overwritten by a stale document. A document
Hub refuses answers 400 `control-plane-invalid` with the reason in its detail.

`POST /admin/{profile}/control-plane/validate` answers what publishing the document in
its body would make active, and publishes nothing. It is authorized as the `PUT` is,
as `admin_write`, and validates and prepares the document exactly as a publication
does, against the control plane active now; it answers `controlPlane`, the document as
Hub would keep and serve it, with its `sha256`, `servers` and `profiles`. A document a
publication would refuse is refused alike, 400 `control-plane-invalid` with the
reason. An installer compares that `sha256` with the `GET`'s to learn whether
publishing would change anything, and compares the two documents to show what, without
reproducing Hub's serialization; it then publishes the same document with `If-Match`.
Each request is audited as the `control_plane_validation` operation; it takes no
`If-Match`, since it changes no revision.

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
the check and release builds compile them offline, from a cold target directory as well
as from the build host's cached one.

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

The tests that start the binary bind its loopback socket first and pass it to every Hub
process they start, so no port is chosen before it is bound and none is free while the
Hub restarts.

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
deployments that both route, that another repository's instance of that slug is
refused while they keep routing, that registering an instance again with a new key,
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
a `Dockerfile`. It checks a first push's data environment, provider token, image, the
factory template's size and driver configuration, restart policy, environment, start
command and composed policy; that a push of the same image and policy leaves a
provisioning sandbox to start and otherwise stops and starts it, a failed or
crash-looping one included, and that a push of another image, of a changed repository
policy or after the factory template's size or driver configuration changed recreates
it with a new token, while the same template published again as a new object restarts
it; that a missing factory template, or one that names an image, is recorded as a
failed creation and changes nothing else; that no push updates a
sandbox's configuration, which the fake gateway refuses as `PermissionDenied` like
OpenShell for the Hub's identity; that a failed build or a refused repository policy
is recorded and changes nothing else; that a
refused provider creation is followed by a new token; that status reports phase, exit
code, restart count and a scheduling condition; and that caps warn above the cap and,
enforced, refuse. Against the binary, removing an instance whose Ledger fixture
registered into a data environment, and so served the applied revision, removes its
sandbox, provider, registration, login, instance and database; and with the factory on
the fake gateway over TLS, the status route refuses a caller without a grant, answers
404 without an instance, and its wait ends when the fixture registers, when the program
exits and when the fixture resumes. Against the binary too, the fake gateway, given the
Hub's token endpoint, mints each rotated Lore token with the instance's client: for the
first pusher, for the next pusher after a push, refused with `invalid_grant` once that
pusher's assignment is removed and recorded as the failed restart, and again for a
pusher who holds the grant; a wrong secret, a requested scope, the previous sandbox's
secret and a removed instance are refused. A Computer's client receives its owner's login
token, with no repository, without a scope, and with a repository's scope that
repository's token alone, with the owner's actions on it; a repository the owner holds no
grant on, another scope, a resource, a wrong or replaced secret and an instance's secret
are refused. A person who creates a repository through an in-process fake of Lore's
repository service, which tells the Hub's `RebacApi` of each creation as Lore does, owns it
at once: the answer comes once the revision is active, and the
repository's next token for their Computer holds it with `owner`; another creator, an
unregistered creation of theirs and a taken name answer as the creation route says.
Images and revisions are synthetic values; no OpenShell, Lore server, BuildKit or image
is involved.

`src/bin/hub/contributions/lore/factory/tests.rs` runs watching against an in-process
fake of Lore and the factory itself on the same fake gateway, a fake image builder and
a disposable database. The fake Lore admits only the people it holds a grant for and
serves each subscription from a channel the test feeds. Watching subscribes as the
watcher and re-checks every branch, applying the unapplied one as its committer,
building nothing for the applied one and removing the instances of a branch without
`openshell/run` and of a deleted branch; a push is read as its pusher and applied with
its image context and policy, or removes the instance; after `ABORTED` the next
subscription re-checks and applies the branch's new revision and nothing else; a
watcher who lost access is recorded as failed at the resubscription and the follower
stops, and the route answers 403, 400 and 204; a failed read is recorded for the branch
until a push is prepared; another process resumes every recorded watch as its watcher;
and a later watch replaces the follower. The real Lore adapter is exercised only
against a deployed Lore.

`tests/lore_catalog.rs` starts the binary with one Lore deployment whose resource IDs
are `urc-{id}` and asks `UrcAuthApi.LookupUserPermissions` over cleartext HTTP/2 on the
Hub's listener, as Lore's catalog asks it, with a repository token a configured
`client_secret_post` client obtained for one of its three synthetic assignments; a group
the client's principal belongs to is assigned a fourth repository and another action on
the first. It checks that the answer names all four in resource-ID order, once each with
the union of its actions, and not another person's, that Lore's walk of pages of two and
of one returns the same entries, that another resource type names none and a context
filter is refused, that no token, a malformed one, one signed by another key, one for
another audience, an expired one and the Hub's MCP token are refused as
`UNAUTHENTICATED`, that `CheckUserPermission` is unimplemented, and that a published
removal of an assignment, and then of the membership, leaves what they granted out of the
next answer to the same token. A second test calls `RebacApi` the way Lore's repository
service calls it, with the same token: a creation registers the repository, titled by its
name, with the caller as its owner, which the next listing names, and changes nothing
again; creating another person's repository, a call without a token and a resource ID of
another form are refused as `PERMISSION_DENIED`, `UNAUTHENTICATED` and
`INVALID_ARGUMENT`; deleting another person's repository or one the caller holds `admin`
on is refused as `PERMISSION_DENIED`; and the owner's deletion removes the repository
and every assignment on it, and changes nothing again. Unit tests in
`src/bin/hub/contributions/lore/tests.rs` check the grant rule: a person's own
assignment, one through a group in either role, the union of both on one repository,
nothing for the same principal in another tenant, and nothing through a membership the
next revision removed, read alike by the exchange, the listing and the status routes'
check. They check that admission refuses an assignment naming both subjects or neither,
an unknown group or one of another tenant, a repeated subject on a repository, a repeated
group ID, a name repeated in a tenant, an invalid name, a repeated member and an unknown
role, and that today's documents read and write as before. They check that a Computer's
client asks for its owner's login token without a scope and for one repository's token
with that repository's scope, refusing any other scope, that a repository its owner holds
no grant on is refused at admission, and that no token carries more than one repository.
They check the sharing changes: an owner shares with a person and with a group, sets a
level that replaces an administrator's actions and removes it, and is refused, with
nothing changed, a change that would leave them without `owner`; someone without
`owner`, an unknown repository or group and another tenant's repository are refused; a
group's creator is its one member and owner, a name is taken once in a tenant, only an
owner sets memberships and no owner lowers or removes themselves; only an owner deletes
a group, which takes every assignment naming it, and not while it is a repository's one
owner; only an owner, own or
through a group and of the repository's tenant, deletes a repository, which takes every
assignment on it, and an unregistered one leaves nothing to remove; and what the changes
leave is admitted. They also check the listing's caller, tenant and deployment bounds,
filters, pages and refusals, and that only an exchange for one repository issues a
repository scope. No Lore server is involved. They check that a group's member holds it
by its ID as `write` and its owner as `admin`, and nobody else or in another tenant.
Unit tests in `src/catalog/tests.rs` check a subject's groups, with a section of their
own: the caller's own call holds its tenant's groups with their roles beside the
identity provider's, as principal and actor alike, and none of another tenant or
another person; a Work Context membership granted to a group is held through it, and
the revision that removed the membership leaves neither; a server's call for the caller
holds none; and a role granted to a group's ID is held through it.

`tests/lore_sharing.rs` starts the binary with the native HTTP fixture's configuration,
records synthetic `hub.person` rows as a sign-in writes them, and calls the access and
group routes as the acceptance callers. It checks the exact answers: the creator's owner
entry, a share with a person named by an email in any case and the list that follows,
by email, and a group created with its creator as owner and its `id`, listed only to its
members.
It checks the refusals by problem type: reading without a grant, sharing or changing a
group without being its owner, an email no one signed in with, whose detail asks for a
sign-in, the caller's own lowering or removal on a repository and in a group, bodies of
another shape, an unknown repository, group and group name, and a taken name. Sharing
the repository with a group and adding the `client_secret_post` client's principal to it
lets that principal's repository token be issued and Lore's listing name the repository
with `owner`; removing the membership takes both away. An administrator's assignment
with other actions is listed with them and replaced by the level an owner sets, and a
principal without a `hub.person` row is listed with a null email and name after the
people. Deleting the group, refused to a member who is not its owner, answers the
caller's remaining groups and takes its assignment off the repository, and a second
deletion answers `group-not-found`. No identity provider or Lore server is involved.

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
login form, the Hub's callback and the code exchange; the callback records the person in
`hub.person` with Keycloak's name and email. It registers the Ledger and Relay fixtures
and leaves Relay's `relay_later` Tasks unclaimed, so each stays live while the test acts
for it. A Task created for the user's call exchanges and calls
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

- Several deployments of one slug may hold registrations. A server without
  instances routes to its newest registration whose process lives; a server with
  instances routes each person to the instance of the branch their selection names,
  or else of the base branch, as [Execution Composition](#execution-composition-and-deployment-identity)
  describes.
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
are live data. Prepare listeners, policy, secret locators and browser-client binding
before committing; failed preparation rejects the update without changing the active
revision or working runtime. The Computer template and provider profiles are read at
entry instead, below.

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
removed permissions. The current catalog still controls browser/session/client/grant
authority. The supplied refresh grant's actual client ID accompanies custody; it is
not inferred from an old provision snapshot. Forwarding capacity remains the current
process-wide budget, not a recorded per-user quota; old instance snapshots reuse that
gateway budget. Requests already using an older budget generation drain naturally when
the installation changes the limit. Unchanged runtime data reuses prepared state
without a new native RPC on a grant-only catalog update. The stable router can
enable/disable Computer routes from a prepared catalog without rebuilding the server
router.

#### Computer versions

A Computer version is everything a new sandbox gets from the installation: the
template, the policy and the Hub's Computer settings, which a typed provision snapshot
holds (workload settings, native policy/template protobufs, selected persistence
backend and model secret locator, never access/refresh tokens or actual model
credentials). Its name is the first twelve hexadecimal digits of the SHA-256, under
the domain `https://veoveo.ai/formats/computer-version` and a NUL byte, of a canonical
document: the template's spec with its environment as a sorted map, OpenShell's
canonical policy JSON, and the settings without the handbook, the forwarding capacity,
the collection setting, the gateway connection and the template's and policy file's
names. Protobuf maps encode
in no fixed order, so neither protobuf enters the digest as stored. The same inputs
name the same version whatever their names or object identity.

The template's annotation `veoveo.ai/release-note` is the version's release note,
written by whoever publishes the template; its first line is the version's summary.
OpenShell keeps a template's annotations through creation. What a version brings over
the one the Computer runs is what differs between them: its note, when that differs
from the note of the version the Computer runs or that one's is unknown, and, with or
without a note, the mechanical differences: a changed image, and each network rule's
endpoints (`host:port` and any path) added or removed. A version that changes only
through the policy or the Hub's Computer settings keeps the template's note, which is
then not new and is not listed again.
The JSON status carries those differences as they are; the start and status pages say
them for people: "The Computer's software was updated" for a new image, and where the
Computer can now, or can no longer, connect, without image references or rule names.
The status page says what the Computer is doing in plain words (running, starting,
stopping, asleep after collection, or failed with its cause) and what the person can
do; the JSON keeps the sandbox's phase.

The installation's template is read when a Computer is entered, at most once in five
seconds across entries, and compared by its object ID and resource version: OpenShell
has no template update, so a changed template is a new object whose resource version
starts again at 1. The version is prepared again only on a change: the gateway's one
driver and its whole-workspace persistence, and the three provider profiles. A missing
or invalid template or profile leaves Computers unavailable, with a page and a problem
that say why, and the Hub still starts; a Computer whose sandbox runs is still served.
The template no longer has to exist before the Hub.

A Computer records the version its sandbox was last created at as `provisioning`. A
Computer has a sandbox at some version, or none. Entry with none, or with one OpenShell
reports stopped or completed, creates one at the current version; entry with a ready
one serves it whatever its version; one still starting is awaited. A sandbox the Hub
can no longer serve safely, whose name, retention, template clone, command, policy,
providers, restart policy or base path differ from its recorded version, gets the
status page with Restart instead of being served. Hub has no stop or resume.

Restart, update, recovery and creation are one replacement, claimed on the Computer
row (`operation_id`, its start and the provision snapshot it creates) and run detached
from the request: delete the sandbox with `keep_workspace`, so the next sandbox of its
name adopts its home; replace the owner's template clone with one of the target version,
whose environment adds `COMPUTER_VERSION` beside `LORE_IDENTITY`; create the sandbox; wait
until it is ready and its listener answers; then record the target as `provisioning`.
Every native request ID derives from the operation's ID. The gateway answers a delete
only once the sandbox is gone, which a Pod's stop can make longer than one request,
for instance when a Computer's Pod outlives its 30 s grace, and it
finishes a delete it has accepted after the request ends; so the delete's answer is
awaited for one RPC deadline and then the sandbox's absence for at most five minutes,
and a replacement that times out names the phase, Pod and conditions that still run.
Absence means the old Pod is gone too: OpenShell deletes the Sandbox resource with
foreground propagation and reports the sandbox `Deleting` until its Pod has gone.
OpenShell keeps an adopted home through any failed create, and refuses a create while a Pod of the name still exists, with reason
`PREVIOUS_SANDBOX_TERMINATING`, which the replacement records as "the previous Computer
is still stopping; try again". The factory awaits the same absence.
One replacement runs at a time;
one older than eleven minutes belongs to a worker that ended without recording it, and
entry may claim another. A restart answers at once and deletes the sandbox only after
three seconds, so a caller inside the Computer, such as an agent restarting its own,
receives the answer first. A failure records its cause, time and target version on the
Computer, which keeps its recorded version; a sandbox not ready within 120 seconds is
named with its unmet conditions, such as the scheduler's `Insufficient cpu`. Restart
tries again, and Start the previous version creates the recorded one when it differs
from the version that failed. A completed update from one version to another
records an update notice, from the version the owner was last told of, which a status
read returns once; when the start page of that replacement showed its changes, none is
recorded. Each replacement logs its duration.

Migration `0024` adds the operation, failure and notice columns and drops the
per-Computer template and sandbox request IDs; installations reset Computers instead of
migrating them. `DeleteSandboxRequest.keep_workspace` comes with the OpenShell pin
`180b9e5a7`, whose Kubernetes driver keeps the volume; other drivers refuse the delete,
which a replacement then records as its failure.

#### Computer collection

With `computers.collectAfterSeconds` set (the installation example sets 7200), the Hub
deletes the sandbox of a Computer idle that long, keeping its home, and records the
Computer as collected (`collected_at`, migration `0025`). Idle means both: no browser
request through the Hub, and no network traffic from the sandbox. A working agent calls
its model through OpenShell's proxy whatever its client, so the second covers work
without a browser. A long job with no traffic is collected; that is the accepted cost.
Without the setting nothing is collected. It is not part of the Computer version.

The proxy records, in memory, the time of each browser request it forwards to a
Computer. OpenShell records the sandbox's traffic as `SandboxStatus.last_activity_time`
(pin `c28524bdd`): the latest 10-second activity summary from the supervisor that held
an allowed proxied connection or L7 request, written at most once a minute and kept
across gateway restarts. A long-lived connection counts once, when it opens. A sandbox
with no recorded traffic counts from its creation. Both times count from no earlier
than the Hub's start: browser requests are lost when the Hub stops, so after a restart
every Computer is active from then, and a restart never collects in a burst.

Every five minutes a sweep reads each ready Computer with no replacement running and its
sandbox. It collects one whose sandbox is ready and both of whose idle times have reached
the setting, through the one lifecycle path: it claims the operation on the Computer row
with the recorded version, as a restart does, so entry shows the start page meanwhile
and no restart, update or creation runs beside it; it deletes the sandbox with
`keep_workspace` and waits until it is gone; then it ends the operation and records
`collected_at`. A sandbox that is not ready, such as one being created or deleted, and a
Computer whose replacement runs are never collected. A browser request between the
decision and the claim keeps the Computer. A collection that fails ends the operation
without a failure, so the Computer stays as it was for the next sweep; it logs WARN with
the cause. Each collection logs INFO with the Computer, its version, both idle durations
and how long the delete took.

A collected Computer is one with no sandbox. Its next entry creates one at the current
version, records that version and clears `collected_at`, and the update notice tells of
a version change as after any creation. The status reports `collected` until then.

The browser record is per Hub process. With several Hub replicas serving browser
requests, one replica would not see another's and could collect a Computer in use
through it; the record then belongs on the Computer row.

Migration `0006` defines the mandatory per-instance snapshot for a reset/reseeded
development application database. It deliberately has no backfill from current
defaults and does not attempt to migrate old Computer state. Native workload or
storage deletion is not performed by this schema change. Installation must add the
exact `GetGatewayInfo` POST and `config:read` to the dedicated control authority;
no native authorization implementation changes. Gateway and supervisor protocol
additions require matching maintained runtime images.

### Private Computer browser ingress

The optional live `computers` installation selects one public PKCE client,
profile/Work Context, authenticated native OpenShell gateway, trusted workload
template, native policy file, fixed service command and global provider profiles.
The gateway authority has explicit workspace/provider/sandbox provisioning rights;
it never enters the Computer. The trusted bearer environment must hold a native
mediated reference whose management credential renews under OpenShell custody.
`raw_grpc_fresh` opens an authenticated call; it does not refresh a copied expiring
JWT.
Browser input selects no image, policy, secret,
provider profile or forwarding target. Both credential keys name native runtime
**environment aliases**, rather than logical profile credential names.

A Computer's `/sandbox` is its owner's home, and the Hub writes nothing into it: it
runs no command in a Computer. The installation's `computers.handbook` is served as
the MCP server instructions of `/mcp/{computers.profile}` to callers whose token
belongs to `computers.oauthClient`, after the Hub's own one-line description; other
callers of the Hub never receive it. Each discovery reads the current catalog, so an
edit reaches a client on its next connection.

`GET /computer` admits the existing opaque host-only session cookie or starts an
ordinary PKCE authorization against this Hub's existing OAuth endpoints. A bounded
single-use database state binds the verifier to a separate initiating-browser
cookie. The callback exchanges the native code, verifies the actual signed access
JWT with the shared verifier, resolves current native subject authority, and binds
the browser only to a refresh-family ID of the exact tenant/principal owner.
No expired or fabricated JWT establishes a browser session. Cookies contain random
selectors; only their hashes are stored. Grant material is never stored in Hub
Computer rows, cookies or UI bundles.

A sign-in to a ready Computer whose family is still admitted hands nothing off: under
the row lock custody uses, the browser joins that family, and the sign-in's own grant
is dropped unused. Another browser, another device or a lost cookie therefore never
touches a running Computer. Custody is handed off only to a Computer without an
admitted family: at its first sign-in, and after its family was revoked or expired.

A precommitted pending handoff invalidates old browser admission before native
mutation. An owned worker locks that exact handoff, ensures a distinct owner
workspace, creates workspace-local providers and replaces their Hub refresh material.
ConfigureProviderRefresh **and** RotateProviderCredential must succeed before ready
publication. Each ConfigureProviderRefresh begins a new authorization epoch, and
OpenShell then no longer resolves the placeholders that the sandbox's running processes
were given, so custody replaces a sandbox that exists, keeping its home, before the
browser enters. Entry creates a missing one with the owner's template clone and
nonsecret external base path. The existing managed runtime access
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

The owner reaches Lore with that one login. Each handoff gives the Computer's own OAuth
client a new secret, whose identifier is the Computer's native name and whose digest
the handoff's precommitted beginning stores on the Computer; the token endpoint reads
it while the worker holds the row lock. The worker ensures workspace-local
`computer-lore` of the configured `loreProvider` profile, hands the client to the
gateway's `oauth2_client_credentials` refresh and rotates it; every sandbox attaches
it with the other providers and names the owner's principal in the clone's
`LORE_IDENTITY`. At `/oauth/token` a `client_credentials` request whose client ID is
a Computer's native name and whose `client_secret` matches the stored digest receives
a Lore token of the Computer's owner for the installation's one Lore deployment. With a
`scope` that names one repository by the deployment's scope template, it is the token of
that repository, which the deployment's public client would obtain by exchange, admitted
against the owner's [assignments](#groups-and-sharing) on every issue; a repository the
owner holds no grant on is refused with `invalid_scope`, as the exchange refuses it, and so
is any other scope. Without a `scope`, it is the owner's login token, which names them and
carries no repository, for Lore's listing and the requests that name no repository. The
client requests no resource. OpenShell asks for the repository each Lore request names,
under the Computer's provider profile, so no token grows with the number of repositories
and the Computer never holds one. A native name is never UUID-shaped, so a Computer's
client and an instance's are never taken for each other.

An assignment or group change needs no renewal: every repository token lives the
deployment's token lifetime, so a change reaches each Computer within it. No process
claims a revision's activation any longer, and migration `0028` drops the claim's
column from `hub.control_active`.
A replacement and browser entry also wait for a successful native ForwardTcp target
connection under the existing 120-second startup bound. The gateway must return
ForwardTcp success after the native relay claim, not before target connection.
Document navigation awaits a sandbox OpenShell is starting and waits on its actual
forwarding connection. Warm asset/API requests open only their real forwarding connection;
there is no extra health endpoint or per-request probe. Retry classification uses
native status codes, never error text; authorization/invalid-target failures stop
immediately. A successful TCP connection does not prove application protocol health.

The custody worker survives HTTP disconnection. Native operations and readiness waits
are bounded; a crash or ambiguous failure leaves the durable handoff pending and
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

A ForwardTcp, a Computer's or a server's, holds one HTTP/2 stream to the gateway for
the whole life of the connection it carries, and the Hub sandbox's supervisor admits at
most 32 concurrent streams on one connection to the gateway: a stream beyond them waits,
unannounced, until another ends. Forwards therefore ride gateway connections of their
own, apart from the native RPCs, at most 32 forwards on each, each opened at its first
forward and kept. Computers have `computers.forwardingConnectionLimit` forwards and
servers 64, so that limit, never a connection's streams, is what refuses a forward.
The online-only native app must be used; mounted UI caches are convenience, not a
same-origin browser security boundary or a guarantee about previously loaded data.

The Computer's routes:

| Route | Behavior |
|---|---|
| `GET /computer` | The way in: sign-in, creation of a missing sandbox, the start page while the sandbox is created or started, then the Computer; the status page when a failure or a sandbox the Hub can no longer serve needs the owner |
| `GET /computer/status` | The owner's status page, or the status as JSON for a bearer |
| `POST /computer/restart`, `POST /computer/previous` | A replacement at the current version, or of the recorded version after a failure: for the owner's session, from this origin only, then `/computer`; for a bearer, `202` with the status, `409` while one runs, `503` when no Computer can start |
| `GET /computer/version` | Public: the current version and its release note's first line only, for a server that names it in a tool description |
| `GET /computer/callback` | The sign-in's code exchange and custody, then `/computer` |

The start page, shown on every creation, names the version it starts and lists what
it brings over the recorded one, and reloads itself every two seconds until the
Computer is ready. The status page names the version the Computer runs and the
sandbox's phase, an update and what it brings, a running restart, the last failure and
its cause, a blocking sandbox, and an update not yet told; it offers Restart and,
after a failure, Start the previous version when the recorded version is not the one
that failed. Both are plain HTML, with no script or
style and a CSP of `default-src 'none'; form-action 'self'`.

Each action path takes either caller. A request with an `Authorization` header is a
bearer's, which the Hub admits for `computers.profile`, the one Computers profile, as
any profile route admits its bearers: the Computer's own Hub credential
(`COMPUTER_HUB_REFERENCE`, a token of the Computers' client) or a registered server
calling the Hub for a caller who reached it through that profile. It serves the
caller's own ready Computer, found by tenant and principal. Without one, the owner's
browser session cookie decides; a browser without a session goes to `/computer` to
sign in. Status JSON (`version`, `sandbox`, `current`, `currentUnavailable`,
`updateAvailable`, `whatsNew`, `restart`, `failure`, `blocked`, `updated`) and the page
both return an update notice once.

An initial authorization refusal on a browser document navigation redirects to
`/computer`, and so does a navigation that finds the Computer's sandbox missing,
stopped or being replaced; one whose last replacement failed with no sandbox left, or
whose sandbox the Hub can no longer serve safely, goes to `/computer/status`. A navigation is a
GET with `Sec-Fetch-Mode: navigate` and, when the browser names it, `Sec-Fetch-Dest:
document`. Entry never replays the stale mount or chat URL, and it returns to the
Computer only once it runs. API/fetch, event-stream and WebSocket requests retain `401`
or `503`, malformed routes retain `404`, and other operational failures retain `503`
rather than redirecting into a recovery loop.

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

### Native contributed credential lifecycle

The executable binds one typed contribution and its claim codecs at composition.
Its admitted section contains deployment endpoint/audience, native recipient,
finite login scopes, repository scope and resource-ID templates, token lifetime,
repository registrations, groups, and exact assignments of native actions to a
principal or a group of the repository's tenant.
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

Every Lore token names its person by Hub principal ID in its `identity` claim, so the
pusher a Lore branch event names is a Hub principal. A repository token's `resources`
hold one entry, for the repository its `scope` names, with the person's native actions
on it as `permission`; a login token holds none. `root_domains` names the private Lore
recipient's host. An assignment without actions still allows Lore's ordinary
unprotected operations, writes included; no read-only role is inferred from it. Only an
explicit assignment, to the person or to a group they belong to, grants a repository.
Creating a repository, through the Hub or with Lore, makes its creator the owner; no
other grant is automatic, and no administrator role or Work Context membership grants repositories.
Removing an assignment or a membership stops further issuance, while tokens already
issued keep their rights until they expire under the deployment's token lifetime.

Whoever creates a repository owns it, in one step. `POST /lore/{profile}/repositories`
with `{"name"}` and an optional `"id"` takes any authenticated caller with a tenant,
directly or through a server that calls the Hub for them, with no administrator role
and no quota. The Hub creates the repository in the one Lore deployment as the
caller, with a Lore login token it issues for them through the deployment's public
client, so Lore records them as creator. It then publishes one revision that
registers the repository, titled by its name, and assigns the caller `owner`. The
Hub prepares no personal repository of its own. The publication takes the process's
publication lock and is built again on a revision conflict. The route answers 201
with `{id, name, remote}`, where `remote` is `<deployment endpoint>/<id>`, once the
revision is active in this process, so the caller's next repository token holds it. Lore
answers a repeat
creation of the same ID and name with the existing record, so a call with the `id`
of a repository Lore records the caller as creating registers it, such as after a
failed publication; that failure answers 503 with the repository's `id`. Another
creator answers 409 `repository-claimed`, a registered repository without the
caller's owner assignment answers 409 `repository-registered`, since a creator never
restores an assignment an administrator removed, and a registered repository the
caller owns answers 200 unchanged. Lore tells the Hub of the creation as it makes it, as
it tells the Hub of every repository it creates ([below](#repositories-lore-creates-and-deletes)),
so that revision is published before the route's own, which then changes nothing; the
route answers 201 for a repository it found unregistered. Owners share it as
[below](#groups-and-sharing), an owner deletes it with Lore, and Lore lists repositories
through the Hub as [below](#repository-listing).

#### Groups and sharing

The Lore section holds groups beside repositories and assignments. A group has a random
32-hex `id`, a `tenant`, a `name` unique in its tenant (1 to 64 of `a`–`z`, `0`–`9` and
`-`, starting with a letter) and `members`, each a principal of the tenant with the role
`member` or `owner`. An assignment names exactly one subject: a `principal`, or a `group`
of the repository's tenant. A person's actions on a repository are the union of their own
assignment's and those of every group of the tenant they belong to; holding any of those
assignments is holding a grant on the repository. Every reader of grants uses that one
rule: repository tokens, for a Computer, an exchange or a factory instance's pusher, the
listing, and the watch and status routes; the selection route reads no grant. Groups are
catalog objects of kind `lore.group`. An assignment keeps its `principal` member and adds
`group`, and admission refuses one that names both or neither, one whose group is not of
the repository's tenant, a second assignment of one subject on a repository, a repeated
group ID, a name repeated in a tenant and a repeated member.

These groups are a person's groups everywhere, not only in Lore. When the Hub
authenticates a request (`resolve_authenticated_subject` in `src/catalog.rs`), it adds to
the principal's `groups`, beside its identity provider's, the `id` of each group of the
principal's tenant they belong to in that catalog revision, and to its `group_roles` their
role in it: `member` as `write` and `owner` as `admin`, as a repository's member writes
and its owner also shares and deletes. It adds them before it resolves the Work Context
membership. From there the existing contracts carry them, and nothing else changes:

| Contract | Carries | Decides |
|---|---|---|
| The installation's roles, grants and policy rules, in `policy::decide` | the subject's `groups` | a role granted to one of them is held through it, so the caller's `tools/list`, admission and forwarded permissions follow it |
| Work Context memberships | the subject's `groups` | a membership naming one of them |
| The forwarded token, RFC 9068 | `groups`, which the SDK requires to equal the embedded actor's | each server's own decisions over groups |
| The invocation claim `https://veoveo.ai/claims/invocation` | the actor's `group_roles` | the SDK's `Principal::group_memberships`, by which Artifact caps a group grant |
| Token exchange, RFC 8693 | the caller as the forwarded token or the Task's route retains it, groups included | the roles evaluated again at each exchange |
| A server's call for a caller | no groups: its actor is the server, and the SDK refuses a server actor with groups | nothing through groups, as before |

The installation names only what exists before it is published: people and its identity
provider's groups. Its roles, rules and Work Context memberships therefore never name a
Lore group, which people make and delete at runtime. A Lore group is named by the grants
people make at runtime: Lore's assignments, and each server's own, such as Artifact's.
Whether a person may call a billed or other risky tool is their role's to decide, never a
Lore group's.

What is granted to a group is therefore held by whomever its owners add. A group granted
`write` on an artifact lets every member write, and one granted `admin` lets its owners
administer and its members write; the Hub's decision over the tool's declared risks comes
first. A membership change applies from the next request. A forwarded token keeps the
groups of its request until it expires, at most 15 minutes for `subscriptions/listen`, and
a Task keeps those of the call that created it, as it keeps that call's authority. Tokens
the Hub issues to clients, and the sign-ins they are issued from, keep only the identity
provider's groups. Outside Lore a group is named by its `id`, never its name: anyone may
create a group, so a grant naming a name could be taken by whoever creates it first, and a
name could equal an identity provider's group. The kernel reads memberships through its
sections: `CatalogSection::group_memberships` lists a section's memberships, none unless
it holds groups, and the catalog keeps them by tenant and principal beside its objects,
once per revision; the Lore section lists its groups' members. Only the subject of a
request (`HubCatalog::with_group_memberships`) and the match of a Work Context
membership read them, so no token the Hub issues to a client carries them.

People are named by email. At every sign-in the Hub records the person in `hub.person`:
tenant, principal, display name and email, which is the identity provider's `email`
claim, or its `preferred_username` when that is an address, in lowercase; without either
the person has no email. An email belongs to one person of a tenant; a sign-in with
another person's email takes it over, and a sign-in that names no tenant records no one.
A person who has never signed in cannot be named. The table is the Hub's own
(`src/person.rs`, migration `0029`), and the Lore contribution resolves an email to a
person, and a principal to an email and a name, through it.

Each route below takes any authenticated caller with a tenant, directly or through a
server that calls the Hub for them, authenticated as a creation is, and names a
repository of the installation's one Lore deployment. Each change publishes one revision
under the process's publication lock, built again on a revision conflict, like a
creation, and answers once the revision is active in this process; a change that
changes nothing publishes nothing and answers the same. The routes are in
`src/bin/hub/contributions/lore/sharing.rs`, and what a caller may change is decided by
the section alone (`sharing/changes.rs`). Fields that need two words are camelCase.

| Route | Who | Behavior |
|---|---|---|
| `GET /lore/{profile}/repositories/{id}/access` | a holder of a grant on it | The access list |
| `PUT /lore/{profile}/repositories/{id}/access` | an owner of it | `{"person": "<email>", "access": …}` or `{"group": "<name>", "access": …}`, exactly one of `person` and `group`, with `access` `member`, `owner` or `none`. Sets that subject's assignment, or removes it, and answers the access list |
| `POST /lore/{profile}/groups` | anyone | `{"name": "<name>"}`: creates the group with the caller as its one member, its owner; 201 with the group |
| `GET /lore/{profile}/groups` | anyone | `{"groups": [<group>]}`: the groups the caller belongs to, by name |
| `PUT /lore/{profile}/groups/{name}/members` | an owner of the group | `{"person": "<email>", "role": "member", "owner" or "none"}`: sets or removes that membership, and answers the group |
| `DELETE /lore/{profile}/groups/{name}` | an owner of the group | Deletes the group and every assignment naming it, and answers the caller's groups as `GET` does. A grant outside Lore naming its `id` holds nothing afterwards: no group is given that ID again |

The access list is `{"repository": "<id>", "access": [<entry>]}`, each entry an
assignment on the repository: `{"person": {"email", "name"}, "access", "actions"}` or
`{"group": {"name"}, "access", "actions"}`, people by email and then groups by name. A
group is `{"id", "name", "members": [{"email", "name", "role"}]}`, its members by email. A
person is named by the email and display name of their last sign-in; one with no
`hub.person` row, such as a principal an administrator assigned, has both `null` and is
listed after the people who signed in. `actions` is the assignment's native action set
as the section stores it, in its canonical order, and `access` its level: `owner` when
the actions hold `owner`, otherwise `member`. An owner sets a level: `member`, an
assignment without actions, or `owner`, the `owner` action, which replaces whatever
actions an administrator published. Groups, assignments and memberships are listed by
email and name. A group also shows its `id`, which grants outside Lore name it by; other
IDs stay internal.

Refusals are RFC 9457 problems whose title and detail say what to do. A caller without
a tenant answers 403 `sharing-forbidden`. An unknown repository, one of another tenant
included, answers 404 `repository-not-found`; reading the access of a repository without
a grant on it 403 `repository-access-forbidden`, and changing it without `owner` 403
`repository-owner-required`. An email no one of the tenant has signed in with answers 404
`person-not-found`, which asks for that person to sign in once, an unknown group 404
`group-not-found`, a group change by someone who is not its owner 403
`group-owner-required`, and a taken group name 409 `group-exists`. A change that would
leave the caller without `owner` on the repository, own or through a group, answers 409
`repository-owner-kept`, and one that would lower or remove the caller's own owner role
in a group 409 `group-owner-kept`, and deleting a group that is the one owner of a
repository 409 `group-owns-repository`, so neither is ever left without an owner by these
routes. A body of another shape answers 400 `invalid-request`, an invalid group name 400
`invalid-group-name`, and a failed read or publication 503 `sharing-unavailable`.

The native HTTP fixture approves a synthetic identity through trusted fixture
state, checks actual hosted HTTP/JWT/persistence/live-publication behavior, and
restarts against a stale seed; its identity is synthetic, not an Entra sign-in.

#### Repository listing

Lore lists repositories through its `auth_service` catalog: for `lore repository list`
it forwards the caller's token to `UrcAuthApi.LookupUserPermissions` at its
`repository_catalog_url`, and only then. The Hub answers that one method as gRPC on its
own listener, at `/epic_urc.UrcAuthApi/LookupUserPermissions`; the installation routes
Lore there. The token must be a Lore token this Hub issued: `at+jwt`, signed by a Lore
deployment's authorization server, for that deployment's audience and unexpired, as a
login or a repository token. A missing token, one the Hub did not issue and one for
another audience answer `UNAUTHENTICATED`. The answer is the token's person's grants on
that deployment in the person's tenant in the active control plane, own and through
groups, not the token's `resources`, so a listing follows a change at once: one
`ResourcePermission` per repository, its `resource_id` rendered by the deployment's
resource-ID template (`urc-{id}` in the installation, the form Lore's catalog parses) and
its `permission` the person's native actions on it.

Lore sends `resource_filter = "urc"`, no `context_filter` and no `page_size`, and
follows `next_page_token` until it is absent. The Hub answers the entries whose
resource ID begins with the filter and `-`, every entry for an empty filter, in
resource-ID order. A positive `page_size` bounds a page, and none or 0 means every
entry. `next_page_token` is the last resource ID of a page that more entries follow,
and a `page_token` continues after that ID, so an assignment published between two
pages lands in its place and nothing repeats. A `context_filter`, a negative
`page_size` and a `page_token` that is no resource ID of the deployment answer
`INVALID_ARGUMENT`. Authorization does not change: Lore authorizes every request from
the token's `resources` and its `auth_url` stays unset, so the Hub is asked only when
something lists, creates or deletes, and the Hub answers none of `UrcAuthApi`'s other
methods.

#### Repositories Lore creates and deletes

Lore tells the endpoint its catalog lists from of each repository it creates and deletes,
with the caller's token: `RebacApi.CreateResource` with the repository's resource ID and
name before it stores a repository, and `DeleteResource` with its resource ID before it
removes one. So `lore repository create` makes the record the creation route makes. The
Hub answers both as gRPC on its own listener, at `/ucs.auth.RebacApi/CreateResource` and
`/ucs.auth.RebacApi/DeleteResource`, and admits the same tokens as the listing.

A creation publishes the revision the creation route publishes: the repository,
registered in the token's person's tenant and titled by its name, and their `owner`
assignment. A repository registered as theirs already changes nothing; one registered
without their own owner assignment answers `PERMISSION_DENIED`, so Lore stores nothing.
A deletion by a person whose actions on the repository, own or through a group, hold
`owner` removes its registration and every assignment on it; anyone else's answers
`PERMISSION_DENIED`, which Lore takes as the refusal of the deletion, and a repository the
Hub does not register leaves nothing to remove. A resource ID that names no repository of
the deployment answers `INVALID_ARGUMENT`, and a publication that fails `UNAVAILABLE`,
so Lore stores or removes nothing then.
