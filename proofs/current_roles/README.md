# Current-role permission evaluation (manual)

The qualified current-role increment of HUB-AUTH-01 extracted the unchanged
evaluator from Hub `b9b6cec1b9c36a2e84afb980e208d2d116a84185`, with SDK pin
`05273d1f90b9a08e90e377187f4eddd084605fe5`. That qualification added only native
`cfg(test)` witnesses in
[grants.rs](../../crates/veoveo-hub/src/policy/grants.rs); its runtime code,
permissions, native collections and dependency pins were unchanged. The connected extension below includes one equivalent iterator closure spelling.
Verification remains manual; there is no CI hook, runner or new authorization gate.

## Preliminary conditional server-decision extension (unqualified)

The current worktree adds an untouched generated superset from Hub `bb2e97f`
with the unchanged compiled SDK `05273d1`, retaining all qualified Held/grant bodies.
Its first milestone is model/type compilation and rerunning existing proofs.
`NativePolicyModelAudit` explicitly exposes TWO native operation symbols:
`chrono::Utc::now` and SDK `AuthorizationDeclaration::access`. They are not filled
with fixed timestamps or a deny-all route implementation. New branch/output
proofs will be conditional on their observed successful results; this is NOT
standard-three-only axiom qualification, a real-clock/SDK-route proof, or a proof
of the asynchronous endpoint. SDK `239622d` adoption/correspondence remains open.
All other additional metadata/clone/action primitives have ordinary value models.
Historical receipts below remain unchanged and do not qualify this extension.

## Connected CurrentPermissions boundary

This extension uses Hub `33849317d6e80c78ba7fd8ed57067a62a2eb677c`, with the same
SDK pin and the equivalent snapshot iterator closure spelling. The original
current-role receipt remains historical. The extended ordinary models compile,
all unchanged role proofs rerun, and the connected theorem/authority bridge pass.
Focused native tests, two semantic return-value controls and a fresh restored
baseline are now qualified in the [new receipt](evidence/current-permissions-receipt.json).

[RegistrationIteratorProof](RegistrationIteratorProof.lean) proves the actual raw
registration cursor, original-caller per-provider grant closure and requested
membership predicate. [FlatMapIteratorProof](FlatMapIteratorProof.lean) proves
finite traversal with actual FnMut/inner-iterator state and derives concrete
terminal cursor stability. [CurrentPermissionsProof](CurrentPermissionsProof.lean)
then proves the ACTUAL `CurrentPermissions::held` returns exactly `.ok` of the
requested permissions having at least one granting raw registration. Its public
membership iff uses the established `Grants` predicate for the original caller's
supplied Principal. Duplicate-slug deployments and permission occurrences are
existential; empty requests/catalogs are legal. No unique-provider, all-provider
agreement, selected-namespace or catalog-nonempty premise is added.

The concrete `HeldModel` and `LawfulHeld` are constructed from that actual method,
then used with the existing authority `issued_membership_bound` theorem. Issued
membership is exactly requested AND declared uses AND such a raw provider grant;
the supplied generic Context is preserved. The theorem does not establish that
the async endpoint built Context.caller from the same caller, or catalog freshness.
The production exchange passes `subject.caller` to both constructors; that is
source correspondence only, not an extracted endpoint proof or native endpoint
qualification.

The extracted `held` retains raw registration traversal, actual per-provider role
evaluation, streaming requested filtering and final set collection. Among local forwarding functions, the synthetic
Arc projection FnMut body is external because the pinned backend cannot
synthesize its borrow abstraction; its model delegates to the Arc value projection
and retains closure state. Native reference/alias/lifetime refinement is not proved.
The generated model also uses a supported extraction-only `charon::rename`
annotation on a temporary source copy: the Rust field is still `control_plane`,
while the emitted field is `control_plane_value`. This avoids namespace shadowing
in generated Lean; source function bodies and the repository's catalog/lib files
are unchanged. Charon registers its tool through selected-crate rustc arguments.
Generated Lean is copied untouched, with no source-text rewriting or tool upgrade.

Four unused infrastructure types and two unused native metadata types
(JSON Value and protobuf Kind) are opaque. No JSON/protobuf, floating-point or
serialization semantics is claimed. New map-values/owned-set/FlatMap
models describe primitive traversal, not a precomputed permission union. FlatMap
retains explicit exhausted state for Fuse value semantics. The pinned native Fuse
specialization may still call a FusedIterator after None; concrete terminal-cursor
stability is derived for the actual cursor to justify value correspondence. No generic stateful post-terminal native
call-count or double-ended iterator trace is claimed. Other new
catalog/context payloads are uninterpreted values; no networking, locking, JSON,
registry, credential or timestamp behavior is claimed from them. No clock or SDK
route-access operation is part of this Held-only closure. `decide_server` remains
next in this slice; its outer admission/caller and native contract boundaries remain
explicit. Verification remains manual, with no new runner or gate.

