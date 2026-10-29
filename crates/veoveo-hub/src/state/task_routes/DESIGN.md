# Hub Task Routes

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP 2026-07-28 Tasks extension | Opaque canonical Task identifiers projected from upstream CreateTaskResult |
| Veoveo invocation authority | Stable tenant, actual actor, Work Context, profile and invocation provenance; every use checks current policy and retained data labels |
| PostgreSQL 18 | `hub.task_route` in core's `hub` schema, read and written through checked sqlx queries beside `task_routes.rs` and pruned through core's `PlatformStore::prune_task_routes` |
| Veoveo `gtr_` route identifiers | 32 random bytes encoded as base64url without padding; identifiers convey no access authority |

`task_routes.rs` owns the mapping between an upstream Task and its public hub
identifier. The source tuple is logical server, immutable deployment, opaque upstream ID, owner and profile, and a
unique constraint holds it. Each mapping also records its tenant, Work Context,
authorization digest, exact execution composition and the owner's actor and provenance. The
composition is retained by its canonical digest before a route is inserted. Authority fields must match
before an existing route is reused.

A route of a server that declares uses keeps `delegated_caller`, the caller a token
exchange with the Task as its subject acts for while the Task runs, unless another
server's call created the Task. Retention removes it with its route.

The tenant, owner and Work Context columns reference their rows in the `identity`
schema. The Hub records the owner as an identity before it writes the route, and
the active configuration defines the Work Context. A route goes with its Work Context
when a later configuration removes it.

An upstream tool retry can return its original Task. Hub projection returns that
Task's existing public route, including across replicas and fresh authentication.
It preserves the original creation time, expiry and execution capture, including when a
later selection reaches the same deployment through another composition. Competing projections insert with
`ON CONFLICT DO NOTHING` and then read the one route the source tuple holds; no lookup
redispatches the upstream tool. A retry neither extends nor recreates an expired
route that retention has not yet deleted.

A route lives for the upstream Task's own `ttl`, or seven days when the Task names
none, and access requires an unexpired route and current authorization. Every replica's
hourly retention pass deletes the routes that have expired, at most 1,000 per
statement in expiry order and each statement its own transaction, until a statement
deletes fewer. A retry inside the route's lifetime therefore meets the original
route. A retry after the route is gone meets an upstream Task whose own retention has
ended, unless the Task named no `ttl` and outlived the seven days, in which case the
retry records a new route.

Every lookup names the tenant: a public identifier is read only in the caller's tenant,
and a projection reads its source tuple in the draft's tenant, with the owner's
identity row in the same tenant. Reads, updates, cancellations and listeners then
match recorded ownership before checking
current profile exposure and policy. Ownership is the principal key, kind, issuer and
subject, the invocation provenance with its initiator and delegation identity, and
the admitted data labels. The route separately binds tenant, Work Context and profile.
Current labels must contain the recorded labels. A new sign-in, an unrelated scope
change, or a Work Context policy revision does not change ownership. Removing required
current permissions still denies access. Every upstream request carries the current
caller authority; the owning server enforces its current domain and named grants.
Creation authority never becomes a credential for later access.

The full invocation fingerprint, stored as a `sha256:` digest, remains admission
evidence and an exact retry constraint. Discovery, continuation and subscription cache
keys retain their full authority fingerprints. Their isolation must not be replaced
with the ownership comparison.

Qualification exercises refreshed scopes and policy revisions, required-scope and
role removal, retained-label removal, wrong tenant, context, profile, actor and
issuer, changed initiator and delegation, current policy revocation and removed Task
exposure. The database tests cover twelve competing projections across two pools,
retained expiry, changed Work Context, authority and labels, expired mappings,
independent servers, overflow-safe retention input and the stored ownership round
trip.

Deployment identity is independent of the logical namespace and the business Work
Context. Different deployments can project the same opaque source Task ID without
colliding. Current registration of the exact immutable deployment may resume after
a process restart; no lookup substitutes the newest registration of another deployment.

A Task's current request can route a slug to another deployment. After
owner/provenance checks, Task routing restores the original retained composition, so a
Task keeps the deployment that created it. Current component/data policy still
applies, and a delegated request must already carry the same capture.
Listeners are keyed by logical server, deployment and composition, so one request
can retain old Task streams while its current catalog selects a replacement. A live
catalog publication completes the listen after an invalidation hint; reconnect
re-admits every Task's own capture and current access rather than retaining stale
subscription authority. The administrative Task cancel finds the caller's route by the
identifier the server issued, in the caller's tenant and profile, and then follows the
same path: ownership, capture, current access and a forwarded `tasks/cancel` to the
route's deployment.

## Decision And Dispatch Boundary

Task continuity has two distinct decisions. Projection checks that the proposed
composition selects the Task's deployment; a retry then either refuses the existing
route or returns that route unchanged, including its original capture and expiry.
It does not replace that capture with the proposed one when both select the same
deployment. Reading, updating, cancelling and listening instead admit the current
caller against the retained route, restore its capture and apply current exposure
and policy before dispatch.

The dependency-light decision module beside this design is shared by production
and the manual proof package. Its inputs retain the binding fields, ownership and
provenance, labels, captured execution and deployment rather than an already-made
authorization decision. Its successful results carry the retained route or the
captured Task request and current caller. Forwarding consumes those results; a
successful decision cannot silently choose the current ordinary deployment or
restore the authority recorded at creation.

The admission phases preserve the existing order: owner and delegated-capture
checks precede the retained-composition read, capture/deployment agreement precedes
exposure, and current policy precedes successful dispatch assembly. Store failures
remain failures, and existing refusal responses retain their meaning. The pure
boundary neither repeats nor replaces a database constraint, atomic insert,
expiration query or policy decision.

Manual proofs concern those production-called decisions and their output fields,
under explicit lawful equality and label-set assumptions. Storage supplies the
requested retained composition and current registration snapshot; policy supplies
its actual current admission, not a grant copied from Task creation. SQL uniqueness,
concurrent projection and pruning, token verification and signing, and transport
delivery remain separate obligations. Core owns Task state transitions, cancellation
and recovery; proving a Hub dispatch decision does not prove that SQL lifecycle.
