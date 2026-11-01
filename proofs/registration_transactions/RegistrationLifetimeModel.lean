import RegistrationModel

/-! Manual instance-only lifetime protocol, not Rust/SQL extraction. Full instance,
metadata/binding, immutable deployment remainder, row and role values are parametric.
No namespace/update/reissue/housekeeping/create-instance writers are hidden here.
A missing deployment is represented by none. Server/group/bootstrap memberships,
external databases and NOTIFY/transport are outside the registration-owned maps.
-/
namespace RegistrationLifetime
open RegistrationTransaction (put rowMap)

structure Deployment (Metadata Stable : Type) where
  metadata : Metadata
  stable : Stable

structure Proposal (Client Login Metadata Row Role Stable : Type) where
  candidate : RegistrationTransaction.Registration Client Login Metadata Row
  role : Role
  firstDeployment : Stable
  externalEnvironment : Bool

inductive Request (Client Login Metadata Row Role Stable : Type)
  | register (proposal : Proposal Client Login Metadata Row Role Stable)
  | remove

@[ext] structure Database (Client Login Metadata Row Role Instance Stable Frame : Type) where
  instanceRow : Option Instance
  deployment : Option (Deployment Metadata Stable)
  registrations : Client → Option (RegistrationTransaction.Registration Client Login Metadata Row)
  logins : Login → Option Role
  frame : Frame

variable {C L M R P I D F : Type}
variable [DecidableEq C] [DecidableEq L]

/-- Only first insertion uses the supplied stable payload; conflict update retains
existing immutable fields (including native created_at/deployment identity). -/
def upsert (old : Option (Deployment M D)) (metadata : M) (stable : D) : Option (Deployment M D) :=
  some ⟨metadata, (old.map (·.stable)).getD stable⟩

inductive Write (Client Login Metadata Row Role Stable : Type)
  | registration (write : RegistrationTransaction.Write Client Login Metadata Row Role)
  | upsertDeployment (metadata : Metadata) (stable : Stable)
  | deleteInstance

/-- Primitive deltas. There is deliberately no instance-presence guard on an
INSERT or on commit: the actual FK is to the retained deployment, not instance. -/
def applyWrite (db : Database C L M R P I D F) : Write C L M R P D → Database C L M R P I D F
  | .registration (.deployment metadata) =>
      { db with deployment := db.deployment.map (fun old => { old with metadata }) }
  | .registration (.deleteRows clients) =>
      { db with registrations := fun key => if key ∈ clients then none else db.registrations key }
  | .registration (.dropLogin login) => { db with logins := put db.logins login none }
  | .registration (.createLogin login role) => { db with logins := put db.logins login (some role) }
  | .registration (.insert row) => { db with registrations := put db.registrations row.client (some row) }
  | .upsertDeployment metadata stable => { db with deployment := upsert db.deployment metadata stable }
  | .deleteInstance => { db with instanceRow := none }

def applyWrites (db : Database C L M R P I D F) (writes : List (Write C L M R P D)) :=
  writes.foldl applyWrite db

def Listing (db : Database C L M R P I D F)
    (rows : List (RegistrationTransaction.Registration C L M R)) : Prop :=
  (rows.map (·.client)).Nodup ∧ db.registrations = rowMap rows

/-- Native SQL constraints, not a desired no-orphan premise. Other SQL/DDL errors
can abort. Instance absence does NOT invalidate INSERT in the original protocol. -/
def ValidWrite (db : Database C L M R P I D F) : Write C L M R P D → Prop
  | .registration (.createLogin login _) => db.logins login = none
  | .registration (.insert row) => db.deployment.isSome = true ∧
      db.registrations row.client = none ∧ (db.logins row.login).isSome = true ∧
      ∀ key stored, db.registrations key = some stored → stored.login ≠ row.login
  | _ => True

inductive Phase
  | idle | preflight | waiting | held | upserting | deleting | dropping
  | creating | inserting | erasing | ready | committing | committed
  | absent | abortPending | aborted
  deriving DecidableEq, Repr

inductive Response
  | pending | registered | removed | invalidToken | notFound | error
  deriving DecidableEq, Repr

structure Transaction (Client Login Metadata Row Role Instance Stable : Type) where
  request : Request Client Login Metadata Row Role Stable
  phase : Phase := .idle
  cachedAuthority : Option Instance := none
  lockedInstance : Option Instance := none
  captured : List (RegistrationTransaction.Registration Client Login Metadata Row) := []
  drops : List Login := []
  staged : List (Write Client Login Metadata Row Role Stable) := []
  response : Response := .pending
  error : Option String := none

