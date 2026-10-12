# Agent Instructions

These rules apply to the `veoveo-service-gateway` crate, in addition to the
repository's [AGENTS.md](../../AGENTS.md).

- The Gateway names no server. Servers arrive through registration; never add a
  server's slug, scheme, permission, route or table to code, tests or configuration
  examples. Tests use core's modular fixture or fixtures they declare themselves.
- The registration protocol, the forwarded token and the policy evaluator belong to
  `veoveo-core`. Change them there, then re-pin the core revision here.
- Keep `src/bin/gateway.rs` a thin entrypoint: commands, startup and route wiring.
  Behavior belongs in focused library or binary modules.
- Put every SurrealQL statement in its own file under `src/queries/` and bind runtime
  values. `DEFINE USER` takes its password only as a literal, so the registration
  store writes that one value as a JSON-encoded string the Gateway generated itself.
- Errors at OAuth endpoints use the defining RFC's format; other HTTP errors use
  `problem::Problem`.
- Tests that need a database or the binary own their fixtures, timeouts and cleanup.
