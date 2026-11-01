# Instance registration/removal: manual source correspondence

Native source is qualified at Hub
`12a132dcbdddcd7fad1defcd007432385d237a76`; see the
[native receipt](evidence/native-instance-removal.json) and
[qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-instance-removal-fix.md).
Those component/full-check results are separate from this handwritten Lean model.

Initial foundation Model, Specification and Proof checks passed in 4.422, 4.020 and 10.932
seconds respectively. Proof passed 30 declaration axiom guards and nine printed
closures, using only `propext`, `Classical.choice` and `Quot.sound`. These are
declarations, not execution scenarios. That canonical named-target build also passed
in 22.08 seconds; its [receipt](evidence/instance-lifetime-foundation.json) records
the actual source graph, artifacts and earlier failed drafts. This is not full
lifecycle verification or Rust/SQL extraction.

## Native mapping and pins

Runtime pins are Core `422dcf268d690fcbc3fee435cf9b81e7f36ffeac` and SDK
`7360c0f96cb616c519d27256df214ff4f2e6e831`. The existing package keeps Lean 4.35.0,
Aeneas support `fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf` and Mathlib
`56a005855ea17e0b0958f73ec801ae140b0a1679`; no translator/backend changes are used.

- [service.rs][service]: earlier token lookup supplies cached authority; a later
  absent locked instance maps to the existing invalid-token response. Native
  signature, slug, deployment and token admission remain input contracts.
- [store.rs][store] and [lock_instance.sql][lock]: registration and removal acquire
  the same instance row before admission or DELETE snapshot, retaining it through
  the server commit/abort decision. The first deployment row may not yet exist.
- Store SQL `replace_deployment`, `replace_registrations`, `drop_login`,
  `ensure_server`, `create_login`, `create_registration` and `delete_instance` maps
  to ordered private upsert, captured key deletion, role drops, admission and removal.
  Full captured rows are ghost snapshot witnesses; native RETURNING selects keys.
- Migration 0007 makes registration reference retained deployment, not instance;
  0015 permits first registration before a deployment row exists.

[service]: https://github.com/VeoveoAI/veoveo-hub/blob/12a132dcbdddcd7fad1defcd007432385d237a76/crates/veoveo-hub/src/registration/service.rs
[store]: https://github.com/VeoveoAI/veoveo-hub/blob/12a132dcbdddcd7fad1defcd007432385d237a76/crates/veoveo-hub/src/registration/store.rs
[lock]: https://github.com/VeoveoAI/veoveo-hub/blob/12a132dcbdddcd7fad1defcd007432385d237a76/crates/veoveo-hub/src/registration/store/lock_instance.sql

