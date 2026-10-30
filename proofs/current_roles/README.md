# Current-role permission evaluation (manual)

The qualified current-role increment of HUB-AUTH-01 extracted the unchanged
evaluator from Hub `b9b6cec1b9c36a2e84afb980e208d2d116a84185`, with SDK pin
`05273d1f90b9a08e90e377187f4eddd084605fe5`. That qualification added only native
`cfg(test)` witnesses in
[grants.rs](../../crates/veoveo-hub/src/policy/grants.rs); its runtime code,
permissions, native collections and dependency pins were unchanged. The connected extension below includes one equivalent iterator closure spelling.
Verification remains manual; there is no CI hook, runner or new authorization gate.

## Current session-family authority

This manual increment qualifies the pure `SessionFamilyAuthority::allows` sub-boundary
of HUB-OAUTH-01, using the same typed identity/collection models as HUB-AUTH-01.
Its [separate receipt](evidence/session-family-receipt.json) records the source,
ordinary primitive models, exact proof, focused native checks, four isolated faults
and fresh restoration. The [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-session-family-qualification.md) separates the measured guarantees and remaining boundaries. The production method is byte-identical to the Hub base;
Rust additions are only `cfg(test)` witnesses. Earlier receipts remain historical.

[SessionFamilyProof](SessionFamilyProof.lean) proves the actual extracted method
returns exactly `.ok (decide Admitted)`, with both acceptance and refusal
characterizations. The independent conjunction contains only existing conditions:
family-ID presence, no revocation, strict family expiry, the supplied resource and
authorization server, OAuth client and Work Context, flat and retained principal
identity/tenant, retained kind/subject, token/current-principal issuer and subject,
and token scopes contained in family scopes. Empty and proper scope subsets are
valid. The expiry comparison retains seconds and nanoseconds: equal expiry refuses,
and one nanosecond later in the same second can accept.

Raw observed-field projections prove result equivalence when those fields stay
fixed. Refresh generation and retained original IdP issuer are intentionally absent;
valid rotation does not revoke access, and the retained IdP issuer may differ from
the Hub issuer of the verified token/current principal. The token and current
principal issuers must still agree. Coherent-family acceptance is proved for arbitrary
retained issuer/generation under the raw binding and subset conditions, without an
assumed session-result Boolean.

The method checks only that the token has some family ID. Its family projection has
no ID to compare. The state caller selects the authoritative record by the exact
signed ID before calling `from_stored` and `allows`. Verified JWT/Principal
projection, Serde decoding, negative/corrupt generation rejection, store identity,
freshness, read deadline, clock sampling and async error/response behavior remain
caller boundaries. Unbound service/exchange tokens use the outer caller's existing
authority; this pure method itself refuses a missing family. Task-specific session
compromise/continuity behavior is a separate path.

Ordinary typed equality, full lexical timestamp comparison and existing finite-set
subset laws supply the new primitive models; no native session-result or comparison
axiom was added. Native Chrono timestamp projection/order preservation, typed-ID
validation/equality, collection ordering/Borrow, compiler/extractor and memory/alias
refinement remain explicit. UTC dictionary/calendar/formatter payloads are not read
by admission; formatter delegation uses the pinned Aeneas approximation, not a
native rendering proof. Executable audits for actual `allows` and the new theorems
permit only `propext`, `Classical.choice` and `Quot.sound`. That standard-only axiom
closure does not discharge native refinement assumptions.

The Linux native baseline and restoration run four pure-method tests, formatting
and strict library Clippy. They cover proper/equal/empty scopes, 13 independent
binding/scope mismatches, missing/revoked families, strict expiry including one
nanosecond, rotation, retained issuer and unread metadata. Separately, the existing
Mac `session_family` integration test uses real private PostgreSQL 18.6 and two
HubState/store clients, including rotation and shared-store revocation. This is
component evidence, not a deployed replica or complete authentication endpoint.
Fixtures are explicitly owned synthetic identities; the pure tests do not write
family records or validate JWT signatures.

