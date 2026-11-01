# Computer replacement and recovery

This manual Lean model covers the existing Computer replacement protocol: claim
an operation, run native replacement stages, then publish success or failure only
for the current operation. It does not change the Hub or add runtime checks.

The first checkpoint passes on Linux with Lean 4.35.0 and the pinned Mathlib
revision. `ComputerLifecycleProof` has 27 guarded declarations; all eleven printed
axiom closures contain only `propext`, `Classical.choice` and `Quot.sound`.
The final incremental check took 3.620 seconds. See the
[receipt](evidence/replacement-protocol.json) for source, tool and artifact hashes,
the earlier authoring errors and their repairs.

The package separates:

- [Model](ComputerLifecycleModel.lean): source-shaped row effects, SQL delivery,
  acknowledged launch/reread and returned native-stage execution.
- [Specification](ComputerLifecycleSpecification.lean): independent complete row
  outputs, ordered attempt list and table-preservation contracts.
- [Witnesses](ComputerLifecycleWitnesses.lean): populated inputs and expected
  outputs, importable without the correctness theorems.
- [Proof](ComputerLifecycleProof.lean): general claims and complete success,
  failure/retry and stale-operation histories.

The populated histories establish valid success as well as refusal. They preserve
custody, notices, provisioning snapshots and unrelated rows. An obsolete finish or
failure cannot overwrite a newer operation's durable state in this model. A SQL
error reply is explicitly allowed after a committed write; it is not treated as
rollback evidence.

The separate [native regression](evidence/native-stale-completion.json) exercises
four stale writes through real PostgreSQL store methods. It corroborates the
operation-ID predicate; it is not formal refinement of the SQL into this model.

Run manually from this directory, with the pinned dependencies available:

```sh
LAKE_NO_UPDATE=1 lake build ComputerLifecycleProof
```

Use an external 180-second bound. There is no CI/default target or new runner.
Lake manages compiled artifacts; `.lake/` and full qualification logs stay local.
Source, pins, specifications, proofs and compact receipts are committed.

Read [SOURCE.md](SOURCE.md) for the correspondence and assumptions. The outer
timeout scheduler, native cancellation, browser recovery and credential custody
remain open. Semantic fault controls are the next check; none is claimed here.
