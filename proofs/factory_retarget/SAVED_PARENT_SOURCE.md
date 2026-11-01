# Saved-parent preparation and guarded publication

Status: the model, foundation, composed update/retry histories, independent
populated examples and both selected semantic controls/restorations are checked. The
[model checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-model.md)
and [foundation checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-foundation.md),
followed by the [history checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-histories.md)
and [populated checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-fixtures.md),
then the [control checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-factory-saved-parent-controls.md),
keep definition elaboration, theorem checks and native evidence separate. Runtime basis
is Hub `77b258ae10837a01ca7c7464e7c4245c94cd4105`; the package source starts on
coverage main `e1101a7ab5b1f44b69d46592c8067d561cfa65d4`. The prior Factory and
watcher models/proofs are reused byte-identically. This is manual source
correspondence, not Aeneas extraction or proof of native async/SQL execution.

## Two phases and source boundary

`watch.rs::retry` obtains the existing parent watcher, immutable revision's branch
ID and committer. `prepare` reads that revision's source, links and own tree into
a temporary directory outside the parent `(repository, branch, None)` guard.
`FactorySavedParentModel.begin` records those actual source arguments and the
Result-valued observation as an Attempt constructor; it is not itself an execution
or refinement proof of native reads and accepts no stored-state parameter. Native read/parser/tempdir
behavior, successful branch-ID/committer decoding and immutable content relations
remain explicit interpretation contracts.

After acquiring the parent guard, native code checks cancellation. A saved retry
then calls `Lore::branch(watcher, parent, saved ID)` and requires matching ID/name,
a live record and exactly the saved revision as current head. Errors and mismatches
return before handling either source outcome. A matching native observation has
another cancellation check. Normal notification preparation does not perform this
saved-retry validation and does not have the second check. The model retains these
separate observation points; it does not say that later cancellation prevents a
status statement already being executed or committed.

The native branch record is an authoritative observation at this source call,
not derived from hub.instance, a failure row or revision ordering. Native Lore can
move after the observation. Within one `Watches::Inner`, parent effects and removal
share the parent guard; runner guards are nested parent-to-runner, while a linked
continuation releases its runner guard before retrying a parent. The proof models
serialized guarded publication/effects and read/effect interleavings, not Tokio ownership,
cross-replica ordering or a distributed compare-and-swap. Stale work yields to the
existing follower/reconnect recheck; automatic recovery/liveness is not established.

## Exact state and successful-application domain

New state wraps the complete existing WatchState and a preparation-failure map.
Its key is `(watched deployment, parent repository, branch)` and its value retains
revision, pusher, message and `failed_at`. This is the actual prepared.sql DELETE
key and unprepared.sql UPSERT payload. Commit and reply are independent observations:
a successful acknowledgement implies commit at native interpretation; a failed or
lost reply can accompany either atomic state outcome. Link-failure diagnostics
keep their existing complete keys/values and NEVER enter the native-head decision.

Guarded `enter` is total for arbitrary state, including an empty/deleted runner
state. It either finishes a discard/current-source-error path or returns concrete
Ready data after preparation-clear publication. Ready is an internal proof
decomposition, not a new native yield, queue or permission to interleave parent
operations: the same native parent guard remains held through completeExisting.
Only `completeExisting` needs an
actual row binding. `finishExisting` requires that evidence conditionally on enter
returning Ready, so no row premise restricts the broad stale-discard claims.

Successful application in this first layer covers a prepared linked branch's
single existing-runner region. Link identity, pinned/follows source inputs, own
tree/entries and parent revision/pusher construct ParentInput inside the operation;
no correct Request or desired after-state is supplied. The operation invokes the
UNCHANGED Watcher.parentLink/Factory.apply engines and reuses their full-output
proofs. Read/resolve/query/status observations keep all existing contracts.

The ordinary-instance removal and membership-retirement prefix is quiescent only
in the explicit domain where this is the branch's only represented runner and
all failures for this watched parent branch name that link. Other repositories,
branches and watched-deployment diagnostics may be populated. This domain excludes
an ordinary instance or unkept sibling slots; it does not assume a desired result. QuiescentPrefix is a structural before-state
condition on represented rows, UniqueKey and exact failure keys. Native prefix
queries must faithfully observe those rows/keys, and absent ordinary removal must
have no effects. The model omits these prefix reads/attempts; it does not prove
their complete native execution.
Missing-runner allocation, ordinary-instance apply and general membership retirement
remain formal obligations. They are not replaced by arbitrary correct-effect
callbacks. Before a stale retry is discarded, no such prefix runs: whole-state
preservation therefore permits arbitrary other membership and diagnostic state.

Native SQL/schema/ID/crypto/provider/URL/protobuf and native sandbox-name no-alias
conditions remain those of the inherited package. A configured client projection
is not a proof of full token issuance; exact submitted argv uses branch@LATEST,
not proof of checkout bytes, remote receipt/readiness or native cleanup.

## Checked foundation

`FactorySavedParentProof` proves arbitrary-state obsolete/native-error/cancellation
outcomes, exact current source-error publication and unrelated-key preservation.
Its accepted path derives concrete Ready fields from admission and calls the
unchanged child operation. `current_head_full` derives the exact child outcome,
actual Factory result with no error, complete stored/native/provider/request
values, client projection and both status-map effects from external no-fault and
capacity observations. It does not assume a desired Request or successful
after-state, or require a diagnostic row to be present.

These theorems are under the native interpretation and existing-runner successful-
application domain stated above.

## Checked histories

`FactorySavedParentHistoryProof.newer_parent_then_saved` first executes the normal
parent preparation and existing-runner child, deriving its exact child/Factory
outcome, complete output values, client and both status maps. A subsequent saved
attempt observes that newer native head and discards its old source success or
error, preserving the first complete state. With an uncommitted status clear,
every pre-existing full link-failure payload survives too. This theorem does not
create that initial diagnostic or order revision hashes.

