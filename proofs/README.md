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

The Task-continuity qualification is a separate source boundary: changed Hub files
based on `651390dbbb8b4e65b5b8bcd3f8714a981fac2371` and the exact sibling SDK files
based on `7e2489dccbee6d00b377b174f68f9d5de692a68b`. Its shared-source package records
both snapshots. At that qualification, Hub's SDK pin was
`a41c9cc6c81f6c0305dd41165defb142740ed6b1`; the private SDK refactor was not
claimed to be present in that dependency or deployed. Subsequent dependency adoption
has its own source-correspondence boundary.

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
| HUB-CONTRACT-01 | Shared MCP extension contracts preserve meaning from declaration to enforcement | partial | Connect declaration generation/validation and current policy to the proved SDK resource requirement |
| HUB-TASK-01 | Captured Task routing and retry integrity | partial | SQL and asynchronous adapter refinement remain |
| HUB-AUTH-01 | Current policy and delegated authority do not amplify access | partial | Connect decide_server to qualified CurrentPermissions/current-role results, then session/issuer continuity |
| HUB-OAUTH-01 | Exact session binding and refresh-family lifecycle | unproved | Specify the pure current-family decision first |
| HUB-CONFIG-01 | Coherent live configuration publication | unproved | Model transaction/CAS semantics |
| HUB-ROUTE-01 | Deterministic fresh routing and stable captured routing | partial | Trace broader routing beyond the linked kernel |
| HUB-CATALOG-01 | Catalog, cursor, cache and listener isolation | unproved | Trace request identity and stale-response fencing |
| HUB-FACTORY-01 | Authoritative reconciliation and safe native lifecycle | unproved | Separate action selection from concurrency/effects |
| HUB-COMPUTER-01 | Owner/session identity and version recovery | unproved | Model admission and version-recording transitions |
| HUB-REGISTER-01 | Registration, replacement and storage identity | unproved | Trace both registration transaction paths and their callers |
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
  not a Lean alias proof. Connect the established route requirement to current
  role/risk evaluation and policy output, then identity, declaration generation/
  validation and typed-route dispatch. Map
  Tasks, usage metadata, discovery degradation and App dependency metadata to their
  owning contracts and relevant guarantees. They are not all interchangeable
  `capabilities.extensions` declarations: some use `_meta` or native MCP extensions.

The qualified [authority package](authority/README.md) consumes decoded
authorization semantics and proves only its stated internal sub-boundary. It does
not discharge this contract chain or prove serialization. The SDK matcher result
now supplies an established sub-boundary; its composition with actual Hub policy
and the asynchronous caller still needs proof. The sibling SDK work is not claimed
to be present in Hub's pinned dependency or deployed.

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
- **Residual/next:** connect `decide_server` admission/output to the established
  evaluator. Principal projection, shared MCP extension correspondence
  and session/issuer continuity remain.
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
- **Residual/next:** specify the pure admission predicate before the asynchronous
  protocol. Atomic consumption, concurrent retries, delivery encryption and real IdP
  behavior require their own evidence; an abstract transition proof is design-level.

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
- **Residual/next:** justify model-to-SQL correspondence before claiming coverage.
  Process failure, replica notification and OpenShell effects are separate; preserve
  database tests rather than treating a Boolean revision comparison as their proof.

The first transaction slice follows `publish_control_revision`: acquire the
`control_active` row lock, compare its revision, create the revision and objects,
replace Work Context identity rows, move the pointer, append audit, then commit.
The schema's singleton constraint/trigger and immutable-revision constraints are
part of the correspondence, not optional modeling conveniences. The competing
publisher must observe the committed predecessor after obtaining the lock (or abort
under stronger isolation); a stale pre-lock read cannot authorize its commit.
Failure at any write or audit append must leave committed state unchanged. A lost
commit acknowledgement is a separate ambiguous outcome, not evidence of rollback.

Local adoption is a second obligation: the publication mutex also covers the
[native reload path](../crates/veoveo-hub/src/bin/hub/control_reload.rs), which reads
the authoritative pointer after taking the guard. Trace actual catalog/HTTP-handle
replacement and notification/reconnect behavior before claiming coherent adoption
or convergence. The active revision may advance elsewhere while one replica prepares
its snapshot; temporary lag remains allowed by the architecture. No new synchronous
global barrier is proposed.

