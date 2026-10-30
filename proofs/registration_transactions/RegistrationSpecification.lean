import RegistrationModel

/- Independent sequential contract and proposed proof statements. No theorem in
this file is asserted without proof; the Guarantee definitions name obligations
for the next proof modules. These are not SQL/Rust extraction claims. -/
namespace RegistrationTransaction
noncomputable section
variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]
local instance (p : Prop) : Decidable p := Classical.propDecidable p

def registeredLogin (db : Database C L M R P F) (login : L) : Prop :=
  ∃ client row, db.registrations client = some row ∧ row.login = login

/-- Independent ordinary replica specification: preserve every old registration,
login and deployment field; add exactly the supplied complete candidate/role. -/
def expectedReplica (db : Database C L M R P F) (request : Request C L M R P) :=
  { db with
    registrations := put db.registrations request.candidate.client (some request.candidate)
    logins := put db.logins request.candidate.login (some request.role) }

/-- Independent replacement specification: remove exactly the old deployment's
registrations and their logins, preserving unrelated login roles and frame. SQL
RETURNING order is irrelevant to this extensional map specification. -/
def expectedReplacement (db : Database C L M R P F) (request : Request C L M R P) :=
  { db with
    metadata := request.candidate.metadata
    registrations := put (fun _ => none) request.candidate.client (some request.candidate)
    logins := fun login =>
      if login = request.candidate.login then some request.role
      else if registeredLogin db login then none else db.logins login }

def sequential (db : Database C L M R P F) (request : Request C L M R P) :=
  if request.kind = .ordinary ∧ db.metadata = request.candidate.metadata
  then expectedReplica db request else expectedReplacement db request

/-- This sequential history is indexed by ACTUAL committed event IDs in the
state, not invented existential contributions. The independent sequential
operation does not evaluate the event's staged write list. -/
inductive SerialHistory (requests : Nat → Request C L M R P) :
    Database C L M R P F → List (Nat × List (Write C L M R P)) →
    Database C L M R P F → Prop where
  | nil (db) : SerialHistory requests db [] db
  | snoc {first middle} {events} (history : SerialHistory requests first events middle)
      (id : Nat) (actualWrites : List (Write C L M R P)) :
      SerialHistory requests first (events ++ [(id, actualWrites)]) (sequential middle (requests id))

def owned (db : Database C L M R P F) (initialFrame : F) := { db with frame := initialFrame }

def Complete (tx : Transaction C L M R P) : Prop :=
  tx.staged = if tx.replacing then replaceWrites tx.request tx.captured else admitWrites tx.request

def ProgramGuarantee : Prop :=
  ∀ (db : Database C L M R P F) requests state,
    Execution true (initial db requests) state →
    ∀ id, (state.transactions id).phase = .committing → Complete (state.transactions id)

def SerialGuarantee : Prop :=
  ∀ (db : Database C L M R P F) requests state,
    WellFormed db → Execution true (initial db requests) state →
    SerialHistory requests db state.commits (owned state.db db.frame)

/-- An admitted request can be replaced before its success acknowledgement.
SerialGuarantee is about committed effects, not HTTP liveness/current survival. -/
def AbortGuarantee : Prop :=
  ∀ (db : Database C L M R P F) requests state,
    Execution true (initial db requests) state →
    ∀ id, (state.transactions id).phase = .aborted →
      ∀ writes, (id, writes) ∉ state.commits

/-- Source map for proof review:
* acquire/read: ordinary deployment.sql FOR UPDATE; instance acquire/upsert is
  replace_deployment.sql's conflict update, with no invented instance comparison.
* replica/replace: store.rs compares FULL metadata plus admitted binding.
* delete: replace_registrations.sql captures finite exact rows at statement time.
* drop/create/insert: drop_login.sql + ensure_server/create_login.sql +
  create_registration.sql. ensure_server is the fixed-namespace Core boundary.
* requestCommit/serverCommit/abort: SQLx transaction commit/drop with no
  application-correctness oracle in the primitive commit function.
  A pending commit can resolve by serverCommit or serverAbort; a client error
  alone is not modeled as proof of a server rollback decision.
* frame: explicitly unrelated application data only.

To prove: ProgramGuarantee, SerialGuarantee, AbortGuarantee; reachable exact-match
replica preserving both candidates/logins; changed and unconditional instance
replacement exact maps; original-protocol read/delete/late-insert counterexample.
These goals remain pending until checked by Lean. -/
def sourceMapping : Unit := ()

end
end RegistrationTransaction
