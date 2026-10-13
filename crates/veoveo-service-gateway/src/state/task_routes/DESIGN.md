# Gateway Task Routes

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP 2026-07-28 Tasks extension | Opaque canonical Task identifiers projected from upstream CreateTaskResult |
| Veoveo invocation authority | Stable tenant, actual actor, Work Context, profile and invocation provenance; every use checks current policy and retained data labels |
| PostgreSQL 18 | `gateway.task_route` in core's `gateway` schema, read and written through checked sqlx queries beside `task_routes.rs` and pruned through core's `PlatformStore::prune_task_routes` |
| Veoveo `gtr_` route identifiers | 32 random bytes encoded as base64url without padding; identifiers convey no access authority |

`task_routes.rs` owns the mapping between an upstream Task and its public gateway
identifier. The source tuple is server, opaque upstream ID, owner and profile, and a
unique constraint holds it. Each mapping also records its tenant, Work Context,
authorization digest and the owner's actor and provenance. Those fields must match
before an existing route is reused.

The tenant, owner and Work Context columns reference their rows in the `identity`
schema. The Gateway records the owner as an identity before it writes the route, and
the active configuration defines the Work Context. A route goes with its Work Context
when a later configuration removes it.

An upstream tool retry can return its original Task. Gateway projection returns that
Task's existing public route, including across replicas and fresh authentication.
It preserves the original creation time and expiry. Competing projections insert with
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