The isolated faults admit at equal expiry, bypass resource equality, reverse scope
containment and add a generation-zero restriction. Each compiles and translates,
builds the generated model/helper prerequisites, then fails unchanged
`allows_exact` and its exact native witness. The scope witness uses a strict token
superset to show unauthorized acceptance; a separate positive-scope mutant run is
not claimed. Symbolic typed-program/predicate residuals are distinguished from a
normalized False goal; failed-declaration recovery axioms are never accepted proof.
The first control's summary retains a false candidate flag because its detector
missed Rust's custom assertion message. The preserved expected panic, test count,
exit status and binary identity support the explicit manual acceptance; later
controls use exact expected-message detection.

Native Boolean values are inferred from the failed positive/negated assertions and
verified source; the custom-message logs do not print a separate actual/expected
Boolean pair. Each mutant test stops at its first assertion failure. Per-stage
binary hashes are historical captures at a reused target path; they do not claim
all older binaries remain there. Fresh unmutated restoration preserves raw generated
outputs and reports exact source-comment normalization separately from body/type
correspondence. No production rule, CI hook, runtime framework, deployment, full
repository Docker gates or paid model qualification was added or performed.

## SDK access and registered-policy composition

This increment adopts published SDK
`239622d12c08b771cc5e813fe3e846385d8d04ad` and imports its pinned
`sdkAuthorizationProof` Lake package. The [separate receipt](evidence/sdk-access-composition-receipt.json)
records exact inputs, tool stages, native binaries, three wrapper controls and
fresh restoration; the [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-authorization-qualification.md) summarizes the measured connection. Earlier qualifications and their receipts remain historical.
The Hub's runtime policy algorithm is unchanged; the native fixture additions are
`cfg(test)`. The SDK edition-2024 change applies only to its proof extraction crate.

The combined extraction retains actual SDK `access`, `NativeMatcher` and its
parse/Option closure. The foreign generic resolver uses an ordinary model that
calls the pinned SDK-owned generated `decision.resolve` through field-exact cursor
and matcher adapters. No resolver algorithm or desired result is copied here.
The local edition-2024 extraction emits exactly the SDK's previously qualified
model. Charon retrieves optimized MIR for dependency functions; that lowering's
partially moved loop cleanup fails the pinned Aeneas translator. Connecting the
foreign call to the local generated model therefore retains an explicit
source-compilation/linkage boundary; source/model hashes do not prove it.

[SDKAccessProof](SDKAccessProof.lean) proves the actual public wrapper returns
exactly `.ok expectedAccess` for Tool, Prompt, ResourceTemplate and Resource,
including absence. Exact resources override templates without parse/matcher-success
hypotheses. Otherwise resource access requires at least one raw matching template
and complete `RouteAccess` agreement among every raw match. `Authenticated` and
`Permission(p)` are distinct values, as are different permission names.
[SDKAccessCorollaries](SDKAccessCorollaries.lean) expresses raw declaration intent,
exact absence, matching occurrence and conflict without assuming authorization.
[SDKRegisteredPolicyProof](SDKRegisteredPolicyProof.lean) supplies the former
whole-access premise from that actual wrapper theorem, derives the ordered
registered-policy result and current grants, and composes with the extracted
narrowing kernel. It includes legitimate `Authenticated` acceptance with no current
grants. The independent server/requirement and request target are related only by
the explicit coherent-call condition.

Native symbols are exactly template `parse`, template `matches`, and the sampled
clock. Parse/decode and native-template-to-generated-view trace laws remain
explicit; no implementation or constructive interpretation of these symbols is
claimed. A nominal template token asserts no parser grammar or Rust borrow
identity. Raw map models propagate supplied Borrow/comparison operations; native
membership refinement retains unique-key, ordering and Borrow coherence
obligations. Collection, memory/alias, primitive parser, extraction-tool and
compiler/linkage boundaries remain. Exact decision theorems retain successful
operational lookup/clock premises, rather than proving native totality, clock
accuracy or freshness. The complete async endpoint, native wrapper/caller wiring
and deployment are outside this extraction.

