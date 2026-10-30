# Control-plane publication

This manually invoked package qualifies a design model of the existing publication
transaction, supported by focused native PostgreSQL evidence. Its full Lean build,
dependency audits, concrete executions, two actual Rust fault controls and fresh
original-source restoration passed. Production behavior is unchanged. The
[receipt](evidence/control-publication-receipt.json) records exact inputs, commands,
timings, hashes, setup failures and limits; the
[dated qualification](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-control-publication-qualification.md)
records coordinator review. This is not extracted async Rust, formal SQL refinement,
a PostgreSQL implementation proof, or a replica-adoption proof.

`PublicationModel.lean` separates database operations from Hub's instruction
order. A server commit applies whatever statements its transaction staged to
the current committed state. It has no completeness, predecessor, program-counter
or successful-audit guard. Work Context grants and membership rows are separate
clear/insert operations. `PublicationProof.lean` relates these operations to an
independent bulk after-image, then establishes staged-program and locked-comparison
invariants over arbitrary finite interleavings. `PublicationExamples.lean` keeps
premature commit, omitted predecessor comparison, split audit commit and uncertain
commit acknowledgement expressible. `PublicationHistory.lean` derives fresh-key
and single-winner properties and the supplied/stored audit-content laws.
`PublicationReplay.lean` explains committed rows using only committed publisher
plans: an aborted attempt contributes no plan, even while another publisher or a
framed unrelated writer progresses. This replay is mathematical ghost history;
there is no runtime journal or second publication implementation.

The checked statements cover complete publication of the modeled row projection,
a fresh revision key before commit, a single committed winner per predecessor
while history is retained, audit-content preservation/idempotence, and no committed
contribution from a server-aborted attempt. `PublicationExecution.lean` constructs
legitimate scheduled initial and replacement successes. `PublicationWitnesses.lean`
instantiates their constraints with retained, removed and new contexts, changed
ordinal grant/membership lists, empty grants, retained history and prior audit.
`PublicationUncertainty.lean` constructs a shared reachable commit-error state
with both server outcomes, and a server commit followed by lost acknowledgement.
A concrete error-response execution commits a previously absent revision. These
are kernel-checked model witnesses, not native network-failure experiments.
Safety does not assert scheduler fairness or eventual lock availability.

## Source mapping

All `src/` and SQL paths below are under `crates/veoveo-hub/`.

| Model operation | Actual source |
|---|---|
| Begin, lock, compare expected predecessor | `src/store/control.rs:84` and `src/store/control/lock_active.sql` |
| Revision insert and duplicate-ID refusal | `src/store/control/create_revision.sql`, mapped unique violation in `publish_control_revision` |
| Supplied object rows | `insert_objects`, `src/store/control/create_objects.sql` |
| Enterprise/tenant insert-if-absent; context upsert | `replace_work_contexts`, `ensure_enterprise.sql`, `ensure_tenant.sql`, `upsert_work_context.sql` |
| Separate grant/membership clear and ordinal inserts | `delete_grants.sql`, `delete_memberships.sql`, `create_grant.sql`, `create_membership.sql` |
| Stale-context deletion | `delete_stale_work_contexts.sql`; grant/membership FK cascades in pinned Core `0200_identity.sql` |
| Pointer activation | `src/store/control/activate.sql` |
| Same-connection supplied audit batch before commit | `records.append(&mut transaction)` then `transaction.commit()` in `publish_control_revision` |
| Actual publication/Work Context drafts | `control_store::record_revision_on` → `control_store/audit.rs::changes` → `AuditAppend` → Core `AuditTransactionWrite::append` |
| Exact-content/idempotent audit insertion | Pinned Core `0500_audit.sql::audit.append`; incremental `ValidBatch` includes duplicate IDs within the incoming batch |
| Queued rollback; uncertain commit response | Pinned SQLx transaction Drop/commit boundary; abort request, server abort, server commit and response delivery are separate model transitions |

