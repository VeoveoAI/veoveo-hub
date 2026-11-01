# Computer replacement protocol — source correspondence

Qualified manual source-correspondence model. No Rust extraction/refinement or
native sandbox safety/liveness qualification is claimed. Initial authoring source basis `ef3c25dc79efa8fab080a62c804a6ad410edb6fc`;
native-qualified regression basis `8b7303d8351ebf356577666c981fbbe9747c79f0`.
The native regression extends only the
existing store test. Existing registration proofs and all runtime behavior stay
unchanged. This addresses the version-recovery part of HUB-COMPUTER-01; browser
session admission/custody issuance remains a separate obligation.

## Representation and source

* `src/store/computers.rs:15–135,284–344` and adjacent operation_claim/finish/fail.sql:
  one table entry per Computer ID,explicit tenant/principal scope. Row models all
  touched columns plus exposed custody/provisioning/notice/collection fields.
  Opaque `storageFrame` retains unprojected columns such as created_at; this is not
  the claim that the native Computer projection exposes those values.
* The optional operation groups its ID/start/whole target snapshot,matching0024's
  null-coherence constraint. The raw notice_shown flag stays separate. Claim writes
  only operation fields,shown=false,failure=None. Finish writes STORED op.target,
  suppresses notice if shown,clears operation/shown/collected;failure is retained.
  Fail writes full supplied cause/time/target version and clears op/shown,keeping
  CURRENT provisioning,notice and collected state. Other fields/rows are framed.
* `lifecycle.rs:208–250,299–371`: target/spec selection/version admission is an input
  boundary. Worker context is constructed only after an acknowledged matched claim and
  successful reread,from original claim inputs plus the actual reread row snapshot.
  The reread is a separate observation and may reflect intervening writes.
  Lost claim replies or reread/parse errors launch no
  worker even when a claim committed. Notice uses previous version/earlier notice source,
  target version and observed whats_new result (default on error). Worker chooses
  finish only after native success,fail on first error. Its recorded bool/error is
  an internal observation,not an HTTP success;Ok(false) is ignored,Err is logged.
* `provisioning.rs:593–672`: native continuation has distinct remove,connect,
  delete-template,build-template,create-template,create,Ready and listener attempts;
  for_spec admission precedes them. Invocation arguments are constructed from
  owner key,operation ID and full target snapshot. Logical attempts are NOT proof
  of remote receipt. Full protobuf construction/UUIDv5 request derivation,source
  serialization,driver behavior and version hashing remain interpretation contracts.
* remove() is deliberately one call contract:missing sandbox returns success;
  already Deleting skips delete and waits;delete-answer timeout continues wait_gone
  rather than immediately failing. Its internals,Pod absence and keep_workspace/home
  survival are not proved here. No lost home/cancellation fiction is introduced.
* `gateway.rs:122–143` listener success means a native TCP forward opened,NOT HTTP
  or application-health validation. Sandbox Ready observation is distinct from
  Computer.ready custody admission (`session.rs:50–68`).

## Assumptions and nonclaims

`runWorker(rows,...)` uses the table observed at completion SQL linearization,
NOT an assumption that the DB stayed unchanged while native effects ran. An old
worker can therefore complete against actual newer-operation rows.

SQL effects are atomic Read-Committed statement outcomes. commitAck applies them
and returns their matched bool;commitError applies them despite error delivery;
abortError does not apply. Thus an error is never rollback evidence. This is an
explicit PostgreSQL/SQLx boundary,not a proof of PostgreSQL. Concurrent histories
use actual intermediate tables;no output property gates a write.

The first native-loop theorems cover returned logical-stage observations. The
outer600-second timeout can interrupt between/in pending calls;its scheduler and
interrupted-prefix outcomes are NOT modeled by this initial loop. Native observed
after-effects remain arbitrary even on returned errors.
A return from the600-second Tokio timeout does not cancel an accepted gateway
mutation. The660-second stale interval is an operational recovery rule,not proof
that an old process/native request ended. SQL op-ID fencing protects durable row
publication only;it does not fence same-name native effects across operations.

`running` uses started_at > now-660;claim uses started_at < cutoff. Equality is not
silently treated as either complementary condition. Clock/scoped fresh operation
IDs/decoded snapshots/owner admission are explicit native interpretations;no global
UUID freshness axiom. External custody,collection,session/token changes,arbitrary
native edits and response construction are outside this writer slice. Native
readiness is historical observation,not guaranteed continuing readiness at SQL ack.

The native regression uses real PostgreSQL store methods and compares every exposed
Computer field while A's finish/fail are obsolete during B and after B succeeds.
It does not observe storage-only columns,real OpenShell or native overlap. It
passed the focused real-SQL run and full Mac/Linux checks (475pass/2known skips
each);the coordinator owns the [native receipt](evidence/native-stale-completion.json).
These native results are separate from the checked Lean model. Existing
fake-gateway lifecycle/provisioning tests remain separate evidence.

The manual package reuses factory_retarget's Mathlib-only pinned context
(Lean4.35.0,Mathlib56a005855ea17e0b0958f73ec801ae140b0a1679). It adds no Aeneas
dependency,runner or CI target. Named targets have180-second external build bounds.

## Checked first-checkpoint claims

The Proof establishes exact admitted claim,finish/fail/show effects,wrong-owner and
stale/ended completion rejection,all unrelated-row frames,SQL commit-error versus
abort-error distinction,acknowledged claim+reread launch,stage-return first-error
prefix and ordered success with exact bound arguments. Closed populated histories
construct claim→success,stage failure→recorded failure→fresh accepted retry,and
actual stale A→newer B claim/success with A completions rejected at both cuts.
Expected full output rows are literal;source execution functions do not call the
independent spec. Closed examples establish exact-cutoff noncomplement and notice suppression.
The closed stale history invokes direct finish/fail effects against B's actual
claim and completed tables. The separate generic `stale_worker` theorem covers
the old captured worker's native-result-to-SQL adapter; the closed history does
not execute both obsolete native loops.

The fixture's `getD seed` context helper is used only at closed nonempty claim
outputs,and proved initial/retry launch equalities establish successful actual rereads yielding
those contexts. It is not missing-row allocation or a desired post-state premise.
Generic stale-worker theorem accepts the actual completion table,so native stages
need not freeze durable state. Opaque native effect markers are synthetic data;
no process readiness/cancellation guarantee follows from them.

`LAKE_NO_UPDATE=1 lake build ComputerLifecycleProof` passed in 3.620 seconds under
an external 180-second bound after two proof-authoring repairs. The 27 declaration
guards passed; eleven printed closures contain only the permitted Lean axioms.
See the [receipt](evidence/replacement-protocol.json). Semantic fault controls
remain pending. No production behavior changed.