The combined formal checkpoint and fresh restored package pass executable axiom
whitelists. Earlier role/Held proofs retain standard Lean axioms only; new access
facts may use parse/matches, and policy composition may also use the clock. No
accepted proof contains `sorryAx`. The
[adapter checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-adapter-models.md)
and [extraction diagnosis](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-extraction-boundaries.md)
record the measured translation boundary.

Native Rust 1.99 checks exercise three registered catalog tests, one actual
SDK-to-policy-to-narrowing test with 23 whole-record cases and eight direct SDK
observations, and two existing delegated tests. Formatting and strict library
Clippy also pass. Case-local Cargo path overrides resolve SDK, fixture and macros
to isolated SDK239 sources; only those three Git source identities change in the
scratch lock. Working dependency pins and shared dependency sources are untouched
by controls. The fixture tests real native lookup/clock/policy code with synthetic
owned declarations, not an async endpoint or deployed environment.

Three legal SDK wrapper mutations are paired with unchanged proof and native
assertion failures: discarding an exact-resource hit, discarding the parsed matcher
result, and discarding a declared prompt lookup. Compile, extraction, generated
model and helper builds must pass first. The exact-hit residual invokes the
resolver loop with `none` despite an exact hit; the matcher residual discards the
native Boolean; the prompt residual evaluates the declared lookup then returns `none` instead of its value. These
are reviewed semantic program mismatches, not automatic Lean counterexamples.
Failed-declaration recovery `sorryAx` is rejected and provides no accepted proof.
The native test stops on its first mismatch; later cases in that mutant run are
not claimed as executed.

Fresh unmutated restoration recompiles SDK and Hub, repeats all six native tests,
formatting and strict Clippy, re-extracts, builds the full proof package and checks
new proof files and all three model audits. Generated output bytes are preserved.
Raw hashes differ from the earlier Git-dependency extraction when generated
`Source:` locations name the isolated SDK tree; comparison normalizes only the exact old/new SDK root prefix on `Source:` comment
lines, preserving source spans, the rest of each line and every declaration/body.
Binary hashes are historical per-stage captures at a reused target path, not
claims that each old binary is still present. No deployment, full Hub Mac/Linux
Docker gates or paid model qualification was performed for this increment.

## Conditional registered-server policy and downstream connection

This earlier qualification used Hub `bb2e97f5c984a53eaa218109e4c01a70e952e365`, compiled SDK
`05273d1f90b9a08e90e377187f4eddd084605fe5`, and the same pinned extraction/Lean
tools. That increment left runtime policy code and dependency pins unchanged; only
`cfg(test)` witnesses and proof artifacts were added. The [separate receipt](evidence/registered-server-policy-receipt.json)
records the checked source/model identities, native baseline, five semantic
controls and fresh restoration. Earlier receipts remain historical and unchanged.

[RegisteredServerPolicyProof](RegisteredServerPolicyProof.lean) proves actual
`registered_target`, the catalog/SDK call trace of `declared_route_access`, all
14 closed kernel action-name spellings and full decision/denial metadata assembly.
The route and clock traces preserve external failure/divergence rather than
inventing fixed results. [RegisteredServerDecisionProof](RegisteredServerDecisionProof.lean)
proves the actual `decide_server` returns exactly its ordered decision table, with
G derived from the qualified role/risk evaluator. It covers both acceptance and
refusal, not only a subset bound:

- Ineligible actions and missing servers refuse before later route decisions.
- Explicit missing Tool/Prompt/Resource/ResourceTemplate routes retain their distinct
  reasons. Their required permission is None; every base denial has empty grants.
- `Authenticated` accepts with empty G. Server/Task/PlatformTask fallback requires
  nonempty G. Permission routes accept iff the required p is in G, and preserve
  `Some(p)` even on `MissingPermission` denial. Success returns the entire G.
- Every other output field preserves the sampled timestamp, request attribution,
  action/target, supplied policy version and trace; the server paths have no rule ID.

