# Audited UPDATE store adapter

The foundation, **populated matched-update success, refused-audit recovery and
actual replacement/stale-no-op executions pass the manual Lean checks**. The
omitted-Update control remains pending. Native source basis is
`3d1b0b969ca8f35a757b74ebc9b959d62bf5ae33`; initial proof basis is
`ba812e6625a1c0706bd1b8815f5b752254693aa9`. No Rust, SQL, existing Lean module,
package pin or default target changes accompany this manual model. This is not an
extraction/refinement proof of PostgreSQL, SQLx, Core or Rust.

## Source correspondence

The owning [Hub design](../../crates/veoveo-hub/DESIGN.md) requires atomic changed
metadata plus Update audit, unchanged issuance/lifetime, and no change audit for
missing/superseded or unchanged updates. This advances HUB-REGISTER-01 and the
transactional-audit part of HUB-CLEANUP-01 in [the coverage map](../README.md).

* [service.rs:470–541](../../crates/veoveo-hub/src/registration/service.rs) authenticates
  and checks metadata, fixed deployment/slug, pinned signing key and statement.
  Only changed metadata calls store.update. These are admission/source
  interpretation boundaries. An unchanged service update is outside this adapter.
* [store.rs:468–512](../../crates/veoveo-hub/src/registration/store.rs) starts a SQL
  transaction, writes/locks deployment metadata, then updates the CURRENT client
  row. A zero count explicitly rolls back and returns Ok(()). A matched update
  appends Update targeting **request metadata.slug()**, then commits. The source
  projection is not taken from the earlier authenticated registration snapshot.
* [update_deployment.sql](../../crates/veoveo-hub/src/registration/store/update_deployment.sql)
  and [update_registration.sql](../../crates/veoveo-hub/src/registration/store/update_registration.sql)
  are represented by the unchanged RegistrationUpdate deployment-first primitive
  writes. The existing MetadataAccepted relation describes selected statement
  constraints; it does not assume correct stored output or lock ownership.
* [store.rs:714–730](../../crates/veoveo-hub/src/registration/store.rs) uses the existing
  transactional Core append, with one Update activity and server target, without
  a client-ID audit field. Core 422dcf2 migration0500 lines407–437 accepts absent
  IDs or exact retries matching BOTH complete JSON draft and stored occurred_at;
  conflicting IDs refuse. JSON time and microsecond stored time remain distinct
  via the reused Record/Encoding/CoreAppend types.
* The existing native audit-refusal fixture in
  [concurrency_tests.rs:1360](../../crates/veoveo-hub/src/registration/store/concurrency_tests.rs)
  uses a BEFORE INSERT exception. The checked model ID-conflict witness is a different
  modeled refusal. SQL error propagation and confirmed rollback remain explicit
  native interpretation; an error alone does not prove abort.

Coordinator's read-only eight-file native map is
`/tmp/hub-registration-audit-20261101/audited-update-native-source-review.json`,
SHA256 `36711b02419ed12471ddc72a5de93c561d8cc9f2e49e5c722ac20b61894350a0`.
It reports byte equality to native-qualified 3d1b0b9. This proof addition does not
require or claim a new native execution.

## Model and independent specification

RegistrationAuditedUpdate.State wraps the unchanged RegistrationUpdate.State and
adds a committed ledger and per-attempt private cursors. Every data transition is
an ACTUAL RegistrationUpdate.Step true; audit-only steps erase to reflexivity.
The inherited mixed model represents one admitted deployment.
The projection therefore inherits existing mixed register/update row locks,
request stability, full data effects and tagged commit history. No data executor
is copied, no new runtime lock or authentication rule is proposed.

At actual UPDATE ready, beginAudit captures the singleton source program. Each
successful private append checks the next intent and CoreAppend, then advances the
cursor. Commit entry and server decision require exhausted cursor, not equality to
a desired audit result. Progress and immutable request binding must DERIVE the
exact singleton record list. The atomic server decision publishes old data writes
and acknowledged records together. Fresh/new identity claims are per-attempt
conditions, not a universal UUID freshness axiom.