[PostgreSQL 18 READ COMMITTED locking](https://www.postgresql.org/docs/18/transaction-iso.html#XACT-READ-COMMITTED)
is an interpretation assumption: wait for the changer, return no row after committed
deletion, or proceed on the original row after rollback. PostgreSQL/SQLx/compiler
and async refinement are not proved here.

## Represented domain and checked foundation

Only instance-authorized registration and removal write one selected lifetime.
Complete instance `I`, metadata/binding `M`, immutable deployment remainder `D`,
registration payloads and roles remain parametric. Deployment is genuinely optional,
without dummy metadata. Upsert retains existing immutable fields and uses original
native request inputs for first insertion. Cached authority cannot replace the
current locking read.

Original-protocol comparison retains registration's actual deployment-upsert lock
and removal's actual later DELETE-instance lock. INSERT validity has native
client/login/deployment-FK constraints, deliberately no instance-existence guard.
Primitive commit applies staged deltas without a desired no-orphan condition.
The constructive witnesses below reproduce that historical orphan through actual
primitive steps, retaining both original locks.

Independent `replacement`, `removal` and `sequential` specify complete owned effects
without replaying staged writes. The checked foundation derives shared ownership,
unique active owner, ordered staging/drop progress, current instance/DELETE
observations and complete programs at reached commit requests.
`reached_removal_effect` derives the full current database effect equal to independent
`removal`, and actual current instance presence. Desired intermediate/final states
are not premises. It frames complete deployment/unrelated roles/application values.

Response and server decision are distinct: an error reply may accompany commit and
never establishes rollback; pending-response guards prevent rewriting acknowledged
success. Responses are TAGS only. Complete returned database, credentials, metadata,
trust/endpoints and postcommit audit/reload response projections remain unproved.

## Boundaries and next work

Full-instance stability is scoped to these two writers. Normal Factory revision,
pusher, served/failure updates are excluded too. Typed SQL/serialization and native
identity/signature/digest/role correspondence remain explicit interpretation
conditions. The foundation's reached-removal theorem assumes neither `SQLValid`
nor `NoOrphanRows`. The history extension below preserves the latter from its
explicit initial condition; SQLValid preservation remains separate.

The login map excludes shared server/group/bootstrap owner state. The disjoint frame
excludes PostgreSQL catalogs, NOTIFY, external databases and transport. Optional
external preparation is a logical success/error observation, not a verified executor;
its role/schema work is not atomic with installation rollback.

The [history extension](evidence/instance-lifetime-history.json) adds exact
optional-deployment registration effects and preserves each attempt's original
request. `SerialHistory` now has an explicit frame-only environment transition;
`execution_serial` establishes its full current database and actual commit order
for every finite fixed execution. `execution_no_orphan` preserves the explicit
initial no-orphan invariant. Commit events imply terminal committed phase, so a
confirmed aborted attempt contributes no event; a response error is insufficient.
The extended graph's canonical History target passes in 22.66 seconds, including
fresh qualification of the changed Specification and dependent foundation Proof.

The [constructive witnesses](evidence/instance-lifetime-witnesses.json) establish
six concrete executions and independently written complete model outputs: the
original orphan, fixed registration then removal, removal before admission despite
cached authority, aborted removal then successful registration, lost reply after
commit, and replacement of an existing registration. The replacement preserves
the old immutable deployment value while removing its prior row and login. Each
case specifies stored state, response tags, released locks and actual commit order.
These witnesses use synthetic inputs; they do not prove native input validation.

Their execution leaf imports only Model and Specification. A separate consequence
leaf then imports History: the original concrete execution cannot satisfy the same
independent serial-history contract, while the fixed execution does. Generic
correctness is not used to manufacture either execution's existence. Thirty-six
new declaration guards and eight printed closures pass.

The [semantic controls](evidence/instance-lifetime-controls.json) now qualify two
deliberate model faults against those unchanged expected outputs. Ignoring the
primitive login-drop effect leaves the candidate login after removal and the old
login after replacement. Returning `invalidToken` from a successful registration
acknowledgement preserves the committed database but violates the expected reply.
Independent complete wrong-output diagnostics pass; the unchanged acceptance
proof fails at the corresponding login-map or response equalities. Both exact
patch reversals restore the full source maps and fresh positive proof graphs.
These are deliberate model faults, not additional native defects or SQL faults.

The scoped store-model qualification includes its original-race witness, valid
executions and semantic controls. Next are complete native response payloads and
postcommit service effects, followed by broader lifecycle composition. At the
pinned native revision `12a132d`, store commit precedes service audits, catalog
reloads, `served_at` and response construction;
an error in those steps cannot be interpreted as rollback. SQLValid preservation
and native SQL refinement remain separate obligations.

One [unchanged-source extraction probe](evidence/information-extraction-boundary.json)
of `RegistrationService::information` reached its real URI/surface construction,
SDK response validation and endpoint-to-resource conversion. Charon and strict
own-body/type/field inventory passed, but the pinned Aeneas backend failed on
nested-loop returns, dynamic/arrow types and string/iterator lifetime constructs.
No Lean response model was emitted. Production code and own helper transparency
were unchanged; no additional response proof follows from that extraction attempt.
Native response/postcommit and audit contracts are the next source-review boundary.

The subsequent [native audit correction](evidence/native-audit-fix.json) moves
registration audits into their owning transactions and records intentional
instance removal. Fault/rollback/retry probes and full Mac/Linux checks pass.
Served markers, reloads and response construction remain postcommit. The seven
checked lifetime modules are unchanged; their earlier model does not thereby
acquire audit coverage. A separate audited store extension must establish its
projection and combined contract before claiming applicability to those effects.
Complete service payloads, successful served writes and liveness remain separate.

Namespace writers, token reissue, RFC 7592 updates, housekeeping, allocation/recreation,
ID reuse, other normal row writers, distributed ordering, listener convergence,
HTTP cancellation, native sandbox cleanup and liveness are outside this checkpoint.
Old register/replace/update proofs retain their recorded retained-deployment scope.

## Manual qualification

All sixteen existing default targets and pins remain unchanged. Seven new libraries
are nondefault manual targets:

```sh
LAKE_NO_UPDATE=1 lake build RegistrationLifetimeModel RegistrationLifetimeSpecification RegistrationLifetimeProof
LAKE_NO_UPDATE=1 lake build RegistrationLifetimeHistoryProof
LAKE_NO_UPDATE=1 lake build RegistrationLifetimeWitnessConsequences
```

Only actual imported old Model/Specification/Effects dependencies are required; the
private check did not replay all sixteen old targets. The canonical context check
preserved source/import/tool identities. Earlier record-layout, erased-type and proof-authoring
failures are retained separately; none is native defect evidence or accepted sorry.

The two `mutations/*.patch` files and their diagnostic `.lean` files are manual
control inputs, not baseline targets. In a disposable copy of this package, apply
one patch with `patch -p1`, copy its diagnostic into the package root, and freshly
compile Model, Specification, Witnesses and that diagnostic before checking the
unchanged WitnessProof. The diagnostic must pass and WitnessProof must fail at
the intended independent output assertion. Reverse that exact patch, discard all
dependent compiler artifacts, and freshly compile the positive graph again.
The receipt records the qualified commands, timings, source maps and failures;
no execution script or automatic check was added to the repository.
