# Hub proof and evidence map

Track important system properties, their implementation boundaries and the evidence
for them. The unit of coverage is an invariant, not a theorem count. A small helper
proof does not establish the whole request, transaction or runtime lifecycle.

Verification remains manual and on demand. The comprehensive coverage roadmap below
is approved for implementation. This map adds no CI job, hook, watcher, proof
requirement or runtime check, and proposes no additional security rules.

## Source basis and status

The initial source review inspected local main
`4d5ae5b4ca56e4f8f0ea4e7766d7fe1e9a129f28` and independently observed origin/main
`5d3f43fb8fdc3b25436f9d7526ae7f9f48b662c5`, which removed named environments and
changed ordinary-deployment routing. Both are parents of the published merge
`fee5e5abc396146b37cfd455375a3276619648e1`; main and origin/main shared that revision
when this review concluded.
Read-only inspection of the merge confirms that `instance` calls `kernel::resolve`
and retains the exact proved kernel/model hashes recorded below. No new proof or
deployed run was performed at the merged revision, and broader fresh routing remains
unproved. Relative links navigate the current tree; function names and the original
review revisions identify the source context.
Historical named-environment checks are not forward-looking requirements here.

A later [source-only applicability audit](request_composition/evidence/source-applicability-20261031.json)
records local Hub `f8faea5`, fetched Hub `origin/main` `0ff0819`, local SDK `6afa18a`
and fetched SDK `origin/main` `7459481`. The Hub HTTP constructor, current-policy
call wiring and caller deadline body match the extracted basis byte for byte in
both Hub revisions. The remote nevertheless adds a separate non-MCP forwarder,
`AppOrigin` policy cases and `RegisteredServer.surfaces`; the existing graph and
complete-field theorems do not cover those additions. SDK issuer preparation and
the remote SDK's newer Core Task-wake fix also require deliberate source integration.
The audit adds no proof, native test or deployment result. Function-text identity
does not establish callee, type or compilation-context equivalence.

The SDK portion is now reconciled in merge `dbfd2a0` and qualified by SDK `e82ceb8`:
35 focused native tests pass, and fresh extraction with Core `4ebcf89` reproduces
the issuer/recipient model except for 95 generated source-location comment lines.
The declarations, bodies, types and external interfaces are unchanged; the existing
proofs retain their scope and assumptions. See the
[merged-source receipt](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/internal_auth/evidence/merged-source-20261031.json).
This does not update Hub's dependency pin or qualify its newer App-origin path,
changed registration record, or the unfinished caller control.

SDK `4e84876` subsequently publishes that historical proof history together with
upstream `db64f9ee`. Its nonproof source matches that upstream exactly; the older
private binder and formatted-identity preparation are not reapplied. The historical
proofs and receipts retain their original applicability, rather than becoming
proofs of the newer repository-grant flow. The
[publication receipt](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/internal_auth/evidence/published-history-integration-20261031.json)
records matching native inputs and full Mac/Linux Docker checks: 328 tests passed
and three were skipped on each host. SDK `db64f9ee` differs from `7360c0f` only in
the Apps design document. Current issuer/claims/verifier qualification remains open;
the unmodified `7360c0f`/Core `422dcf2` extraction includes repository grants and
decoded-claims identity reconstruction. This publication does not change Hub's pin.
The subsequent SDK
[translation investigation](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/internal_auth/evidence/current-linked-translation-20261031.json),
committed in `ce85c3d`, extracts all four issuer/recipient/verifier roots and passes
the own-graph inventory, but Aeneas rejects unsupported loop exits and function-pointer
types. No Lean output is produced. Current verifier continuity remains unproved.
The subsequent [repository-grant extraction](request_composition/evidence/repository-grant-translation-20261031.json)
retains all producer fields and reaches 38 transparent own functions, but the pinned
backend rejects a required associated-type equality in `Iterator::cloned` before
generating Lean. The independent SDK
[detail contract](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/repository_grants/README.md)
now has checked construction, projection and complete roundtrip proofs, including
valid empty-action read grants. An actual Rust action-dropping fault is caught by
both the unchanged proof and the existing native test; an independent witness
establishes the wrong output. Standard collection/clone correspondence remains an
explicit boundary. This does not replace the open producer or verifier requirement.

Hub merge `1ccb868` subsequently integrates upstream `0ff0819`, retaining all local
proof work. Its runtime/configuration bytes match that upstream parent exactly;
the published SDK pin is now `154a473`, with Core still `120306e`. Thirteen focused
native tests, formatting and strict library Clippy pass on the exact merged tree.
The [integration receipt](request_composition/evidence/merged-hub-source-20261031.json)
records the source and results. Re-extraction and formal qualification of the added
App-origin, registration-surface and administrative-revocation domains remain open
at this checkpoint. Native success does not extend the old model's domain.

The subsequent [current-domain qualification](request_composition/evidence/current-domains-20261031.json)
freshly extracts that exact merged Hub/SDK stack and checks the shared proof graph.
It covers the fifth authorization target (`AppOrigin`), including legitimate
zero-grant `Authenticated` access and exact permission/undeclared refusals; every
constructed and selected registration is an unchanged input occurrence, with all
surface fields and route order preserved; administrative revocation is included
in the refresh predicate's domain. Existing policy, session/refresh, routing and
caller targets pass with the refreshed types. This does not qualify the separate
HTTP forwarder, declaration validation, native SQL or deployed behavior. The
prepared SDK issuer bridge retains its own native-source equivalence boundary.

The [policy-to-issuer composition](request_composition/evidence/policy-issuer-composition-20261031.json)
connects the constructed and selected row, the same supplied generation's current
policy, optional carried permission ceiling, complete HTTP client and SDK request,
and exact issuer outcome. It derives client/grant provenance and representation
bounds from raw inputs. Missing-route and context-refusal branches are included;
context acceptance, a prepared client and a desired token are not premises of the
final theorem. This is composition of extracted subcalls with source-reviewed
wiring, not a proof of async acquisition/transport, TTL wrappers, recipient
verification or the prepared SDK's equivalence to Hub's native dependency.

The Task-continuity qualification is a separate source boundary: changed Hub files
based on `651390dbbb8b4e65b5b8bcd3f8714a981fac2371` and the exact sibling SDK files
based on `7e2489dccbee6d00b377b174f68f9d5de692a68b`. Its shared-source package records
both snapshots. At that qualification, Hub's SDK pin was
`a41c9cc6c81f6c0305dd41165defb142740ed6b1`; the private SDK refactor was not
claimed to be present in that dependency or deployed. Subsequent dependency adoption
has its own source-correspondence boundary.

The linked-runner integration combines local `305aeb2` with upstream `000f74e`.
It preserves the local cache/authority and live-watch corrections while adopting
upstream's linked routing, Core `422dcf2`, SDK `7360c0f` and Lore `c33e017`.
That merge left the frozen proof files unchanged and was **not** current-source formal
qualification. `DiscoveryCacheKey` now includes an optional link; registration and
selection include runner metadata; request construction contributes a repository
grant and calls `issue_with_repository`. The complete policy-to-issuer composition
still needs fresh extraction and review; cache key/order qualification is recorded below.
The unchanged generic selection and Task-decision kernels retain their recorded
proofs; new runner candidate assembly and deployment lookup are outside them.
Registration's new runner persistence path is also outside the old transaction
models. Native integration checks, when recorded, do not close these formal gaps.

The [integrated native check receipt](discovery_cache/evidence/integrated-native-checks-20261031.json)
qualifies Hub `c23d9ae`, including both discovery-watch corrections and the merged
Computer/linked-runner changes. `docker build --target check .` passed on Mac
Docker Desktop and Ubuntu: 454 tests passed and two were skipped on each, including
all six watch/subscription regressions. SQL metadata, formatting, strict Clippy,
dependency policy and feature checks also passed. These are component checks of
Linux containers on each host architecture, not new Lean coverage or deployment.

The [fresh cache extraction](discovery_cache/evidence/current-key-extraction-20261031.json)
now translates the five supported synchronous operations from that exact current
source, with the optional link and actual Core principal/server identifier derives.
All 37 discovered own-component function bodies are transparent and error-free.
The [current cache proof qualification](discovery_cache/evidence/current-linked-cache-proofs-20261031.json)
checks and adopts that unchanged generated model, its four model modules and all nine
proof modules, with 103 executable axiom guards. Full-key invalidation preserves
another link's complete cached value, ticket and retry state, including absent versus
present links. Existing operation/state/history guarantees pass for the extended key
domain. The actual identifier wrappers replace their old handwritten primitive
substitutes; standard-library and native-wrapper correspondence remain boundaries.
The [link-comparison source control](discovery_cache/evidence/link-order-control-20261031.json)
now qualifies that baseline: an actual Rust comparator fault causes the native cache
regression to fail and the unchanged comparison theorem to reject its wrong result.
An independent generated-function witness confirms the collision. Reversing the
fault restores the exact source/model and all nine proof modules pass afresh.
This deliberately injected fault is not another production defect; broader native
key construction, asynchronous discovery and database refinement remain open.

Keep two independent fields for each result:

- **Formal status:** `unproved`, `partial`, or `proved-within-stated-boundary`.
  Partial means named subproperties have proofs, not that the whole invariant does.
- **Applicability:** the exact source/model bytes and assumptions examined, or
  `not rechecked` for a changed implementation. Similar-looking code is not a
  source-correspondence argument.

Tests below are evidence locations, not claims that this review reran or passed
them. Receipts establish their own measured scope. A handwritten abstract state
machine proves a design model until refinement to actual Rust/SQL is justified.
No percentage or blanket “Hub verified” claim follows from this map.

## Priorities

| ID | System property | Formal status | Next useful work |
| --- | --- | --- | --- |
| HUB-CONTRACT-01 | Shared MCP extension contracts preserve meaning from declaration to enforcement | partial | Extend the qualified SDK-to-policy connection to declaration generation/validation and dispatch |
| HUB-TASK-01 | Captured Task routing and retry integrity | partial | SQL and asynchronous adapter refinement remain |
| HUB-AUTH-01 | Current policy and delegated authority do not amplify access | partial | Current policy through issuer outcome is composed; recipient continuity and native SDK equivalence remain |
| HUB-OAUTH-01 | Exact session binding and refresh-family lifecycle | partial | Refresh concurrency slice qualified; initial creation, caller/SQL refinement and cleanup composition remain |
| HUB-CONFIG-01 | Coherent live configuration publication | partial | Transaction model and native faults qualified; SQL refinement and replica adoption remain |
| HUB-ROUTE-01 | Deterministic fresh routing and stable captured routing | partial | Retained route/current-policy/issuer provenance is composed; async acquisition, TTL wrappers and combined source controls remain |
| HUB-CATALOG-01 | Catalog, cursor, cache and listener isolation | partial | Finite supported cache histories/fences and source controls checked; native watch publication defect fixed; formal listener/cursor and native refinement open |
| HUB-FACTORY-01 | Authoritative reconciliation and safe native lifecycle | partial (manual model) | Saved-parent histories/controls; allocation, retirement and native/distributed correspondence |
| HUB-COMPUTER-01 | Owner/session identity and version recovery | unproved | Model admission and version-recording transitions |
| HUB-REGISTER-01 | Registration, replacement and storage identity | partial | Register/replace/update concurrency and snapshot-to-route identity have scoped qualification; removal, full snapshot-input refinement and broader lifecycle remain open |
| HUB-LORE-01 | Native repository grants and contributed token issuance | unproved | Trace assignment/group decisions through common token assembly |
| HUB-CLEANUP-01 | Reference-preserving cleanup and audit consistency | unproved | Trace expiry, deletion races and transactional audit boundaries |

