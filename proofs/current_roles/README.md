# Current-role permission evaluation (manual)

This is a production-linked, scoped increment of HUB-AUTH-01. It extracts the
unchanged evaluator from Hub `b9b6cec1b9c36a2e84afb980e208d2d116a84185`, with SDK pin
`05273d1f90b9a08e90e377187f4eddd084605fe5`. The only Rust additions are native
`cfg(test)` witnesses in [grants.rs](../../crates/veoveo-hub/src/policy/grants.rs).
Runtime code, permissions, native collections and dependency pins are unchanged.
Verification remains manual; there is no CI hook, runner or new authorization gate.

## Exact checked boundary

The generated definitions are the actual production `held_roles`,
`effective_risks`, `granted_permissions`, and `InstallationRole::allows`, including
all their grant/filter/map/any predicates. The following specifications establish
exact `.ok` output/totality in the model, not merely conditional safety:

- [HeldRoleProof](HeldRoleProof.lean): a role name is returned iff a raw grant to
  that name matches the supplied Principal's tenant and direct ID/group. An
  unscoped grant accepts an absent tenant; a scoped grant does not.
- [EffectiveRiskProof](EffectiveRiskProof.lean): `None` iff a matching Deny occurs;
  otherwise `Some` contains declared risks union every matching AddRisks. Other
  names, duplicates and contradictory raw overrides are handled without assuming
  configuration validation.
- [NativeSetProof](NativeSetProof.lean): `allows` is exactly access membership AND
  full risk subset in the same role. Equality/order/clone and extensional native
  set-model laws are derived for concrete enum/tagged-ID models.
- [GrantedPermissionsProof](GrantedPermissionsProof.lean): returned membership iff
  ONE raw permission occurrence has the requested name, no matching deny, and ONE
  raw configured held role supplies its access AND every declared/added risk.
  Duplicate role names do not merge access/risk sets; duplicate permission names
  are existential occurrences. Empty inputs are legal. Both acceptance and
  absence/refusal follow, so always-deny is not a satisfying implementation.

[IteratorProof](IteratorProof.lean), [CollectionProof](CollectionProof.lean) and
[PolicyPrimitives](PolicyPrimitives.lean) derive finite raw traversal, actual FnMut
state preservation, filter/map behavior, any short circuit and collection output.
The existing Aeneas Vec collector is reused. Its machine-length guard succeeds
from the ORIGINAL roles Vec's bound and a proved filter-length bound, not an
assumption that the computed selected list fits. Initial closure/cursor validity
is constructed for the actual inputs; no vacuous validity predicate or opaque
computed-policy flag is used.

Whole-program normalizations use existing monad equations plus kernel-checked
definitional equality to the actual generated body. They do not substitute a
second policy algorithm. `Types.lean` and `Funs.lean` are untouched generated
outputs under [HubCurrentRoles](HubCurrentRoles/).

## External models and trust

The handwritten [TypesExternal](HubCurrentRoles/TypesExternal.lean) and
[FunsExternal](HubCurrentRoles/FunsExternal.lean) fill all 57 documented external
placeholders with ordinary definitions. IDs have distinct tags and arbitrary,
unbounded String values. Risk/Access use their actual generated variants. Native
set operations are finite mathematical models invoking supplied comparison,
borrow and clone operations; iterator models invoke actual extracted closures
and retain returned FnMut state. Unbounded scans use Aeneas Result/divergence,
never arbitrary fuel. Final total specifications exclude failure/divergence under
these models.

Primitive laws do not assume the grant result. Concrete model equality/order and
clone laws are proved, while refinement to Rust's sealed IDs, lawful native total
ordering/Borrow/Clone, BTreeSet traversal/memory representation, allocation and
standard-library implementation remains a stated trust boundary. Arbitrary user
Ord/Borrow/Clone implementations are not declared lawful. Finset value equality
is extensional; it does not prove native aliases, sorted-memory or allocation
traces. Unused DateTime/map metadata models are not serialization/identity proofs.
No runtime performance or zero-overhead benchmark is claimed; Rust is unchanged.