UpdateDecision independently requires sequentialUpdate on the ACTUAL precommit
DB, one Update record for the ORIGINAL requested metadata's server, pointwise full
record extension and preservation of every previously committed complete record.
The sequential specification preserves the current row's client/login, digest,
issuance and stable remainder, plus other replicas, roles, binding and frame.

## Explicit interleaved-registration boundary

The unchanged mixed model permits actual registration commits. The wrapper never
silently erases their audit effects or permits arbitrary ledger replacement:
externalRegisterCommit requires an actual old data step with its register event
and a finite inductive AppendBatch of CoreAppend-valid complete records. The batch
is published atomically with that data commit. Its activity/target program is
**uncertified**: namespace ClaimNamespace, replica Register and replacement Replace
correctness are OPEN. This atomic-contribution interpretation is an external
composition contract, not a proof of those native register transactions.

No desired registration audit vocabulary or nonempty batch is used to gate a
commit. Scoped claims concern UPDATE relative to the actual precommit ledger;
there is no full-global assertion that every registration was properly audited.
The checked replacement/noop example below provides a faithful full external
contribution and names its existing-namespace/no-new-claim boundary.

External contributions and foreign appends preserve committed full records and
are disjoint from acknowledged IDs of ACTIVE update-private cursors. This is the
scoped native unique-key-lock interpretation; absent or exact same-ID retry
semantics are reused unchanged. Retired aborted cursors do not reserve IDs. Audit
retention/deletion and foreign conflicting writers are excluded. Audit sequence or
sealer order is never identified with chronological commit order.

## First foundation checkpoint and remaining work

The checked Proof establishes projection, derived singleton completion, complete data/full
record publication at the actual update commit, external full-record/private-ID
preservation, inherited data history and per-own-step noop/abort preservation.
An additional reached cursor-settled invariant excludes a cursor on every reached
no-op outcome: cursor creation requires the updated-ready path, and no later
transition can convert that attempt to noop. The theorem directly states cursor
absence at every reached no-op state; it is not a separate temporal trace theorem.
The commit theorem constructs the actual serverCommit and its independent result
from a reached committing state, exhausted cursor and fresh-record premises.
Noop/abort statements concern their OWN rollback transition: they do not deny
foreign writes before/after it or freeze the entire ledger through a mixed history.
The inherited data history has its existing `owned` frame projection; individual
UPDATE effects preserve the CURRENT frame, without assuming the environment stops.

The separate populated matched execution below supplies actual acceptance with
independently specified full outputs and an ordinary successful acknowledgment.
The separate recovery execution below adds refusal, explicit abort and fresh retry.
The replacement/noop execution below includes explicit external audit provenance.
A later omission control will remove only the Update instruction
and require a legal modified execution plus wrong full result and rejection of the
unchanged acceptance assertion. No control has been authored or run here.

Store response tags do not prove native ClientInformation construction. Service
reload/information, postcommit errors, authority/token reuse, physical SQL/crypto
refinement, ordinary instance writers, removal/expiry and complete namespace/register
audit adapters remain outside this foundation. PostgreSQL transaction atomicity,
shared lock semantics and faithful query/metadata/serialization projections are
native assumptions, not new axioms or desired-output guards in Lean.

## Manual validation

Use the existing pinned package context and named NONDEFAULT targets. Each command
must have an external 180-second process bound via the existing builder logger:

```sh
lake build RegistrationAuditedUpdateModel
lake build RegistrationAuditedUpdateSpecification
lake build RegistrationAuditedUpdateProof
```

