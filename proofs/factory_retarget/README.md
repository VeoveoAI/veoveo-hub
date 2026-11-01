# Factory retarget row contract (manual)

This checkpoint proves the complete successful instance row for the manual
Factory retarget model corresponding to Hub `48d7e25`. It is not an Aeneas
translation or a proof of the entire native Factory protocol. The model retains
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

## Manual checks

Lean 4.35.0 and Mathlib `56a005855ea17e0b0958f73ec801ae140b0a1679` are pinned.
No Aeneas dependency or historical instance-name patch is used. Both named targets
are manual; no default/CI target or runtime behavior is added.

```sh
LAKE_NO_UPDATE=1 lake build FactoryRetargetModel
LAKE_NO_UPDATE=1 lake env lean FactoryRetargetProof.lean -o .lake/build/lib/lean/FactoryRetargetProof.olean
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

The full protocol remains partial. Next steps are deriving capacity acceptance
from success, path-sensitive native outputs (keep retains the prior image/provider
state; restart refreshes credentials but does not compare every spec field), exact
failure-prefix and conditional retry connections, and finite serialized histories.
Multi-replica publication ordering, independent linked-follower supersession,
removal interleavings, nonreturn/crash and eventual liveness remain outside this
checkpoint. Runtime/native regression evidence is separate from these model proofs.