structure State (Client Login Metadata Row Role Instance Stable Frame : Type) where
  db : Database Client Login Metadata Row Role Instance Stable Frame
  instanceOwner : Option Nat := none
  deploymentOwner : Option Nat := none
  transactions : Nat → Transaction Client Login Metadata Row Role Instance Stable
  /-- Ghost actual server decisions, independent of service/transport reply. -/
  commits : List (Nat × List (Write Client Login Metadata Row Role Stable)) := []

def initial (db : Database C L M R P I D F) (requests : Nat → Request C L M R P D) :
    State C L M R P I D F :=
  { db, transactions := fun id => { request := requests id } }

def update (s : State C L M R P I D F) (id : Nat) (tx : Transaction C L M R P I D) :=
  { s with transactions := put s.transactions id tx }

def release (owner : Option Nat) (id : Nat) := if owner = some id then none else owner

def committedState (s : State C L M R P I D F) (id : Nat) : State C L M R P I D F :=
  { update s id { s.transactions id with phase := .committed } with
    db := applyWrites s.db (s.transactions id).staged
    instanceOwner := release s.instanceOwner id
    deploymentOwner := release s.deploymentOwner id
    commits := s.commits ++ [(id, (s.transactions id).staged)] }

def absentResponse : Request C L M R P D → Response
  | .register _ => .invalidToken
  | .remove => .notFound

def successResponse : Request C L M R P D → Response
  | .register _ => .registered
  | .remove => .removed

def afterLock : Request C L M R P D → Phase
  | .register _ => .held
  | .remove => .deleting