Model passed in 30.465 s and Specification in 7.778 s. After proof-body repairs,
Proof passed in 45.943 s: 27 explicit standard-axiom guards and ten printed closures,
using only `propext`, `Classical.choice` and `Quot.sound`. These are declarations,
not 27 execution scenarios. Each command used the 180-second bound and default
heartbeat budget. Source/import/tool maps remained unchanged. The copied Model,
Specification and Proof artifacts were independently checked at 529,880, 27,728
and 982,064 bytes; exact identities are in the
[receipt](evidence/audited-update-foundation.json).

Three earlier authoring failures involved record syntax, case-bound names/list
equality, and implicit helper parameters. Their complete log identities and repairs
are retained; no contract or axiom guard was weakened. A logger directory-exists
failure and a transient SSH banner timeout occurred before compiler execution and
are recorded separately. No native checks were rerun: the native source remains
byte-identical to the previously qualified implementation. No deployed result is
claimed. See the
[qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-audited-update-foundation.md).

## Checked populated matched-update success

The separate Witnesses, WitnessExecution and WitnessProof modules construct nine
actual model steps for actor 0 updating client 30/login 40. The initial database
contains another complete replica (31/41), unrelated login 99, retained binding and
unrelated state. Its inherited key/login constraints and selected admission facts
are proved; a native history creating this synthetic seed is not claimed.

Only the first replica's metadata, statement and update time change. Independent
literal expected maps preserve its identity, digest, issuance time and full stable
payload, the other replica, all roles and unrelated state. Complete prior audit
record 900 remains, and locally fresh record 100 is exactly Update for the original
requested server. Draft and stored timestamps remain distinct. Before commit,
both data writes and the full acknowledged audit record are private, with actor 0
holding the lock. Final acceptance includes all transaction/cursor maps, the sole
update event, released lock and ordinary store success; it does not claim native
ClientInformation construction.

The executable path is importable independently of output acceptance and the
generic audited UPDATE correctness proof. Its optional-append helper reads the
actual cursor, allowing the checked omission control below to remain executable. No output specification computes its expected DB via the executor.

One final named target, `LAKE_NO_UPDATE=1 lake build
RegistrationAuditedUpdateWitnessProof`, built all three new modules and passed in
37.729 s with 15 declaration guards and nine printed closures. The first two
authoring attempts failed at missing decidability and unfinished projection
equalities; narrow proof-body repairs retained every assertion. The final compiler
exited 0. A later wrapper reporting bug looked for `.lean.olean` filenames and
exited 1; all three actual `.olean` outputs, logs and maps were independently
verified without rerunning the compiler. The raw summary and corrective supplement
are preserved in the [receipt](evidence/audited-update-success.json) and explained in
the [record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-audited-update-success.md).

## Checked refused-audit recovery

RecoveryWitnesses and RecoveryProof reuse the same populated seed and independently
literal expected result. Actor 0 reaches the audit call after both private writes.
Its record 900 has the correct Update instruction, target and timestamp encoding,
but that ID already names a different complete committed record. The proof excludes
every valid CoreAppend result for it. An actual failure step retains the original
committed database and ledger, sets the error outcome and still holds the lock.
A separate actual serverAbort releases the lock; error alone never proves abort.

Actor 1 then executes a fresh nine-step update from that exact aborted state,
appending locally fresh record 100 and returning success. The final complete maps
retain actor 0's failed/aborted state alongside actor 1's successful state, preserve
the prior complete audit record and untouched data, and contain exactly the retry's
update event. The top-level theorem composes all sixteen actual steps. The aborted
cursor is inactive model state, not a new runtime tombstone or ID reservation.
This proves a possible successful retry schedule, not eventual recovery or lost
commit-reply behavior.

One bounded `LAKE_NO_UPDATE=1 lake build RegistrationAuditedUpdateRecoveryProof`
passed in 37.976 s, with twenty declaration guards and nine printed closures. Three
closures need no axioms; the others use only `propext` and `Quot.sound`. An earlier
38.026 s authoring attempt failed because two local binders used Lean's reserved
`prefix` keyword; renaming them to `readyPath` retained every definition, theorem
statement and guard. No runtime source changed and no native check was rerun.
See the [receipt](evidence/audited-update-recovery.json) and
[record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-audited-update-recovery.md).

