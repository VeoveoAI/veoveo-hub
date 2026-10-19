# Agent Instructions

These rules apply to the `veoveo-hub` crate, in addition to the
repository's [AGENTS.md](../../AGENTS.md).

- The Hub names no server. Servers arrive through registration; never add a
  server's slug, scheme, permission, route or table to code, tests or configuration
  examples. Tests use the SDK's Observatory fixture or fixtures they declare themselves.
- The registration protocol, the forwarded token and the MCP audit vocabulary belong to
  `veoveo-hub-sdk`; the database kernel, Tasks and the audit envelope belong to
  `veoveo-core`. Change them there, then re-pin the revision here. The control plane,
  the policy evaluator, the `hub` schema and the `hub.*` audit classes belong
  to this crate.
- Keep `src/bin/hub.rs` a thin entrypoint: commands, startup and route wiring.
  Behavior belongs in focused library or binary modules.
- Put every SQL statement in its own `.sql` file beside the module that runs it and
  run it through sqlx's `query_file!` macros; tests keep theirs under `tests/`. The
  `hub` schema's migrations live in `migrations/` and append; installations reset
  instead while nothing is released. After changing a query or a migration, regenerate
  `.sqlx` against a database with core's kernel migrations and then this crate's
  migrations applied.
- Errors at OAuth endpoints use the defining RFC's format; other HTTP errors use
  `problem::Problem`.
- Tests that need a database or the binary own their fixtures, timeouts and cleanup;
  databases come from core's PostgreSQL fixture with the Hub's migrations and audit
  declarations applied (`src/test_store.rs`, `tests/support/database.rs`).
- Every audit record names a declared class: the SDK's `mcp.*` classes for MCP
  requests, core's `task.*` and `audit.*` classes, or this crate's `hub.*` classes
  in `src/audit/vocabulary.rs`, each with a closed JSON Schema.

- The user-authorized native credential contribution is a scoped exception only at
  the executable composition boundary: `src/bin/hub/contributions/lore` and its
  native integration fixtures may name Lore's existing protocol. The generic
  library, OAuth handlers, policy evaluator and ordinary fixtures remain independent
  of native server names. Do not add another process or an authorization service.