### Held-only regeneration in a scratch source copy

Use the pinned tools/environment below and a frozen Hub source copy at
`33849317d6e80c78ba7fd8ed57067a62a2eb677c` with the reviewed snapshot iterator
closure (snapshot.rs SHA-256
`4d35ca1aa547817387d7922ec552513516c18e4bf00cd3e99ce4da4a3e68678d`). Apply the
[naming metadata patch](extraction/catalog-field-name.patch) **only in that scratch
copy**, never the runtime checkout. Removing its one annotation restores catalog.rs
byte-for-byte; Rust's field spelling/types/order and function bodies do not change.
The two rustc arguments register that supported annotation for Charon's selected
crate only. No runtime feature, dependency or tool upgrade is required.

From the scratch Hub root, with `PROOF_PACKAGE` pointing to this proof package and
`EXTRACTION_OUT` to an empty absolute output directory:

```sh
patch -p1 < "$PROOF_PACKAGE/extraction/catalog-field-name.patch"
SQLX_OFFLINE=true RUSTUP_TOOLCHAIN=nightly-2026-09-17 charon cargo --sysroot default --preset=aeneas \
  --rustc-arg='-Zcrate-attr=feature(register_tool)' \
  --rustc-arg='-Zcrate-attr=register_tool(charon)' \
  --start-from '{impl veoveo_hub::delegation::authority::Held for veoveo_hub::delegation::CurrentPermissions}::held' \
  --include '{impl veoveo_hub::delegation::authority::Held<_> for veoveo_hub::delegation::CurrentPermissions}::held' \
  --include veoveo_hub::policy::grants::held_roles \
  --include veoveo_hub::policy::grants::effective_risks \
  --include veoveo_hub::policy::grants::granted_permissions \
  --include veoveo_hub::control_plane::authorization_config::_::allows \
  --opaque veoveo_hub::mcp::upstream_http::HubUpstreamHttpClientPool \
  --opaque veoveo_hub::contract::catalog::SectionDeclaration \
  --opaque veoveo_hub::contract::catalog::TargetDeclaration \
  --opaque openshell_sdk::config::AuthConfig \
  --opaque serde_json::value::Value \
  --opaque prost_types::protobuf::value::Kind \
  --opaque veoveo_hub::registration::snapshot::_::iter::_::call_mut \
  --dest-file "$EXTRACTION_OUT/hub_current_roles.llbc" -- \
  --manifest-path crates/veoveo-hub/Cargo.toml --lib --locked
aeneas -backend lean -filter-trait-methods -split-files \
  -dest "$EXTRACTION_OUT" "$EXTRACTION_OUT/hub_current_roles.llbc"
```

Inspect transparency and compare prior policy declarations before copying the
untouched generated Types/Funs into `HubCurrentRoles/`. Keep the handwritten
external models; generated templates contain unproved placeholders. From this
package run `lake build`, then `lake env lean ModelAudit.lean` and
`lake env lean CurrentPermissionsModelAudit.lean`. This remains a manual recipe,
not a runner. The original qualification receipt is historical. The completed
model-only checkpoint is recorded in
[the model record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-29-current-permissions-model.md).

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
[FunsExternal](HubCurrentRoles/FunsExternal.lean) fill the original 57 external
placeholders and the connected traversal primitives with ordinary definitions. IDs have distinct tags and arbitrary,
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
No runtime performance or zero-overhead benchmark is claimed. Native collections
and the streaming algorithm are retained; the runtime source delta is the
equivalent snapshot projection closure spelling described above.

Charon/Aeneas translation and the compiler remain trusted. Lean's kernel checks
the proof terms. Accepted axiom closures must contain only `propext`,
`Classical.choice`, `Quot.sound`; no `sorryAx` or custom unproved facts.

The supplied signed Principal, its native group projection/freshness, current
catalog/storage, profile/principal/scope admission, native declaration decoding,
async caller/context construction and `decide_server` output assembly remain
outside this formal boundary. The actual registration flat_map/request-filter/collect
is now connected to the authority package; the endpoint, storage and native
primitive refinements remain explicit obligations.
The separate SDK route proof at `239622d` is not adopted by the compiled `05273d1`
dependency. An Authenticated route can legitimately have an empty permission set;
this evaluator proof introduces no contrary runtime condition.

## Historical current-role qualification

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

