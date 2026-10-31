# Request composition (manual)

This package connects the actual registration-selection and forwarded-request
assembly code to the authorization and recipient contracts tracked in
[Hub proof coverage](../README.md). It is an implementation-proof increment, not
a new runtime component or authorization rule. Proofs run manually.

## Shared model ownership

The active proofs import the single generated `HubCurrentRoles` superset and its
handwritten models from [current_roles](../current_roles/README.md). The existing
`RequestCompositionHub` target is only a re-export/build group. Policy and route
proofs therefore use the same actual types and operational functions, including
the finite iterator models; no nominal converters or duplicate recursive models
are introduced. The independent specifications and theorem contracts are retained
modulo symbol relocation. The consolidation passes all 25 current-policy default
targets, the model audit, request defaults, six routing targets and the new direct
policy/client theorem; see the [receipt](evidence/shared-policy-request-graph.json).

The source baseline remains Hub `3bc2923`, SDK `239622d` and Core `120306e`, not
a re-extraction of current main or its newer OpenShell dependency. Current Apps
runtime changes, including downstream `WithCauses` formatting outside the HTTP
constructor, are preserved. The shared model records the real endpoint wrapper
and expanded `Ord.min` dictionaries. Current main's additional
`RefreshRevocation::AdministratorRevocation` variant is outside the baseline enum
domain even though `bound_to` and `deliverable` bodies are unchanged; a future
source refresh must include it before claiming whole-current-main applicability.
Namespace-helper completion and ordering, SDK issuer/recipient composition and
async caller wiring remain separate obligations.

Historical mutation witnesses and receipts are frozen recipes for their recorded
generated namespaces and hashes. They do not import the consolidated graph and
are not active package targets; replay them against their pinned model rather
than adding a compatibility namespace to the maintained proofs.

[DirectPolicyClientProof.lean](DirectPolicyClientProof.lean) is the checked consolidation
acceptance leaf. It directly reuses the existing grant theorem
and the HTTP constructor theorem: on the source-reviewed nondelegated call path,
the complete accepted client preserves the computed permission set, whose members
are exactly the independent existing `Grants` relation for that installation,
row declaration and current subject principal. No supplied desired-grant premise,
new policy rule, delegated narrowing claim or async caller proof is introduced.
The first theorem takes installation and row as supplied inputs. The subsequent
`constructed_direct_policy_route_exact` derives the row from the computed retained
route and uses authorization from that same supplied catalog generation. Actual
constructor success supplies key/row coherence; actual selected lookup returns
exactly that row, or `none` without fallback. The present branch proves the complete
client and independent grant membership without assuming a desired row or set.
The [route receipt](evidence/direct-policy-route.json) records its passing manual
check. Finite cursor/native collection correspondence, composition-key uniqueness,
successful namespace-helper completion, snapshot acquisition and async caller
wiring remain explicit boundaries; this is not a globally latest-state claim.
The direct theorem's audit permits only standard Lean
axioms and the constructor's two existing error-formatting symbols. The check
passes in 2.69 seconds with one unused-simp warning and no unreviewed axiom. The
route theorem additionally names the existing namespace helper; both pass together
in 2.64 seconds.

## Native narrowing and the complete selected client

[DelegatedPolicyClientProof.lean](DelegatedPolicyClientProof.lean) now proves the
actual native set adapter returns exactly the intersection of current and carried
permissions. It reuses the existing owned cursor, comparison, filter and collection
laws; no new primitive model is introduced. The actual `SubjectDelegation::narrow`
wrapper and optional caller branch retain that exact result. Empty intersections
remain successful and are accepted by the HTTP constructor whenever its existing
context check accepts.

`constructed_policy_route_exact` connects computed retained-route lookup, the same
catalog generation's policy, the current subject principal, actual optional native
narrowing and complete HTTP construction. It covers direct and delegated fields,
exact missing-route behavior and independent permission membership in both directions.
It assumes no desired row or grant set. This is source-reviewed composition of
extracted subcalls; it does not extract the async caller or establish global freshness.

All seven scoped audits pass. Native narrowing/membership use only standard Lean
axioms; the complete client adds the existing two error-formatting symbols; the
constructed route also names the existing namespace helper. Model defaults and
audit, both request targets, and the focused native set regression pass. See the
[receipt](evidence/delegated-policy-client.json) for inputs, exact commands,
iterations and source applicability.