The raw theorem retains independent supplied server/requirement and request.target
arguments. [RegisteredServerAdmissionProof](RegisteredServerAdmissionProof.lean)
uses the actual `registered_target` equation for coherent-call corollaries and
derives accepted required-permission membership from the actual evaluator.
[RegisteredServerDownstreamProof](RegisteredServerDownstreamProof.lean) then calls
the previously extracted authority `narrow` kernel with the established native
collection laws. It discharges the former current-policy consistency premise:
accepted permission routes retain p in G intersect carried, and every accepted
delegated output has exactly that complete intersection. `Authenticated` permits
an empty intersection; server/Task access requires a nonempty intersection.

The wrapper projection faithfully represents `mem::take`: kernel metadata holds
the base record with its grant field emptied, then reconstruction restores the
kernel effect/reason/grants and preserves all other fields. Prior denied input is
unchanged when managed=true. Managed=false takes precedence and can retain the
incoming grant payload. Thus empty grants on every denial is a property of
`decide_server`, not of all downstream denied records. Native route classification
and this wrapper/caller correspondence are reviewed/tested adapters; the complete
async caller was not extracted.

### Explicit native boundaries

That qualification's `NativePolicyModelAudit` allowed exactly TWO additional native operation symbols:
`chrono.offset.utc.Utc.now` and
`veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access`.
Pure projections, action/clone models and all earlier role/Held claims retain
standard-only axiom closures; declared-route facts may add only SDK access,
metadata assembly only the clock, and actual full decisions/downstream traces
only the applicable subset of these two. Executable per-declaration guards reject
unexpected axioms and `sorryAx`.

The exact total decision theorems assume a sampled clock `.ok τ`, installation
lookup, and total successful catalog/SDK access interpretations for ALL arguments.
These are explicit operational premises, not proofs of native totality or just
one observed call. No fixed-zero clock or deny-all SDK implementation is supplied;
no constructive implementation of the native axiom constants is claimed. The
focused native fixtures exercise real SDK05273 lookups, clock sampling and both
positive and negative policy outputs, without discharging those universal laws.
That snapshot did not adopt SDK239622d's qualified matcher seam. The newer
SDK adoption and connected qualification are recorded above.

Ordinary native value models retain the documented collection/equality/ordering,
borrow/clone, iterator, parser/allocator, memory/alias and extraction-tool trust
boundaries. The closed action-name spellings are kernel checked; the whole Core
parser implementation was not extracted. Catalog freshness, full outer principal/
profile/scope admission, async caller construction, signing and session binding
remain outside these theorems. No deployed or model-driven acceptance run, whole-Hub
Mac/Linux gate, benchmark or new runtime authorization rule is claimed.

### Regenerate the current superset manually

In an exact scratch source copy, apply the existing [naming metadata patch](extraction/catalog-field-name.patch)
only to that extraction copy. The native source keeps its original catalog field.
With PROOF_PACKAGE pointing to the maintained package and an empty absolute output
directory, use the pinned tools listed below:

