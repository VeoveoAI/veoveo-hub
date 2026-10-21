# veoveo-hub

veoveo-hub is the Veoveo Hub, the MCP entry point of an installation.
MCP servers register with it and declare the permissions their tools, resources and
prompts require. The Hub authenticates callers, grants permissions through the
installation's roles, and forwards each request with an access token that names the
caller and the permissions granted. It knows no server until one registers.

The [crate design](crates/veoveo-hub/DESIGN.md) describes the Hub. It
builds on [veoveo-core](https://github.com/VeoveoAI/veoveo-core) for the database
kernel, identity, Tasks, the audit envelope and the usage ledger, and on
[veoveo-hub-sdk](https://github.com/VeoveoAI/veoveo-hub-sdk) for MCP, the forwarded
token and the [server registration protocol](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/a1d9d653ae3d723821cdeeab92f4a67acdca39ce/crates/veoveo-hub-sdk/src/registration/DESIGN.md)
every server follows. The Hub owns its control plane, its policy evaluator and the
`hub` database schema.

## Gate

```sh
docker build --target check .
```

The `Dockerfile` defines the `dev` environment, the `check` gate, the release `build`
and the `runtime` image. The same commands run inside the development environment.

## Development Environment

```sh
docker build --target dev -t veoveo-hub:dev .
```

Use this image as the component-owned toolchain environment. Installation factories
and deployed services run through native OpenShell; the former Compose Hub service
and application edge route are retired. See the installation's
[native setup](../veoveo-template-installation/openshell/README.md) and the current
[Hub/Time integration gates](../veoveo-docs/OPENSHELL_HARDCUT_PLAN.md#subsequent-adoption).
The earlier native deployment is qualified in its recorded scope; the integrated
environment/source-activation candidate still needs deployed qualification. Native
upstream placements are authoritative live control data. Revision publication
prepares replacement transport generations and preserves retained exact bindings;
ordinary component source changes require no Hub restart.

Tests that need a database start a private PostgreSQL cluster per test process from the
PostgreSQL 18 programs on `PATH`; the `dev` image installs the PGDG `postgresql-18`
18.6 packages, and on macOS Homebrew's `postgresql@18` provides the same programs.
The browser-session test logs a user in at a private Keycloak server it starts per test
process. The `dev` image builds Keycloak 26.8.1 at `/opt/keycloak` for an in-memory
database and runs it on Debian's OpenJDK 21. Elsewhere, `KEYCLOAK_HOME` names a
distribution built the same way with `kc.sh build --db=dev-mem`, and Java 21 must be
on `PATH`.
Queries are checked at compile time against the metadata in `.sqlx`, which also holds
the queries of the core crates the check stage compiles. After changing a query or a
pin, apply core's kernel migrations and then the Hub's own migrations in
`crates/veoveo-hub/migrations` to a scratch database, and run
`cargo sqlx prepare --workspace --all -- --all-targets --all-features` with
`DATABASE_URL` pointing to it. The `--all` option also recompiles query macros in
pinned dependencies outside this workspace, so a warm Core build cannot silently
omit its metadata from the shared set. The check stage applies the same migrations
before `cargo sqlx prepare --workspace --all --check`.

## Local Test Configuration

`generate-local-installation` creates a standalone hosted-MCP test configuration
with two deterministic application callers. It does not provision OpenShell,
IdP clients, Lore grants, DNS, ingress or a service deployment. Integration tests
use it with disposable databases and manage their own Hub process. To generate a
fixture manually from this repository:

```sh
cargo build --locked --bin veoveo-hub
mkdir -p /tmp/veoveo-hub-fixture
target/debug/veoveo-hub generate-local-installation \
  --directory /tmp/veoveo-hub-fixture --issuer http://127.0.0.1:8080
```

The command writes into the selected existing directory and refuses to overwrite
files:

| File | Contents |
|---|---|
| `hub.json` | The installation configuration: one profile, `local`, the `member` and `admin` roles, the registration secret reference, two client-credentials callers |
| `hub.env` | The generated secrets, mode 0600: forwarded-token, access-token, refresh-delivery and audit keys, and `VEOVEO_REGISTRATION_SECRET` |
| `authorization-server.jwks.json` | The hosted authorization server's public key |
| `acceptance-member.pem`, `acceptance-admin.pem` | RSA keys of the two callers, mode 0600, with their public `.jwks.json` |

It prints the token endpoint, the profile's MCP endpoint, the scope and each client's
identifier and key identifier, and no secret. Test caller private keys stay with
the test harness; factory callers use the SDK's environment-bearer input and native
OpenShell credential mediation.

## Runtime Contract

The Hub connects as the installation database owner, applies Core's kernel
migrations and then its own `hub` migrations, and declares its audit classes.
`VEOVEO_DATABASE_URL` names the host, database and user without a password;
`VEOVEO_DATABASE_PASSWORD` supplies that trusted runtime credential separately.
`VEOVEO_SERVER_DATABASE_URL` selects the database address issued to registrations
when services reach it at a different address. Qualify real PostgreSQL session
locks, notifications and reconnect from deployed sandboxes.

The Hub application origin is installation-owned. Qualify native OpenShell
`ServiceExposure` with `BearerPassthrough`, sandbox DNS/request authority and TLS
verification before publishing application setup instructions. The current native
upstream draft binds registered logical endpoints to approved workspaces,
sandboxes and ports, using authenticated `ForwardTcp` beneath the existing HTTP
client. It needs deployed streaming, cancellation, reconnect and management
credential-renewal qualification. Keep native management authority with the trusted
Hub. Do not use the retired Compose/Envoy Hub route as an ingress fallback.

The application routes are:

| URL | Answers |
|---|---|
| `/livez` | HTTP 204 while the process runs |
| `/readyz` | HTTP 200 once the audit service is ready |
| `/.well-known/oauth-authorization-server` | RFC 8414 metadata with `registration_endpoint`, `token_endpoint` and `jwks_uri` |
| `/jwks.json` | The keys that verify the tokens the Hub forwards to servers |
| `/oauth/.well-known/openid-configuration` | The OAuth authorization server's RFC 8414 metadata, where OpenID Connect Discovery clients  read its issuer and `jwks_uri` |
| `/oauth/jwks.json` | The keys that verify the authorization server's tokens |
| `/oauth/token` | Client-credentials tokens for the profile `local`, and the token exchange a registered server performs to call other servers for its caller |
| `/mcp/local` | The MCP endpoint of every registered server |

An installation/base server registers at `/oauth/register` under its namespace
bootstrap authority. An independently keyed override uses a one-use,
deployment-scoped admission bound to its metadata, current owner, native placement
and explicit storage binding. Namespace private keys and the global initial secret
do not enter override workers. Native OpenShell policy/provider binding limits
bootstrap-secret mediation to its admitted request. Preserve the SDK's subsequent
registration credentials and resume them on ordinary process restart; explicit
retirement deletes the registration. Missing registration needs freshly admitted
authority rather than automatic global bootstrap. The registration response grants
the server its own PostgreSQL login; the server stays registered while that login holds
a live kernel instance.

A caller obtains a token by presenting an RFC 7523 client assertion, signed
with its key and naming its key identifier, at `/oauth/token` with
`grant_type=client_credentials`, `resource=http://127.0.0.1:8080/mcp/local`
and `scope=mcp`. `acceptance-member` receives every permission without a risk, and
`acceptance-admin` every permission in that test configuration. External-issuer
application tokens require the configured issuer/audience/claims and matching Hub
catalog authority; the Hub does not mint tokens for an external authorization
server. Lore credentials come directly from the installation IdP, never the Hub.


## Environment source activation

The owner API and packaged `veoveo-environment` client share one
[typed environment definition](crates/veoveo-hub/src/bin/hub/activation/types.rs).
The full candidate is awaiting component gates and deployed qualification; earlier
helper/mechanism probes do not prove this integrated path.

An authenticated user can create an empty environment or update their own definition
with `PUT /environments/{profile}/{environment}` and
`{"idempotency":"<request-key>","definition":{...}}`. The definition contains
`id`, `displayName`, optional `base` and `components`. The authenticated tenant and
principal determine ownership; clients cannot supply another owner. Unknown IDs
are created by this operation, while reads and edits of another owner's ID fail.
`GET` returns the complete definition. Ordinary owner edits need no AdminWrite.
An operator separately admits each environment/component's runtime template,
provider attachments and explicit storage binding through the live control plane.
That eligibility does not come from the source file.

Each component contains `source`, `runtimeTemplate` and `recipe`. Source is either
`{"kind":"local_snapshot","digest":"<sha256>","size":123}` or
`{"kind":"lore","deployment":"<registered-native-id>","repository":"<repo-id>",
"selection":{"kind":"follow","branch":"<branch-id>"}}`; an exact source pin
uses `{"kind":"pin","revision":"<native-revision>"}` instead. Recipes contain
structured `check`/`build` commands with `executable`, `args`, relative `cwd`, and
one `worker`: compiled output from a source/cache-relative path, or an admitted
interpreter with immutable source/imports. Commands are foreground commands, not
background daemons. Activation supplies deployment, endpoint, key directory and
listener identity; a recipe cannot substitute registration credentials.

The client derives control endpoints from `COMPUTER_HUB_URL` and reuses
`COMPUTER_HUB_REFERENCE`, the existing native opaque user reference:

```sh
veoveo-environment apply --definition environment.json --local component=./source --exclude target
veoveo-environment watch --definition environment.json --local component=./source --exclude target
```

Local apply reserves before upload. Completion attaches the authorized Artifact ID
to the same accepted BuildId; Hub independently verifies the producer's digest and
size while materializing private PVC bytes. Transfer uses the configured platform
profile, independently of a candidate application's selected environment. Watch
retries the same reservation after transport failure and observes actual current
files before submitting changed intent. It does not resubmit obsolete intent merely
because it reconnects.

`GET /environments/{profile}/applies/{build}` reports desired and prior serving
state; `POST .../retry` retries that still-current accepted input, and
`GET .../diagnostics` returns bounded command output/truncation information for its
owner. Incomplete local transfers need fresh caller authority through
`POST .../complete-local`; retained source/output does not require uploading again.
Failed builds keep the previously serving worker, rather than falling back to base.

`GET/POST /environments/{profile}/{environment}/source` reads or explicitly selects
native definition following, using `{"replaces":<current-source-generation-or-null>,
"source":{"deployment":"<native-id>","repository":"<repo-id>",
"branch":"<branch-id>","path":"environments/work.json"}}`. The CLI's
`follow-definition --environment ID --deployment ID --repository ID --branch ID
--path environments/work.json` performs this CAS. The ordinary file contains the
same definition DTO, not another manifest. Local apply switches definition mode to
local atomically. Component source followers remain independent. Subscribe-before-read,
periodic authoritative reads, and selector/predecessor CAS handle missed events,
reconnect and branch rewinds without overwriting a later local selection.

Source changes reuse an admitted native sandbox/PVC and incremental cache. A copied
compiled executable uses a unique BuildId path; s6 owns revision workers and keeps
old inputs unchanged. Exact live registration and the application's `/readyz` gate
route/catalog publication. `/readyz` does not prove MCP discovery/tool behavior.
Retained canonical Task routes authorize only recovery of their exact worker while
current environment/template/storage/native placement eligibility remains; they do
not reselect an old override or grant new source/build authority.

Build the initial helper with `docker build --target environment-artifact ...`.
The Hub runtime image additionally needs a reviewed matching-platform `lore` named
artifact context containing `/usr/local/bin/lore`; native Lore reconstructs followed
definition files. No additional Lore runtime library/allocator is linked into Hub.
The check target does not require this runtime context. Initial Computer/runtime
image packaging is separate from per-edit source activation.

## License

Apache-2.0. See [LICENSE](LICENSE).