## Checked actual replacement followed by stale UPDATE no-op

ReplacementWitnesses, ReplacementExecution and ReplacementProof construct twenty
actual wrapper steps: actor 0's preflight, fourteen ordinary replacement steps by
actor 1, and five stale-update steps through explicit rollback and acknowledgment.
The seed has the same two old replicas. Replacement admits client 50/login 60 with
complete version-3 metadata and fresh credentials; its issuance and update times
are both 501, matching `create_registration.sql`'s use of the same `now` argument.
The old request still carries version 2 and its originally authenticated row.

The actual mixed register commit has an explicit finite CoreAppend-valid batch:
records 200/201 Replace the two captured row servers, then 202 Register names the
candidate server. The concrete batch's multiplicity, source targets, encoding and
local freshness are checked. All use the existing namespace's slug; no new claim
is appropriate under the stated already-held namespace interpretation. One valid
complete Listing order is chosen, without assuming a PostgreSQL return order.
The generic external-registration audit vocabulary boundary remains open.

After replacement acknowledges, the old update takes the deployment lock and
privately writes version 2, then finds client 30 absent. Its explicit rollback
preserves the complete committed version-3 database and ledger and releases the
lock; the store returns ordinary no-op success. Independent literal outputs cover
all rows, logins, credentials, binding, unrelated state, full records, transaction
and cursor maps, and the sole register commit/event. No Update cursor or record is
created. Execution is separate from output acceptance; no result oracle gates a
transition.

The final bounded `LAKE_NO_UPDATE=1 lake build
RegistrationAuditedUpdateReplacementProof` passed in 40.282 s, with 24 declaration
guards and eleven printed closures. The prior 37.977 s attempt failed to normalize
an impossible empty-list membership in a freshness proof. A local case split fixed
it without changing any statement or guard. Three closures require no axioms;
the others use only `propext` and `Quot.sound`. The source-review timestamp correction
preceded compilation; neither repair is a production defect. See the
[receipt](evidence/audited-update-replacement.json) and
[record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-audited-update-replacement.md).

## Checked omitted-Update control

A private one-line mutation replaces the singleton Update source instruction with
an empty list. It changes no transition, commit guard, fixture or original
acceptance statement. The unchanged optional append helper then stutters, so the
mutant has eight actual steps rather than nine. An independently compiled diagnostic
proves the legal execution, whole expected data result, successful store response,
complete wrong ledger (only prior record 900), empty cursor and all transaction
maps. It also proves the original UpdateAccepted predicate is false.

The original WitnessProof then fails at line 19: `decide` establishes that
`acknowledged.audit 100 = some updateRecord` is false. Later complete-ledger/cursor
equality errors and dependent axiom-guard refusals are secondary diagnostics. All
88 original package files, including the original Lake configuration, are restored;
the temporary diagnostic is removed. Ordinary Lake invalidation rebuilds the
affected modules and the original positive target passes. No compiled artifacts
were manually seeded, altered or removed to influence the check.

The three bounded commands took 37.927 s (diagnostic pass), 3.670 s (expected
acceptance failure) and 12.485 s (restored positive pass). Seven diagnostic guards
and four printed closures pass; the restored witness retains fifteen guards and
nine closures. The observed new closures use only `propext` and `Quot.sound` or
no axioms. A precompiler phase-list mismatch and a post-diagnostic Python reporting
error were repaired only in temporary orchestration; the diagnostic was not rerun.

This is a deliberate handwritten-model fault, not a new production bug. SQL/Core
and source correspondence, plus generic external registration audit vocabulary,
remain explicit boundaries. See the [receipt](evidence/audited-update-omission-control.json)
and [record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-audited-update-control.md).