Charon/Aeneas translation and the compiler remain trusted. Lean's kernel checks
the proof terms. Accepted axiom closures must contain only `propext`,
`Classical.choice`, `Quot.sound`; no `sorryAx` or custom unproved facts.

The supplied signed Principal, its native group projection/freshness, current
catalog/storage, profile/principal/scope admission, registration declarations,
`CurrentPermissions` registration flat_map/request-filter/collect and
`decide_server` output assembly remain outside this formal boundary. This does
not yet instantiate the authority package's whole current-permissions adapter.
The separate SDK route proof at `239622d` is not adopted by the compiled `05273d1`
dependency. An Authenticated route can legitimately have an empty permission set;
this evaluator proof introduces no contrary runtime condition.

## Qualification

The [compact receipt](evidence/current-roles-receipt.json) records source/tool/
model/proof identities, commands, stage timings, log digests and exclusions. The
baseline passed 12 native grant tests plus one config test. Fresh extraction
regenerated byte-identical Types/Funs; generated-model compilation, every default
proof target, direct grant proof and axiom audit passed. Seven isolated legal
mutants then compiled/extracted and built their models, failed the mapped primary
semantic statement, and failed the matching native assertion (exit 101).

A dedicated fresh-target restoration passed the same 12+1 native tests, exact
model hashes and all proof/audit stages. Baseline/restored native grant commands
took 191.208/192.311 seconds including compilation; tests themselves reported
0.00s. Fresh Charons took 194.861/194.261 seconds, Aeneas 1.018/1.068 seconds, and
the default proof builds 13.787/13.687 seconds. These are tool-stage timings, not
request latency or a native performance benchmark. Accepted axiom sets contain
only the stated standard Lean axioms; no unfinished/custom axioms are accepted.

This is source/model and focused native qualification. No whole-Hub Mac/Linux
Docker gates, deployment or model-driven acceptance were run for this slice.
Raw logs, LLBC and temporary snapshots remain in the workspace cache. Frozen
input hashes in the receipt remain historical when this README or final artifact
filenames change after qualification.

Seven legal controls and native witnesses are reviewed in the matrix below.
Each control must compile/extract, compile its generated model, then fail the
unchanged PRIMARY semantic statement and a named actual Rust assertion. A merely
nonzero exit or dependent import failure is not accepted evidence.

| Patch | Primary statement | Native test (`policy::grants::tests::…`) |
|---|---|---|
| [tenant-different](mutations/tenant-different.patch) | `HeldRoleProof.tenant_closure_exact` | `tenant_scope_preserves_missing_and_matching_tenants` |
| [subject-inverted](mutations/subject-inverted.patch) | `HeldRoleProof.subject_closure_exact` | `subjects_match_only_direct_id_or_group_membership` |
| [held-role-inverted](mutations/held-role-inverted.patch) | `GrantedPermissionsProof.selection_closure_exact` | `unheld_or_unconfigured_roles_do_not_grant` |
| [deny-retains-risks](mutations/deny-retains-risks.patch) | `EffectiveRiskProof.effective_loop_correct` | `matching_deny_wins_in_any_position` |
| [omit-risk-additions](mutations/omit-risk-additions.patch) | `EffectiveRiskProof.effective_loop_correct` | `overrides_accumulate_all_matching_risks` |
| [risk-or-access](mutations/risk-or-access.patch) | `NativeSetProof.role_allows_exact` | `one_configured_role_must_cover_access_and_all_risks` |
| [invert-role-acceptance](mutations/invert-role-acceptance.patch) | `GrantedPermissionsProof.allows_closure_exact` | `valid_permission_occurrences_and_empty_inputs` |

The deny/omission artifacts use false reflexive-name inequality guards to keep
the original translated Option loop/helper structure and native operations
reachable. They deliberately return Some for a deny or omit matching additions.
These guards exist solely in isolated negative-control artifacts; production is
unchanged. The initial tenant `!=`, direct deny-Some and accumulator-replacement
attempts were not accepted: they introduced proof normalization or translated
interface failures. Their original hashes/logs remain in the receipt/cache, not
as additional qualified controls. Baseline proofs were never edited to make
faulty programs pass.

