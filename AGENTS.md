# Agent Instructions

These rules apply to veoveo-hub.

- The Hub's design is in [crates/veoveo-hub/DESIGN.md](crates/veoveo-hub/DESIGN.md) and
  the `DESIGN.md` beside each module it covers; the platform's is in the
  [architecture](https://github.com/VeoveoAI/veoveo-docs/blob/main/ARCHITECTURE.md). Change the design in the same change as the code.
- Use upstream runtimes such as PostgreSQL and Keycloak unmodified. Adapt through
  supported configuration.
- Every name, identifier, format, wire field and protocol element follows a published
  standard or core's [conventions](https://github.com/VeoveoAI/veoveo-core/blob/main/docs/CONVENTIONS.md).
- The Hub names no server; crate rules are in
  [crates/veoveo-hub/AGENTS.md](crates/veoveo-hub/AGENTS.md).
- Pin every `veoveo-core` crate to one revision of core's `main`, and every
  `veoveo-hub-sdk` crate to one revision of the SDK's `main`.
- Prefer strong types: typed structs, enums and domain identifiers over raw strings and
  JSON wherever the shape is known.
- Keep test logic in Rust and tool behavior in each tool's own configuration. Do not add
  a task runner or shell scripts that hold test logic.
- Pin exact versions and image digests. When changing a pin, verify the latest stable
  release at its source.
- Run `docker build --target check .` on macOS and on Linux before pushing.
- Commit small, coherent changes. Commits carry no AI or tool attribution.