/-- fixed=true adds the early shared SELECT. Original registration still obtains
its real deployment upsert lock, and original removal still obtains its actual
later DELETE-instance row lock. No primitive assumes the target safety property.
Full lockedInstance/captured rows are ghost witnesses; native queries return only
their actual selected/RETURNING projections, as documented in SOURCE.md. -/
inductive Step (fixed : Bool) : State C L M R P I D F → State C L M R P I D F → Prop
  | preflight (s) (id) (proposal) (inst)
      (phase : (s.transactions id).phase = .idle)
      (method : (s.transactions id).request = .register proposal)
      (authority : s.db.instanceRow = some inst) :
      Step fixed s (update s id { s.transactions id with
        phase := .preflight, cachedAuthority := some inst })
  | beginRegister (s) (id) (proposal)
      (phase : (s.transactions id).phase = .preflight)
      (method : (s.transactions id).request = .register proposal) :
      Step fixed s (update s id { s.transactions id with phase := .waiting })
  | beginRemove (s) (id) (phase : (s.transactions id).phase = .idle)
      (method : (s.transactions id).request = .remove) :
      Step fixed s (update s id { s.transactions id with phase := .waiting })
  | acquire (s) (id) (inst) (mode : fixed = true)
      (phase : (s.transactions id).phase = .waiting)
      (free : s.instanceOwner = none) (present : s.db.instanceRow = some inst) :
      Step fixed s { update s id { s.transactions id with
        phase := afterLock (s.transactions id).request, lockedInstance := some inst }
        with instanceOwner := some id }
  | missing (s) (id) (mode : fixed = true)
      (phase : (s.transactions id).phase = .waiting)
      (free : s.instanceOwner = none) (absent : s.db.instanceRow = none) :
      Step fixed s (update s id { s.transactions id with phase := .absent })
  | originalNoSelect (s) (id) (mode : fixed = false)
      (phase : (s.transactions id).phase = .waiting) :
      Step fixed s (update s id { s.transactions id with phase := afterLock (s.transactions id).request })
  /- Optional external ensure_server is a logical operation/result boundary; its
  non-atomic external role/schema effects are excluded from the owned DB frame. -/
  | prepared (s) (id) (proposal) (phase : (s.transactions id).phase = .held)
      (method : (s.transactions id).request = .register proposal) :
      Step fixed s (update s id { s.transactions id with phase := .upserting })
  | upsert (s) (id) (proposal) (phase : (s.transactions id).phase = .upserting)
      (method : (s.transactions id).request = .register proposal)
      (available : s.deploymentOwner = none ∨ s.deploymentOwner = some id) :
      Step fixed s { update s id { s.transactions id with
        phase := .deleting
        staged := (s.transactions id).staged ++
          [.upsertDeployment proposal.candidate.metadata proposal.firstDeployment] }
        with deploymentOwner := some id }
  | delete (s) (id) (victims) (phase : (s.transactions id).phase = .deleting)
      (snapshot : Listing (applyWrites s.db (s.transactions id).staged) victims) :
      Step fixed s (update s id { s.transactions id with
        phase := .dropping
        captured := victims
        drops := victims.map (·.login)
        staged := (s.transactions id).staged ++ [.registration (.deleteRows (victims.map (·.client)))] })
  | drop (s) (id) (login) (remaining) (phase : (s.transactions id).phase = .dropping)
      (next : (s.transactions id).drops = login :: remaining) :
      Step fixed s (update s id { s.transactions id with
        drops := remaining
        staged := (s.transactions id).staged ++ [.registration (.dropLogin login)] })
  | droppedRegister (s) (id) (proposal) (phase : (s.transactions id).phase = .dropping)
      (method : (s.transactions id).request = .register proposal) (done : (s.transactions id).drops = []) :
      Step fixed s (update s id { s.transactions id with phase := .creating })
  | droppedRemove (s) (id) (phase : (s.transactions id).phase = .dropping)
      (method : (s.transactions id).request = .remove) (done : (s.transactions id).drops = []) :
      Step fixed s (update s id { s.transactions id with phase := .erasing })
  | create (s) (id) (proposal) (phase : (s.transactions id).phase = .creating)
      (method : (s.transactions id).request = .register proposal)
      (valid : ValidWrite (applyWrites s.db (s.transactions id).staged)
        (.registration (.createLogin proposal.candidate.login proposal.role))) :
      Step fixed s (update s id { s.transactions id with
        phase := .inserting
        staged := (s.transactions id).staged ++ [.registration (.createLogin proposal.candidate.login proposal.role)] })
  | insert (s) (id) (proposal) (phase : (s.transactions id).phase = .inserting)
      (method : (s.transactions id).request = .register proposal)
      (valid : ValidWrite (applyWrites s.db (s.transactions id).staged) (.registration (.insert proposal.candidate))) :
      Step fixed s (update s id { s.transactions id with
        phase := .ready
        staged := (s.transactions id).staged ++ [.registration (.insert proposal.candidate)] })
  | erase (s) (id) (inst) (phase : (s.transactions id).phase = .erasing)
      (method : (s.transactions id).request = .remove)
      (available : s.instanceOwner = none ∨ s.instanceOwner = some id)
      (present : s.db.instanceRow = some inst) :
      Step fixed s { update s id { s.transactions id with
        phase := .ready
        lockedInstance := some inst
        staged := (s.transactions id).staged ++ [.deleteInstance] }
        with instanceOwner := some id }
  | eraseMissing (s) (id) (phase : (s.transactions id).phase = .erasing)
      (method : (s.transactions id).request = .remove)
      (available : s.instanceOwner = none ∨ s.instanceOwner = some id) (absent : s.db.instanceRow = none) :
      Step fixed s (update s id { s.transactions id with phase := .absent })
  /- SQLx drop/rollback for the early or late absent return. Partial private DELETE
  writes in the original late-missing path never become committed deltas. -/
  | returnAbsent (s) (id) (phase : (s.transactions id).phase = .absent) :
      Step fixed s { update s id { s.transactions id with
        phase := .aborted
        response := absentResponse (s.transactions id).request } with
        instanceOwner := release s.instanceOwner id, deploymentOwner := release s.deploymentOwner id }
  | requestCommit (s) (id) (phase : (s.transactions id).phase = .ready) :
      Step fixed s (update s id { s.transactions id with phase := .committing })
  | serverCommit (s) (id) (phase : (s.transactions id).phase = .committing) :
      Step fixed s (committedState s id)
  | acknowledge (s) (id) (phase : (s.transactions id).phase = .committed)
      (pending : (s.transactions id).response = .pending) :
      Step fixed s (update s id { s.transactions id with response := successResponse (s.transactions id).request })
  | fail (s) (id) (message : String)
      (beforeCommit : (s.transactions id).phase ≠ .committing)
      (notCommitted : (s.transactions id).phase ≠ .committed)
      (notAborted : (s.transactions id).phase ≠ .aborted)
      (pending : (s.transactions id).response = .pending) :
      Step fixed s (update s id { s.transactions id with
        phase := .abortPending, error := some message, response := .error })
  | serverAbort (s) (id)
      (phase : (s.transactions id).phase = .abortPending ∨ (s.transactions id).phase = .committing) :
      Step fixed s { update s id { s.transactions id with phase := .aborted } with
        instanceOwner := release s.instanceOwner id, deploymentOwner := release s.deploymentOwner id }
  | abortReply (s) (id) (message : String) (phase : (s.transactions id).phase = .aborted)
      (pending : (s.transactions id).response = .pending) :
      Step fixed s (update s id { s.transactions id with error := some message, response := .error })
  | deliveryError (s) (id) (message : String)
      (phase : (s.transactions id).phase = .committing ∨ (s.transactions id).phase = .committed)
      (pending : (s.transactions id).response = .pending) :
      Step fixed s (update s id { s.transactions id with error := some message, response := .error })
  | frame (s) (value : F) : Step fixed s { s with db := { s.db with frame := value } }

inductive Execution (fixed : Bool) : State C L M R P I D F → State C L M R P I D F → Prop
  | refl (s) : Execution fixed s s
  | tail {first middle last} : Execution fixed first middle → Step fixed middle last → Execution fixed first last

end RegistrationLifetime
