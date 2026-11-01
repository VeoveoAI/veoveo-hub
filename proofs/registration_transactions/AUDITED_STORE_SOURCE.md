# Audited instance-store lifetime layer

This is a handwritten source-correspondence model, not extracted Rust or a proof
of PostgreSQL/Core. The model, specification and foundation proof now pass Lean;
[the receipt](evidence/audited-store-foundation.json) records exact inputs and outputs.
The prior lifetime model, specification, proofs and pins remain unchanged. This checkpoint represents
`register_instance` and `remove_instance` only; namespace claims, RFC 7592 updates,
expiry and ordinary deregistration require separate adapters.

## Source boundary

The native implementation is the qualified transactional audit fix at Hub
`3d1b0b969ca8f35a757b74ebc9b959d62bf5ae33`, based on
`e060b328ac79ad5e1870de37af30a0ec627e9d64`. Its native qualification is separate
from this Lean checkpoint. Core is
`422dcf268d690fcbc3fee435cf9b81e7f36ffeac`, SDK is
`7360c0f96cb616c519d27256df214ff4f2e6e831`.

In `crates/veoveo-hub/src/registration/store.rs`, `register_instance` performs
`replace`, `admit`, one `Replace` append for each returned ended registration,
then `Register` for the candidate, then commits. `remove_instance` captures ended
rows, drops their logins, deletes the instance, appends `Deregister` for each
ended row, then commits. `audit_ended` reads each **ended row's server**; it does
not substitute the candidate server or current deployment metadata. Its loop has
no total-list bound, and `append_audit` submits one record per Core append on the
same caller-owned SQL transaction. Audit detail contains activity and a server
target, not a registration client ID.

The old `RegistrationLifetime.Step true` owns the complete represented data
effects and real lock order. The new layer replaces only its commit entry/decision
with cursor-gated entry and one atomic data-plus-audit decision. Other steps erase
to the old transitions; audit-only steps stutter. The ordinary-step side condition
only excludes bypassing commit entry/decision, not any desired audit result.
Source construction reads the actual transaction's request and captured rows at
audit entry. Cursor advancement requires an explicit successful private append.
Program completion must be derived from these steps, never supplied as a commit
precondition. Captured-row/source binding at commit and independent exact stored
effects are proved in the checked leaf, not assumed by the cursor.
A settled-phase invariant derives the request/captured-row stability needed to
connect that cursor back to the actual commit attempt.

## Core and database interpretation

Core `platform/store/migrations/0500_audit.sql` lines 407–437 inserts by record ID;
an existing ID succeeds only when both the entire JSON draft and the stored
`occurred_at` column agree. `platform/audit` PostgreSQL conversion truncates the
column to microseconds while the JSON draft retains its original timestamp.
`Record` therefore retains an opaque **complete draft** and a separate stored
timestamp. `Encoding` interprets ID, activity/server and column-time conversion;
it is not a proof of serialization, UUID generation, target URI construction,
schema admission or cryptography. Identical full-record retry is idempotent;
conflicting reuse is refused. Exact *new* multiplicity needs explicit distinct,
noncolliding record IDs, not a global freshness axiom.

PostgreSQL transaction atomicity, the shared instance/deployment row-lock
interpretation and Core's append/identity behavior are trusted native contracts.
The audit overlay is private until the same server decision applies both it and
the old staged data. Confirmed abort exposes neither; private staging may remain
in the ghost trace. A service error or lost reply is not evidence of rollback.
Foreign audit appends are permitted only at IDs disjoint from active private
records (ready/committing/abort-pending). Retired ghost records do not reserve IDs.
Retention/deletion and conflicting foreign writers are excluded.
No audit sequence/sealer order is identified with wall-clock commit order.

The full instance payload remains parametric. Ordinary Factory/reissue/readiness
writers are outside this two-writer domain, not hidden in the disjoint frame.
The first success cut is the **store return/commit cut**. It does not establish
complete `ClientInformation`, listener convergence or that a candidate still
exists after legal concurrent removal. External environment preparation and
bootstrap role/schema effects retain the existing lifetime boundary.

## Contracts and semantic control

The independent specification counts activity/server-target **bags**, retaining
repeated server targets, and preserves full prior/unrelated audit records. It is
not equality to the executable cursor fold. The checked populated replacement
uses two distinct old clients/logins at old server A and a candidate at B: two
Replace-A records and one Register-B record. Its eighteen-step execution and
independent complete database/ledger outputs are proved, including preserved
unrelated state and released locks. The seed is explicitly a valid store-model
input; native creation of that two-row seed is not claimed.

The planned first postcommit example is a confirmed **no-write served-marker refusal**
after store commit with no external lifetime writer in that suffix. Successful
`served_at` writes and full successful responses are pending: they cannot erase
to the old frame-only environmental transition, which preserves the whole
instance.

