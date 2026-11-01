# Factory lifecycle, retarget and recovery contracts (manual)

These checkpoints prove the complete successful instance row and path-sensitive
modeled outputs for the manual Factory retarget model corresponding to Hub
`48d7e25`. It is not an Aeneas translation or a proof of the entire native
Factory protocol. The model retains
all 21 instance columns, staged SQL/native effects, direct versus recorded errors,
unknown SQL commit outcomes and separately observed RPC results.

`successful_apply_exact` derives the independent complete `successRow` from the
modeled operation. Its premises explicitly bind the initial row/key and runner,
require an existing source capacity-acceptance fact and normal return, and never
assume the desired after-state. Corollaries connect the complete target to the
client projection and composed repository/tracked-ID push selector. Other checked
statements establish unrelated-state and immutable-identity frames for arbitrary
returned errors, the exact stopped-handler row including uncertain commits,
constructed request fields, client-secret digest/query correspondence and the
historical omitted-field mismatch.

The final direct proof check passed in 10.278 seconds. All **24 executable axiom
guards** passed, allowing only `propext`, `Classical.choice` and `Quot.sound`.
These are dependency checks, not 24 execution scenarios. The updated model built
in 6.686 seconds; its exact artifact was reused for the proof retry. The compact
[receipt and source mapping](SOURCE.md#checked-row-checkpoint) retain identities and
the failed authoring attempts. The broader dated record is
[Factory retarget row proof](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-retarget-row-proof.md).

The watcher extension now also checks actual parent/queued-update histories:
repository or tracked-ID retarget preservation and valid same-target acceptance
with the current parent program. Its 23 axiom guards pass in 5.773 seconds.
See the [history contracts and receipt](WATCHER_SOURCE.md#checked-parent-and-queued-update-histories).
Failure/status ordering and scoped deletion now also pass, with ten axiom guards
in 5.473 seconds; see the [status contracts](WATCHER_SOURCE.md#checked-status-fallback-and-deletion-boundaries).
Populated watcher witnesses and semantic controls remain pending; the overall
lifecycle contract is still partial.

## Checked native-output checkpoint

`success_implies_capacity` derives capacity acceptance from normal modeled return;
`successful_apply_complete` therefore needs no separate capacity premise.
`successful_outputs_complete` combines the independent full-row contract with
path-sensitive sandbox observations, provider material, issued creation requests
and unrelated-state frames. Keep preserves the observed sandbox and prior
providers without refresh or creation. Restart refreshes the modeled client
projection, retaining the native observation unless the source phase condition
selects stop/start. Create and replace issue the complete constructed request and
provider material while retaining arbitrary returned native observations.

`changed_present_accepts` constructs successful execution under explicit no-fault
observations. The populated changed-target witness combines that acceptance with
complete outputs, valid identifiers, distinct schema-constrained digests, actual
SHA-256 fixture pairs and exact source-script construction. Native hash/formatter
and schema interpretation remain explicit premises. The witness does not prove
token issuance, remote readiness or an executed checkout. The coordinator independently
checked all four SHA-256 fixture pairs and the source script bytes.

The new leaf passed in **6.945 seconds** with all **14 executable axiom guards**
and eight printed closures allowing only the same three standard axioms. Its
model and prior row-proof artifacts were hash-checked and reused. A finite
800,000-heartbeat limit applies only to `complete_output_algebra`; there is no
global limit change. Failed syntax, tactic and heartbeat attempts are retained in
the [source receipt](SOURCE.md#checked-native-output-checkpoint), rather than
classified as native defects. The dated record is
[Factory native-output proof](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-native-output-proof.md).

## Checked failure-prefix and conditional retry checkpoint

`failed_run_prefix` derives the exact successful prefix, first refused
observation, complete stopped state and ordered event trace from any failed
modeled return. `failed_row_prefix` retains all 21 row fields through successful
writes, uncertain commit of the failed SQL statement and independently committed
failure logging. The original error remains the returned error.

`retarget_precommit_failure` checks the changed-target program's ordering against
its preparation/read/delete/admission prefix. A failure there preserves the old
repository, tracked ID, tracked name and linked pusher. This includes the direct
`runner_applied` query error. Other row fields may have changed, and a deletion
error may already have left the worker absent; no rollback or last-good-worker
claim is made. The same-target path's earlier name/pusher refresh is outside this
old-target preservation statement.

`retry_after_failure` derives an existing Runner and immutable identity from the
actual intermediate state, constructs acceptance from new no-fault and capacity
observations, and obtains the independent full row/native/provider/request output
contract. Both present and absent native observations are allowed, including
retry after uncertain target commit or failed creation. The precommit
specialization additionally preserves the old target and derives that the request
still changes it. Each call supplies its own native-read observation. The generic
failure premise is unused because row/Runner survival holds after every returned
apply; this is a conditional repeated-application result, not automatic recovery,
eventual retry or liveness.

The recovery leaf passed in **14.984 seconds** with **21 executable axiom guards**
and eight printed closures permitting only the three standard axioms. All three
prior artifacts were hash-verified and reused. The finite 600,000-heartbeat limit
is local to `no_fault_capacity_accepts`. Tactic/heartbeat authoring failures are
retained in the [receipt](SOURCE.md#checked-recovery-checkpoint). The dated record
is [Factory recovery proof](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-recovery-proof.md).

## Checked independent lifecycle contracts

`FactoryChoiceProof` imports only the model and proves necessary-and-sufficient
contracts for all four outcomes. The contract predicates independently spell the
linked repository/tracked-ID identity, own-tree/linked-revision content and exact
resources, driver, policy, command and provider comparisons. They call none of
`choice`, `changed`, `runs` or `besideImage`. Creation is exactly native absence;
missing spec/template or changed target forces replacement. Keep takes precedence
when content and compared fields match, even if a rebuilt image differs. Restart
requires the same target, compatible image/fields and changed content. The
replacement contract covers the remaining present-worker cases.

Corollaries show provider permutation preserves the decision while duplicate
multiplicity matters, and that environment, restart policy and unspecified
configuration/template remainders are ignored by the source comparisons. These
are manual model contracts; native sorted-list/`Perm`, URL/UUID, canonical policy
and protobuf equality correspondence remain interpretation boundaries. They do
not establish a concurrent request's freshness or selection under new inputs.

The direct check passed in **5.066 seconds** with **15 executable axiom guards**
and eight printed closures allowing only the standard three axioms. The exact
model artifact was reused; earlier proof leaves are not imports. Three finite
600,000-heartbeat settings apply only to lifecycle iff proofs. The first attempt
failed on nested record layout, corrected without changing its statement; the
[receipt](SOURCE.md#checked-choice-checkpoint) retains that diagnostic. See the
[Factory choice proof record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-choice-proof.md).

## Standalone semantic controls

`FactoryControlFixture` and the two positive control leaves import only the model,
without the generic row, output, recovery or choice proofs. Their populated
execution assertions independently specify all 21 row fields, complete modeled
provider/client/request/native outputs and ordered events. Neither assertion
assumes success. The baseline retarget and exact-capacity assertions passed in
22.710 and 21.644 seconds, respectively, with standard-only axiom audits.

Two isolated **manual-model** faults then produced their intended wrong results.
Omitting the repository/tracked-ID assignments leaves old stored authority while
the new request names the new target. Changing enforced admission from `>` to
`>=` wrongly refuses a native-absent creation at count = limit = 1. In each fresh
mutant graph an independent populated witness first compiled, checking the whole
wrong row/output or refusal; the unchanged corresponding standalone positive
assertion then failed on its expected-result conjunction. Its cascading `sorryAx`
was rejected, and no failed correctness artifact was accepted. These are not
claims that unchanged generic Choice/Recovery theorems were tested under faults.
The historical native retarget failure remains separate implementation evidence.

The false-on-baseline witnesses and exact patches are kept under `mutations/`,
without baseline library targets. To repeat a control, copy its witness to the
root of an isolated scratch package, apply only its model patch, and compile
fresh model/fixture/witness artifacts before the unchanged positive assertion.
Reverse the patch, verify the exact baseline source map and use another fresh
own-artifact graph for restoration. No stale proof artifacts may cross a model
change. Both witnesses and assertions use kernel-checked `decide` with a finite,
theorem-local recursion depth of 4096, not `native_decide` or new axioms.

Both exact patch reversals restored all nine private source files. Each restored
baseline passed fresh model, fixture and both positive assertion stages, with
measured artifact hashes matching the baseline. The final capacity restoration
took 6.699, 3.884, 30.345 and 20.733 seconds across those four stages. The compact
[control receipt](SOURCE.md#standalone-controls-receipt) records timings, identities
and the distinction between setup fixes and semantic rejection. The dated record
is [Factory semantic controls](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-semantic-controls.md).

## Checked current-runner watcher foundation

The [watcher model and source receipt](WATCHER_SOURCE.md) add a manual composition
layer over the unchanged Factory operation. Current SQL selection is related to
the complete backing row; source arguments and Requests are constructed from the
current **applied** parent metadata and native preparation observations.
`relevant_full_output` derives valid acceptance and complete Factory outputs.
Obsolete/absent candidates preserve the whole stored state, and
`capture_independent` makes stale captured parent metadata irrelevant when key and
event target agree. Query/log attempts remain visible even on state no-ops.

Failure storage has its full watched-deployment/parent/branch/link key and complete
timestamped payload, distinct from the mutex/query key. Preparation success,
swallowed Factory error and status publication remain separate channels. Native
SQL consistency, mutex serialization, callback/formatting and existing backing-row
conditions are explicit. This is neither async Rust extraction nor proof of new
allocation, native cleanup, parent-retry freshness or distributed ordering.

The model built in **8.232 seconds**; the foundation proof passed in **9.383
seconds** with **18 axiom guards** (14 lemmas and four definitions) and seven
printed standard-only closures. Six exact accepted prerequisites were reused.
First-attempt constructor and tactic errors are retained in the receipt. No native
tests were repeated. The subsequent history checkpoint above adds parent/queued
retarget and program-advance compositions and unrelated-key failure framing.
The subsequent status checkpoint composes preparation refusal with parent
success, proves fallback/error distinctions and frames completed row deletion.
Populated client-facing witnesses and two selected semantic controls remain open.

## Manual checks

Lean 4.35.0 and Mathlib `56a005855ea17e0b0958f73ec801ae140b0a1679` are pinned.
No Aeneas dependency or historical instance-name patch is used. All twelve named
targets are manual; no default/CI target or runtime behavior is added.

```sh
LAKE_NO_UPDATE=1 lake build FactoryRetargetModel
LAKE_NO_UPDATE=1 lake env lean FactoryRetargetProof.lean -o .lake/build/lib/lean/FactoryRetargetProof.olean
LAKE_NO_UPDATE=1 lake env lean FactoryNativeOutputProof.lean -o .lake/build/lib/lean/FactoryNativeOutputProof.olean
LAKE_NO_UPDATE=1 lake env lean FactoryRecoveryProof.lean -o .lake/build/lib/lean/FactoryRecoveryProof.olean
LAKE_NO_UPDATE=1 lake env lean FactoryChoiceProof.lean -o .lake/build/lib/lean/FactoryChoiceProof.olean
LAKE_NO_UPDATE=1 lake env lean FactoryControlFixture.lean -o .lake/build/lib/lean/FactoryControlFixture.olean
LAKE_NO_UPDATE=1 lake env lean FactoryRetargetControl.lean -o .lake/build/lib/lean/FactoryRetargetControl.olean
LAKE_NO_UPDATE=1 lake env lean FactoryCapacityControl.lean -o .lake/build/lib/lean/FactoryCapacityControl.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherModel.lean -o .lake/build/lib/lean/FactoryWatcherModel.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherProof.lean -o .lake/build/lib/lean/FactoryWatcherProof.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherHistoryProof.lean -o .lake/build/lib/lean/FactoryWatcherHistoryProof.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherStatusProof.lean -o .lake/build/lib/lean/FactoryWatcherStatusProof.olean
```

## Interpretation boundaries and remaining work

SQL atomic committed-or-not statements, caller serialization and an existing row
without concurrent removal are explicit interpretation conditions. External
registration/served-at writes are excluded from the sequential operation. Native
URL/UUID formatting, raw-token/digest hashing, protobuf/default/equality values,
policy/template/provider preparation and RPC effects remain source/native
correspondence boundaries. `Prepared.providers` and `ungranted` are observations
of the actual active-catalog helper, not proved provider filtering. No global
UUID, truncated-name or secret freshness assumption is introduced.

The direct `runner_applied` query/parse failure follows `readSandbox`. The native
read value is observed at that call after preparation, not assumed stable through
the image build. Refresh material retains the client ID, raw custody secret,
credential key, workspace/provider and strategy; the stored digest is separate.
The client-query corollary uses the explicit native hash relation and proves no
complete issuer/token flow.

Constructed creation requests retain name, workspace, environment, provider
prefixes, argv, image/policy/shape, restart policy and remaining native defaults.
Submission, remote receipt, readiness, workspace cleanup and an executed checkout
are different claims. The script requests `branch@LATEST`, not a measured checkout
of the recorded applied revision. RPC success does not force a specific observed
phase/spec. Native maps use complete deployment keys; physical unrelated-object
framing additionally needs distinct scoped `(workspace, sandboxName)` keys because
the native name truncates UUID text.

The full protocol remains partial. Independent lifecycle contracts are now checked
alongside the full output and conditional retry contracts. Two populated manual
controls now detect specific wrong-output and wrongful-refusal faults. Further
work includes populated watcher histories and semantic controls;
the earlier native concurrency qualification is recorded separately. These
controls are not exhaustive fault coverage. Modeled provider fields do not verify entire native
Provider protobuf objects; their profiles, defaults and native interpretation
remain external contracts.
Multi-replica publication ordering, independent linked-follower supersession,
removal interleavings, nonreturn/crash and eventual liveness remain outside this
checkpoint. Runtime/native regression evidence is separate from these model proofs.