## Reproduce manually

Pins: Aeneas `fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf`, Charon
`de8736f7c995ac54dc490345cef9ea5143c06c62`, Rust `nightly-2026-09-17`, Lean
`4.35.0`. [lake-manifest.json](lake-manifest.json) pins transitive source inputs.
Use those executables/toolchain; no modifications to Aeneas/Core libraries.

From the Hub root, choose a scratch directory and extract all four actual bodies:

```sh
ROLE_PROOF_OUT="$(mktemp -d)"
SQLX_OFFLINE=true RUSTUP_TOOLCHAIN=nightly-2026-09-17 charon cargo --sysroot default --preset=aeneas \
  --start-from 'veoveo_hub::policy::grants::held_roles' \
  --start-from 'veoveo_hub::policy::grants::effective_risks' \
  --start-from 'veoveo_hub::policy::grants::granted_permissions' \
  --start-from 'veoveo_hub::control_plane::authorization_config::_::allows' \
  --include 'veoveo_hub::policy::grants::held_roles' \
  --include 'veoveo_hub::policy::grants::effective_risks' \
  --include 'veoveo_hub::policy::grants::granted_permissions' \
  --include 'veoveo_hub::control_plane::authorization_config::_::allows' \
  --dest-file "$ROLE_PROOF_OUT/hub_current_roles.llbc" \
  -- --manifest-path crates/veoveo-hub/Cargo.toml --lib --locked
aeneas -backend lean -filter-trait-methods -split-files \
  -dest "$ROLE_PROOF_OUT" "$ROLE_PROOF_OUT/hub_current_roles.llbc"
```

There are no additional `--opaque` selectors; native external operations retain
these documented model boundaries. `--sysroot default` avoids the observed
Miri-sysroot crc-fast link failure. The supported `-filter-trait-methods` option
removes unsupported unused Iterator fields, preserving all 43 policy/helper/
closure declaration blocks. Split output separates primitive models from policy.

Compare generated `Types.lean`/`Funs.lean` with the recorded source/model hashes,
then copy only those two files into `proofs/current_roles/HubCurrentRoles/`.
Keep external templates in scratch as references; do not overwrite maintained
external models or hand-edit generated policy. Review changed source correspondence
and primitive obligations before accepting changed generated output.

From `proofs/current_roles`, run:

```sh
lake build HubCurrentRoles NativeSetProof IteratorProof CollectionProof PolicyPrimitives \
  HeldRoleProof EffectiveRiskProof GrantedPermissionsProof
lake env lean NativeSetProof.lean
lake env lean HeldRoleProof.lean
lake env lean EffectiveRiskProof.lean
lake env lean GrantedPermissionsProof.lean
lake env lean ModelAudit.lean
```

These files print target axiom closures. Check actual module names, exits and
closure output; successful model compilation alone is not a policy proof. In the
measured Linux x86_64 Rust 1.99.0 environment, from the Hub root (the receipt
records exact compiler/OS/tool identities):

```sh
cargo fmt --all --check
SQLX_OFFLINE=true cargo test --locked -p veoveo-hub --lib policy::grants::tests
SQLX_OFFLINE=true cargo test --locked -p veoveo-hub --lib control_plane::authorization_config::tests
```

Apply one mutation only in an isolated source copy; regenerate its model, compile
that model, check its primary proof, and run the named witness with `-- --exact`.
Use a fresh native target, or explicitly clean/recompile the Hub package, for
restoration. An intermediate shared-target
restore showed no Hub recompilation and prior-mutant assertions and was excluded.
Artifact reuse is a plausible explanation; the precise mtime/fingerprint cause
was not conclusively instrumented. The seven accepted
negative cases each have observed current-case Hub compilation, source/command/
binary-path and assertion logs. Their per-case binary content hashes were not
captured before the shared path was overwritten. The dedicated fresh-target
restoration established the clean outcome. Do not edit proofs to accommodate
mutants. These commands are manual documentation, not a committed runner or gate.
