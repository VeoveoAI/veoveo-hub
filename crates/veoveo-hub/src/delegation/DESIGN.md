# Delegated authority decisions

The exchange validates requested uses before reading the subject's profile, checks
that exact profile resource before looking for providers, and only then evaluates
the original caller's current permissions and assembles the exchange grant. The
request and Task subject paths establish their own lifetime before this decision;
a later request retains its signed deadline, while a live Task has its bounded
renewable lifetime. A downstream namespace is not constrained to the permission
names of the upstream namespace.

`authority.rs` owns the shared pure staged decisions and output assembly. Native
`BTreeSet` adapters retain ordered membership and intersection; they do not replace
set lookup with a quadratic list scan. The stages carry the admitted request and
resource into the exact grant, retaining the original caller, immediate actor,
profile, Task and deadline. Parsing, principal/group projection, catalog selection,
current role evaluation, provider discovery, Task/session reads and the SDK's chain
validation, deadline clamp and signing remain their existing owners' work.

The same module assembles delegated policy output after current policy and managed
client admission. A denied decision is unchanged except for an explicit managed
client denial. An allowed delegated decision intersects its actual granted set with
the carried permissions and checks the declared route: `Authenticated` permits an
empty set, a permission route requires that carried permission, and server/Task
access requires a nonempty narrowed set. An undeclared route is refused. Every
other decision field is retained. This adds no authority or rejection condition.