The invariant IDs remain stable. Routing and Tasks form one connected workstream;
the shared extension-contract foundation connects the ten approved coverage areas.
An unproved entry is not evidence that its implementation is defective.

## Approved implementation roadmap

The goal is assurance over complete operations: caller authority, destination,
committed state and recovery under concurrent changes, retries and failures. For
each operation, establish legitimate success and output preservation as well as
required refusals. In particular, retain declared `Authenticated` routes, native
Lore permissions, the existing Task/logout semantics and asynchronous propagation.

Implement in this dependency order, finishing a qualified slice before the next:

0. **Shared extension contracts:** after the qualified delegated-assembly slice,
   prioritize the SDK-to-Hub contract foundation below. Prove the semantics that
   producers and consumers share before accumulating more isolated internal
   predicates. The interface is part of the existing architecture, not a new API.
1. **Authority and session continuity:** production-linked delegation decisions and
   output/deadline assembly, then exact refresh-family admission. Current policy
   evaluation and its principal projection must have explicit boundaries; a set
   intersection lemma does not establish the whole exchange.
2. **Transactions:** configuration publication first, then refresh concurrency and
   registration/replacement. Connect the actual Rust transaction scope, SQL,
   constraints and locks to the state transitions. Include audit where it commits
   in the same transaction. Evaluate contributed token/registry publication against
   these established boundaries.
3. **Request composition:** connect registration identity, fresh routing, retained
   Task routing, current authority and SDK recipient validation; cover discovery,
   cursor and listener invalidation under concurrent publication. Include local
   catalog adoption and reconciliation after replica reconnect.
4. **Recovery:** factory and Computer transitions, interrupted operations and
   cleanup. Prove progress only under stated scheduling, retry and dependency-recovery
   assumptions; native and browser observations remain separate evidence.
5. **Connected guarantees:** compose the established contracts for an admitted call,
   a retained Task across configuration changes, concurrent refresh/registration and
   interrupted lifecycle operations. Reduce assumptions about our own code by using
   earlier results rather than silently broadening their scope.

Three evidence classes remain distinct:

- **Extracted production decisions:** actual shared Rust, generated Aeneas model and
  Lean proofs of successful/refused outcomes and returned values. Review concrete
  adapters and test the production caller path. Avoid changing efficient data
  structures or semantics just to make extraction easier.
- **State/concurrency models:** Lean specifications of operations and interleavings.
  They are design-model proofs until model-to-Rust/SQL correspondence is justified.
  Reviewed mappings and concurrent database tests support implementation confidence,
  but do not themselves constitute formal SQL refinement.
- **Integration and recovery evidence:** focused component, database, native-runtime
  and browser probes. None substitutes for another class or proves the external
  service itself.

For PostgreSQL, assume only the atomicity, isolation level, locking and constraint
semantics actually used by the queries. Do not silently assume serializability,
cross-database/RPC atomicity or exactly-once external effects. Cryptography, the
Rust/extraction/Lean toolchain, clocks and external services are explicit trust
boundaries. Liveness additionally names fairness and recovery assumptions. No
assumption may merely restate the property claimed as proved.

Each completed slice records source/model identity, specifications, assumptions,
axiom audit, useful semantic mutants, restored baseline and relevant caller tests.
The existing packages and receipts are enough; no proof service, CI gate, new
runner or separate bookkeeping format is introduced. Source changes make prior
applicability historical until rechecked. Count covered contracts and remaining
obligations, not theorems or a blanket percentage of Hub verified.

## HUB-CONTRACT-01 — Shared extension contracts

**Invariant/failure prevented.** A typed server contract has the same meaning when
it becomes a route table, a registration document, an MCP advertisement, a Hub
authorization decision and a server dispatch requirement. No representation or
consumer silently changes the permission, route, service identity or declared use.
Legitimate routes remain usable, including explicit `Authenticated` routes with
no permission requirement. This is a cross-repository foundation, not a new check
in front of an already admitted request.

The SDK owns these protocols and their types. Start with `ai.veoveo/authorization`
and `ai.veoveo/registration` in MCP `capabilities.extensions`, linked to the
registration document's `veoveo_authorization` and identity fields. The same typed
authorization declaration carries permissions, resource types, access kinds, risks,
route requirements, downstream `uses` and `callsHub`. Preserve the distinction
between MCP camelCase and OAuth snake_case without duplicating their semantics.

- **Source owners:** [registration design](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/registration/DESIGN.md),
  [authorization types/check/access](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/registration/authorization.rs),
  [resource-template matching](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/registration/authorization_template.rs),
  [MCP advertisement/readback](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/registration/mcp.rs),
  [OAuth representation](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/registration/oauth.rs),
  [typed server setup](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/server_contract.rs)
  and [route construction/dispatch](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/crates/veoveo-hub-sdk/src/hosting/routes.rs).
  Hub's [registration service](../crates/veoveo-hub/src/registration/service.rs),
  [policy evaluation](../crates/veoveo-hub/src/policy/evaluation.rs) and delegated
  assembly consume the admitted contract.
- **Properties:** declaration generation matches typed route policy; registration
  and MCP representations preserve all semantic fields; existing declaration
  validation has exact success/refusal conditions; target lookup preserves exact
  resource precedence and refuses ambiguous template access while accepting
  unambiguous routes; current policy/risk evaluation, discovery and SDK dispatch
  agree about what a declared route requires. Trace the actual registered-versus-
  advertised relationship before claiming runtime equality checking.
- **Evidence locations:** SDK `tests/registration_wire.rs`, `tests/server_contract.rs`,
  native route/template tests and Hub policy/registration/calls tests. Wire round trips
  and generated-schema checks are test evidence unless their serialization mapping
  is itself proved. Rust traits establish structure, not behavioral equivalence.
- **Qualified formal boundary:** the SDK's
  [manual authorization package](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/authorization/README.md)
  proves actual shared Rust resource resolution and parsed root/path/query matching.
  Exact entries win without visiting templates; otherwise all matching complete
  access values must agree. The first conflict stops at its immediate next cursor.
  Total matcher results cover literal/scalar/tail segments, trailing slashes and
  query names, accepting valid empty tails and explicit `Authenticated` requirements.
  A composed generated-function trace derives matching before applying agreement;
  it does not assume an already-authorized candidate list. A concrete interpretation
  establishes satisfiable primitive laws. Five legal semantic mutations fail their
  intended theorem and separate Rust witness, and a fresh restored run passes.
  The [receipt](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/authorization/evidence/authorization-receipt.json)
  identifies SDK `8489a58d12f880e88bb71b226c720e57a5399084` plus its exact overlay.
  These decisions are **proved-within-stated-boundary**; HUB-CONTRACT-01 stays partial.
- **Boundary:** standard MCP transport, Serde/Schemars, cryptography and domain handler
  correctness remain separate. Prove our semantic transformations and decisions;
  do not claim every conforming server's application behavior is verified. Pure SDK
  contracts are proved at their owner; Hub composes those results. Protected hosting
  and durable-Task modules keep their existing ownership and admission rules.
- **Residual/next:** native exact lookup/iteration, template grammar, Core decoding,
  primitive text/set semantics and parent call/borrow wiring remain explicit adapter
  obligations. Borrow identity has source review and native pointer-identity tests,
  not a Lean alias proof. Extend the qualified route/current-policy result below to
  identity, declaration generation/validation and typed-route dispatch. Map
  Tasks, usage metadata, discovery degradation and App dependency metadata to their
  owning contracts and relevant guarantees. They are not all interchangeable
  `capabilities.extensions` declarations: some use `_meta` or native MCP extensions.

The qualified [authority package](authority/README.md) consumes decoded
authorization semantics and proves only its stated internal sub-boundary. It does
not discharge this contract chain or prove serialization. The SDK matcher result
supplies the sub-boundary used in the Hub policy composition checkpoint below;
the asynchronous caller remains separate. SDK `239622d` is published after
exact-source Mac/Linux component checks; see the
[publication record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-authorization-publication.md).
Hub now pins SDK `239622d`; its access-wrapper extraction and the composed
qualification below have passed.
The qualified Hub server-evaluator receipt below still identifies SDK `05273d1`;
neither publishing nor updating a pin alone establishes the new proof connection
or deployed applicability.
The adoption investigation also found different compilation contexts: the SDK's
standalone Rust proof crate originally used edition 2021, while the production
crate uses edition 2024, and pinned Charon reads promoted MIR for the local proof function
but optimized MIR for that function as a foreign dependency. Source-byte identity
alone does not discharge this correspondence. An isolated edition-2024 extraction
now produces a byte-identical qualified SDK model, and the SDK proof crate has
been aligned to edition 2024. The foreign optimized cleanup
still fails Aeneas translation. The approved connection reuses the SDK-owned
generated resolver through typed adapters, preserving explicit compilation/linkage
and native-template trace boundaries. See the
[extraction checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-extraction-boundaries.md).
The combined wrapper extraction now succeeds with that explicit resolver interface;
see
the [wrapper extraction record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-wrapper-extraction.md).
The ordinary resolver model, field-exact adapters and actual NativeMatcher
parse/Option/closure trace now pass the full package build and explicit audits;
see the [adapter model checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-adapter-models.md).
The generated access wrapper now has a total result theorem for all four target
kinds. Raw declaration membership and unanimous complete template requirements
derive its result; the actual server-policy and downstream-delegation proofs use
that result instead of a whole-access hypothesis. Exact resource precedence,
matching conflicts, current grants, full output and zero-grant Authenticated
acceptance are covered within the explicit models. The combined package and three
audits pass; see the
[policy composition checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-policy-composition.md).
The newer SDK adoption also passes six focused native tests, including 23 whole
policy-output scenarios and eight direct SDK observations, plus formatting and
strict library Clippy; see the
[native baseline](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-native-baseline.md).
The connection is now **proved-within-stated-boundary and qualified**: three SDK
wrapper mutations compiled/extracted/built and failed the intended unchanged proof
plus native assertion; fresh restoration passes all six native tests, formatting,
strict library Clippy, extraction, full Lean build, four direct proofs and three
audits. The failures are symbolic semantic residuals with concrete Rust
counterexamples, not normalized Lean `False`. Restored generated files preserve
every definition/type; raw hashes differ only in SDK source-root comments, with
that exact normalization and raw outputs recorded. See the
[qualification](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-sdk-hub-authorization-qualification.md)
and [receipt](current_roles/evidence/sdk-access-composition-receipt.json).
Native parsing/matching and collection adapter laws, compiler/module linkage and
asynchronous caller correspondence remain explicit obligations. No deployment or
declaration-generation/serialization proof follows from this qualification.
The previously qualified model remains identified by its original receipt.

The following inventory records the inspected SDK `8489a58` snapshot. Separate
extension naming/removal work is outside this qualification; current SDK definitions
own the active surface. This snapshot explains the proof trace, and presence in it
is not evidence of formal coverage:

- `ai.veoveo/registration` and `ai.veoveo/authorization` are advertised in
  `capabilities.extensions`. Cover identity/declaration preservation, route
  interpretation and its use by authorization as described above.
- `ai.veoveo/usage` is Task-result and `tasks/get` metadata. Connect its projection
  to the owning usage ledger's amount, currency and estimate/actual semantics;
  recording and deduplication require their own transactional boundary.
- `ai.veoveo/task-retention-pin` is request metadata for internal Task evidence
  retention. Distinguish evidence retention from protocol TTL, authority and Task
  liveness, and connect it to the cleanup/reference obligations.
- `ai.veoveo/hub-discovery-degradation` is list-result metadata. Cover exact
  attribution of isolated discovery failures alongside the catalog/cursor proof;
  a partial result must not silently claim completeness.
- `ai.veoveo/app-resource-dependencies` and `ai.veoveo/app-tool-dependencies` are
  App dependency metadata. Connect declaration filtering and projected names to
  the same current authorization used for the eventual resource or tool call;
  listing a dependency is not an independent authority grant.

