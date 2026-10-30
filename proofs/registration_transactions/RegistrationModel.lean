import Mathlib.Data.List.Basic
import Mathlib.Tactic

/-!
Design model of `registration/store.rs`, not extraction of async Rust/SQL.

Scope: one retained deployment, admitted fixed namespace/key, register/replace
writers only. `Metadata` is the FULL equality payload compared by ordinary
registration (ClientMetadata plus its admitted storage binding), not selected
version/endpoint fields. `Row` and `Role` retain the remaining complete candidate
row/login payloads; native typed serialization/projection is a separate boundary.
All key types are parametric; no global UUID freshness is postulated.

`Frame` contains only disjoint application state, not PostgreSQL catalogs or
NOTIFY effects. Fixed namespace/binding/creation fields are outside this mutable
projection. Core ensure_server/login internals remain a correspondence boundary.
These are not a model of RFC7592 updates, removal, housekeeping, key changes, preparation
in another environment database, or service postcommit audit/reload.
-/
namespace RegistrationTransaction

structure Registration (Client Login Metadata Row : Type) where
  client : Client
  login : Login
  metadata : Metadata
  payload : Row

inductive Kind where
  | ordinary
  | instanceReplacement
  deriving DecidableEq, Repr

structure Request (Client Login Metadata Row Role : Type) where
  candidate : Registration Client Login Metadata Row
  role : Role
  kind : Kind

@[ext] structure Database (Client Login Metadata Row Role Frame : Type) where
  metadata : Metadata
  registrations : Client → Option (Registration Client Login Metadata Row)
  logins : Login → Option Role
  frame : Frame

variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]

def put {α β : Type} [DecidableEq α] (rows : α → β) (key : α) (value : β) : α → β :=
  fun other => if other = key then value else rows other

def rowMap (rows : List (Registration C L M R)) : C → Option (Registration C L M R) :=
  rows.foldr (fun row rest => put rest row.client (some row)) (fun _ => none)

/-- Finite exact DELETE RETURNING result at THAT statement's snapshot. Order is
arbitrary; duplicate client keys are excluded by the physical registration PK. -/
def Listing (db : Database C L M R P F) (rows : List (Registration C L M R)) : Prop :=
  (rows.map (·.client)).Nodup ∧ db.registrations = rowMap rows

def WellFormed (db : Database C L M R P F) : Prop :=
  (∀ key row, db.registrations key = some row → row.client = key ∧
    (db.logins row.login).isSome = true) ∧
  (∀ first second left right,
    db.registrations first = some left → db.registrations second = some right →
    left.login = right.login → first = second)

inductive Write (Client Login Metadata Row Role : Type) where
  | deployment (metadata : Metadata)
  | deleteRows (clients : List Client)
  | dropLogin (login : Login)
  | createLogin (login : Login) (role : Role)
  | insert (row : Registration Client Login Metadata Row)

def applyWrite (db : Database C L M R P F) : Write C L M R P → Database C L M R P F
  | .deployment metadata => { db with metadata }
  | .deleteRows clients =>
      { db with registrations := fun key => if key ∈ clients then none else db.registrations key }
  | .dropLogin login => { db with logins := put db.logins login none }
  | .createLogin login role => { db with logins := put db.logins login (some role) }
  | .insert row => { db with registrations := put db.registrations row.client (some row) }

def applyWrites (db : Database C L M R P F) (writes : List (Write C L M R P)) :=
  writes.foldl applyWrite db

/-- PostgreSQL publishes the STAGED key deltas. In particular DELETE is not
re-executed against rows inserted after its statement snapshot. No application
completeness/metadata-equality/lock guard is built into this primitive. -/
def commit (db : Database C L M R P F) (staged : List (Write C L M R P)) :=
  applyWrites db staged

/-- Selected actual statement constraints. Additional SQL/DDL constraints may
abort; they do not create another successful program path. Valid examples must
establish these conditions from their concrete keys/rows, not a freshness axiom. -/
def ValidWrite (db : Database C L M R P F) : Write C L M R P → Prop
  | .createLogin login _ => db.logins login = none
  | .insert row => db.registrations row.client = none ∧
      (db.logins row.login).isSome = true ∧
      ∀ key stored, db.registrations key = some stored → stored.login ≠ row.login
  | _ => True