The [source-control receipt](evidence/delegation-narrowing-controls.json) qualifies
two one-line actual Rust faults. The wrapper bypasses its carried set in
[one patch](mutations/bypass-delegation-carried-set.patch); the concrete adapter
drops valid permissions in [the other](mutations/drop-valid-delegation-permissions.patch).
Both re-extract successfully and fail the unchanged delegated theorem for the
intended semantic reason. Independent witnesses prove an accepted
[extra permission](mutations/NarrowingOvergrantWitness.lean) and an accepted
[loss of a common permission](mutations/NarrowingOverdenialWitness.lean), including
inequality to the independent intersection specification. Each witness has four
standard-axiom-only audits and imports no failed theorem or its artifact.

Both scratch Rust sources were restored; a fresh extraction reproduced all four
canonical Lean files byte-for-byte. The direct/delegated targets and model audit
then passed in a fresh package build. No maintained model, theorem statement,
runtime rule or production source changed. SDK issuer/recipient composition remains
the next connected contribution.

These witnesses are outside default targets. Apply each patch separately to a
private copy of the recorded source, re-extract with the same selectors and use the
private tooling workflow below. Build `DirectPolicyClientProof` and
`RegistrationIteratorProof`, then run the frozen delegated leaf directly (expected
failure) and the corresponding `mutations/NarrowingOvergrantWitness.lean` or
`mutations/NarrowingOverdenialWitness.lean` with `lake env lean` (expected success).
The direct check emits no `.olean`; the witness imports only the raw model and
unaffected helper proofs. The witness is expected to fail against correct source.
Restore the Rust, re-extract and compare canonical outputs before checking the
ordinary targets again. Do not edit generated Lean to enact or undo a fault.

Using the qualified private dependency workflow in [tooling](tooling/README.md):

```sh
LAKE_NO_UPDATE=1 lake build DirectPolicyClientProof DelegatedPolicyClientProof
```

## First checked assembly checkpoint

[SnapshotAssemblyProof.lean](SnapshotAssemblyProof.lean) checks six declarations:

- `ofWith_success_iff` and `ofWith_arbitrary_success`: snapshot assembly preserves
  its entire server map for arbitrary namespace-helper results; arbitrary successful
  namespace values are possible in the parameterized interface.
- `extracted_of_connection` and `extracted_of_success`: that interface is connected
  to the untouched extracted `of`; a successful actual result preserves the map.
  This does not prove the unresolved helper completes or chooses correct namespaces.
- `request_context_exact`: the actual function exactly constructs direct context,
  rejects missing direct execution, preserves delegated context, and rejects an
  explicitly conflicting delegated execution.
- `http_new_exact`: given successful context construction, the actual constructor
  returns exactly the independently specified complete client value. Recipient,
  actor, authority, supplied permissions, context, delegation and transport fields
  are preserved. This does not establish where the supplied registration or current
  permissions came from.

The [assembly receipt](evidence/assembly-checkpoint.json) records the exact nine
source/config files and Linux Lean 4.35.0 checks. Model compilation passed in
11.032 s, the direct proof check in 2.819 s and the default package build in 3.221 s.
The generated files retain their canonical extraction hashes.

Executable axiom audits reject unreviewed symbols and `sorryAx`. Universal/context
proofs permit only standard Lean axioms. The extraction connection permits the
explicit unresolved namespace helper. The HTTP theorem also permits the two native
error-formatting symbols referenced by the full constructor's error branch; its
accepted-branch proof assumes neither their success nor their contents. No issuer
result is allowed in that theorem's audit. Primitive-model/native correspondence
remains separate from these audits.

This is a checked proof checkpoint. Actual-source semantic controls, selected-route
composition and actual SDK issuance remain unfinished qualification work.

Run from this directory, using the committed toolchain and dependency manifest:

```sh
LAKE_NO_UPDATE=1 lake build
lake env lean SnapshotAssemblyProof.lean
```

Combining this package's proofs with the current-role collection proofs exposes
an instance-name collision in the pinned Aeneas support library. The
[manual tooling checkpoint](tooling/README.md) records a two-line naming patch,
stock/patched regression and successful checks of both unchanged packages and
their combined imports. It uses explicit private dependency overrides; the
baseline manifests above still select the unpatched tool. It adds no new routing
or issuer theorem.

## Checked registration lookup

[SelectedRegistrationProof.lean](SelectedRegistrationProof.lean) checks the actual
extracted `RegistrationSnapshot::get` against an independent specification: filter
the snapshot's values by slug, return the complete registration if exactly one
matches, and return none for zero or multiple matches. The theorem covers arbitrary
snapshot contents and uses no namespace-helper or desired-route premise.