The [checked semantic control](evidence/audited-store-register-control.json)
removes only the final Register instruction in a private model copy. Its actual
execution and complete wrong ledger are independently proved: both Replace-A
records and the prior unrelated record remain, while Register-B is absent; the
complete replacement data, lock release and successful store tag are unchanged.
The unchanged acceptance proof rejects the missing full record and the shorter
cursor. Exact source reversal and a fresh affected compilation graph pass. This
tests the proof's sensitivity to a model fault, not a new native bug.

## Checked foundation and remaining qualification

The three new nondefault targets are `RegistrationAuditedLifetimeModel`,
`RegistrationAuditedLifetimeSpecification` and `RegistrationAuditedLifetimeProof`.
The proof leaf has generic Core identity/refusal, private append
progress, source binding, actual lifetime erasure, complete stored decision with
an independent bag, exact full-record extension, and abort/disjoint-frame claims.
All compile, including thirty declaration guards and eleven printed axiom closures.
Guards allow only `propext`, `Classical.choice` and `Quot.sound`. The principal
stored-decision theorem starts from a reached committing transaction with an
exhausted cursor and scoped fresh record identities; it constructs the combined
decision and establishes the independent contract. The separate
[replacement witness receipt](evidence/audited-store-replacement.json) qualifies
one concrete populated execution; fourteen declaration guards and seven axiom
closures pass without importing generic correctness proofs. Execution and whole
database proofs now live in `RegistrationAuditedLifetimeWitnessExecution`; the
independent ledger and combined acceptance assertions remain in WitnessProof.
Their theorem statements are unchanged. The private wrong-output diagnostic adds
five guards and three closures. Further operation cases remain unfinished.

Manual rechecks use the three named targets, with a 180-second process-group
limit per command:

```sh
lake build RegistrationAuditedLifetimeModel
lake build RegistrationAuditedLifetimeSpecification
lake build RegistrationAuditedLifetimeProof
```

Only byte-identical previously checked imports may be reused. The populated case
has its own nondefault targets, `RegistrationAuditedLifetimeWitnesses`,
`RegistrationAuditedLifetimeWitnessExecution` and
`RegistrationAuditedLifetimeWitnessProof`. This is not a replay of the sixteen
default targets or a new native full check. The control receipt retains the exact
private patch and diagnostic, source/tool identities, commands, failures and
restoration results; its launcher and compiled artifacts remain temporary.
The removal recovery layer below now qualifies partial-append failure, confirmed
rollback and retry. Served-refusal examples and the remaining operation adapters
are still required before this audited slice can be called complete.

## Checked removal recovery layer

`RegistrationAuditedLifetimeRecoveryWitnesses` and its proof leaf extend the
synthetic two-row seed with actor 1's partial removal audit, confirmed abort, and
actor 2's new removal attempt. No existing model, specification or primitive is
changed. The qualified native refusal fixture removes one registration; this
two-row partial-append schedule is additional model coverage, not a claim that
the native fixture executed it.

Native `store.rs` 405–448 stages the row/login/instance deletion, then calls
`audit_ended` before commit. Its loop at 703–730 propagates each append error on
the same caller transaction. The fixture's later record conflicts with committed
ID 900 in both complete draft and stored time, matching pinned Core revision
`422dcf268d690fcbc3fee435cf9b81e7f36ffeac`'s
`0500_audit.sql` 407–430 refusal contract. CoreAppend refusal is proved in the
checked leaf; converting that observed native error into `Lifetime.fail`, then
confirmed `serverAbort`, remains explicit Core/SQLx source correspondence.
Absence of a successful append is not a failed-call event or automatic rollback.

The lost-reply branch forks at retry commit-pending: an error reply can precede
the actual server commit, retaining committed data and audit. It is not a retry
after an unknown outcome, a precommit append error, or a served-marker operation.
Both branches retain exact synthetic error strings, full prior/unrelated state,
and the existing store-response-tag boundary.

The [recovery receipt](evidence/audited-store-recovery.json) records the successful
Fixture and Proof commands, four earlier authoring failures, input/output hashes
and interpretation assumptions. Nineteen declaration guards and seven printed
closures pass; only `propext` and `Quot.sound` occur, and the explicit append
conflict theorem has no axioms. Each complete path constructs 24 modeled steps;
this is not a measurement of native calls. No generic audited correctness leaf
is imported. The independent expected database and ledger are full equalities,
including prior record 900 and unrelated state, rather than only record counts.

Run manually with the existing pinned package:

```sh
LAKE_NO_UPDATE=1 lake build RegistrationAuditedLifetimeRecoveryWitnesses
LAKE_NO_UPDATE=1 lake build RegistrationAuditedLifetimeRecoveryProof
```

The final Proof command passed in 21.754 seconds under a 180-second process bound.
Three declarations scope `maxHeartbeats` to 800000; the rest retain the default
200000. The final repair uses restricted projection rewriting without changing
the scenario, specifications or public theorem statements. This adds no runtime
runner, automatic proof trigger or production check. Postcommit service behavior,
full native response payloads and other registration operation adapters remain open.