def admitWrites (request : Request C L M R P) : List (Write C L M R P) :=
  [.createLogin request.candidate.login request.role, .insert request.candidate]

def replaceWrites (request : Request C L M R P)
    (victims : List (Registration C L M R)) : List (Write C L M R P) :=
  [.deployment request.candidate.metadata, .deleteRows (victims.map (·.client))] ++
    (victims.map fun row => .dropLogin row.login) ++ admitWrites request

inductive Phase where
  | idle | waiting | locked | read | upserting | deleting | dropping | creating
  | inserting | ready | committing | committed | abortPending | aborted
  deriving DecidableEq, Repr

inductive Response where
  | pending | success | error
  deriving DecidableEq, Repr

structure Transaction (Client Login Metadata Row Role : Type) where
  request : Request Client Login Metadata Row Role
  phase : Phase := .idle
  observed : Option Metadata := none
  replacing : Bool := false
  captured : List (Registration Client Login Metadata Row) := []
  drops : List Login := []
  staged : List (Write Client Login Metadata Row Role) := []
  response : Response := .pending

structure State (Client Login Metadata Row Role Frame : Type) where
  db : Database Client Login Metadata Row Role Frame
  lock : Option Nat
  transactions : Nat → Transaction Client Login Metadata Row Role
  /-- Ghost events record actual serverCommit transitions, not native machinery. -/
  commits : List (Nat × List (Write Client Login Metadata Row Role)) := []

def initial (db : Database C L M R P F) (requests : Nat → Request C L M R P) : State C L M R P F :=
  { db, lock := none, transactions := fun id => { request := requests id } }

def update (state : State C L M R P F) (id : Nat) (tx : Transaction C L M R P) :=
  { state with transactions := put state.transactions id tx }

def release (lock : Option Nat) (id : Nat) := if lock = some id then none else lock

def committedState (state : State C L M R P F) (id : Nat) : State C L M R P F :=
  { update state id { state.transactions id with phase := .committed } with
    db := commit state.db (state.transactions id).staged
    lock := release state.lock id
    commits := state.commits ++ [(id, (state.transactions id).staged)] }

