# Instance registration/removal: manual source correspondence

Native source is qualified at Hub
`12a132dcbdddcd7fad1defcd007432385d237a76`; see the
[native receipt](evidence/native-instance-removal.json) and
[qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-11-01-registration-instance-removal-fix.md).
Those component/full-check results are separate from this handwritten Lean model.

Private Model, Specification and Proof checks passed in 4.422, 4.020 and 10.932
seconds respectively. Proof passed 30 declaration axiom guards and nine printed
closures, using only `propext`, `Classical.choice` and `Quot.sound`. These are
declarations, not execution scenarios. The canonical named-target build also passes
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
A constructive historical orphan execution remains a following obligation.

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
conditions. `SQLValid` and `NoOrphanRows` are defined for the following history slice;
this foundation's reached-removal theorem does not assume either of them or
establish their general preservation.

The login map excludes shared server/group/bootstrap owner state. The disjoint frame
excludes PostgreSQL catalogs, NOTIFY, external databases and transport. Optional
external preparation is a logical success/error observation, not a verified executor;
its role/schema work is not atomic with installation rollback.

Next: optional-deployment exact register effects; independent mixed commit histories
with explicit frame interference or exact owned projection; no-orphan/abort
consequences; constructive acceptance/removal-first/rollback/original executions;
complete response payloads and semantic controls. Current commit-only `SerialHistory`
is not yet an all-executions theorem with `Step.frame`.

Namespace writers, token reissue, RFC 7592 updates, housekeeping, allocation/recreation,
ID reuse, other normal row writers, distributed ordering, listener convergence,
HTTP cancellation, native sandbox cleanup and liveness are outside this checkpoint.
Old register/replace/update proofs retain their recorded retained-deployment scope.

## Manual qualification

All sixteen existing default targets and pins remain unchanged. Three new libraries
are nondefault manual targets:

```sh
LAKE_NO_UPDATE=1 lake build RegistrationLifetimeModel RegistrationLifetimeSpecification RegistrationLifetimeProof
```

Only actual imported old Model/Specification/Effects dependencies are required; the
private check did not replay all sixteen old targets. The canonical context check
preserved source/import/tool identities. Earlier record-layout, erased-type and proof-authoring
failures are retained separately; none is native defect evidence or accepted sorry.
