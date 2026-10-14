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
token and the [server registration protocol](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/7446181806258d48a352ecdbd2bc00998d303701/crates/veoveo-sdk-rust/src/registration/DESIGN.md)
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
```

A local installation from
[veoveo-template-installation](https://github.com/VeoveoAI/veoveo-template-installation)
runs this image, under its own layer, as the Compose service `vv-srv-gateway`. The
environment works in the installation's Lore repository at `/sandbox/src`, keeps its
state in `/sandbox`, and reaches PostgreSQL at `vv-postgres.internal:5432` and
registered servers at their `.internal` names. Run the commands below from the
installation's directory.

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

A local installation runs PostgreSQL 18, the Gateway in `vv-srv-gateway` on port 8080
and each server in its own environment. The Gateway connects as the installation owner,
a role that owns the installation database and may create roles. At startup it applies
core's kernel migrations, then its own migrations of the `gateway` schema, and records
the audit classes it writes in `audit.detail_type`. Start PostgreSQL first, then build
the Gateway and generate its installation once, with a Lore client for each
environment:

```sh
docker compose exec vv-srv-gateway bash -lc 'cd /sandbox/src \
  && cargo build --locked --target-dir /sandbox/target-serve --bin veoveo-service-gateway \
  && mkdir -p /sandbox/state/gateway \
  && /sandbox/target-serve/debug/veoveo-service-gateway generate-local-installation \
       --directory /sandbox/state/gateway --issuer http://vv-gateway.internal:8080 \
       --lore-resource lores://vv-lore.internal:41337 \
       --lore-environment vv-srv-gateway --lore-environment vv-mcp-time \
       --lore-environment vv-mcp-artifact --lore-environment vv-mcp-media'
```

The command writes into `/sandbox/state/gateway`:

| File | Contents |
|---|---|
| `gateway.json` | The installation configuration: one profile, `local`, the `member` and `admin` roles, the registration secret reference, two client-credentials callers and the Lore resource with one grant per Lore client |
| `gateway.env` | The generated secrets, mode 0600: forwarded-token, access-token, refresh-delivery and audit keys, and `VEOVEO_REGISTRATION_SECRET` |
| `authorization-server.jwks.json` | The hosted authorization server's public key |
| `acceptance-member.pem`, `acceptance-admin.pem` | RSA keys of the two callers, mode 0600, with their public `.jwks.json` |
| `lore-admin.pem`, `lore-<environment>.pem` | RSA keys of the Lore clients, mode 0600, with their public `.jwks.json` |

It prints the token endpoint, the profile's MCP endpoint, the scope and each client's
identifier and key identifier, and no secret. Put the installation owner's password from
the installation's `.env` into `/sandbox/state/database.env` as
`VEOVEO_DATABASE_PASSWORD=...`.

The served binary builds into its own target directory, `/sandbox/target-serve`, apart
from `target/`, where test builds enable test-only dependency features, so a test build
is never served. Start it in its own terminal:

```sh
docker compose exec vv-srv-gateway bash -lc 'cd /sandbox/src \
  && cargo build --locked --target-dir /sandbox/target-serve --bin veoveo-service-gateway \
  && cd /sandbox/state/gateway && set -a && . ./gateway.env && . ../database.env && set +a \
  && VEOVEO_ISSUER=http://vv-gateway.internal:8080 \
     VEOVEO_LISTEN_ADDRESS=0.0.0.0:8080 \
     VEOVEO_ALLOW_LOOPBACK_HOSTS=true \
     VEOVEO_CONFIG=/sandbox/state/gateway/gateway.json \
     VEOVEO_DATABASE_URL=postgres://veoveo_owner@vv-postgres.internal:5432/veoveo \
     exec /sandbox/target-serve/debug/veoveo-service-gateway serve'
```

`VEOVEO_DATABASE_URL` names the host, the database and the installation owner, and
never a password. Every registration grant names the same URL without its user, so a
server environment reaches PostgreSQL at `vv-postgres.internal:5432` too; set
`VEOVEO_SERVER_DATABASE_URL` when servers reach the database at another address.

The Gateway listens on port 8080 of its environment. Other environments reach it through
the installation's edge at `http://vv-gateway.internal:8080`, and the host at
`http://127.0.0.1:8080`:

| URL | Answers |
|---|---|
| `/livez` | HTTP 204 while the process runs |
| `/readyz` | HTTP 200 once the audit service is ready |
| `/.well-known/oauth-authorization-server` | RFC 8414 metadata with `registration_endpoint`, `token_endpoint` and `jwks_uri` |
| `/jwks.json` | The keys that verify the tokens the Gateway forwards to servers |
| `/oauth/.well-known/openid-configuration` | The OAuth authorization server's RFC 8414 metadata, where OpenID Connect Discovery clients such as Lore read its issuer and `jwks_uri` |
| `/oauth/jwks.json` | The keys that verify the authorization server's tokens, Lore tokens included |
| `/oauth/token` | Client-credentials tokens for the profile `local` and for the installation's Lore server, and the token exchange a registered server performs to call other servers for its caller |
| `/mcp/local` | The MCP endpoint of every registered server |

A server registers at `/oauth/register` with the registration secret as its initial
access token. An environment holds only a placeholder, which the installation's edge
replaces with the secret on `POST /oauth/register`. The registration response grants
the server its own PostgreSQL login; the server stays registered while that login holds
a live kernel instance.

A caller obtains a token by presenting an RFC 7523 client assertion, signed
with its key and naming its key identifier, at `/oauth/token` with
`grant_type=client_credentials`, `resource=http://vv-gateway.internal:8080/mcp/local`
and `scope=mcp`. `acceptance-member` receives every permission without a risk, and
`acceptance-admin` every permission.

## Lore Tokens

An installation that runs Lore names its server in the configuration's `lore` section:
the server's URL, which Lore tokens carry as their audience, the authorization server
that issues them, and one grant per client. A grant lists repository IDs or every
repository, the privileged Lore actions (`admin`, `migrate`, `obliterate` and `owner`),
and whether the client is a Lore service account, which Lore lets push to protected
branches. Every client that lists the Lore server in its `allowedResources` holds one
grant.

```json
"lore": {
  "resource": "lores://vv-lore.internal:41337",
  "authorizationServer": "local",
  "grants": [
    { "client": "lore-admin", "allRepositories": true, "actions": ["admin", "owner"], "serviceAccount": true },
    { "client": "lore-vv-mcp-time", "repositories": ["01a1397efdb27152b7259d027ce59c45"] }
  ]
}
```

A client requests a Lore token at `/oauth/token` with its RFC 7523 assertion,
`grant_type=client_credentials`, `resource` set to the Lore server and no scope. The
token lives 15 minutes, and its `lore_resources` claim lists `urc-<repository ID>`, or
`urc-*` for every repository, with the granted actions: the shape Lore's per-repository
authorizer reads. Lore finds the issuer's keys through
`/oauth/.well-known/openid-configuration`.

`generate-local-installation` with `--lore-resource` and one `--lore-environment` per
environment adds the Lore admin client `lore-admin`, granted every repository with
`admin` and `owner` as a service account, and a client `lore-<environment>` for each
environment, which reaches no repository until the installation grants it one.

`lore-tokens` keeps the environments' token files current. It runs as root and, every
ten minutes, requests a token for each `--target <client>=<file>` with the client's key
from `--directory`, then replaces the file with one owned by root and readable only by
`--group`. It prints no token.

## License

Apache-2.0. See [LICENSE](LICENSE).
