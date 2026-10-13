# Agent Instructions

These rules apply to veoveo-service-gateway.

- Use upstream runtimes such as OpenShell and PostgreSQL unmodified. Adapt through
  supported configuration.
- Every name, identifier, format, wire field and protocol element follows a published
  standard or core's [conventions](https://github.com/VeoveoAI/veoveo-core/blob/main/docs/CONVENTIONS.md).
- The Gateway names no server; crate rules are in
  [crates/veoveo-service-gateway/AGENTS.md](crates/veoveo-service-gateway/AGENTS.md).
- Pin every `veoveo-core` crate to one revision of core's `main`, and every
  `veoveo-sdk-rust` crate to one revision of the SDK's `main`.
- Prefer strong types: typed structs, enums and domain identifiers over raw strings and
  JSON wherever the shape is known.
- Keep test logic in Rust and tool behavior in each tool's own configuration. Do not add
  a task runner or shell scripts that hold test logic.
- Pin exact versions and image digests. When changing a pin, verify the latest stable
  release at its source.
- Run `docker build --target check .` on macOS and on Linux before pushing.
- Commit small, coherent changes. Commits carry no AI or tool attribution.