`current_error_then_obsolete` composes actual current source-error publication with
an obsolete success/error discard. The new full preparation payload follows the
observed commit even if its acknowledgement is lost. `deleted_name_reuse_rejects`
preserves arbitrary state when the old ID is deleted despite the same name and
head; it does not itself construct a live replacement at that name.

`normal_then_current_duplicate` derives the first successful row, Runner shape and
structural prefix domain from actual child effects. Both committed diagnostic clears
establish absent records. A current-head retry on that resulting state still enters
and derives complete successful child outputs. Neither the second operation's
valid state nor a present failure row is a premise. A second native resolution may
legitimately observe newer linked content; the theorem does not require a no-op.

All seven history declarations pass standard-axiom guards. These remain scoped
manual model compositions under the same native interpretation, not extraction or
formal refinement of async Rust, SQL or external services.

## Checked independent populated sequences

`FactorySavedParentFixture` imports only executable models and public
`FactoryWatcherFixture` data, not the foundation/history correctness leaves. Its
structural Ready helper supplies before-row evidence to the actual composition;
subsequent row presence is proved only for closed actual preceding executions.
Expected B records, client, native spec, providers, submitted request and argv,
and related/unrelated status payloads are independently constructed fixture data.

`FactorySavedParentFixtureProof` checks five sequences: successful B then obsolete
success; B with a noncommitted link-status clear then obsolete success and error;
B then a committed current preparation error with lost reply then obsolete error;
actual new-parent-ID application at the reused name then deleted-old-ID retry; and
success clearing both diagnostic keys followed by accepted current-head retry.
The last preparation builds a different image but keeps the same source tree,
command and requirements; the actual child succeeds while retaining the first
native image/request/providers. Old diagnostics are represented starting data,
not claimed created by these examples. The replacement operates on an existing
row, not a missing-row allocation. External phase observations and checkout-byte
limitations remain unchanged. Eight guards and five closure audits pass.

## Checked semantic controls

`saved_parent_preserves_and_accepts` in `FactorySavedParentControl.lean` requires both
`AcceptedThenOldSuccess` and `ClearedThenValidDuplicate`. It imports only the
executable fixture/models, not the generic foundation/history proofs. The correct
baseline passes in 20.802 s. Two independent faults are checked in private copies:

- `mutations/omit-saved-head-check.patch` removes only the native head comparison.
  ID, name, deletion and cancellation checks remain. The independent
  `FactorySavedParentHeadOmissionWitness` derives actual B success followed by
  successful obsolete A replay: the complete old row/client/native spec/providers,
  both submitted requests/argv and the status maps. The unchanged positive assertion
  is false. The four-stage sequence takes 46.446 s.
- `mutations/require-saved-link-failure.patch` requires a matching link diagnostic
  after successful current-head source preparation. The independent
  `FactorySavedParentDiagnosticRefusalWitness` derives actual first B success and
  both cleared diagnostics, then wrong discard with no child and the complete B
  provisioning state retained. The same positive assertion is false. The four-stage
  sequence takes 44.340 s. Diagnostic absence is not a legitimate new authority rule.

Both wrong-outcome witnesses pass standard-axiom audits. Each check of the positive
assertion under a fault reports `decide` establishing a false proposition and the rejected
`sorryAx`; no positive artifact is emitted. Syntax, imports, missing decidability
or resource failures do not count as detection. No generic correctness artifact is
reused under either fault.

Each exact reverse patch restores the full twelve-file source/config map in the
same private tree. Fresh Model → Fixture → Control checks pass in 26.395 s and
83.066 s respectively; the latter includes the full five-scenario FixtureProof,
which takes 57.075 s. Restored model, fixture, control and full fixture-proof
artifacts match their accepted baseline hashes. The final two positive leaves
pass nine guards and six printed closures: Control one/one, FixtureProof eight/five.

For manual reproduction, use each patch separately in a disposable copy, with the
matching witness copied from `mutations/` to that copy's module root. Rebuild
SavedParentModel and Fixture before the witness and unchanged Control; retain only
the unchanged RetargetModel, WatcherModel and WatcherFixture prerequisites. Reverse
the exact patch, remove all derived artifacts, and rebuild the restored positive
graph. Run FixtureProof after the final restoration. The dated control record has
the complete command order, input/output identities, logs and authoring repair.
These are controls of two chosen handwritten-model faults, not new native defects
or exhaustive fault coverage.

## Contracts for the complete slice

- Whole-state discard for stale source success AND source error, lookup error,
  deletion/name reuse or cancellation, retaining read/query/log trace attempts.
- Valid current-head acceptance independent of link/preparation-failure presence,
  with constructed source arguments and complete Factory row/client/native/provider/
  request outputs from actual parentLink, plus unrelated-key frames.
- Exact current-head error publication/full payload and frame with committed-or-not
  SQL/reply semantics; an obsolete source error cannot overwrite a newer failure.
- Actual newer-parent application followed by old prepared retry preserves all B
  outputs even when the link-status clear failed and the genuine old row persists.
- Deleted old ID at the same head/name cannot overwrite a live replacement slot;
  valid same-head duplicate acceptance is not conditioned on a diagnostic row.
- Independent populated accepted and stale-output witnesses with valid public
  fixture data, full client/request/argv and executable standard-axiom audits.

Theorems assume truthful native snapshots and the stated serialized/no-external-
write domain, not desired Requests or successful after-states. Dated records above
retain exact artifacts and commands for completed checks. Initial allocation,
general membership retirement and native/distributed correspondence remain open.