```sh
POLICY_PROOF_OUT="$(mktemp -d)"
patch -p1 < "$PROOF_PACKAGE/extraction/catalog-field-name.patch"
SQLX_OFFLINE=true RUSTUP_TOOLCHAIN=nightly-2026-09-17 charon cargo --sysroot default --preset=aeneas \
  --start-from '{impl veoveo_hub::delegation::authority::Held for veoveo_hub::delegation::CurrentPermissions}::held' \
  --start-from 'veoveo_hub::policy::evaluation::decide_server' \
  --start-from 'veoveo_hub::policy::evaluation::declared_route_access' \
  --start-from 'veoveo_hub::policy::evaluation::registered_target' \
  --include '{impl veoveo_hub::delegation::authority::Held<_> for veoveo_hub::delegation::CurrentPermissions}::held' \
  --include 'veoveo_hub::policy::evaluation::decide_server' \
  --include 'veoveo_hub::policy::evaluation::declared_route_access' \
  --include 'veoveo_hub::policy::evaluation::registered_target' \
  --include 'veoveo_hub::policy::evaluation::decision' \
  --include 'veoveo_hub::policy::evaluation::deny' \
  --include 'veoveo_hub::policy::grants::held_roles' \
  --include 'veoveo_hub::policy::grants::effective_risks' \
  --include 'veoveo_hub::policy::grants::granted_permissions' \
  --include 'veoveo_hub::control_plane::authorization_config::_::allows' \
  --include 'veoveo_hub::contract::actions::_::targets_registered_server' \
  --include 'veoveo_hub::contract::actions::_::mcp_method' \
  --include 'veoveo_hub::contract::actions::_::name' \
  --opaque 'veoveo_hub::mcp::upstream_http::HubUpstreamHttpClientPool' \
  --opaque 'veoveo_hub::contract::catalog::SectionDeclaration' \
  --opaque 'veoveo_hub::contract::catalog::TargetDeclaration' \
  --opaque 'openshell_sdk::config::AuthConfig' \
  --opaque 'veoveo_hub::registration::snapshot::_::iter::_::call_mut' \
  --opaque 'prost_types::protobuf::value::Kind' \
  --opaque 'serde_json::value::Value' \
  --dest-file "$POLICY_PROOF_OUT/hub_current_roles.llbc" \
  --rustc-arg='-Zcrate-attr=feature(register_tool)' \
  --rustc-arg='-Zcrate-attr=register_tool(charon)' \
  -- --manifest-path crates/veoveo-hub/Cargo.toml --lib --locked
aeneas -backend lean -filter-trait-methods -split-files \
  -dest "$POLICY_PROOF_OUT" "$POLICY_PROOF_OUT/hub_current_roles.llbc"
```

Review LLBC transparency and generated identities, then copy only untouched
Types.lean and Funs.lean into HubCurrentRoles/. Preserve handwritten external
models; never replace them with generated axiom templates. From this package:

```sh
lake build
lake env lean RegisteredServerPolicyProof.lean
lake env lean RegisteredServerDecisionProof.lean
lake env lean RegisteredServerAdmissionProof.lean
lake env lean RegisteredServerDownstreamProof.lean
lake env lean ModelAudit.lean
lake env lean CurrentPermissionsModelAudit.lean
lake env lean NativePolicyModelAudit.lean
```

### Focused native witnesses and semantic controls

Run against ORIGINAL unannotated native source, Rust1.99 and SQLX_OFFLINE=true.
Use a fresh native target or explicitly clean/recompile the Hub package. The
measured run retained dependencies, required actual Hub compilation from each
source copy, and captured binary identities before/after each control witness.
Reused target paths do not preserve every historical binary at that path.

```sh
SQLX_OFFLINE=true rustup run 1.99.0 cargo fmt --all --check
SQLX_OFFLINE=true rustup run 1.99.0 cargo test --locked -p veoveo-hub --lib catalog::tests::registered_
SQLX_OFFLINE=true rustup run 1.99.0 cargo test --locked -p veoveo-hub --lib mcp::authorization::tests::actual_registered_policy_narrows_current_grants -- --exact
SQLX_OFFLINE=true rustup run 1.99.0 cargo test --locked -p veoveo-hub --lib mcp::authorization::tests::delegated_
SQLX_OFFLINE=true rustup run 1.99.0 cargo clippy --locked -p veoveo-hub --lib -- -D warnings
```

The groups contain 3 catalog tests, 1 actual-base/narrow integration test and
2 existing assembly tests. Fixtures include two current Read permissions and one
ungranted Write, complete expected records, real Authenticated-empty acceptance,
missing target kinds, Server/Task/PlatformTask and managed/prior-denial behavior.
Expected metadata is independent of actual output except the observed timestamp,
which is bracketed and then checked for exact preservation. These fixtures do not
contact the synthetic identity-provider/JWKS URLs or call a model provider.

