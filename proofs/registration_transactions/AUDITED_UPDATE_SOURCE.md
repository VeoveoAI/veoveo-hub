# Audited UPDATE store adapter

The foundation **passes the manual Lean checks**; populated execution witnesses
and the omitted-Update control remain pending. Native source basis is
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
  uses a BEFORE INSERT exception. A future model ID-conflict witness is a different
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
A later replacement/noop example must explicitly provide a faithful full external
contribution and name its existing-namespace/no-new-claim boundary.

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

This foundation does not yet include populated actual acceptance, refusal/abort/
retry or replacement/noop executions. Those are required next with independently
specified full outputs, explicit external audit provenance and ordinary successful
acknowledgements. A later omission control will remove only the Update instruction
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
