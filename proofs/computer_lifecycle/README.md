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

Two [semantic controls](evidence/semantic-controls.json) now qualify sensitivity
to a missing operation-ID check and an omitted listener attempt. Each independent
diagnostic proves the full wrong outcome; the original proof rejects it and passes
after exact restoration. The receipt retains both one-line faults, diagnostic
sources and temporary target additions for manual reproduction in private copies.
The committed model and proof assertions remain unchanged.

Read [SOURCE.md](SOURCE.md) for the correspondence and assumptions. The outer
timeout scheduler, native cancellation, browser recovery and credential custody
remain open. The [browser-membership extraction probe](evidence/browser-membership-translation.json)
retains actual current catalog and contributed-authority paths in Charon, but
Aeneas cannot translate their dynamic dispatch and borrowed group iterator.
Its partial Lean output is not adopted. The independent rule-selection calculation
[translates successfully](evidence/work-context-membership-extraction.json); its
additive shared graph is integrated, and the
[membership proof](../current_roles/evidence/work-context-membership-proof.json)
checks the exact highest matching level and absence, with valid client-only grants.
[Four shared-package regression commands](../request_composition/evidence/shared-model-regression-20261101.json)
also pass within their original source scopes. Both additional membership semantic
controls remain unqualified/deferred: the first diagnostic failed Lean proof
authoring and the second control was not run. The
[closeout receipt](../current_roles/evidence/work-context-membership-closeout.json)
records exact private baseline restoration and the original audit's passing rerun.
Full browser admission and custody remain open, and the broad verification roadmap
is paused pending better Rust/Aeneas support.