| Isolated patch | Primary unchanged obligation | Observed native mismatch |
|---|---|---|
| [authenticated-needs-grant](mutations/registered-authenticated-needs-grant.patch) | decide_server_exact Authenticated branch | Deny instead of valid empty-G Allow |
| [omit-denied-required](mutations/registered-omit-denied-required.patch) | decide_server_exact nonmember Permission branch | None instead of required Some(write) |
| [bypass-membership](mutations/registered-bypass-membership.patch) | decide_server_exact nonmember Permission branch | Write route allowed with two Read grants |
| [wrong-resource-reason](mutations/registered-wrong-resource-reason.patch) | decide_server_exact Resource/ResourceTemplate branches | Resource UnknownTool instead of UndeclaredTarget |
| [drop-tool-requirement](mutations/registered-drop-tool-requirement.patch) | registered_target_exact Tool projection | Missing Tool allowed through server fallback |

Each legal control compiled/extracted and built its model/helper prerequisites
before the unchanged primary proof and exact native assertion failed. The first,
third, fourth and fifth failures are symbolic program/record/projection mismatches;
only the second normalized to False. These are not all Lean-generated concrete
counterexamples. Error-recovery sorryAx in failed declarations is excluded. The
resource mutant's native test stops at Resource; its ResourceTemplate mismatch is
formal, not a separately executed negative native case.

The first native draft used parse instead of the actual Core new URI constructors;
that compile failure executed no tests and was corrected only in cfg(test).
Control-launch missing-input setup attempts were excluded before mutation stages.
No baseline proof/model was changed to accommodate a control. The native-only baseline2 reused the earlier checked formal inputs after an exact
39-file correspondence review; it did not perform a new pre-control extraction.
Fresh unmutated restoration regenerated the exact original Types/Funs and passed
all focused native, complete package proof, four direct theorem and three audit
gates. Native binary hashes are per-stage historical measurements at a reused
target path, not separately retained binaries at that path.


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
route-access operation is part of this Held-only closure. The separate conditional
server-decision extension above adds `decide_server`; its outer admission/caller and
native contract boundaries remain explicit. Verification remains manual, with no new runner or gate.

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
That qualification's compiled `05273d1` dependency did not adopt the separate SDK
route proof; the newer `239622d` connection is recorded above. An Authenticated
route can legitimately have an empty permission set;
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

The following is the HISTORICAL four-function extraction recipe. The historical
connected Held receipt uses the Held-only scratch recipe above. The current
conditional package uses the superset recipe at the top, retaining all earlier
bodies and their documented native boundaries:

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

## Refresh binding and delivery predicates

[RefreshPredicateProof.lean](RefreshPredicateProof.lean) proves exact total outputs
of the extracted `RefreshFamily::bound_to` and `deliverable` bodies. Binding is
three-field equality. Delivery requires the complete consumed/replay/revocation,
family, checked-generation, current-generation, token-expiry and envelope-window
conditions. The proof includes a positive coherent delivery, strict refusal at the
delivery deadline, and the checked generation's unbounded successor equation.

The field-exact `sessionAuthority` projection connects generation-only family
updates to the already checked session admission invariant at the same request
and time. Actual store decoding, `from_stored`/Serde and caller wiring remain
outside this mathematical projection. No plaintext, crypto or digest operation is
modeled: UUID, digest and secret-envelope payloads are nominal values. Family-ID
equality and Chrono timestamp projection/order are ordinary primitive models,
with native representation correspondence still explicit.

The complete existing package and prior audits pass with the generated refresh
superset; the new predicate/audit closures use only standard Lean axioms. The
[compact checkpoint](evidence/refresh-predicate-checkpoint.json) records exact
inputs, commands, timings and logs. The [extraction record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-refresh-predicate-extraction.md)
retains the source/body inventory and runner correction. Existing generated
definitions are unchanged after the recorded Source-comment comparison.

Run `lake build RefreshPredicateProof` and `lake env lean RefreshModelAudit.lean`
manually. This checkpoint proves the two pure predicates and mathematical
session projection within the stated models; semantic fault controls remain pending. Refresh transaction ordering, SQL/lock semantics, maintenance
interleavings, ciphertext recovery and HTTP response behavior require their
separate model/native evidence; this is not refresh transaction qualification.
