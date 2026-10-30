import RegistrationUpdateModel

/-! Independent per-client UPDATE contract. This does not impose equality of
all replica metadata, repeat service authentication, or verify SQL execution. -/
namespace RegistrationUpdate
open RegistrationTransaction
variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- Independent row field specification, preserving the CURRENT row's identity,
login, digest, issuance and stable remainder. The second metadata component is
proof-carried admission context, not a native registration column. -/
def expectedRow (row : Row C L M B S D T R) (request : Request C M S D T) : Row C L M B S D T R :=
  { client := row.client
    login := row.login
    metadata := (request.metadata, row.metadata.2)
    payload :=
      { statement := request.statement
        tokenDigest := row.payload.tokenDigest
        issuedAt := row.payload.issuedAt
        updatedAt := request.now
        stable := row.payload.stable } }

/-- Independent matched UPDATE effect: the deployment keeps its binding; only
this client row is changed, and every login/other replica/frame is preserved. -/
def expectedMatched (db : DB C L M B S D T R P F) (request : Request C M S D T)
    (row : Row C L M B S D T R) : DB C L M B S D T R P F :=
  { metadata := (request.metadata, db.metadata.2)
    registrations := fun client =>
      if client = request.client then some (expectedRow row request) else db.registrations client
    logins := db.logins
    frame := db.frame }

/-- One admitted store operation, evaluated against its serialized current row.
Absent row is the no-effect path; success acknowledgement need not occur before
some OTHER writer changes the database. Stale preflight data is not used here. -/
def sequentialUpdate (db : DB C L M B S D T R P F) (request : Request C M S D T) :=
  match db.registrations request.client with
  | none => db
  | some row => expectedMatched db request row

def Complete (tx : Transaction C L M B S D T R) : Prop :=
  ∃ row, tx.matched = some row ∧ tx.outcome = .updated ∧
    tx.staged = [.deploymentMetadata tx.request.metadata,
      .matchedRow tx.request.client (patched row tx.request)]

def NoopProgram (tx : Transaction C L M B S D T R) : Prop :=
  tx.matched = none ∧ tx.staged = [.deploymentMetadata tx.request.metadata]

/-! Source correspondence for this extension:
* preflight: service.authenticate's read + digest comparison precedes store.update.
  Changed metadata, authority/signature/deployment/slug admission are boundaries;
  service equal-metadata short-circuit, audit, reload and reply are not modeled.
* metadata: update_deployment.sql changes ClientMetadata only and locks the
  retained deployment BEFORE the per-client UPDATE. MetadataAccepted describes
  selected native statement constraints, not an application correctness oracle.
* matchRow/missRow: update_registration.sql WHERE client_id=request.client.
  Matched full row is a logical snapshot witness; native execute supplies count.
  matchedRow is keyed by request.client even for an arbitrary malformed DB map.
  Native PK projection is represented by WellFormed when connecting row.client.
* patched: only metadata/software_statement/updated_at change. Stable row payload
  includes server/deployment/orphaned_at; branch is joined context outside row.
  Storage binding is on DEPLOYMENT, not a registration column. Row binding is
  carried proof-level admission context shared with the existing register model.
* missRow→rollbackNoop→acknowledgeNoop: affected0 explicitly rolls back before
  Ok(()). Rollback contributes no commit event. It does not freeze later writers.
* requestCommit/serverCommit: unguarded primitive publication of staged deltas;
  complete application ordering must be proved. A committing transaction can also
  receive a serverAbort decision; client transport failure alone proves neither.
* register: ACTUAL previous fixed protocol constructors/guards, linked by a Prop
  relation; not an assumed atomic serial registration operation. RegisterAction
  records actual commit identity even if writes happen to be idempotent.
* fixed=false changes ONLY old update missing-row ordering. Registration still
  uses fixed SELECT FOR UPDATE/upsert locks. Full old successful-row protocol and
  its inverse row-lock deadlocks are outside this paired missing-row control.
* Frame remains disjoint application data, not PostgreSQL catalogs or NOTIFY.
  Namespace-claim admission, retained-deployment existence, removal/housekeeping,
  broader ID-reuse/authority composition and physical SQL/Core refinement remain
  boundaries. No new runtime guard or denial is encoded by the ghost method map.
-/
def sourceMapping : Unit := ()

end RegistrationUpdate
