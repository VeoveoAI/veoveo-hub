# veoveo-service-gateway

veoveo-service-gateway is the Veoveo Gateway: the MCP entry point that authenticates callers, routes requests to registered servers and enforces authorization.

This repository is a skeleton created from
[veoveo-template-mcp-rust](https://github.com/VeoveoAI/veoveo-template-mcp-rust). It
owns its toolchain, gate configuration, environment image and OpenShell policy. The
placeholder crate in `crates/veoveo-service-gateway` gives the gates something to check until
Gateway code arrives.

## Gate

```sh
docker build --target check .
```

The `Dockerfile` defines the `dev` environment, the `check` gate, the release `build` and the `runtime` image.

## Development environment

```sh
docker build --target dev -t veoveo-service-gateway:dev .
openshell sandbox create --name vv-srv-gateway --from veoveo-service-gateway:dev \
  --policy openshell/policy.yaml --env HOME=/sandbox --detach -- sleep infinity
```

Sandbox names use the short form `vv-srv-gateway`, because OpenShell limits sandbox
names to 19 characters.

## License

Apache-2.0. See [LICENSE](LICENSE).