The iterator and actual slug-predicate contracts establish a finite scan. The
separate `lookup_exact` lemma connects the handwritten comparator-driven map model
to independent keyed list lookup. That lemma supplies a model boundary for the next
selection proof; it does not separately verify Rust's `deployment` method or the
native standard-library map implementation.

All five declarations have executable standard-axiom-only audits. The
[lookup receipt](evidence/registration-lookup-checkpoint.json) records a successful
direct check in 2.869 s and exact target build in 3.171 s, with 61 effective source
and configuration hashes unchanged. Generated outputs and the previous assembly
proof retain their checked hashes.

This target is deliberately nondefault because its combined imports require the
explicit [private patched-tool workflow](tooling/README.md). In that prepared
request-composition directory, run:

```sh
LAKE_NO_UPDATE=1 lake env lean SelectedRegistrationProof.lean
LAKE_NO_UPDATE=1 lake build SelectedRegistrationProof
```

The original five-declaration lookup checkpoint is followed by the checked
selection-entry contract below. Full collection, uniqueness of selected routes,
current-policy provenance and issuer/recipient composition remain open.

The [lookup semantic-control receipt](evidence/registration-lookup-control.json)
now qualifies a deliberate actual-Rust ambiguity bypass. In a private source copy,
the [patch](mutations/bypass-get-ambiguity.patch) replaces the second-match check
with `true`. Charon/Aeneas re-extraction changes only the generated getter body;
the unchanged proof fails. An independent
[witness](mutations/GetterAmbiguityWitness.lean) proves that two complete same-slug
registrations with distinct coherent deployment keys return `Some`, while the
unchanged singleton specification returns `None`, in both enumeration orders.
All four witness audits permit only standard axioms. The witness imports the
mutant model alone, not the failed theorem module or an old proof artifact.

Reversing the patch in the same scratch Rust tree reproduced all four canonical
generated Lean files byte-for-byte. A fresh restored package passed model/assembly
compilation in 13.586 s, the unchanged getter directly in 2.768 s and its exact
target in 3.269 s. The independent witness passed in 2.469 s. These are extracted
model checks; native collection/compiler correspondence remains an assumption.

The witness is deliberately outside every default target. To reproduce it, use
the documented private tooling workflow and canonical extraction selectors on a
private Rust copy with the patch applied, retain the unchanged external models,
and run `lake env lean GetterAmbiguityWitness.lean` against that mutant model.
It is expected to fail against the correct canonical getter. Restore the Rust,
re-extract and verify canonical hashes before rechecking the ordinary proof.
Do not apply the mutation to the running installation or edit generated Lean.

## Checked selection-entry contract

The same [proof module](SelectedRegistrationProof.lean) now proves the actual
extracted `selected` closure against `chooseOne`, an independent keyed-lookup
specification. Unavailable selections, missing deployments and rows with a different
server slug return none. A matching selection returns the requested deployment key
and the complete registration. Every case preserves the entire snapshot state.
No well-formedness, uniqueness or desired-route premise is required.

The proof-local `selected_entry_body` exposes the exact generated pair destructuring
and clone operation under an explicit tuple type. Lean kernel conversion checks
that connection; no generated code, primitive model or trust assumption changes.
The returned key is the requested deployment. Equality to the row's own deployment
field is a separate constructor-coherence obligation, not an extra runtime check.

The [entry receipt](evidence/selected-entry-checkpoint.json) records the direct
check (**2.723 s**) and exact nondefault target (**3.273 s**). All ten named audits,
including the five prior getter/lookup declarations, permit only standard axioms.
The 61-entry effective source map differs from the lookup checkpoint only in this
proof file and stayed unchanged during validation. The same manual commands and
private patched-tool workflow above apply.

This checks the entry closure, not the complete `selected` method. Collection,
derived output uniqueness, namespace construction, current-policy provenance and
actual SDK issuer/recipient composition remain open. Its actual-source semantic
control is now qualified separately from the getter ambiguity control.

The [entry control receipt](evidence/selected-entry-control.json) records the
[actual-Rust slug-check bypass](mutations/bypass-selected-slug.patch), unchanged
proof rejection and [independent witness](mutations/SelectedEntrySlugWitness.lean).
For a complete registration keyed by its own deployment and a different requested
slug, the mutant returns the whole keyed row and unchanged state while `chooseOne`
returns none. The witness proves outer `Result` success and the contradiction;
all four audits permit only standard axioms. It imports only the mutant model.