## HUB-ROUTE-01 — Fresh routing and captured routing

**Invariant/failure prevented.** Fresh requests choose deterministically from ordinary
deployments and the principal's explicit branch/base selection. A missing explicit
branch is unavailable, never silently replaced by base or an ordinary deployment.
Already captured requests/Tasks retain their exact composition rather than rerouting.

- **Current source:** [admit_execution/route/select_instances](../crates/veoveo-hub/src/execution/selection.rs)
  and [RegistrationSnapshot::of](../crates/veoveo-hub/src/registration/snapshot.rs).
  The original ordinary-routing review used
  [5d3f43f](https://github.com/VeoveoAI/veoveo-hub/blob/5d3f43fb8fdc3b25436f9d7526ae7f9f48b662c5/crates/veoveo-hub/src/execution/selection.rs#L39).
  Ordinary ordering prefers live, then newest registration, then deployment ID.
  When all are orphaned, the newest may still route until grace collection removes it.
- **Tests:** [selection scenario](../crates/veoveo-hub/src/execution/tests.rs)
  and [routing test](../crates/veoveo-hub/tests/routing.rs),
  `the_newest_registered_deployment_serves_and_captures_keep_their_routing`.
- **Formal boundary:** the [selection pilot](selection/README.md) is partial
  evidence for this system invariant; its precise proved subproperties are below.
- **Applicability:** the merge preserves exact proved kernel/model bytes and caller
  linkage by read-only inspection. Proof runs remain tied to their recorded inputs;
  the changed surrounding routing has no new formal or deployed qualification here.
- **Residual/next:** trace the broader routing contract. Map assembly, ordinary
  deployment ordering, retention, selection storage and request dispatch remain unproved.
  Avoid more low-value corollaries of the already covered local branch scan.

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
- **Boundary/effort:** moderate for key/cursor comparisons; substantial for the
  cache/listener state machine. Assume collision-resistant identity hashing, correct
  authenticated inputs and mutex/watch-channel semantics where effects are modeled.
- **Residual/next:** trace invalidation tokens and publication ordering before choosing
  a shared predicate. A key-equality proof does not establish freshness, reconnect
  delivery, actual client caching behavior or cross-replica notification reliability.

## HUB-FACTORY-01 — Reconciliation and native lifecycle recovery

**Invariant/failure prevented.** Subscription/reconnect performs authoritative branch
reconciliation; pushes use their pusher's authority. Applications to one instance
serialize. Image-build/policy preparation failure before native restart/replacement
does not replace the old sandbox. Image/policy changes choose replacement; unchanged
settings choose the appropriate restart/start path. Later source-build/startup failure
does not imply automatic preservation of the previous serving worker.

- **Source:** [follow/recheck/change](../crates/veoveo-hub/src/bin/hub/contributions/lore/factory/watch.rs#L129),
  [Factory::apply/restart](../crates/veoveo-hub/src/factory.rs#L214), and
  [sandbox::runs](../crates/veoveo-hub/src/factory/sandbox.rs#L152).
- **Tests:** [watch fixture](../crates/veoveo-hub/src/bin/hub/contributions/lore/factory/tests.rs#L493)
  covers resubscription and replaced watchers; [factory scenarios](../crates/veoveo-hub/tests/factory.rs#L532)
  cover changed image/policy, crash loops and failed builds.
- **Boundary/effort:** moderate for lifecycle action selection, substantial for
  cancellation/supersession and recovery. Current code uses cancellation plus an
  instance mutex, not a pure generation-CAS activation guard. In-flight apply may finish.
- **Residual/next:** choose a meaningful action decision only after Task work. Eventual
  successor reconciliation needs scheduling/failure assumptions; do not claim “stale
  completion can never publish” from a cancelled-token helper or fake native fixture.

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
- **Next:** trace statement order, uniqueness, lock coverage, successful responses and
  failure/rollback paths before choosing the transaction model. Compose the resulting
  deployment/endpoint association with routing and SDK recipient binding.

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
