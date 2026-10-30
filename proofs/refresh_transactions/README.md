# Refresh transaction model

This manually invoked Lean package models one retained refresh-family incarnation,
using the actual extracted family/token types and the checked pure delivery
predicate from [current_roles](../current_roles/README.md#refresh-binding-and-delivery-predicates).
It is a design model of application ordering and PostgreSQL primitives, not an
extraction or formal refinement of async Rust, SQLx, PostgreSQL or HTTP.

The first checked increment proves that arbitrary finite protocol/environment
interleavings cannot request commit before staging the complete chosen program.
It also proves the exact generation-only family effect of the rotation statement
list, its mathematical session-admission connection, duplicate family/token
preservation, UPDATE-if-absent noncreation, and preservation of an existing first
revocation time/reason. See the [compact checkpoint](evidence/prefix-checkpoint.json).
Reachable outcome-to-program/history composition, no-fork, aborted-attempt
exclusion, positive rotation/duplicate executions and semantic controls remain
pending. The given-list field lemmas alone do not establish those properties.

Primitive commit applies whatever deltas were staged, preserving unrelated rows.
It has no correct-rotation, comparison, audit-completeness or success guard. The
program separately validates replacement shape, handles consumed tokens before
fresh-token refusal, stages writes/audit, requests commit and acknowledges it.
Invalid/already-revoked early returns drop and release rather than commit. Lost
commit responses may coexist with server commit or abort. Responses here are
store outcomes; state-layer decrypt/digest checks and HTTP replies are outside.

| Model element | Source and boundary |
|---|---|
| Digest lookup, family lock, token reread | `src/store/refresh.rs` and `store/refresh/{lock_family,token_by_digest}.sql`; lookup snapshots before waiting can lose the token before reread. |
| Candidate projection and shape | `NewRefreshToken`→stored construction supplies family/unconsumed fields; only generation/expiry/issued-before-expiry/delivery presence are native validation guards. |
| Rotation/duplicate/replay | Actual store branch order; raw successor lookup precedes checked deliverability, with the sampled prelock request time. |
| Explicit revocation | `revoke_refresh_family` binding/expiry checks; existing revocation returns without a write. `COALESCE` retains first reason/time. |
| SQL effects | UPDATE-if-present, PK, digest uniqueness, family/generation uniqueness, stored nonnegative i64 range, FK and replacement `ON DELETE SET NULL` are explicit modeled primitives/domain predicates. SQL/schema/decoder correspondence is unproved. |
| Maintenance | Separate token row locks for clear/prune; family prune requires its row lock and cascade availability. Primitive committed steps abstract statement/lock scheduling, batch limits and query planning. |
| Audit | Core exact-content idempotent batch semantics, within the supplied transaction; HTTP denial/failure audit is a separate framed projection. |

A family deletion ends this tracked incarnation; recreation and UUID reuse are not
silently excluded by an eternal token-identity theorem. Other-family rows are
disjoint/framed. No global family-lock-only-writer assumption is made. In
particular a later maintenance timestamp can remove a predecessor despite a
request's older sampled time, and consume/mark can affect zero rows. Positive
retry/chain claims require compatible retained records and cleanup schedules.
`WellFormed` enumerates the relevant row-domain constraints; it is not a native
wellformed-store or successful-transaction oracle.

The original SQL lock comment overstates family-lock coverage; this package
records the retention exception. Only the Rust module comment was corrected, with
its production body unchanged. No SQL comment/hash churn or new runtime checks
were added. The separately [checked native baseline](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-refresh-native-baseline.md)
uses actual PostgreSQL, real audit append and private state-layer crypto. Fixture
PostgreSQL durability settings do not establish physical durability.

Run `lake build` or `lake env lean RefreshTransactionProof.lean` manually with the
pinned Lean toolchain. Native scripts/checks are operator-invoked; no CI, runtime
framework or production failpoints are installed.