The last five use `_meta`, not capability declarations. Upstream MCP Tasks and MCP
Apps remain upstream contracts. Prove Veoveo's adapters and semantic projections
without treating the upstream protocol or every handler as verified.

## HUB-TASK-01 — Captured Task routing and retry integrity

**Invariant/failure prevented.** A retained Task continues to its original deployment
and composition. A same-deployment retry preserves its first capture and expiry;
another deployment or mismatched owner/profile/WorkContext/authority cannot rebind
that route. A changed proposed composition is not itself a reason to replace the
stored capture. Prevents Task misrouting, ownership confusion and retention extension.

- **Source:** [shared decisions](../crates/veoveo-hub/src/state/task_routes/decision.rs),
  [creation/capture check and reusable](../crates/veoveo-hub/src/state/task_routes.rs#L267),
  [conflict-preserving insert](../crates/veoveo-hub/src/state/task_routes/create.sql),
  [authorize_task_route](../crates/veoveo-hub/src/mcp/authorization.rs#L84),
  [Task forwarding](../crates/veoveo-hub/src/mcp/tasks.rs#L31), and
  [ownership comparison](../crates/veoveo-hub/src/state/task_routes.rs#L83).
- **Formal boundary:** [manual Task package](task_continuity/README.md) proves total
  label containment and exact owner/capture/retry/recipient outcomes, including
  success iff the conditions hold and preservation of the original canonical/source
  IDs, server, deployment, capture, expiry and opaque current caller payload. The
  composed extracted-function trace uses the actual private SDK recipient decision
  with an arbitrary expected recipient; it is not a proof of asynchronous Hub dispatch.
  These decisions are **proved-within-stated-boundary**; the system property stays partial.
- **Tests/adapters:** [task route tests](../crates/veoveo-hub/src/state/task_routes/tests.rs):
  `same_deployment_retry_preserves_first_execution_capture`,
  `identical_source_tasks_from_two_deployments_never_share_a_route`, and concurrent
  projection/changed-authority/expiry cases. The retry test now explicitly uses a
  different proposed composition selecting the same Task deployment.
  [Whole-subject adapter regression](../crates/veoveo-hub/src/mcp/task_ownership_tests.rs#L93)
  checks that only execution is replaced. [Routing](../crates/veoveo-hub/tests/routing.rs)
  and [calls](../crates/veoveo-hub/tests/calls.rs) exercise retained routing/delegation.
- **Assumptions/applicability:** exact source/model snapshots; total, correct `eq`
  and `ne`; faithful typed identity/label projections; exact strict time ordering.
  The [receipt](task_continuity/evidence/task-continuity-receipt.json) records fresh
  baseline, four legal semantic mutants and restored passes, plus the exact focused
  Rust targets. Mutants compile/extract but fail unchanged semantic proofs and concrete
  witnesses. No deployed or model-driven run is claimed by this slice.
- **Residual/next:** authoritative SQL lookup/expiry selection, uniqueness/concurrent
  insertion, pruning, current policy, Task/session liveness, selected-registration
  endpoint association, the concrete subject/token adapters, cryptography and transport
  remain outside the formal boundary; review and focused tests cover only the cases
  in the receipt.
  Core's Task claim/revision/cancellation/recovery SQL is not proved. A serious next
  lifecycle proof needs implementation-linked SQL refinement, not a Rust shadow reducer.

## HUB-AUTH-01 — Current policy and delegated authority

**Invariant/failure prevented.** A hop cannot manufacture permissions or extend its
admitted lifetime; it preserves original caller/captured routing while naming the
actual actor. Current required scopes and declared uses still apply. Hub-owned rule
evaluation gives matching deny rules precedence and otherwise denies without an
allow. Delegated callers do not acquire administrative access from an admin principal.

- **Source:** [native adapters/issue/subject_of](../crates/veoveo-hub/src/delegation.rs),
  [shared authority decisions](../crates/veoveo-hub/src/delegation/authority.rs),
  [policy decide and assemble_rule_outcome](../crates/veoveo-hub/src/policy/evaluation.rs#L38),
  [MCP delegation narrowing](../crates/veoveo-hub/src/mcp/authorization.rs#L404),
  and [existing delegated-admin refusal](../crates/veoveo-hub/src/http/action_audit.rs#L45).
- **Tests:** [calls scenario](../crates/veoveo-hub/tests/calls.rs#L66) checks narrowed
  downstream permissions, actors, Task-derived calls, expiry and delegated-admin
  refusal; [shared_policy](../crates/veoveo-hub/tests/shared_policy.rs#L13) compares
  Hub/background policy behavior on fixtures.
- **Formal status:** partial. The [authority package](authority/README.md) proves
  exact ordered exchange-stage outcomes and successful grant/context assembly,
  downstream acceptance/refusal and output, and their composed permission bound.
  Valid `Authenticated` routes accept an empty grant; previous denial and opaque
  policy metadata retain their existing meaning. The generated functions and their
  composition are proved within the stated boundary, not the complete endpoint.
- **Applicability/evidence:** Hub `ee0bdf3` plus the exact frozen source overlay in
  the [receipt](authority/evidence/authority-receipt.json). Fresh baseline and
  restoration passed six Rust witnesses, extraction and all three Lean files;
  four legal semantic mutants compiled/extracted, then failed their intended
  theorem and witness. Native production2 passed 12 focused unit tests and the
  calls integration; production3 reran seven affected unit tests and passed strict
  Clippy after the equivalent conditional simplification. The receipt distinguishes
  the snapshots. No full-platform gate or deployment is claimed for this slice.
- **Boundary:** native collection operations have explicit finite-set laws.
  Current-role filtering has a coarse requested/intersection premise; the finite
  interpretation demonstrates satisfiable premises, not correctness of role
  policy or BTreeSet. Deadline preservation is equality of an opaque input value,
  not proof of SDK clamping/signing. Production sequencing/adapters were reviewed
  and tested; Lean composition is not extraction of the asynchronous caller.
- **Qualified current-role evaluator:** the [manual package](current_roles/README.md)
  extracts unchanged `held_roles`, `effective_risks`, `granted_permissions` and
  `InstallationRole::allows` at Hub `b9b6cec`, with actual SDK pin `05273d1`.
  Total exact-result and membership equivalences cover arbitrary finite raw
  inputs, including duplicates and empties: tenant/subject membership, all matching
  risk additions, any matching deny, and one held configured role covering access
  and every risk. Actual extracted iterator closures and grant composition are
  included. Seven semantic mutants each failed the intended proof and native
  assertion after successful compilation/extraction. Final dedicated-target
  restoration passed 12 grant tests, one configuration test, extraction, all proofs
  and standard-axiom audits. See the [receipt](current_roles/evidence/current-roles-receipt.json)
  and [record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-29-current-role-proof.md).
  Native collections remain extensional models; principal/catalog freshness and
  external validation remain inputs. That evaluator qualification alone does not
  discharge the earlier authority proof's coarse `CurrentPermissions` premise.
- **Qualified current-permission connection:** at Hub `33849317` plus the
  recorded extraction overlay, [CurrentPermissionsProof](current_roles/CurrentPermissionsProof.lean)
  proves the actual registration traversal, requested filtering and exact collected
  set. Its concrete `LawfulHeld` discharges the earlier current-permission premise
  and instantiates the issuance theorem. The supplied context is preserved; its
  construction from the same caller by the async endpoint is not proved here.
  The [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-29-current-permissions-qualification.md)
  and [receipt](current_roles/evidence/current-permissions-receipt.json) record
  successful Lean checks and standard-axiom audits, five native adapter tests and
  two registration tests, two incorrect return-value controls caught by the
  unchanged proof and native assertions, and fresh restored success with exact
  generated model hashes. The one production edit is an equivalent snapshot
  iterator closure spelling; no authorization rule changes.
- **Qualified server evaluator and downstream connection:** at unchanged Hub `bb2e97f5` and SDK
  `05273d1`, [RegisteredServerDecisionProof](current_roles/RegisteredServerDecisionProof.lean)
  derives the current grant set from the qualified evaluator and proves the actual
  `decide_server` function's exact ordered admission/output table. Target projection,
  native route-call arguments and full decision metadata have separate proofs.
  The theorem keeps supplied server/requirement and request target independent;
  total successful catalog/native-access interpretations and the observed clock
  are explicit premises. SDK access and the clock are the two allowed native
  symbols, not verified external implementations. The
  [checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-registered-server-policy-decision.md)
  records passing frozen proof builds and dependency audits. The
  [admission corollaries](current_roles/RegisteredServerAdmissionProof.lean) also
  derive exact role/risk-based admission, coherent route arguments and the actual
  allowed decision's required-permission membership; see their
  [record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-registered-server-policy-admission.md).
  [Downstream composition](current_roles/RegisteredServerDownstreamProof.lean) now
  derives exact permission-route acceptance and complete grant intersection from
  the actual evaluator, discharges the prior policy-consistency premise and
  preserves the wrapper's metadata projection. Authenticated and server/Task
  positive cases retain their existing semantics; see the
  [record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-registered-server-policy-downstream.md).
  The [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-registered-server-policy-qualification.md)
  and [receipt](current_roles/evidence/registered-server-policy-receipt.json) now
  record six passing native tests, including twelve actual evaluator/narrowing
  scenarios, five legal semantic faults caught by unchanged proofs and native
  assertions, and a fresh restored extraction with exact generated-model hashes.
  Full package, direct proofs, all three model audits, formatting and strict library
  Clippy pass. Native wrapper correspondence has focused test evidence; the async
  endpoint remains unproved. No runtime policy or dependency change was needed.
- **Residual/next:** connect the qualified session-family predicate and SDK-to-policy
  composition through their actual callers. Principal projection, the remaining shared
  MCP extension correspondence and wider session/issuer continuity remain.
  Catalog/provider discovery, replay, Task reads, route wiring, concrete token
  serialization and clocks remain separate obligations. A new downstream namespace
  is constrained by current caller grants and declared uses; it need not be a
  subset of the earlier namespace's permission names. No new policy is introduced.

## HUB-OAUTH-01 — Session binding and refresh lifecycle

**Invariant/failure prevented.** Session-bound access matches its authoritative
family's owner, issuer/resource/client/WorkContext and admitted scopes, and respects
expiry/revocation. Rotation does not invalidate an otherwise valid family-bound
access token solely by changing generation. Refresh cannot widen scopes or rebind
the family; bounded duplicate delivery and replay handling retain their distinct meaning.

- **Source:** [SessionFamilyAuthority::allows](../crates/veoveo-hub/src/policy/session.rs#L47),
  [exact family read](../crates/veoveo-hub/src/state/session.rs#L18),
  [refresh processing](../crates/veoveo-hub/src/state/refresh_tokens.rs#L319), and
  [transactional refresh store](../crates/veoveo-hub/src/store/refresh.rs).
- **Tests:** [session_family](../crates/veoveo-hub/tests/session_family.rs#L20),
  [native_issuer](../crates/veoveo-hub/tests/native_issuer.rs#L83), and
  [browser_session](../crates/veoveo-hub/tests/browser_session.rs#L56) cover distinct
  binding/rotation/live-grant and Task-replay scenarios.
- **Boundary/effort:** moderate for pure family admission; substantial for a refresh
  transaction model/refinement. Assume the store returns the exact requested family,
  trusted decoded fields, verified token claims and supplied current time.
- **Extracted boundary:** the actual `SessionFamilyAuthority::allows` checks
  family presence, revocation, strict family expiry, owner/tenant, authorization
  server/resource, OAuth client, WorkContext, retained identity and scope containment.
  It deliberately ignores the current refresh generation. The retained principal's
  upstream IdP issuer may differ from the Hub-issued token/principal issuer; the
  latter pair must agree. Prove these legitimate success cases alongside mismatches.
  The pure function checks only that a token family ID is present: selection of
  that exact record belongs to `access_token_session_valid` and its store lookup.
  JWT signature, audience and token-expiry verification are outside this function;
  neither their correctness nor the lookup binding follows from its Boolean result.
- **Extraction/model checkpoint:** unchanged `allows` now extracts transparently
  with the preceding policy package retained. The extended package and four audits
  pass; the session method's dependency closure has only standard Lean axioms.
  Equality, scope ordering and full timestamp comparison have ordinary models with
  native correspondence still explicit. That checkpoint precedes the exact
  admission proof and native baseline below. See the
  [checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-session-family-extraction-model.md).
- **Admission baseline:** the exact independent predicate has checked acceptance/refusal and
  observed-field invariance proofs, including generation, retained issuer and
  subsecond expiry. The full package/audits, four native method tests and the
  existing PostgreSQL session test pass; see the
  [admission baseline](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-session-family-admission.md).
- **Qualified admission boundary:** four private predicate faults compile/extract
  and build their helper models, then fail the unchanged exact-result proof and
  their intended native assertion. Fresh unchanged-source extraction, the complete
  Lean package/direct session proof/four audits, all four native tests, formatting
  and strict library Clippy pass. The separate PostgreSQL baseline also passed.
  Source-comment path normalization alone reconciles regenerated definitions/types.
  See the [receipt](current_roles/evidence/session-family-receipt.json) and
  [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-session-family-qualification.md).
  The exact predicate is **proved-within-stated-boundary and qualified**; the whole
  OAuth/session/refresh lifecycle remains partial. No production session rule changed.
- **Residual/next:** exact lookup/decoded-input and asynchronous caller correspondence,
  atomic consumption, concurrent retries, delivery encryption and real IdP
  behavior require their own evidence; an abstract transition proof is design-level.

The next active slice follows the actual refresh store and state caller. Extract
`RefreshFamily::bound_to` and `deliverable`, then connect their exact decisions to
a separate transaction model with rotation, same-successor duplicate delivery,
replay/client revocation, maintenance and commit uncertainty. Success audit and
duplicate audit commit with their respective store operations; replay's HTTP denial
audit is later, and duplicate decryption occurs after the store commit. Preserve
first-revocation reason/time and the request's pre-lock time sample. Source review
and implementation scope are recorded in the
[refresh transaction review](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-refresh-transaction-source-review.md).
The two actual predicates and the delivery closure now translate transparently
through Charon/Aeneas with the previous extraction roots preserved; see the
[extraction checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-refresh-predicate-extraction.md).
The subsequent [exact predicate checkpoint](current_roles/RefreshPredicateProof.lean)
proves binding and complete delivery acceptance/refusal, coherent success, strict
deadline refusal and checked generation arithmetic. It also composes a generation-only
update with session admission through an explicit mathematical projection. The full
package and eight theorem axiom checks pass; see the
[proof record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-refresh-predicate-proofs.md).
Separately, four new PostgreSQL transaction tests and five existing `hub_state`
tests, formatting and strict library Clippy pass; see the
[native baseline](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-refresh-native-baseline.md).
The [controls receipt](current_roles/evidence/refresh-controls-receipt.json)
qualifies these pure predicates with paired extracted/native faults for deadline
equality and client binding. A separate premature-commit fault breaks the native
audit-rollback assertion. Fresh extraction, the full proof package, five audits
and all nine native tests pass on the restored original source. The controls do
not prove SQL refinement or complete the refresh lifecycle.

The separate [transaction history checkpoint](refresh_transactions/evidence/history-checkpoint.json)
now derives complete reachable rotation programs, stable family-lock snapshots,
and strictly increasing committed generations from arbitrary finite modeled
executions. The retained generation is its initial value plus committed rotation
count; no initially duplicate-free history is assumed. This is a design-model
proof for one retained family incarnation. The
[replay checkpoint](refresh_transactions/evidence/replay-checkpoint.json) now
ties committed-state replay to actual model-step contributions and excludes
aborted attempts from every such trace, while retaining other commits and
maintenance. A [constructive concurrent witness](refresh_transactions/evidence/concurrent-success-checkpoint.json)
now derives a successful rotation and waiting duplicate delivery from valid initial
data, with the same original successor and exact family/token/audit maps. Reached
commit-request witnesses now show both complete commit and abort after an observed
error, plus commit followed by a lost acknowledgement. The
[final refresh receipt](refresh_transactions/evidence/refresh-transactions-receipt.json)
closes this concurrency slice with the full nine-module build, native tests and
semantic controls kept as distinct evidence. Initial creation, native caller and
SQL/lock correspondence, and cleanup composition remain separate obligations;
the whole OAuth area is still partial. Registration/replacement concurrency is
the next transaction slice.

## HUB-CONFIG-01 — Coherent atomic live publication

**Invariant/failure prevented.** Publication compares the expected active revision and
commits revision objects, WorkContexts, active pointer and audit together. A losing
concurrent publication does not partly apply.

- **Source:** [publish_control_revision](../crates/veoveo-hub/src/store/control.rs#L82)
  and [control SQL](../crates/veoveo-hub/src/store/control/lock_active.sql).
- **Tests:** [control_store](../crates/veoveo-hub/tests/control_store.rs#L22):
  `publishes_immutable_revisions_and_moves_active_pointer_atomically` and
  `concurrent_publications_activate_one_revision_and_refuse_the_other`.
- **Boundary/effort:** substantial: SQL transaction/lock semantics are central, not
  incidental I/O around a pure helper. A first abstract CAS model can clarify
  intent, assuming PostgreSQL atomicity/isolation and correct transaction scope.
- **Residual/next:** justify model-to-SQL correspondence before claiming implementation-linked coverage.
  Process failure, replica notification and OpenShell effects are separate; preserve
  database tests rather than treating a Boolean revision comparison as their proof.

The first transaction slice follows `publish_control_revision`: acquire the
`control_active` row lock, compare its revision, create the revision and objects,
replace Work Context identity rows, move the pointer, append audit, then commit.
The schema's singleton constraint/trigger and immutable-revision constraints are
part of the correspondence, not optional modeling conveniences. The competing
publisher must observe the committed predecessor after obtaining the lock (or abort
under stronger isolation); a stale pre-lock read cannot authorize its commit.
Failure at any write or audit append must contribute none of that transaction's
revision/object/context changes or successful audit batch; another publisher may
still commit. Separate rejection/failure audit records remain allowed. A lost commit
acknowledgement is an ambiguous outcome, not evidence of rollback. The model must
separate PostgreSQL's ability to commit staged writes from Hub's obligation to stage
the complete publication before committing; an incomplete commit must remain a
representable fault. Revision-ID non-reuse follows from insertion constraints while
history is retained, rather than an unchecked fresh-ID assumption.

The [manual transaction package](control_publication/README.md) is now qualified
within its **design-model boundary**. For arbitrary finite interleavings, reachable
commits stage the complete planned publication, preserve the locked predecessor,
and produce the specified row effects. Retained revision history and actual modeled
insertion constraints imply one committed successor per predecessor. Committed
state equals replay of committed plans; aborted attempts contribute no publication
writes or successful batch. Constructive initial/replacement executions establish
valid success, and reachable uncertain-outcome/lost-acknowledgement executions show
why a client error cannot imply rollback. The replay is proof-only state.

The full Lean package and executable standard-axiom audits pass. Four PostgreSQL
tests exercise late append failure, actual lock overlap, exact winning rows/audit
and the existing publication scenarios. Both private Rust faults fail their
intended native assertions; fresh original-source tests, formatting and strict
library Clippy pass again. Final formal source differs from accepted native inputs
only in proof files. The actual production publication method is unchanged.
See the [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-control-publication-qualification.md)
and [receipt](control_publication/evidence/control-publication-receipt.json).

Source correspondence distinguishes mutable Work Context payloads from preserved
key/creation fields, supplied audit content from its generation, and rollback
requests from completed server aborts. PostgreSQL primitives, SQLx behavior and the
concrete audit implementation remain explicit assumptions. Model proofs plus
source review and database tests do not establish formal SQL refinement. Physical
crash durability, history deletion, other writers sharing tracked keys, replica
adoption and liveness remain outside this result; HUB-CONFIG-01 stays partial.
No runtime journal, new publication rule or proof gate was introduced.

Local adoption is a second obligation: the publication mutex also covers the
[native reload path](../crates/veoveo-hub/src/bin/hub/control_reload.rs), which reads
the authoritative pointer after taking the guard. Trace actual catalog/HTTP-handle
replacement and notification/reconnect behavior before claiming coherent adoption
or convergence. The active revision may advance elsewhere while one replica prepares
its snapshot; temporary lag remains allowed by the architecture. No new synchronous
global barrier is proposed.

## HUB-ROUTE-01 — Fresh routing and captured routing

**Invariant/failure prevented.** Fresh requests choose from ordinary deployments and
an explicit branch/base selection. A missing explicit branch is unavailable, never
silently replaced. Captured requests and Tasks retain their exact composition.

**Checked implementation-linked coverage.** The shared extraction uses Hub
`3bc2923`, SDK `239622d` and Core `120306e`; subsequent source/domain differences
are documented in [request composition](request_composition/README.md).

- Actual selection-entry processing omits unavailable, missing and wrong-slug
  entries and preserves the complete matching row. The finite scan and ordered-map
  collection derive exact eligible contents, ordering and unique keys.
- The actual slug getter accepts exactly one matching registration and returns
  no row for zero or multiple matches. With native composition-key uniqueness,
  selected lookup equals the independently computed retained route, without fallback.
- Successful actual snapshot construction derives deployment-key/row coherence,
  including overwriting repeated keys. This discharges that premise for the
  [constructed route/context proof](request_composition/SnapshotConstructorProof.lean).
- Actual request-context construction covers direct and delegated cases, including
  its existing refusal cases. Given accepted context, the actual HTTP constructor
  preserves the entire independently specified client, including destination,
  identity, authority, supplied permissions and delegation fields.
- The [direct policy/client proof](request_composition/DirectPolicyClientProof.lean)
  derives the row from computed retained-route resolution and policy from the same
  supplied catalog generation. It proves complete direct-client construction and
  permission membership exactly equal to the independent existing grant relation;
  neither a desired row nor desired permissions are assumed.
- The [delegated policy/client proof](request_composition/DelegatedPolicyClientProof.lean)
  now connects the actual native set adapter and `SubjectDelegation::narrow` to
  that route. Direct calls retain current grants; delegated calls retain exactly
  their intersection with carried permissions. Both directions of membership and
  valid empty results are proved, followed by complete client/context preservation.
  The same current subject principal drives policy in both branches. Seven scoped
  audits pass; no new handwritten primitive or opaque assumption was added.

**Qualification evidence.** Actual Rust faults that bypass getter ambiguity,
ignore the selected slug, or erase constructor permissions are rejected by frozen
proofs. Independent witnesses establish the erroneous results and fresh restoration
passes. Receipts: [getter](request_composition/evidence/registration-lookup-control.json),
[entry](request_composition/evidence/selected-entry-control.json),
[constructor](request_composition/evidence/http-permission-preservation-control.json),
[complete collection](request_composition/evidence/complete-selection-collection-checkpoint.json),
[constructed route](request_composition/evidence/constructed-route-context-checkpoint.json),
[direct policy composition](request_composition/evidence/direct-policy-route.json),
[native narrowing and connected client](request_composition/evidence/delegated-policy-client.json).
The narrowing checkpoint includes a passing focused Rust set-adapter test.
Its [actual-source controls](request_composition/evidence/delegation-narrowing-controls.json)
now reject a carried-set bypass and loss of a valid common permission. Independent
witnesses prove the incorrect successful outputs with standard-only audits. Both
scratch sources were restored, all four generated Lean files reproduce the canonical
bytes, and fresh direct/delegated targets plus model audit pass. These deliberate
faults establish proof sensitivity; they are not newly found production defects.
The [native routing receipt](task_continuity/evidence/request-routing-native-receipt.json)
separately records actual Hub/PostgreSQL and SDK-verified recipient/context checks,
a retained Task's current-policy narrowing/revocation, and a policy-bypass control.
Native probes do not formally refine the async caller or SQL.

**Remaining boundaries.** Native set/map/iterator/Arc correspondence and finite
cursor laws remain explicit. Constructor and selection results are conditional on
successful namespace-helper completion; the unchanged helper currently exceeds
pinned Aeneas support. Full constructor-input presence/latest-row behavior and
ordinary deployment ordering are not established by key/row coherence. The
[selection pilot](selection/README.md) covers only its recorded local decision
properties. The captured catalog is a supplied immutable generation, not a theorem
of globally latest database state. Snapshot acquisition, actual transport/caller
wiring, broader selection storage and dispatch remain separate obligations.

**Next connected contribution.** Connect actual SDK issuance and recipient checks
to the qualified native narrowing and selected-client contract. Existing
native probes do not replace those missing implementation proofs. All claims remain
partial; no whole-request or whole-Hub verification is implied.

The separately extracted SDK now has an exact all-mode context-admission theorem
in SDK `f72f91f`: the actual validator accepts iff its independent field predicate
holds, and otherwise returns the existing context error. Direct and named manual
checks pass with only standard Lean axioms under explicit text representation bounds.
See the [SDK contract and receipt](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/internal_auth/README.md#exact-context-admission)
and [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-context-admission-proof.md).
SDK `b645f07` additionally proves populated Direct, Automated and Delegated
acceptance witnesses, including a closed no-premise three-mode example. Both
manual targets and all eight new axiom audits pass; see the
[valid-context record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-context-valid-witnesses.md).
SDK `4fb723e` qualifies two actual-source context controls. Removing Direct actor
equality admits an independently refused altered Principal; changing Automated's
required kind refuses an independently admitted Service. Frozen contracts reject
both faults and standalone witnesses prove the wrong results. Exact source and
generated-output restoration plus fresh nine-audit baseline builds pass; see the
[control record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-context-semantic-controls.md).
These deliberate faults are not new production defects.
SDK `95ecd75` proves generic complete identity construction and exact delegation
refusal priority, including optional contexts, full output preservation and the
conditional source-token expiry cap. Populated Direct and inherited-chain acting
examples pass with the final named targets and all nine audits; five explicit
native provider/time/ID boundaries remain. See the
[constructor record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-full-identity-construction.md).
SDK `4655e9d` qualifies both constructor deadline controls: the frozen contract rejects
actual-source cap removal and an extra cap on acting calls, while independent
witnesses prove the wrong complete output and wrong refusal. Same-tree source and
raw-model restoration plus fresh nine-audit baseline builds pass; see the
[deadline-control record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-identity-deadline-controls.md).
SDK `ed21505` proves complete actual claims assembly against the unchanged
independent specification: all identity/invocation fields, scope text, actor-chain
order and empty-or-complete permission details. Populated delegated and empty
Direct examples pass, and all fourteen new audits in the final manual targets are
standard-only. See the [claims-assembly record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-claims-assembly.md).
SDK `859990c` qualifies the actual-source permission-detail inversion. The unchanged
theorem rejects it, independent witnesses derive both complete wrong results, and
same-tree source/raw restoration plus a fresh fourteen-audit baseline passes; see
the [permission-control record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-claims-permission-control.md).
SDK `6665e9c` adds a generic public-issuer proof composing exact first-foreign refusal,
complete identity and claims construction, exact signer arguments and preserved
signing errors/results. Its two manual targets and all four audits pass; see the
[public-composition record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-public-issuer-composition.md).
SDK `e9e6d41` adds passing populated public-issuer examples: a complete two-permission Direct result,
exact signing claims and source-capped expiry, plus the first foreign permission
before an independently invalid client context. Their two manual targets pass all
seven new audits; see the
[issuer-examples record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-public-issuer-examples.md).
SDK `6afa18a` qualifies the public issuer's actual-source ownership-guard inversion:
the unchanged theorem rejects it, an independent witness derives the wrong refusal,
and exact same-tree source/raw restoration plus a fresh four-target baseline passes
all eleven public audits; see the
[issuer guard-control record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-sdk-public-issuer-guard-control.md).
Connecting Hub's actual caller to the checked SDK issuer is next, preserving complete
values and stating applicability to the explicitly composed source revisions. Native
permission ownership, cryptographic signing, the verification wrapper and
integration with Hub's pinned SDK remain open. Claims preservation is not
JSON/cryptographic verification.
The checked [value bridge](current_roles/SdkInternalAuthBridgeValues.lean) now
establishes identifier/payload, ordered-set and bounded-timestamp correspondence
between the two type graphs, including ordering and roundtrips. Its
[receipt](current_roles/evidence/sdk-issuer-bridge-values.json) records a joint
tool context and standard-only axiom guards. The subsequent
[complete record bridge](current_roles/SdkInternalAuthBridgeProof.lean) derives
available request/identity conversions from independent physical timestamp bounds,
preserves every named field and canonical set invariant, and establishes the
SDK-to-Hub output/error direction. Ordered grants and actor chains retain their
positions; returned Rust errors and modeled effects, failures and divergence are
preserved. Its [qualification](current_roles/evidence/sdk-issuer-bridge-records.json)
distinguishes field meaning from invertibility. The
[operational connection](current_roles/SdkInternalAuthBridgeOperations.lean)
replaces the Hub graph's opaque issuer with the SDK-owned extracted implementation.
Its separate [contract](current_roles/SdkInternalAuthBridgeOperationsProof.lean)
derives conversion and the SDK text domain from independent Hub representation
bounds, then connects the complete request to the independently specified SDK
outcome. No successful issuance or desired-output premise is introduced. This is
an explicitly composed prepared stack, not native equivalence to Hub's older
SDK dependency. The [receipt](current_roles/evidence/sdk-issuer-operational-connection.json)
records the qualification and the proof-module import adjustment. The
[actual caller theorem](current_roles/SdkInternalAuthCallerProof.lean) now connects
the extracted `issue_bearer_token_until` body to that complete independent SDK
outcome. It preserves every client/request field, applies the optional inherited
deadline with full seconds/nanoseconds comparison, and returns the exact bearer
or wrapped error while retaining modeled effects/failures. Independent physical
and text bounds supply conversion; no successful admission or issuance is assumed.
The [caller receipt](current_roles/evidence/sdk-issuer-caller.json) records the
passing generic theorem. The
[populated caller witnesses](current_roles/SdkInternalAuthCallerWitnesses.lean)
now derive actual acceptance with complete signer claims, two permissions and two scopes,
an acting server and a nonempty inherited chain with its originating Task. A
separate admitted-input case whose caller cap equals the sampled clock returns
`ExpiredDelegation`, even when the requested deadline is one nanosecond later.
Only typed external observations are premises; fixture ownership/context/chain
conditions and conversion are independently proved. Their
[receipt](current_roles/evidence/sdk-issuer-caller-examples.json) records passing
fixture/outcome targets. The [deadline-control checkpoint](current_roles/evidence/sdk-issuer-caller-deadline-control.json)
records an actual Rust fault that compiles and is rejected by the unchanged caller
contract at its deadline equation. Exact same-tree source/raw restoration and a
fresh build of the three original caller targets pass. The independent wrong-output
witness failed elaboration and remains an explicitly unqualified draft; the full
control is still incomplete. These results cover a supplied client and deadline, not their
asynchronous/current-policy provenance, the 60/900-second TTL/clock wrappers,
HTTP attachment, transport or native signature verification. The prepared-source
applicability boundary remains; no runtime conversion is added.
Core's private permission-namespace helper is still represented by a handwritten
adapter; extracted ownership wrappers alone do not close its refinement gap.
A [bounded unchanged-helper probe](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-permission-namespace-extraction-boundary.md)
passes Charon with the helper transparent, but pinned Aeneas rejects the
string-pattern method signature and emits no Lean. No canonical model changed.
The [composition review](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-issuer-composition-boundaries.md)
records the original set/traversal, timestamp and complete-field obligations. The
bridge above supplies their stated model-level correspondence; native refinement
and the other explicit applicability boundaries remain. No runtime adapter is added.

## HUB-CATALOG-01 — Request/catalog/cache/listen isolation

**Invariant/failure prevented.** Descriptors and continuations belong to the admitted
caller/routing context. A stale in-flight fetch cannot replace newer contents, and a
catalog publication/selection change does not leave a stale listener silently active.
Prevents cross-context disclosure and stale routing represented as current discovery.

- **Source:** [composition-bound fingerprint](../crates/veoveo-hub/src/mcp.rs#L309),
  [DiscoveryCacheKey and begin/finish](../crates/veoveo-hub/src/mcp/discovery.rs#L22),
  [catalog cursor](../crates/veoveo-hub/src/mcp/catalog_pages.rs#L79), and
  [listeners](../crates/veoveo-hub/src/mcp/subscriptions.rs).
- **Tests:** [discovery tests](../crates/veoveo-hub/src/mcp/discovery.rs#L577), including
  `invalidated_in_flight_response_cannot_replace_new_contents_or_finish_new_request`;
  [cursor tests](../crates/veoveo-hub/src/mcp/catalog_pages/tests.rs#L175) and
  [ended-listener test](../crates/veoveo-hub/src/mcp/subscriptions/tests.rs#L28).
- **Native finding and fix:** the [authority-isolation receipt](discovery_cache/evidence/native-authority-isolation-20261031.json)
  records two unchanged regressions failing against the original key and passing
  after a four-line correction. Different delegated permission ceilings, or
  different current principal groups behind the same delegated actor, reused the
  first caller's admitted descriptors and denied count. Both broad-first extra
  visibility and narrow-first omission were reproduced. The key now includes the
  stable current principal and optional ceiling; timestamp/audit-only reuse is
  preserved. Twenty-two focused unit tests, formatting and strict library Clippy
  pass. These tests execute native catalog/policy/narrowing/cache operations, not
  token endpoints, HTTP dispatch, upstream responses or watches. Tool execution
  still reauthorizes. This establishes no new Lean theorem or deployed result.
- **Transition preparation:** six existing critical sections now live on the
  private `SurfaceState<T>`; the async cache wrappers acquire the same mutex and
  call them. Normalized body comparison and 22 focused native tests, formatting
  and strict library Clippy pass. The
  [factoring receipt](discovery_cache/evidence/native-state-factoring-20261031.json)
  records this source preparation, not a formal equivalence or cache theorem.
  The pinned translator could not select inherent methods by their impl owner,
  and its generated effect model has no stateful UUID/clock observations. A fixed
  no-argument axiom must not be treated as producing a fresh ticket on each call.
  A subsequent [native-qualified refinement](discovery_cache/evidence/native-lazy-inputs-20261031.json)
  gives the six private state methods unique selector names and supplies lazy
  clock/UUID function inputs from the locked wrappers. Sampling stays in the same
  branches; the same 22 tests, formatting and strict Clippy pass. This prepares
  generic per-operation observations without a global freshness axiom.
  [Fresh extraction and elaboration evidence](discovery_cache/evidence/retain-interface-boundary-20261031.json)
  now records all six operations, five predicates and four key-derived methods
  as transparent, error-free translated bodies. The unchanged generated Lean
  nevertheless fails at six `discard_server` callback dictionary assignments:
  mutable payload returns do not fit the pinned `FnOnce`/`FnMut` interfaces.
  Its three import modules compile using uninterpreted external templates; the
  function module does not. This is a translator/library limitation, not a
  qualified cache model. No native algorithm or generated body was rewritten.
  The [five-operation extraction](discovery_cache/evidence/five-operation-extraction-20261031.json)
  includes the pinned SDK TTL initializer and compiles unchanged generated
  Types/Funs. Server-wide invalidation stays explicitly open.
- **Checked read/invalidation contracts:** the independent
  [cache proof package](discovery_cache/README.md) now implements primitive models
  and proves the actual generated key comparator, complete cached reads,
  cached-only removal and full key invalidation. Whole payloads and denied counts
  are retained; removal preserves generation and unrelated entries. Cached-only
  removal preserves in-flight tickets and retry records. Full key invalidation
  removes the selected key from all three maps; absence after removal uses the
  native map's unique-key precondition. Ten executable axiom guards pass; the
  read/removal proofs have only standard Lean logical axioms. See the
  [qualification receipt](discovery_cache/evidence/read-key-invalidation-20261031.json).
  Native collection/identifier/clone correspondence and async mutex refinement
  remain explicit boundaries. Later sections qualify finite histories and semantic
  controls; server-wide invalidation remains open. The read/removal checkpoint alone does not
  establish stale-response fencing or listener freshness.
- **Checked fetch-start contract:** [the start receipt](discovery_cache/evidence/start-contract-20261031.json)
  connects the actual generated operation to an independent complete-state
  specification. It proves old-generation no-op, newer-generation reset, cooldown
  before pending/capacity checks, and valid acceptance with the exact key, ticket,
  zero denied count and updated state. Other tickets and the cached/retry maps
  are preserved on equal-generation acceptance. Non-coalesced acceptance requires
  no clock, retry or pending-record premise; capacity still refuses replacements.
  Callback Results retain failure/divergence rather than assuming success.
  The final leaf passed in 7.398 s with sixteen executable standard-only axiom
  guards. This start contract alone does not establish native invocation counts
  or map invariant closure; the latter is established below.
- **Checked completion and direct key fence:** [the completion receipt](discovery_cache/evidence/complete-contract-20261031.json)
  connects actual completion to an independent full-state specification. Missing
  or mismatched tickets produce a complete no-op. Current success preserves the
  exact vector/denied fields and comparison Result, consumes ticket/retry records
  and names any capacity eviction. Current failure preserves cached descriptors
  and stores the observed retry deadline. A false changed flag can still consume
  a current ticket. The actual `discard_key` followed by actual `complete` is
  proved to leave exactly the invalidated state, assuming unique in-flight keys.
  The first Linux check passed (14.361 s specification, 4.474 s proof). Six
  primitive/frame axiom guards are standard-only; seven actual-completion guards
  additionally admit precisely the existing native `Instant.add` Result boundary.
  This does not prove native addition or arbitrary payload structural equality.
  No comparison/clock success assumption is required for stale rejection. Later
  sections establish supported state invariants and finite histories.
- **Checked key order and map preservation:** the
  [key/map receipt](discovery_cache/evidence/key-map-invariants-20261031.json)
  connects the actual extracted comparator to a lossless full-field lexicographic
  code and proves its strict-order laws. The concrete external map model preserves
  strict ordering and unique keys through insertion/removal. Insertion returns the
  full previous value, replaces without growth, and frames other complete values.
  Room followed by insertion preserves the 4096 bound; the named head victim is
  the minimum key. The key leaf passed in 12.449 s and the final map leaf in 2.533 s,
  with 32 executable standard-only axiom guards across the pair. This does not
  verify Rust's `BTreeMap` implementation. The later state/history composition
  establishes model-relative reachable invariants.
- **Checked actual-operation state invariants:** the
  [state receipt](discovery_cache/evidence/state-invariants-20261031.json)
  proves the empty represented state has strictly ordered, unique, bounded maps
  with every stored key at the state's generation. Actual start, completion,
  cached-only removal and key invalidation preserve these properties whenever
  they return normally. Start never decreases generation; other mutations preserve
  it exactly. Earlier physical usize bounds are derived, not assumed separately.
  An older-generation completion is therefore a whole-state no-op without a
  ticket-freshness premise. The leaf passed in 2.776 s with seventeen guards:
  fourteen standard-only and three permitting the existing native `Instant.add`
  boundary. No after-state invariant or separate callback-success premise is assumed.
  Histories are established below; native constructor/mutex correspondence remains
  external.
- **Checked finite actual-operation histories:** the
  [history receipt](discovery_cache/evidence/finite-histories-20261031.json)
  records induction over actual read/start/complete/removal equations, with no
  invariant built into the history relation. Histories from empty preserve all
  cache invariants and nondecreasing generation. Generation fencing survives any
  later supported finite suffix. After full key invalidation, a suffix containing
  accepted replacement requests also fences the old response when accepted starts
  at that key do not reuse its ticket; final ticket exclusion is derived. Accepted
  initial and generation-advancing histories are constructed with exact outputs.
  The leaf passed in 2.847 s with twelve guards and no new primitive axiom. The
  existing time-addition boundary also appears in start-history theorem closures
  because the history relation includes completion equations. This is neither
  universal UUID freshness nor a proof of native async scheduling.
- **Qualified actual-source controls:** the [control receipt](discovery_cache/evidence/semantic-controls-20261031.json)
  records two isolated source faults. Native timings include Cargo compilation;
  test bodies take about 0.00–0.01 s, not the reported whole-command durations.
  Omitting newer-generation in-flight clearing
  fails the unchanged native old-cache assertion (33.092 s) and unchanged
  `start_exact` (2.926 s) after its fresh model/prerequisites compile. Weakening
  refusal to ticket mismatch **and** key absence fails the native assertion
  (31.325 s) and unchanged `complete_exact` stale no-op equation (2.984 s), with
  the equality dictionary/helper intact. Failed target OLeans are absent; audits
  reject cascading `sorryAx`. Same-tree reversal restores both original full maps
  and raw4 exactly. Selected native tests pass restored (10.829/9.995 s), and two
  separate fresh baseline packages compile all thirteen own modules and all guards.
  Nine proof OLean hashes match earlier accepted artifacts; model binary identity
  is not claimed. An earlier key-presence-only candidate demonstrates native wrong
  behavior but prunes required dictionaries; it was rejected before Lean and is
  retained as an interface diagnostic, not a qualified formal control. These are
  deliberate test faults, not newly discovered production defects. Native ticket
  setup uses server invalidation; the formal history fence uses key invalidation.
- **Native watch publication finding:** the
  [watch receipt](discovery_cache/evidence/native-watch-publication-20261031.json)
  records three unchanged regressions against an actual admitted/selected handler
  and a loopback MCP source. The original watch used the immutable selected
  catalog's private signal: live publication left its stream and cache active,
  late work repopulated that cache, and an old admission removed a newer watch.
  All three intended aggregate assertions fail on the original runtime
  (38.749 s command, 6.92 s harness). The correction retains selected manifests
  and routing but uses the live generation/signal, rechecks the epoch after the
  map-lock wait, and checks it after upstream opening. The byte-identical tests
  pass (32.360 s command, 1.01 s harness), including real source-listen disposal,
  whole descriptor/denied-count preservation, late completion fencing, and
  healthy reuse. All 23 focused native tests, formatting and strict library
  Clippy pass. A prior fixture API-name compilation error ran zero tests and is
  recorded separately. This is a production defect found through source review
  and native execution during proof work, not a newly checked Lean theorem.
  The synchronous cache source and existing generated/proof files are unchanged.
- **Pagination translation boundary:** the
  [extraction receipt](discovery_cache/evidence/catalog-pagination-boundary-20261031.json)
  records an unmodified full-pager attempt on the pre-integration source and its
  exact SDK/Core pins. Adding the omitted SDK pagination include makes all ten
  selected roots, required derives/constants and 258 own-component functions
  transparent and error-free in Charon's output. Aeneas then exits 2 on unsupported
  outer-loop continuation, identifier function-pointer/region types and external
  string/serde signatures, before producing any Lean files. This is a translator
  feasibility limit, not a runtime defect or a semantic counterexample. No native
  algorithm, generated model or translator was patched to obtain a passing result;
  pagination remains unproved. The newer linked-runner dependency context has not
  been extracted by this attempt.
- **Native deployment isolation finding:** the
  [deployment memo receipt](discovery_cache/evidence/native-unchanging-deployment-20261031.json)
  records a second watch defect on the integrated runtime. Remembering effective
  unchanging capabilities by server name lets one static deployment suppress a
  dynamic deployment's discovery and subscription. Both different links in one
  selection and the same link in different selections reproduce this behavior,
  retaining a complete cached descriptor with denied count 17. The correction
  keys the existing memo by generation and actual deployment, including its two
  subscription callers. The byte-identical regressions and three earlier watch
  tests pass (5/5), as does the subscription regression (1/1). Final formatting
  passes after a whitespace-only adjustment. This is native qualification, not
  an extension of the frozen Lean graph; full integrated Mac/Linux checks and
  current-source formal correspondence are separate obligations. The integrated
  component checks subsequently passed in the receipt linked above.
- **Current linked-key qualification:** the
  [current graph receipt](discovery_cache/evidence/current-linked-cache-proofs-20261031.json)
  adopts fresh `c23d9ae`/SDK `7360c0f`/Core `422dcf2` extraction. All nine proof
  modules pass, including full five-field clone/comparison, lossless key order,
  exact operations and finite supported histories. Actual `discard_key` preserves
  another link's complete cached payload, pending ticket and retry state; absent
  and present links are distinct without a parsing premise. This is not a general
  write-frame guarantee across capacity eviction. An initial optional-clone tactic
  failure was corrected without changing claims or guards; all 103 axiom guards
  pass. Source controls from the older graph retain their historical boundary;
  a current link-comparison source fault is being qualified separately.
- **Boundary/effort:** substantial for the cache/listener state machine. Native
  collection/identifier/clone/equality/time and mutex/observation refinement remain
  explicit. Any hash-based identity correspondence is conditional on no collisions
  among the relevant finite observed contexts, not impossible global injectivity.
- **Residual/next:** close server-wide invalidation's translation boundary and
  establish the outstanding catalog-cursor/listener isolation and freshness
  contracts. Supported cache histories and source controls do not establish native
  async scheduling, reconnect delivery, client caching behavior or cross-replica
  notification reliability.

## HUB-FACTORY-01 — Reconciliation and native lifecycle recovery

**Invariant/failure prevented.** Subscription/reconnect performs authoritative branch
reconciliation; pushes use their pusher's authority. Applications to one instance
serialize. Image-build/policy preparation failure before native restart/replacement
does not replace the old sandbox. Image/policy changes choose replacement; unchanged
settings choose the appropriate restart/start path. Later source-build/startup failure
does not imply automatic preservation of the previous serving worker.

A linked runner's complete target must also agree across its stored repository and
tracked branch, native start command, credential lookup and subsequent linked-push
routing. Retargeting the same link keeps its deployment and data environment, but
must not retain the previous target in any of those consumers. Live registration
publication must observe its joined instance target even when registration metadata
and client identity have not changed. This is a consistency requirement on existing
state, not an additional repository authorization rule.

- **Source:** [follow/recheck/change](../crates/veoveo-hub/src/bin/hub/contributions/lore/factory/watch.rs#L129),
  [Factory::apply_to](../crates/veoveo-hub/src/factory.rs#L403), and
  [sandbox::runs](../crates/veoveo-hub/src/factory/sandbox.rs#L380).
- **Tests:** [watch fixture](../crates/veoveo-hub/src/bin/hub/contributions/lore/factory/tests.rs#L493)
  covers resubscription and replaced watchers; [factory scenarios](../crates/veoveo-hub/tests/factory.rs#L532)
  cover changed image/policy, crash loops and failed builds.
- **Native retarget correction:** `48d7e25` repairs a reproduced mismatch between
  native command, stored target, credential lookup and later linked-push routing.
  Three new regressions cover complete retargeting, refused deletion/retry and
  joined registration publication without a new process registration. The integrated
  Mac/Linux Docker checks each pass 460 tests, with two skipped; see the
  [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-retarget-fix.md).
  This is component evidence, not a deployed or formally verified transition.
- **Checked manual contract:** [Factory retarget](factory_retarget/README.md) proves
  the independent complete successful 21-column row, target/client/push-selection
  agreement, immutable row identity and unrelated-state preservation at every
  modeled normal return. Failed-step contracts retain both committed and
  uncommitted SQL outcomes and preserve the original error. The direct proof and
  24 executable axiom audits pass; see the
  [proof checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-retarget-row-proof.md).
  This is handwritten source correspondence to `48d7e25`, not Aeneas extraction
  or formal refinement of the async Rust/SQL/native implementation. Success
  requires an existing bound runner and a normal return;
  physical native framing requires distinct scoped native names.
  The [modeled output extension](factory_retarget/FactoryNativeOutputProof.lean)
  now derives capacity acceptance, connects the row to independent path-sensitive
  provider/native-observation/request outputs and proves a populated constructive
  retarget. All 14 new axiom guards pass; the four synthetic credential digests and
  exact runner-script correspondence were independently checked. See the
  [output checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-native-output-proof.md).
  This establishes model outputs, not remote receipt/readiness or complete token issuance.
  The [recovery extension](factory_retarget/FactoryRecoveryProof.lean) now proves
  exact first-error prefixes, surviving row/runner identity, old-target preservation
  before target SQL, and conditional two-call retry with complete modeled outputs.
  It permits an absent sandbox after a failed deletion and uncertain target commits;
  retry acceptance is derived from no-error external observations and capacity facts,
  not assumed. All 21 new axiom guards pass; see the
  [recovery checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-recovery-proof.md).
  This does not prove that a retry is scheduled or that the remote worker becomes ready.
  [Independent lifecycle contracts](factory_retarget/FactoryChoiceProof.lean)
  characterize all four choices with necessary-and-sufficient conditions written
  without the implementation's comparison/choice helpers. They preserve keep
  precedence across rebuilt image changes, require replacement on target changes,
  retain provider multiplicity and identify fields native comparison ignores.
  All 15 new axiom guards pass; see the
  [choice checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-choice-proof.md).
  Two [standalone semantic controls](factory_retarget/README.md#standalone-semantic-controls)
  now detect omitted target assignments and erroneous refusal at the exact capacity
  limit. Each independent complete wrong-outcome witness passes before the unchanged
  populated acceptance assertion fails. Both exact source restorations and fresh
  baseline artifact graphs pass. These mutate the manual model, not native Rust,
  and exercise standalone execution assertions rather than the generic theorems;
  see the [control record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-semantic-controls.md).
- **Boundary/effort:** moderate for lifecycle action selection, substantial for
  cancellation/supersession and recovery. Current code uses cancellation plus an
  instance mutex, not a pure generation-CAS activation guard. In-flight apply may finish.
- **Native concurrency finding:** a deterministic component regression now proves
  that a linked preparation captured before the runner lock can undo a completed
  parent retarget. The complete runner, command and client lookup return to the old
  repository; later pushes follow that old target. See the
  [reproduction](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-stale-preparation.md).
  This exposes the sequential model's caller-freshness boundary. `c340a2d` now
  rereads the current runner and prepares under the existing lock, including
  parent resolution and failure publication; `a69e3cb` fixes a test-only lint.
  Five native regressions cover retarget/removal preservation, valid current-parent
  acceptance, reverse ordering and failure publication. Both full Mac/Linux checks
  pass with 465 tests passed and two skipped each; see the
  [fix qualification](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-preparation-serialization.md).
  This is component evidence, not a new Lean theorem or deployed qualification.
- **Checked watcher foundation:** the separate
  [manual watcher layer](factory_retarget/WATCHER_SOURCE.md) now composes the
  current-row projection and internally constructed request with the existing
  Factory outputs. Fourteen lemmas and 18 axiom guards pass: full obsolete/absent
  no-op preservation, captured-metadata independence, exact preparation/Factory
  result channels and valid current-parent acceptance with complete state outputs.
  SQL observation consistency, mutex correspondence and native preparation remain
  explicit interpretation contracts. See the
  [foundation record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-foundation.md).
- **Checked watcher histories:** actual modeled parent applications now compose
  with queued linked notifications. Repository or tracked-ID retargets preserve
  the entire resulting state against obsolete notifications; same-target parent
  advances accept relevant notifications using the current program and complete
  client/native/provider/request outputs. Intermediate row/key uniqueness follows
  from actual apply identity effects. The leaf passes 23 axiom guards in 5.773 s;
  see the [history record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-histories.md).
  This is manual model composition, not formal refinement of async Rust or SQL.
- **Checked watcher status/recovery:** preparation refusal now composes with an
  actual successful parent operation, retaining exact failure payloads and
  committed-versus-acknowledged publication. Successful recorded fallback may
  still publish resolution failure; Factory errors remain separate from
  preparation failure. Completed row deletion followed by a consistent reread
  cannot resurrect the runner. Ten axiom guards pass in 5.473 s; see the
  [status record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-status.md).
  The deletion contract does not prove full native cleanup.
- **Populated watcher baseline:** six independently specified complete-output
  histories now pass using only the executable models: repository/branch-ID
  retargets, current-parent acceptance, selected native deletion failure,
  preparation-refusal then success, and successful recorded fallback retaining
  its resolution error. Ten guards pass; the proof stage took 92.718 s.
  See the [fixture record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-fixtures.md).
  These are synthetic modeled executions, not deployed/native probes.
- **Checked watcher controls:** two deliberately faulty manual models now produce
  independently proved wrong outcomes: stale captured parent metadata replaces
  current program/identity, or an overstrict equality guard refuses a valid
  same-target update. The unchanged standalone populated acceptance assertion
  rejects both. Exact patch reversals restore the full source maps, and fresh
  model/fixture/positive checks pass; the final six-history proof passes again
  in 92.624 s with its original artifact hash. See the
  [control record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-controls.md).
  These controls exercise the executable model and standalone assertion, not
  native Rust faults or the generic history/status proof artifacts.
- **Saved-parent native correction:** a deterministic regression exposed a retry
  restoring an obsolete parent program, target and identity after a newer update
  completed. Hub `77b258a` now serializes parent publication/membership with the
  existing parent guard and checks the saved revision against native Lore's live
  branch identity and head before publishing either success or error. Five focused
  component tests pass, including an uncleared diagnostic failure, deleted-ID/name
  reuse and valid first provisioning. Full Mac/Linux checks each pass 469 tests
  with two skipped; see the
  [qualification record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-retry-fix.md).
  This adds native evidence, not a Lean theorem or distributed freshness guarantee.
- **Saved-parent model:** a new two-phase layer now elaborates in 18.699 s.
  It constructs accepted child inputs and preserves separate full preparation and
  link-failure maps. Discard and source-error paths need no runner row; only the
  accepted existing-runner continuation requires one. No correctness theorem is
  claimed from elaboration; see the
  [model checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-model.md).
- **Saved-parent foundation:** arbitrary-state obsolete/native-error/cancellation
  outcomes, exact preparation-error publication and complete current-head existing-
  runner acceptance now pass 28 axiom guards in 5.426 s. Accepted inputs and exact
  child/Factory outputs are derived by composing admission with the unchanged
  execution model; no desired Request/after-state or present diagnostic is assumed.
  See the [foundation record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-foundation.md).
  Successful application retains the explicit existing-row/quiescent-prefix domain.
- **Saved-parent histories:** actual newer-parent application followed by an old
  prepared retry now preserves the complete newer state, including pre-existing
  diagnostics after a noncommitted clear. Current source-error publication survives
  obsolete work. Valid same-head retry after both successful clears derives its
  next runner/domain from the first operation and produces complete child outputs.
  Seven guards and closure audits pass in 4.873 s; see the
  [history record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-saved-parent-histories.md).
  Deleted-ID/name-reuse rejection preserves arbitrary replacement state; its actual
  creation, other independently populated sequences and semantic controls remain.
- **Residual/next:** the bounded watcher slice now includes its semantic controls
  and restoration. The complete Factory lifecycle remains partial. Saved-parent
  initial allocation/general membership effects, cross-replica reconciliation, concurrent
  registration publication order, native workspace effects and eventual recovery
  remain separate obligations. Eventual successor reconciliation needs scheduling/
  failure assumptions; do not claim “stale completion can never publish” from a
  cancelled-token helper or fake native fixture.

## HUB-COMPUTER-01 — Owner/session identity and version recovery

**Invariant/failure prevented.** Browser access remains bound to its current ready
Computer, owner and admitted family. Runtime failure is not mistaken for successful
authentication/replacement. A version is recorded after native replacement readiness.
Failed replacement may retain the prior version record while leaving no running
sandbox; it must not claim the new version succeeded.

- **Source:** [browser session admission](../crates/veoveo-hub/src/computers/session.rs#L46),
  [lifecycle](../crates/veoveo-hub/src/computers/lifecycle.rs), and
  [ComputerHandoff::complete](../crates/veoveo-hub/src/store/computers.rs#L144).
- **Tests:** [session test](../crates/veoveo-hub/src/computers/session.rs#L156),
  [lifecycle tests](../crates/veoveo-hub/src/computers/lifecycle/tests.rs#L49), and
  [provisioning tests](../crates/veoveo-hub/src/computers/provisioning/tests.rs#L53).
- **Boundary/effort:** moderate for identity/version decisions; substantial for detached
  operations and SQL/native refinement. Assume fresh authoritative records, valid
  signed claims, time and native observations—not merely a cached success flag.
- **Residual/next:** follow one replacement failure/retry transition before modeling
  the lifecycle. Actual browser recovery, OpenShell termination/readiness, credential
  custody and persistent home survival need native/deployed evidence, not only fixtures.

## HUB-REGISTER-01 — Registration and deployment identity

**Invariant/failure prevented.** A registration is bound to its admitted namespace or
instance and deployment. Replacement changes endpoint, metadata, storage binding and
registration logins coherently; replica addition follows its distinct existing path.
Concurrent operations cannot reassign unrelated work or leave a partly replaced
registration. These are target properties to establish, not new admission rules.

- **Source:** [registration service](../crates/veoveo-hub/src/registration/service.rs),
  [registration store](../crates/veoveo-hub/src/registration/store.rs), its adjacent
  SQL, [snapshot](../crates/veoveo-hub/src/registration/snapshot.rs), and
  [data environment binding](../crates/veoveo-hub/src/data_environment.rs).
- **Tests/evidence locations:** [registration](../crates/veoveo-hub/tests/registration.rs)
  and [data environments](../crates/veoveo-hub/tests/data_environments.rs).
- **Boundary:** token/signature verification and kernel login/schema operations have
  separate owners. Ordinary namespace-pinned registration and instance-token
  registration deliberately differ; the specification must retain both. Preparation
  in a different database is not atomic with the installation registration transaction.
- **Next:** connect deployment/endpoint association with fresh and retained
  routing and SDK recipient binding. Removal and housekeeping remain outside
  the qualified concurrency slice and belong to later lifecycle composition.

The [source review](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-registration-transaction-source-review.md)
separates namespace replica admission, replacement, per-client update and instance
preparation. The [native overlap probe](registration_transactions/evidence/native-replica-replacement.json)
confirmed that a replacement could miss an unfinished same-metadata replica,
leaving both old and new registrations/logins. Locking the deployment read before
comparison fixes this schedule: the unchanged regression and six existing
registration integration tests pass, as do formatting and strict library Clippy.
This is native component evidence. The subsequent
[complete-program theorem](registration_transactions/evidence/program-completeness.json)
derives all staged writes, including captured login removals, for every reachable
committing attempt in the source-mapped transaction model. The subsequent
[lock/observation and effect proofs](registration_transactions/evidence/locked-observations-effects.json)
derive exclusive active ownership, current metadata/DELETE snapshot stability and
exact complete-program effects against the independent sequential specification.
The [serial-history proof](registration_transactions/evidence/serial-history.json)
now composes those invariants into independent sequential effects in actual
server-commit order for every finite modeled execution. Aborted attempts never
appear in that history. The [constructed replica executions](registration_transactions/evidence/valid-replicas.json)
now establish valid initial constraints, successful admission and completed
same-metadata overlap with exact independent output maps and both acknowledgements.
The [replacement and abort witnesses](registration_transactions/evidence/replacement-abort.json)
also establish changed-metadata ordinary replacement, unconditional instance
replacement and abort after staged private writes, with exact effects and responses.
The [original-protocol midpoint](registration_transactions/evidence/original-midpoint.json)
constructs the old replica's successful commit while replacement still holds its
real lock and has captured only the seed. The completed
[counterexample and qualification](registration_transactions/evidence/registration-transactions-receipt.json)
continue through both successful acknowledgements, prove the exact mixed output
maps and refute the independent serial contract in actual commit order. The first
register/replace concurrency slice is qualified within its stated model/native
boundaries. The later checkpoints below extend it to per-client updates;
removal and authority composition remain outside. Captured full rows
are proof-level snapshot witnesses; native DELETE returns key/server/login fields.
No SQL refinement follows from these model results.

The subsequent [native stale-update qualification](registration_transactions/evidence/native-stale-update.json)
reproduces an update with a row captured before replacement that later overwrites
the replacement's deployment metadata after its original row is gone. The minimal
fix uses deployment-first locking and rollback on a missing registration, retaining
no-op success and existing per-client semantics. Both native concurrency tests and
six instance-registration tests pass. The subsequent
[update program/effects checkpoint](registration_transactions/evidence/update-program-effects.json)
derives complete update staging in arbitrary finite mixed executions and exact
captured-row field effects, plus the no-op's staged program and rollback effect.
The [shared-lock/current-state checkpoint](registration_transactions/evidence/update-stability.json)
now derives both methods' lock ownership and stable observations, connecting a
committing update to its independent current-row operation and a pending no-op to
an absent row. The [mixed-history proof](registration_transactions/evidence/update-serial-history.json)
now establishes independent register/replace/update effects in actual tagged commit
order for arbitrary finite modeled interleavings, excluding aborted contributions
even when a no-op later acknowledges success. The
[normal-update witness](registration_transactions/evidence/update-valid-success.json)
constructs a well-formed two-replica seed and seven actual steps through successful
acknowledgement, with exact complete output and other-replica/role/binding
preservation. The [replacement/no-op witness](registration_transactions/evidence/update-replacement-noop.json)
now constructs the old client's preflight, a complete ordinary replacement and the
stale update's rollback and successful return. The entire replacement database,
winner response and sole registration commit event are preserved. A shared reached
replacement state is constructed through real registration steps under both update
source orders. The [paired counterexample and qualification](registration_transactions/evidence/update-transactions-receipt.json)
fork that exact reached state: fixed code rolls back without effects, while the
former update order commits only stale deployment metadata after finding no client
row. Both calls report success. The complete old output preserves the winner row
and roles but disagrees with the same independent serial contract in actual tagged
commit order. Together with generic histories, normal/no-op executions and separate
native regressions, this qualifies register/replace/update concurrency within the
stated boundaries. It is not formal Rust/SQL refinement or complete registration
lifecycle coverage.

## HUB-LORE-01 — Native grants and contributed token issuance

**Invariant/failure prevented.** Hub calculates the intended repository grant from
the caller's tenant, assignments and groups, and emits that grant with the intended
native token context. Common issuer, resource, client, identity and purpose cannot be
replaced by owner claims. Ownership/sharing operations preserve the declared native
semantics, including legitimate access; there is no second Lore authorization layer.

- **Source:** [Lore contribution](../crates/veoveo-hub/src/bin/hub/contributions/lore/mod.rs),
  [sharing changes](../crates/veoveo-hub/src/bin/hub/contributions/lore/sharing/changes.rs),
  [native catalog](../crates/veoveo-hub/src/bin/hub/contributions/lore/catalog.rs),
  [repository operations](../crates/veoveo-hub/src/bin/hub/contributions/lore/repositories.rs),
  and [common issuance](../crates/veoveo-hub/src/issuance.rs).
- **Tests/evidence locations:** [native issuer](../crates/veoveo-hub/tests/native_issuer.rs),
  [Lore catalog](../crates/veoveo-hub/tests/lore_catalog.rs) and
  [sharing](../crates/veoveo-hub/tests/lore_sharing.rs).
- **Boundary:** Lore's verification/enforcement, OpenShell custody and signed-token
  lifetime remain native responsibilities. Revocation must respect actual token
  expiry/reuse semantics, not promise instantaneous global removal. Shared group
  projection and common issuance are dependencies to trace, not assumed correct by
  describing the resulting grant as authorized.
- **Next:** establish production-linked assignment/group and token-assembly decisions,
  then connect sharing publication to HUB-CONFIG-01 and native protocol tests.

The immediate implementation-linked target is the unchanged
[`AdmittedCatalogSections::repository_grant`](../crates/veoveo-hub/src/contract/catalog.rs)
and its `group_memberships` dependency. This target is **not yet proved**. Its
specification is a relation over the admitted inputs, independent of the iterator
implementation:

- A matching assignment is in the exact `(tenant, repository)` entry and names
  either the requested principal or a group in that principal's memberships in
  that tenant. Memberships are taken from the admitted sections, not assumed from
  an identity provider's group strings.
- The result is absent exactly when no such assignment exists. A matching
  assignment with no actions still returns a grant: this is legitimate read access.
- A returned grant preserves the requested repository and contains exactly the
  union of all matching assignments' actions. Both inclusion directions matter:
  no extra authority and no loss of legitimate authority.
- Projecting group membership ignores its role and deduplicates the group. The
  proof must not introduce a role threshold, require nonempty actions, or invent
  a repository delegation ceiling.

Native map/set/iterator correspondence remains explicit. Admitted-input provenance,
catalog publication, SDK detail construction and verification, and the native Lore
contribution are separate composition obligations; proving this calculation alone
does not discharge them.

The [translation receipt](request_composition/evidence/repository-grant-translation-20261031.json)
records two source extractions and a backend continuation. The producer graph has
38 transparent own functions and 37 transparent own types, with all retained field
checks passing. Aeneas stops at the required `Iterator::cloned` associated-type
equality, before emitting Lean. An additional empty default trait method is recorded
as extra extraction scope; it is not the cause of that required dependency's failure.
No runtime or backend rewrite was made, and this is not a producer proof. The separate
SDK [detail qualification](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/main/proofs/repository_grants/evidence/qualification-20261031.json)
checks complete construction/projection and roundtrip preservation over the actual
extracted functions, including exact repository text, action membership and valid
empty-action reads. Thirteen closure guards admit only standard Lean axioms.
The source fault drops nonempty actions: the unchanged projection proof fails at
that exact mismatch, while an independent Funs-only witness proves the wrong result
and retained empty-read behavior. The existing native test separately catches the
same fault and passes after exact source restoration. Native library value models,
parser/serde/signature correspondence and the complete issuer/verifier path remain
outside this dependency proof; HUB-LORE-01 remains open.

## HUB-CLEANUP-01 — Cleanup, references and transactional audit

**Invariant/failure prevented.** Retention and deletion affect only their intended
records and preserve references required by live work. The installation database is
never a cleanup target. Where a mutation promises transactional audit, either both
commit or neither does. Interrupted external cleanup has an explicit residual state
and retry behavior rather than an unsupported atomicity claim.

- **Source:** [environment cleanup](../crates/veoveo-hub/src/data_environment.rs),
  [registration removal/collection](../crates/veoveo-hub/src/registration/store.rs),
  [Hub store](../crates/veoveo-hub/src/store), adjacent prune/refresh/control SQL, and
  the factory/Computer lifecycle paths referenced above.
- **Tests/evidence locations:** registration/data-environment/control-store tests and
  targeted concurrent cleanup/refresh/lifecycle probes added with each slice.
- **Boundary:** database drops and native sandbox deletion are external steps, not one
  transaction. A successful reference query alone does not prove a later destructive
  action safe against concurrent creation. Inspect actual exclusion/serialization;
  do not assume it. Core's maintenance/audit implementation is a separate dependency.
- **Next:** inventory each destructive operation's protected references and commit
  boundary, then prove the necessary local decision/transaction and check interruption
  and concurrent creation against the actual implementation.

Refresh review identified a specific composition obligation:
[`session_compromised`](../crates/veoveo-hub/src/state/session.rs) observes replay
revocation only while the family row exists, and
[`prune_families.sql`](../crates/veoveo-hub/src/store/refresh/prune_families.sql)
removes expired families. Task route expiry is independently derived from its TTL,
and the delegated caller is stored as JSON rather than a family foreign key.
Do not infer permanent Task-compromise history from the retained-family predicate.
Check the actual live-Task/expired-family cleanup case before deciding whether
the intended lifecycle contract needs a fix; no tombstone or new denial rule is
introduced by this source observation.

## Existing formal evidence and its limits

The local [original report](selection/RESULTS.md) and
[lookup report](selection/LOOKUP_RESULTS.md) link compact receipts and manual commands.
Within their stated model boundary, explicit-selection base independence and total
zero/one/multiple lookup classification are **proved-within-stated-boundary**.
Lookup assumes a total, correct branch comparator; it counts entries even when their
deployment IDs coincide. The corresponding local kernel hash is
`3f0b53882fd043a3292184d3e3f71799de77058175fcd58ef51f82ce7abfe7c1`, and generated model is
`949f66438af4be34f2c68ad5b83ab7ccf951538da60b2b436ec74866dad61951`.

Fresh extraction, semantic mutants and restoration were measured. The duplicate
mutant compiled/extracted and passed the older conditional resolver proofs, while
the stronger lookup proof and concrete same-deployment Rust test rejected it.
The published merge retains those exact kernel/model bytes and the caller linkage,
as inspected without rerunning the proofs. These facts do not prove global
registration selection or qualify the changed surrounding routing. Extraction/models, lawful comparison and Lean foundations
remain explicit trust boundaries; cryptography, Core, Lore and OpenShell are not proved.

## Manual updates and reuse

When verification is requested, choose one important property and trace its actual
implementation/callers before writing another theorem. Record the source revision,
shared source/model correspondence, assumptions, specification, checked output and a
meaningful negative control. Keep effects/integration evidence distinct. After code
changes, retain earlier receipts as historical and mark applicability not rechecked
until correspondence is established again; this is human reporting, not a new gate.

Use this small entry outline here, or adapt the concept in another repository later:

```text
Stable ID and invariant; concrete failure prevented:
Formal status / exact boundary:
Source revision, files and extraction/model correspondence:
Assumptions and applicability (measured / not rechecked):
Proof/test/deployed evidence, commands and receipt:
Residual effects or unsupported claims:
Next smallest useful investigation; owner if assigned:
```

Keep IDs stable when the implementation moves. Update the boundary rather than
quietly broadening an old proof claim. The roadmap orders the approved work; these
entries record evidence and gaps without introducing a new bookkeeping schema.