Restoring the same scratch Rust source reproduced all four canonical generated
files byte-for-byte. A fresh model/assembly build passed in **13.686 s**, the frozen
entry proof directly in **2.818 s**, and its target in **3.320 s**, with all ten
module audits passing. The standalone witness passed in **2.518 s**. The committed
proof and production code were unchanged throughout qualification.

Reproduction follows the getter control workflow above, using the entry patch and
`lake env lean SelectedEntrySlugWitness.lean` against the freshly extracted mutant.
This witness is outside default targets and is expected to fail on the correct
canonical model. Keep raw generated outputs untouched and restore the source before
checking canonical hashes and the ordinary proof. This supplies no native-execution
or full selected-pipeline guarantee beyond the stated model boundary.

## Checked finite selection scan

[SelectedCollectionProof.lean](SelectedCollectionProof.lean) establishes the next
collection building block. Its independent `Eligible` relation requires a component
requesting the exact deployment key, a matching keyed lookup of the whole row, and
the requested server slug. The reference emission sequence contains exactly these
rows. Equal emitted deployment keys imply equal whole rows by deterministic lookup;
no component-uniqueness or desired-route premise is used.

The component cursor and this package's `filterMapNext` recurrence are total over
finite suffixes. The actual extracted entry predicate supplies each decision and
preserves the complete snapshot closure. Each resulting cursor step advances through
the reference emission sequence. These recurrence proofs concern the explicit
standard-library operational models used by the extraction, not independent native
verification of Rust's iterators, BTreeMap or Arc.

The [finite-scan receipt](evidence/finite-selection-scan-checkpoint.json) records
the direct check (**2.774 s**) and nondefault target build (**3.429 s**), with five
standard-only audits and an unchanged 62-entry effective source/configuration map
for the target. The prior entry proof and generated models retain their hashes.
With the documented private patched-tool setup, run explicitly:

```sh
LAKE_NO_UPDATE=1 lake env lean SelectedCollectionProof.lean
LAKE_NO_UPDATE=1 lake build SelectedCollectionProof
```

The scan alone does not establish ordered insertion, `collectMap`'s complete result
or connection to the actual `selected` body. Per-slug route uniqueness, constructor
coherence, namespace-helper completion and request/issuer composition remain open.
The finite-scan proofs
use no namespace-helper success assumption. This checkpoint does not close the full
selection slice or its semantic qualification.

## Ordered map insertion

[SelectedMapInsertionProof.lean](SelectedMapInsertionProof.lean) proves total
insertion into the explicit comparator-driven map model, preserving strict key
ordering, deriving unique output keys, and retaining the exact whole-row union.
For replacement at an existing key, it requires equal-key row compatibility;
`emitted_compatible` derives that invariant from the checked selection emissions.
It never assumes the desired final route map. Unique but unsorted input alone would
not suffice, because insertion stops at the first larger key.

The [insertion receipt](evidence/ordered-map-insertion-checkpoint.json) records
manual direct and nondefault target checks with four standard-only axiom audits.
Use the documented private patched-tool setup:

```sh
LAKE_NO_UPDATE=1 lake env lean SelectedMapInsertionProof.lean
LAKE_NO_UPDATE=1 lake build SelectedMapInsertionProof
```

This verifies the handwritten collection primitive used by the extracted caller;
native BTreeMap correspondence remains a boundary. Insertion alone does not establish
`collectMap`'s complete output, the full `selected` method or namespace completion.
No production defect was found or runtime behavior changed in this checkpoint.

## Complete collection and selected-result preservation

[SelectedMapCollectionProof.lean](SelectedMapCollectionProof.lean) composes the
qualified entry predicate, finite cursor and ordered insertion. `collect_complete`
derives an actual successful collector result from the empty accumulator, with
strict key ordering, unique keys and whole-row membership if and only if the
independent `Eligible` relation holds. No desired output or unique-component premise
is assumed. The recursive collector contract also covers prior accumulator rows.

`selected_factorization` connects the exact extracted `selected` wrappers to that
derived map before the actual `of` call. `selected_success_complete` proves that
any successful `selected` result contains exactly the same eligible whole rows,
with ordering and unique deployment keys. The collector's two audited declarations
use standard axioms only; the two caller declarations additionally name the existing
uninterpreted `build_namespaces` helper. That boundary grants no helper-success or
namespace-ordering guarantee.