/-- `locking=true` is fixed source. The original ordinary SELECT corresponds to
`locking=false`'s unlockedRead; it DOES NOT bypass the other real program steps.
Instance upsert always acquires the row lock. Upsert also acquires it for an
original ordinary replacement that read without locking. Other later statement
constructors have no artificial ownership guard: fixed-source ownership is a theorem to derive.
Each acquire/upsert pair abstracts one PostgreSQL lock-taking statement; ordinary
acquire/read abstracts SELECT FOR UPDATE returning the locked metadata.
-/
inductive Step (locking : Bool) : State C L M R P F → State C L M R P F → Prop where
  | begin (state) (id) (phase : (state.transactions id).phase = .idle) :
      Step locking state (update state id { state.transactions id with phase := .waiting })
  | acquire (state) (id) (phase : (state.transactions id).phase = .waiting)
      (free : state.lock = none)
      (required : locking = true ∨ (state.transactions id).request.kind = .instanceReplacement) :
      Step locking state
        { update state id { state.transactions id with phase := .locked } with lock := some id }
  | read (state) (id) (phase : (state.transactions id).phase = .locked)
      (ordinary : (state.transactions id).request.kind = .ordinary) :
      Step locking state (update state id
        { state.transactions id with phase := .read, observed := some state.db.metadata })
  | unlockedRead (state) (id) (original : locking = false)
      (phase : (state.transactions id).phase = .waiting)
      (ordinary : (state.transactions id).request.kind = .ordinary) :
      Step locking state (update state id
        { state.transactions id with phase := .read, observed := some state.db.metadata })
  | replica (state) (id) (phase : (state.transactions id).phase = .read)
      (same : (state.transactions id).observed = some (state.transactions id).request.candidate.metadata) :
      Step locking state (update state id { state.transactions id with phase := .creating })
  | replace (state) (id) (phase : (state.transactions id).phase = .read)
      (different : (state.transactions id).observed ≠ some (state.transactions id).request.candidate.metadata) :
      Step locking state (update state id
        { state.transactions id with phase := .upserting, replacing := true })
  | instanceStart (state) (id) (phase : (state.transactions id).phase = .locked)
      (kind : (state.transactions id).request.kind = .instanceReplacement) :
      Step locking state (update state id
        { state.transactions id with phase := .upserting, replacing := true })
  | upsert (state) (id) (phase : (state.transactions id).phase = .upserting)
      (available : state.lock = none ∨ state.lock = some id) :
      Step locking state
        { (update state id { state.transactions id with
            phase := .deleting
            staged := (state.transactions id).staged ++
              [.deployment (state.transactions id).request.candidate.metadata] }) with
          lock := some id }
  | delete (state) (id) (victims) (phase : (state.transactions id).phase = .deleting)
      (snapshot : Listing (applyWrites state.db (state.transactions id).staged) victims) :
      Step locking state (update state id
        { state.transactions id with
          phase := .dropping
          captured := victims
          drops := victims.map (·.login)
          staged := (state.transactions id).staged ++ [.deleteRows (victims.map (·.client))] })
  | drop (state) (id) (login) (remaining)
      (phase : (state.transactions id).phase = .dropping)
      (next : (state.transactions id).drops = login :: remaining) :
      Step locking state (update state id
        { state.transactions id with
          drops := remaining
          staged := (state.transactions id).staged ++ [.dropLogin login] })
  | dropped (state) (id) (phase : (state.transactions id).phase = .dropping)
      (done : (state.transactions id).drops = []) :
      Step locking state (update state id { state.transactions id with phase := .creating })
  | create (state) (id) (phase : (state.transactions id).phase = .creating)
      (valid : ValidWrite (applyWrites state.db (state.transactions id).staged)
        (.createLogin (state.transactions id).request.candidate.login (state.transactions id).request.role)) :
      Step locking state (update state id
        { state.transactions id with
          phase := .inserting
          staged := (state.transactions id).staged ++
            [.createLogin (state.transactions id).request.candidate.login (state.transactions id).request.role] })
  | insert (state) (id) (phase : (state.transactions id).phase = .inserting)
      (valid : ValidWrite (applyWrites state.db (state.transactions id).staged)
        (.insert (state.transactions id).request.candidate)) :
      Step locking state (update state id
        { state.transactions id with
          phase := .ready
          staged := (state.transactions id).staged ++ [.insert (state.transactions id).request.candidate] })
  | requestCommit (state) (id) (phase : (state.transactions id).phase = .ready) :
      Step locking state (update state id { state.transactions id with phase := .committing })
  | serverCommit (state) (id) (phase : (state.transactions id).phase = .committing) :
      Step locking state (committedState state id)
  | acknowledge (state) (id) (phase : (state.transactions id).phase = .committed)
      (pending : (state.transactions id).response = .pending) :
      Step locking state (update state id { state.transactions id with response := .success })
  | fail (state) (id)
      (beforeCommit : (state.transactions id).phase ≠ .committing)
      (notCommitted : (state.transactions id).phase ≠ .committed)
      (notAborted : (state.transactions id).phase ≠ .aborted) :
      Step locking state (update state id
        { state.transactions id with phase := .abortPending, response := .error })
  /- A database abort decision may also resolve a pending commit request. This
  is distinct from assuming a client's transport error implies rollback. -/
  | serverAbort (state) (id)
      (phase : (state.transactions id).phase = .abortPending ∨
        (state.transactions id).phase = .committing) :
      Step locking state
        { update state id { state.transactions id with phase := .aborted } with
          lock := release state.lock id }
  | frame (state) (value : F) : Step locking state { state with db := { state.db with frame := value } }

inductive Execution (locking : Bool) : State C L M R P F → State C L M R P F → Prop where
  | refl (state) : Execution locking state state
  | tail {first middle last} : Execution locking first middle → Step locking middle last →
      Execution locking first last

end RegistrationTransaction
