# veoveo-service-gateway

veoveo-service-gateway is the Veoveo Gateway, the MCP entry point of an installation.
MCP servers register with it and declare the permissions their tools, resources and
prompts require. The Gateway authenticates callers, grants permissions through the
installation's roles, and forwards each request with an access token that names the
caller and the permissions granted. It knows no server until one registers.

The [crate design](crates/veoveo-service-gateway/DESIGN.md) describes the Gateway. It
builds on [veoveo-core](https://github.com/VeoveoAI/veoveo-core) for the database
kernel, identity, Tasks, the audit envelope and the usage ledger, and on
[veoveo-sdk-rust](https://github.com/VeoveoAI/veoveo-sdk-rust) for MCP, the forwarded
token and the [server registration protocol](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/67314b4913e16a86cff6cb2558507f818227a79f/crates/veoveo-sdk-rust/src/registration/DESIGN.md)
every server follows. The Gateway owns its control plane, its policy evaluator and the
`gateway` database schema.

## Gate

```sh
docker build --target check .
```

The `Dockerfile` defines the `dev` environment, the `check` gate, the release `build`
and the `runtime` image. The same commands run inside the development environment.

## Development Environment

```sh
docker build --target dev -t veoveo-service-gateway:dev .
openshell sandbox create --name vv-srv-gateway --from veoveo-service-gateway:dev \
  --policy openshell/policy.yaml --env HOME=/sandbox --detach -- sleep infinity
```

Sandbox names use the short form `vv-srv-gateway`, because OpenShell limits sandbox
names to 19 characters. The environment's [policy](openshell/policy.yaml) lets the
built binary reach a local installation's PostgreSQL at `host.openshell.internal:5432`
and registered servers forwarded to the host on ports 8800 to 8809 of
`host.openshell.internal`.

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
pin, apply core's kernel migrations and then the Gateway's own migrations in
`crates/veoveo-service-gateway/migrations` to a scratch database, and run
`cargo sqlx prepare --workspace -- --all-targets --all-features` against it with an
empty `CARGO_TARGET_DIR`, so every core query is compiled and recorded. The check stage
does the same before `cargo sqlx prepare --check`.

## Local Launch

A local installation runs PostgreSQL 18 from
[veoveo-template-installation](https://github.com/VeoveoAI/veoveo-template-installation)
on the host, the Gateway in `vv-srv-gateway` on port 8080 and each server in its own
environment. The Gateway connects as the installation owner, a role that owns the
installation database and may create roles. At startup it applies core's kernel
migrations, then its own migrations of the `gateway` schema, and records the audit
classes it writes in `audit.detail_type`. Start PostgreSQL first, then build the Gateway and generate its installation
once:

```sh
openshell sandbox exec vv-srv-gateway -- bash -lc 'cd /sandbox/src \
  && cargo build --locked --bin veoveo-service-gateway \
  && mkdir -p /sandbox/state/gateway \
  && target/debug/veoveo-service-gateway generate-local-installation \
       --directory /sandbox/state/gateway --issuer http://host.openshell.internal:8080'
```

The command writes into `/sandbox/state/gateway`:

| File | Contents |
|---|---|
| `gateway.json` | The installation configuration: one profile, `local`, the `member` and `admin` roles, the registration secret reference, and two client-credentials callers |
| `gateway.env` | The generated secrets, mode 0600: forwarded-token, access-token, refresh-delivery and audit keys, and `VEOVEO_REGISTRATION_SECRET` |
| `authorization-server.jwks.json` | The hosted authorization server's public key |
| `acceptance-member.pem`, `acceptance-admin.pem` | RSA keys of the two callers, mode 0600, with their public `.jwks.json` |

It prints the token endpoint, the profile's MCP endpoint, the scope and each caller's
client identifier and key identifier, and no secret. Put the installation owner's
password from the installation's `.env` into `/sandbox/state/database.env` as
`VEOVEO_DATABASE_PASSWORD=...`.

OpenShell pins every policy-listed executable path to the SHA-256 it first observes and
denies the path once the file changes. The Gateway therefore runs from a
content-addressed copy under `/sandbox/run`, which `openshell/policy.yaml` allows to
reach PostgreSQL and registered servers, so every build gets a fresh path. Start it in
its own terminal and forward its port:

```sh
openshell sandbox exec vv-srv-gateway -- bash -lc 'cd /sandbox/src \
  && cargo build --locked --bin veoveo-service-gateway \
  && bin=/sandbox/run/veoveo-service-gateway-$(sha256sum target/debug/veoveo-service-gateway | cut -c1-16) \
  && install -D -m 0755 target/debug/veoveo-service-gateway "$bin" \
  && cd /sandbox/state/gateway && set -a && . ./gateway.env && . ../database.env && set +a \
  && VEOVEO_ISSUER=http://host.openshell.internal:8080 \
     VEOVEO_CONFIG=/sandbox/state/gateway/gateway.json \
     VEOVEO_DATABASE_URL=postgres://veoveo_owner@host.openshell.internal:5432/veoveo \
     "$bin" serve'
openshell forward start 8080 vv-srv-gateway -d
```

`VEOVEO_DATABASE_URL` names the host, the database and the installation owner, and
never a password. Every registration grant names the same URL without its user, so a
server environment reaches PostgreSQL at `host.openshell.internal:5432` too; set
`VEOVEO_SERVER_DATABASE_URL` when servers reach the database at another address.

The Gateway listens on `127.0.0.1:8080` inside the environment. Through the forward,
other environments reach it at `http://host.openshell.internal:8080`:

| URL | Answers |
|---|---|
| `/livez` | HTTP 204 while the process runs |
| `/readyz` | HTTP 200 once the audit service is ready |
| `/.well-known/oauth-authorization-server` | RFC 8414 metadata with `registration_endpoint`, `token_endpoint` and `jwks_uri` |
| `/jwks.json` | The keys that verify the tokens the Gateway forwards to servers |
| `/oauth/token` | Client-credentials tokens for the profile `local`, and the token exchange a registered server performs to call other servers for its caller |
| `/mcp/local` | The MCP endpoint of every registered server |

A server registers at `/oauth/register` with the registration secret as its initial
access token. An environment receives that secret through an OpenShell provider, so it
holds only a placeholder that the proxy replaces on requests to the registration
endpoint. The registration response grants the server its own PostgreSQL login; the
server stays registered while that login holds a live kernel instance.

A caller obtains a token by presenting an RFC 7523 client assertion, signed
with its key and naming its key identifier, at `/oauth/token` with
`grant_type=client_credentials`, `resource=http://host.openshell.internal:8080/mcp/local`
and `scope=mcp`. `acceptance-member` receives every permission without a risk, and
`acceptance-admin` every permission.

## License

Apache-2.0. See [LICENSE](LICENSE).