The [collection receipt](evidence/complete-selection-collection-checkpoint.json)
records source identities, manual validation and per-declaration audits. With the
documented private patched-tool setup:

```sh
LAKE_NO_UPDATE=1 lake env lean SelectedMapCollectionProof.lean
LAKE_NO_UPDATE=1 lake build SelectedMapCollectionProof
```

Native collection/representation correspondence, namespace completion, constructor
coherence, per-slug route uniqueness, full request/issuer composition and semantic
qualification of this combined contract remain open. Successful-result correctness
does not establish unconditional `selected` success. No production defect or runtime
change is claimed by this proof-only checkpoint.

The subsequent [unchanged-helper translation probe](evidence/namespace-helper-translation-probe.json)
exposed `build_namespaces` without changing its Rust. Charon extracted all sixteen
requested roots and the helper's closures without errors. Aeneas failed on nested
borrowed collections and while translating `deployments[0].deployment`, producing
a partial helper containing `sorry`. Those outputs were not adopted. This records
a translator limitation, not a production bug or additional proof coverage; the
namespace-helper boundary remains open.

## Intended contract

The unchanged `RegistrationSnapshot::new` constructor is now included in the
raw extraction. Its generic iterator, mapping closure and complete row allocation
remain visible; existing generated definitions are unchanged. The
[extraction receipt](evidence/snapshot-constructor-extraction.json) records the
16-root probe. The [model checkpoint](evidence/snapshot-constructor-model-checkpoint.json)
records a passing six-target build of the model and prior proofs. The handwritten
model adds allocation-erased `Arc.new` and the missing generic `Iterator.map`
method spelling, forwarding to the existing default. No generated code or Rust
was patched. Constructor coherence and the connected route theorem have their
separate qualification below; translation and compilation alone did not establish them.

For an admitted request, the selected registration determines the recipient slug,
deployment and resource. The HTTP client preserves the admitted actor, authority,
permissions and request context. Issuance passes those exact fields to the actual
SDK issuer, with the existing delegation deadline cap. The issuer validates the
context, constructs the identity and claims, and the recipient checks its own
registered identity.

Selection must establish both refusal and useful acceptance: an unavailable,
missing or foreign-namespace selection cannot fall back to another deployment;
a present correctly selected deployment remains usable. Map-key/deployment
coherence and unique composition keys must be stated or derived, rather than
assuming the desired selected registration.

Direct request context is constructed from the subject's execution, audit, token
and original principal. A delegated request retains its previously admitted
context; an explicitly supplied conflicting execution is rejected. These are
different paths and must not be conflated in the forwarding contract.

The overall policy-to-issued-identity theorem is unfinished. Successful translation
alone does not establish it.

## Constructed snapshot, exact route and HTTP context

`SelectedRouteContextProof.selected_get_exact` derives the actual selected getter's
result from an independent resolver: captured slug/component, current snapshot
deployment lookup, then slug match. Complete selected-map contents and unique
native composition keys yield exactly zero or one matching registration. No desired
returned server is assumed; unavailable, absent, missing and wrong-slug selections
have no fallback.

`SnapshotConstructorProof.new_success_coherent` derives map-key/row-deployment
coherence from the actual constructor mapping and finite collection. Arbitrary
overwriting insertion preserves that invariant without sortedness, unique input
keys or compatible duplicate values. The theorem requires an explicit finite cursor
law and successful input conversion; it does not assert termination for arbitrary
native iterators or complete last-written-row contents from the constructor input.

`SnapshotConstructorProof.constructed_route_context_exact` discharges that coherence
premise and connects the exact route to the actual HTTP constructor. The client
names the captured deployment and requested slug, keeps the current row's endpoint,
and preserves all expected subject, context, delegation and supplied-permission
fields. The actual request-context result is exact for both direct and delegated
calls. Current means this supplied catalog generation, not globally latest SQL state.
Permissions are preserved as supplied; their current-policy provenance is not yet
connected here. The async caller's use of the same returned row is a reviewed
source/native adapter boundary.

Both direct compilations and the two nondefault targets pass with nine executable
axiom audits and no `sorryAx`. The [receipt](evidence/constructed-route-context-checkpoint.json)
records source identities, failed proof-local iterations, commands, timings and
compiled artifacts. In the documented private patched-tool setup, run manually:

```sh
LAKE_NO_UPDATE=1 lake build SelectedRouteContextProof SnapshotConstructorProof
```

