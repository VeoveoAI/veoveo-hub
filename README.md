# veoveo-service-gateway

veoveo-service-gateway is the Veoveo Gateway, the MCP entry point of an installation.
MCP servers register with it and declare the permissions their tools, resources and
prompts require. The Gateway authenticates callers, grants permissions through the
installation's roles, and forwards each request with an access token that names the
caller and the permissions granted. It knows no server until one registers.

The [crate design](crates/veoveo-service-gateway/DESIGN.md) describes the Gateway.
Registration, the forwarded token and the policy evaluator are defined in
[veoveo-core](https://github.com/VeoveoAI/veoveo-core), whose
[server registration protocol](https://github.com/VeoveoAI/veoveo-core/blob/main/mcp/ext-registration/DESIGN.md)
every server follows.

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
built binary reach a local installation's SurrealDB at `host.openshell.internal:8000`
and registered servers forwarded to the host on ports 8800 to 8809 of
`host.openshell.internal`.

## Local Launch

A local installation runs SurrealDB from
[veoveo-template-installation](https://github.com/VeoveoAI/veoveo-template-installation)
on the host, the Gateway in `vv-srv-gateway` on port 8080 and each server in its own
environment. Start SurrealDB first, then build the Gateway and generate its
installation once:

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
client identifier and key identifier, and no secret. Put the database password from
the installation's `.env` into `/sandbox/state/database.env` as
`VEOVEO_SURREAL_PASSWORD=...`, then start the Gateway and forward its port:

```sh
openshell sandbox exec vv-srv-gateway -- bash -lc 'cd /sandbox/state/gateway \
  && set -a && . ./gateway.env && . ../database.env && set +a \
  && VEOVEO_ISSUER=http://host.openshell.internal:8080 \
     VEOVEO_CONFIG=/sandbox/state/gateway/gateway.json \
     VEOVEO_SURREAL_ENDPOINT=ws://host.openshell.internal:8000 \
     VEOVEO_SURREAL_NAMESPACE=veoveo VEOVEO_SURREAL_DATABASE=platform \
     VEOVEO_SURREAL_AUTH_LEVEL=root VEOVEO_SURREAL_USERNAME=root \
     /sandbox/src/target/debug/veoveo-service-gateway serve'
openshell forward start 8080 vv-srv-gateway
```

The Gateway listens on `127.0.0.1:8080` inside the environment. Through the forward,
other environments reach it at `http://host.openshell.internal:8080`:

| URL | Answers |
|---|---|
| `/livez` | HTTP 204 while the process runs |
| `/readyz` | HTTP 200 once the audit service is ready |
| `/.well-known/oauth-authorization-server` | RFC 8414 metadata with `registration_endpoint` |
| `/oauth/token` | Client-credentials tokens for the profile `local` |
| `/mcp/local` | The MCP endpoint of every registered server |

A server registers at `/oauth/register` with the registration secret as its initial
access token. An environment receives that secret through an OpenShell provider, so it
holds only a placeholder that the proxy replaces on requests to the registration
endpoint. A caller obtains a token by presenting an RFC 7523 client assertion, signed
with its key and naming its key identifier, at `/oauth/token` with
`grant_type=client_credentials`, `resource=http://host.openshell.internal:8080/mcp/local`
and `scope=mcp`. `acceptance-member` receives every permission without a risk, and
`acceptance-admin` every permission.

## License

Apache-2.0. See [LICENSE](LICENSE).
