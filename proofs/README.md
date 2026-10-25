# Hub proof and evidence map

Track important system properties, their implementation boundaries and the evidence
for them. The unit of coverage is an invariant, not a theorem count. A small helper
proof does not establish the whole request, transaction or runtime lifecycle.

Verification remains manual and on demand. This map adds no CI job, hook, watcher,
proof requirement or runtime check. It is not authorization to implement every
candidate below, nor a proposal for additional security rules.

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
| HUB-TASK-01 | Captured Task routing and retry integrity | unproved | **Next investigation:** trace creation through dispatch |
| HUB-AUTH-01 | Current policy and delegated authority do not amplify access | unproved | Isolate an existing permission/rule decision |
| HUB-OAUTH-01 | Exact session binding and refresh-family lifecycle | unproved | Specify the pure current-family decision first |
| HUB-CONFIG-01 | Coherent live configuration publication | unproved | Model transaction/CAS and activation-claim semantics |
| HUB-ROUTE-01 | Deterministic fresh routing and stable captured routing | partial | Trace broader routing beyond the linked kernel |
| HUB-CATALOG-01 | Catalog, cursor, cache and listener isolation | unproved | Trace request identity and stale-response fencing |
| HUB-FACTORY-01 | Authoritative reconciliation and safe native lifecycle | unproved | Separate action selection from concurrency/effects |
| HUB-COMPUTER-01 | Owner/session identity and version recovery | unproved | Model admission and version-recording transitions |

These priorities reflect impact and a viable proof boundary, not a commitment to
prove all eight or a judgment that the existing implementation is defective.

## HUB-TASK-01 — Captured Task routing and retry integrity

**Invariant/failure prevented.** A retained Task continues to its original deployment
and composition. A same-deployment retry preserves its first capture and expiry;
another deployment or mismatched owner/profile/WorkContext/authority cannot rebind
that route. A changed proposed composition is not itself a reason to replace the
stored capture. Prevents Task misrouting, ownership confusion and retention extension.

- **Source:** [creation/capture check and reusable](../crates/veoveo-hub/src/state/task_routes.rs#L219),
  [conflict-preserving insert](../crates/veoveo-hub/src/state/task_routes/create.sql),
  [authorize_task_route](../crates/veoveo-hub/src/mcp/authorization.rs#L83),
  [Task forwarding](../crates/veoveo-hub/src/mcp/tasks.rs#L31), and
  [ownership comparison](../crates/veoveo-hub/src/state/task_routes.rs#L83).
- **Tests:** [task route tests](../crates/veoveo-hub/src/state/task_routes/tests.rs):
  `same_deployment_retry_preserves_first_execution_capture`,
  `identical_source_tasks_from_two_deployments_never_share_a_route`, and concurrent
  projection/changed-authority/expiry cases. [Calls](../crates/veoveo-hub/tests/calls.rs)
  covers retained delegation; origin's composition-only context retains this intent.
- **Boundary/effort:** moderate for an extracted comparison kernel; substantial for
  end-to-end creation/read/authorization/dispatch correspondence. Assume correct ID
  comparisons, time and authoritative composition lookup; do not silently assume SQL.
- **Residual/next:** trace all four stages first, then choose a useful shared boundary.
  A predicate-only proof is partial. Uniqueness, atomic competing inserts, pruning,
  current policy and actual upstream routing still need database/integration evidence.

## HUB-AUTH-01 — Current policy and delegated authority

**Invariant/failure prevented.** A hop cannot manufacture permissions or extend its
admitted lifetime; it preserves original caller/captured routing while naming the
actual actor. Current required scopes and declared uses still apply. Hub-owned rule
evaluation gives matching deny rules precedence and otherwise denies without an
allow. Delegated callers do not acquire administrative access from an admin principal.

- **Source:** [narrow/issue/held/subject_of](../crates/veoveo-hub/src/delegation.rs),
  [policy decide and assemble_rule_outcome](../crates/veoveo-hub/src/policy/evaluation.rs#L38),
  [MCP delegation narrowing](../crates/veoveo-hub/src/mcp/authorization.rs#L392),
  and [existing delegated-admin refusal](../crates/veoveo-hub/src/http/action_audit.rs#L45).
- **Tests:** [calls scenario](../crates/veoveo-hub/tests/calls.rs#L66) checks narrowed
  downstream permissions, actors, Task-derived calls, expiry and delegated-admin
  refusal; [shared_policy](../crates/veoveo-hub/tests/shared_policy.rs#L13) compares
  Hub/background policy behavior on fixtures.
- **Boundary/effort:** small–moderate for generic set/rule decisions; larger across
  issuer/SDK actor-chain composition. Assume verified signed input and lawful set/ID
  operations. A set-intersection lemma alone does not prove every HTTP route uses it.
- **Residual/next:** choose one real decision and its caller path, not a new rule.
  Token verification, current grants, replay checks, route wiring and expiry clocks
  remain external obligations; cryptography/SDK correctness is not promised here.

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
concurrent publication does not partly apply. Activation claims coalesce changes
through one database claim; that is not exactly-once external lifecycle execution.

- **Source:** [publish_control_revision/claim_control_activation](../crates/veoveo-hub/src/store/control.rs#L54)
  and [control SQL](../crates/veoveo-hub/src/store/control/lock_active.sql).
- **Tests:** [control_store](../crates/veoveo-hub/tests/control_store.rs#L22):
  `publishes_immutable_revisions_and_moves_active_pointer_atomically`,
  `concurrent_publications_activate_one_revision_and_refuse_the_other`, and
  `one_process_claims_each_activation_from_the_last_claimed_revision`.
- **Boundary/effort:** substantial: SQL transaction/lock semantics are central, not
  incidental I/O around a pure helper. A first abstract CAS/claim model can clarify
  intent, assuming PostgreSQL atomicity/isolation and correct transaction scope.
- **Residual/next:** justify model-to-SQL correspondence before claiming coverage.
  Process failure, replica notification and OpenShell effects are separate; preserve
  database tests rather than treating a Boolean revision comparison as their proof.

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
quietly broadening an old proof claim. This map is neither a new bookkeeping schema
nor an execution plan to implement every candidate.