The following is the HISTORICAL four-function extraction recipe. For the connected
package use the Held-only scratch recipe above, which transitively retains these
actual bodies and its required metadata boundaries:

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
lake build
lake env lean NativeSetProof.lean
lake env lean HeldRoleProof.lean
lake env lean EffectiveRiskProof.lean
lake env lean GrantedPermissionsProof.lean
lake env lean CurrentPermissionsProof.lean
lake env lean ModelAudit.lean
lake env lean CurrentPermissionsModelAudit.lean
```

These files print target axiom closures. Check actual module names, exits and
closure output; successful model compilation alone is not a policy proof. In the
measured Linux x86_64 Rust 1.99.0 environment, from the Hub root (the receipt
records exact compiler/OS/tool identities):

```sh
rustup run 1.99.0-x86_64-unknown-linux-gnu cargo fmt --all --check
SQLX_OFFLINE=true rustup run 1.99.0-x86_64-unknown-linux-gnu cargo test --locked -p veoveo-hub --lib policy::grants::tests
SQLX_OFFLINE=true rustup run 1.99.0-x86_64-unknown-linux-gnu cargo test --locked -p veoveo-hub --lib control_plane::authorization_config::tests
SQLX_OFFLINE=true rustup run 1.99.0-x86_64-unknown-linux-gnu cargo test --locked -p veoveo-hub --lib delegation::authority_adapter_tests
SQLX_OFFLINE=true rustup run 1.99.0-x86_64-unknown-linux-gnu cargo test --locked -p veoveo-hub --lib registration::snapshot::tests
SQLX_OFFLINE=true rustup run 1.99.0-x86_64-unknown-linux-gnu cargo clippy --locked -p veoveo-hub --lib -- -D warnings
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


## CurrentPermissions qualification

The [new receipt](evidence/current-permissions-receipt.json) records this connected
increment separately from the historical role qualification. The native baseline
passed five authority-adapter tests (three new actual-held witnesses), two snapshot
tests, formatting and strict library Clippy. It used Linux x86_64 Rust 1.99.0,
SQLX_OFFLINE=true and a fresh target. Each accepted control used the same native
toolchain with an explicit Hub-package clean and observed current-source
compilation. Binary hashes were captured before and after the exact witness. The native target
and binary path were later reused: these are historical stage measurements, not
a claim that each earlier binary is still retained at that path.

| Patch | Primary statement | Native witness (`delegation::authority_adapter_tests::…`) | Observed actual / expected |
|---|---|---|---|
| [return-requested](mutations/current-permissions-return-requested.patch) | `CurrentPermissionsProof.held_exact` | `current_permissions_use_existential_raw_providers_and_filter_requested` | `{missing, read, write}` / `{read}` |
| [return-empty](mutations/current-permissions-return-empty.patch) | `CurrentPermissionsProof.held_exact` | `current_permissions_use_existential_raw_providers_and_filter_requested` | `{}` / `{read}` |

These names abbreviate the fixture-owned `held-fixture.records.*` identifiers.
Both artifacts evaluate the original collection before returning a different set.
They compile natively, extract with the actual method/helpers transparent, and
compile their generated models and helper prerequisites. The unchanged exact
result proof fails at normalization of the semantically changed return (line 146):
collect-then-requested-clone or collect-then-empty-constructor differs from returning
the collected value. This is a reviewed typed-program/output discrepancy, paired
with an independent native whole-set assertion failure; it is not a Lean-generated
counterexample or a normalized False goal. Error-recovery `sorryAx` in failed
mutant declarations is diagnostic, never accepted proof evidence.

The empty control freshly reaches the ordinary `.ok ∅` BTreeSet constructor model.
Its exact signature and direct axiom audit are recorded; no permission-result
assumption or axiom was introduced. One control's generated Types file differed
only in build-path comments; the raw identity and the comparison are retained.
Generated files were not edited to remove those comments.

Fresh restoration verified unmutated input hashes, rebuilt the native Hub package,
passed the same five-plus-two tests/fmt/Clippy, and regenerated Types/Funs exactly
at the checked package hashes. Default Lake build, direct actual-held proof and
both audits passed with only the standard Lean axioms; the constructor uses only
`propext` and `Quot.sound`. The restored native binary identity is recorded.
Baseline/restored authority-test commands took 191.848/34.022 seconds including
compilation. Restored Charon, Aeneas and default Lake stages took 24.354, 1.219 and
30.713 seconds. These are tool-stage timings with dependency reuse, not request
latency or a performance benchmark.

Native compilation used the original unannotated catalog source. Only the separate
Charon source copy received the supported field-naming annotation, with tool
registration arguments supplied once. Failed setup attempts (duplicate tool
arguments, annotation in a native copy, archive depth, wrong native toolchain and
missing authority prerequisite) were excluded and retained. The interrupted
wrong-toolchain run did not provide an accepted native binary or control result.
Final commands keep the native and extraction environments separate.

The sole runtime source edit is the equivalent snapshot `map` closure spelling;
no additional collection construction or heap allocation is introduced in that
adapter. No benchmark or native lifetime/alias refinement is claimed. No
deployment, full repository Mac/Linux Docker gates, paid model call or async
endpoint qualification was performed for this increment.