Successful constructor/selection remains conditional on the unresolved namespace
helper. Native collection/allocation refinement, current-policy provenance and the
SDK issuer/recipient connection remain open. Entry/getter mutation controls retain
their narrower scope. No runtime change, native check, deployment or paid model call
was part of this milestone.

## HTTP permission-preservation source control

The [actual Rust mutation](mutations/erase-http-permissions.patch) replaces only the
HTTP constructor's supplied permissions with an empty set in a disposable source
copy. The unchanged `SnapshotAssemblyProof.http_new_exact` rejects the re-extracted
code at its complete-client equality. An [independent witness](mutations/HttpPermissionsWitness.lean)
imports only the raw model, constructs an accepted direct context and supplies a
singleton permission. It proves the mutant returns the complete client with no
permissions, contradicting the unchanged independent specification. All four
executable witness audits pass without `sorryAx`; no failed proof module is imported.

Reversing the same patch in the same source tree reproduces all four canonical raw
outputs byte for byte. A fresh restored package passes the model build, direct
assembly proof and both connected route/constructor targets. The
[receipt](evidence/http-permission-preservation-control.json) records the final
private-tool configuration, source and artifact identities, commands, timings and
earlier setup/proof-local failures. The initial fresh packages omitted the existing
private dependency overrides, causing the known discriminant-name collision;
corrected mutant and restored packages were both checked before qualification.

This qualifies sensitivity to permission loss in the HTTP constructor, not every
edge of the composed route proof, current-policy provenance or SDK issuance. The
witness establishes acceptance by the extracted context function, not native
authentication of its arbitrary typed inputs. No production defect or runtime
change was found. The next composition step will reuse the existing policy proof
through one shared generated graph, avoiding duplicate nominal types and recursive
collection models.

## Source and model ownership

The maintained [HubCurrentRoles/Types.lean](../current_roles/HubCurrentRoles/Types.lean)
and [Funs.lean](../current_roles/HubCurrentRoles/Funs.lean) are untouched
Charon/Aeneas outputs. The earlier receipts name the historical
`RequestCompositionHub` layout. The canonical extraction uses Hub
`3bc2923ca1d2e0c559e532286c16ec3d6a5ceb7a`, including runtime preparation
`91ab4cc0b63f7cc1950627df59f5ae3f1cb633e7`, and its actual SDK dependency
`239622d12c08b771cc5e813fe3e846385d8d04ad`. The existing catalog-field naming
annotation is extraction-only. Generated files are never patched to obtain a proof.
The [extraction receipt](evidence/extraction-checkpoint.json) records exact tool
arguments, input/output hashes and source-map checks. Its translation status is
separate from any subsequent Lean proof qualification.

The shared `current_roles/HubCurrentRoles/TypesExternal.lean` and
`FunsExternal.lean` are handwritten operational models defined once for both
proof packages. Their correspondence to native collections, identifiers, timestamps,
cloning and memory behavior remains an explicit boundary; a standard-axiom audit
does not establish that correspondence.

Two own-code boundaries need particular care:

- Namespace construction is separated from snapshot assembly. Any theorem about
  preserving the deployment map must hold for arbitrary helper results. It does
  not establish the helper's completion or ordinary namespace ordering.
- The SDK issuer is unresolved in the Hub extraction. It must be connected to the
  separately extracted actual SDK implementation through field-preserving typed
  adapters. An assumed issuer returning a desired identity is not a proof of
  forwarded identity or successful issuance.

Native SDK source-preparation commits after `239622d` are not part of Hub's current
dependency. Their tests and extraction cannot be claimed as proof of the pinned
stack without establishing and recording that integration.
SDK `e268032` successfully translates the actual issuer and context path after an
ordered-guard rewrite; its 35 native token tests pass. The
[translation record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-request-composition-translation.md)
states the exact inputs and limitations.

## Remaining scope

The full request-composition contract also requires current-policy/Task-route
linkage, actual SDK issuance and recipient checks, and explicit handling of each
error. Source-token expiry caps direct issuance; a valid delegated Task can outlive
that source token under its separate admitted deadline. Claims use JWT timestamp
seconds rather than preserving timestamp nanoseconds.

Cryptography and serialization, parser and URL/string primitives, async caller
wiring, store freshness, physical SQL transactions, deployment and recovery are
not established by translating or compiling this package. The
[native routing qualification](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-request-routing-native-qualification.md)
and [source checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-request-composition-source-checkpoint.md)
are separate evidence, with their own inputs and limits.
