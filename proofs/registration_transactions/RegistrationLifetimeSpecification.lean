import RegistrationLifetimeModel

/-! Independent instance-only sequential contracts. Complete stored values are
specified extensionally, not by replaying a staged statement list. Response TAGS
are modeled; complete native API payload/credential projections remain pending. -/
namespace RegistrationLifetime
noncomputable section
variable {C L M R P I D F : Type}
variable [DecidableEq C] [DecidableEq L]
local instance (p : Prop) : Decidable p := Classical.propDecidable p

def registeredLogin (db : Database C L M R P I D F) (login : L) : Prop :=
  ∃ key row, db.registrations key = some row ∧ row.login = login

/-- Only the already existing immutable deployment payload is retained; the
actual proposal supplies first-insertion payload when deployment is absent. -/
def replacement (db : Database C L M R P I D F) (proposal : Proposal C L M R P D) :=
  { db with
    deployment := some ⟨proposal.candidate.metadata,
      match db.deployment with | none => proposal.firstDeployment | some old => old.stable⟩
    registrations := RegistrationTransaction.put (fun _ => none) proposal.candidate.client (some proposal.candidate)
    logins := fun login => if login = proposal.candidate.login then some proposal.role
      else if registeredLogin db login then none else db.logins login }

def removal (db : Database C L M R P I D F) : Database C L M R P I D F :=
  { db with
    instanceRow := none
    registrations := fun _ => none
    logins := fun login => if registeredLogin db login then none else db.logins login }

/-- This depends on the CURRENT database, never cached authority. Actual source
proofs must establish that the selected operation equals this independent spec. -/
def sequential (db : Database C L M R P I D F) (request : Request C L M R P D) :=
  match db.instanceRow, request with
  | none, _ => db
  | some _, .register proposal => replacement db proposal
  | some _, .remove => removal db

def expectedResponse (db : Database C L M R P I D F) (request : Request C L M R P D) : Response :=
  match db.instanceRow, request with
  | none, _ => absentResponse request
  | some _, _ => successResponse request

/-- Native constraints and admitted initial no-orphan state are distinct: only
native SQL constraints belong in ValidWrite, never the lifetime invariant. -/
def SQLValid (db : Database C L M R P I D F) : Prop :=
  (∀ key row, db.registrations key = some row → row.client = key ∧
    (db.logins row.login).isSome = true ∧ db.deployment.isSome = true) ∧
  (∀ first second left right, db.registrations first = some left →
    db.registrations second = some right → left.login = right.login → first = second)

def NoOrphanRows (db : Database C L M R P I D F) : Prop :=
  db.instanceRow = none → ∀ key, db.registrations key = none

def InitialValid (db : Database C L M R P I D F) : Prop := SQLValid db ∧ NoOrphanRows db

def registerPrelude (proposal : Proposal C L M R P D) : List (Write C L M R P D) :=
  [.upsertDeployment proposal.candidate.metadata proposal.firstDeployment]

def prelude : Request C L M R P D → List (Write C L M R P D)
  | .register proposal => registerPrelude proposal
  | .remove => []

def deletedRows (rows : List (RegistrationTransaction.Registration C L M R)) : List (Write C L M R P D) :=
  [.registration (.deleteRows (rows.map (·.client)))]

def droppedLogins (logins : List L) : List (Write C L M R P D) :=
  logins.map (fun login => .registration (.dropLogin login))

def deletionProgram (rows : List (RegistrationTransaction.Registration C L M R)) : List (Write C L M R P D) :=
  deletedRows rows ++ droppedLogins (rows.map (·.login))

def completeProgram (request : Request C L M R P D)
    (rows : List (RegistrationTransaction.Registration C L M R)) : List (Write C L M R P D) :=
  match request with
  | .register proposal => registerPrelude proposal ++ deletionProgram rows ++
      [.registration (.createLogin proposal.candidate.login proposal.role),
        .registration (.insert proposal.candidate)]
  | .remove => deletionProgram rows ++ [.deleteInstance]

def Complete (tx : Transaction C L M R P I D) : Prop :=
  tx.staged = completeProgram tx.request tx.captured

/-- Actual commit-event sequence, independent request operation; aborts and missing
returns have no commit contribution. Frame-only environmental changes are explicit
steps with no commit event; full final DB equality is retained. Client response
timing is a separate channel. -/
inductive SerialHistory (requests : Nat → Request C L M R P D) :
    Database C L M R P I D F → List (Nat × List (Write C L M R P D)) →
    Database C L M R P I D F → Prop
  | nil (db) : SerialHistory requests db [] db
  | frame {first middle} {events} (previous : SerialHistory requests first events middle)
      (value : F) :
      SerialHistory requests first events { middle with frame := value }
  | snoc {first middle} {events} (previous : SerialHistory requests first events middle)
      (id : Nat) (actual : List (Write C L M R P D)) :
      SerialHistory requests first (events ++ [(id, actual)]) (sequential middle (requests id))

end
end RegistrationLifetime