The nominal IDs and payloads are abstract physical row keys and admitted serialized
values. Context payloads cover only mutable title, policy, owner, classification,
labels and updated-at fields. Actual upsert preserves tenant, context_key and
created_at on conflict; those fields are outside the formal payload and checked
by native replacement assertions. This is a bounded projection, not a complete
physical context-row model. Audit content represents the full supplied draft and
occurred_at pair; native generated sequence/txid/recorded_at fields are not proved.
Row-plan generation, digest/catalog admission, the detailed SQL CHECK/FK
constraints and deterministic ID encoding are correspondence boundaries, not new
application validation rules. The tracked initial database must correspond to the
actual schema (including the seeded singleton and active-revision FK). The model
does not verify serializers or interpret arbitrary same-ID previous documents as
authentic. The concrete audit change batch requires the actual immutable prior
document supplied by the caller. Its success record plus zero or more Work Context
records is a supplied batch, not a promise of one row or unconditional row growth.
The generic `TransactionAppend` interface permits arbitrary implementations. The
model's same-transaction append operation corresponds to the concrete
`AuditAppend(AuditTransactionWrite)` composition, which appends the supplied batch
without committing internally. It does not prove every possible trait implementor
respects that contract, or formally verify Core's append SQL.

Protocol publishers are the only modeled writers of the active pointer, retained
revision history, revision objects and Work Context projection. Arbitrarily
interleaved unrelated-row writes and independent rejection/failure audits have
explicit disjoint framed projections. They are not allowed to alias tracked keys.
Commit applies deltas instead of replacing a database/audit snapshot. Cleanup,
owner SQL deleting history, concurrent non-publication Work Context writers and
Task-route cascade rows require further composition. No-aborted-success-batch
claims allow the endpoint's separate rejection/failure audit and identical audit
records that already existed.

PostgreSQL statement atomicity, transactional visibility, row locking, constraints
and Read Committed's post-wait current row are external database semantics. A
server abort discards staged changes and releases its lock; requesting rollback
does not synchronously establish either event. A successful response follows a
server commit, while an error can follow either server outcome. Ordinary successive
SELECTs need not share a snapshot. Sequence allocation and crash durability are
excluded. Core's private fixture disables fsync, synchronous_commit and
full_page_writes; those tests cannot establish crash durability.

## Native scope and controls

Two focused tests in `src/control_store/publication_tests.rs` use existing `TransactionAppend`
and the actual successful audit batch. The late-failure wrapper appends that batch
then returns an error, waits for a new locker to establish rollback/release and
compares a single-statement relational snapshot. The overlap wrapper holds the
first transaction after append; the test observes the second publisher's blocked
lock query through `pg_blocking_pids` before releasing the first. One statement
observes the old committed projection while all new writes are still staged.
Both wrappers inspect isolation on their actual supplied append connection. The
stale second publisher never reaches its appender; its blocked lock query is
observed separately. Tests own
their synthetic configuration rows, timeout, synchronization and private database.

Two private Rust source copies were compiled with the following isolated faults:

- Omitting the locked predecessor comparison made the observed stale writer
  return `Ok`, failing `stale predecessor must refuse`. Later mutated-state
  assertions were not reached.
- Committing activation before audit append left the failed revision, objects and
  changed context rows committed after the deliberate append error, while only
  the original audit batch remained. The exact snapshot comparison failed;
  the later active-load assertion was not reached.

Each mapped test exited 101 with one failed assertion, with the same freshly
compiled binary before and after execution. The model's separate constructive
counterexamples show these faulty instruction orders are expressible by the
unrestricted commit primitive. Neither native fault is presented as failing an
extracted async proof. Fresh original-source restoration passed both new unit
tests, both existing `control_store` PostgreSQL integration tests, formatting and
strict library Clippy. Historical executable hashes are per-stage measurements;
the shared native target was reused and does not retain every historical binary.

Builder commands, run only against a frozen input:

```text
lake build
lake env lean PublicationProof.lean
lake env lean PublicationExamples.lean
lake env lean PublicationHistory.lean
lake env lean PublicationReplay.lean
lake env lean PublicationExecution.lean
lake env lean PublicationUncertainty.lean
lake env lean PublicationWitnesses.lean
```

Four query-metadata entries were prepared against Core+Hub migrations on the private
PostgreSQL 18.6 fixture; the prior 251 entries remained byte-identical. Native
commands used explicit Rust 1.99 and `SQLX_OFFLINE=true`, the two
`control_store::publication_tests::publication_` library tests, existing
`--test control_store`, `cargo fmt --all -- --check`, and strict library Clippy.
Formal work used pinned Lean 4.35 and the existing pinned Aeneas/Mathlib packages;
all executable dependency guards passed with only `propext`/`Quot.sound` or subsets.
An initial integration-binary parser miss was recovered from the exact logged
executable and linked Clippy continuation without repeating successful tests.
The final fresh Lean snapshot initially lacked the Examples import prerequisite;
the preserved setup failure was followed by its missing target and the remaining
checks on the same unchanged source. No CI, hook, deployment or model call was added.
