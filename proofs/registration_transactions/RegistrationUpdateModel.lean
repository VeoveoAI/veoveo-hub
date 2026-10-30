import RegistrationHistory

/-!
Source-mapped extension for admitted CHANGED-metadata store.update calls and the
existing fixed register protocol (not the former unlocked registration source).
No asynchronous Rust/SQL extraction or new runtime seam is claimed.

ClientMetadata, signatures, digest values, timestamps, role payloads and remaining
row fields are parametric. Deployment metadata is specialized to (ClientMetadata,
StorageBinding). A registration's second comparison component is carried proof
admission context, NOT a native storage_binding column. Deployment binding is the
native field preserved by update. Native stable row fields include server,
deployment and orphaned_at; joined instance branch context is outside the row.

The method table is a static ghost classification of each physical transaction's
Rust method, not an authorization check. Unused request/map entries are ghost
data. Both kinds use core.lock, so no second independent deployment lock exists.
Preflight authentication/admission and broader ID-reuse/authority composition
remain explicit source-correspondence boundaries.
-/
namespace RegistrationUpdate
open RegistrationTransaction

structure Payload (Statement Digest Timestamp Stable : Type) where
  statement : Statement
  tokenDigest : Digest
  issuedAt : Timestamp
  updatedAt : Timestamp
  stable : Stable

abbrev DB (C L M B S D T R P F : Type) :=
  RegistrationTransaction.Database C L (M × B) (Payload S D T R) P F
abbrev Row (C L M B S D T R : Type) :=
  RegistrationTransaction.Registration C L (M × B) (Payload S D T R)
abbrev RegisterState (C L M B S D T R P F : Type) :=
  RegistrationTransaction.State C L (M × B) (Payload S D T R) P F

structure Request (Client Metadata Statement Digest Timestamp : Type) where
  client : Client
  metadata : Metadata
  statement : Statement
  presentedDigest : Digest
  now : Timestamp

variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- Source assignments in update_registration.sql. Immutable fields come from
the CURRENT matched row, not the previously authenticated snapshot. -/
def patched (row : Row C L M B S D T R) (request : Request C M S D T) : Row C L M B S D T R :=
  { row with
    metadata := (request.metadata, row.metadata.2)
    payload := { row.payload with statement := request.statement, updatedAt := request.now } }

inductive Write (Client Login Metadata Binding Statement Digest Timestamp Stable : Type) where
  | deploymentMetadata (metadata : Metadata)
  /- A logical keyed delta after a matched UPDATE. Full rows are proof-level
  witnesses; native execute returns affected count, not a copied row payload. -/
  | matchedRow (client : Client) (row : Row Client Login Metadata Binding Statement Digest Timestamp Stable)

def applyWrite (db : DB C L M B S D T R P F) : Write C L M B S D T R → DB C L M B S D T R P F
  | .deploymentMetadata metadata => { db with metadata := (metadata, db.metadata.2) }
  | .matchedRow client row => { db with registrations := put db.registrations client (some row) }

def commit (db : DB C L M B S D T R P F) (staged : List (Write C L M B S D T R)) :=
  staged.foldl applyWrite db

inductive Method where
  | register
  | update
  deriving DecidableEq, Repr

inductive Phase where
  | idle | preflight | begun | metadataWritten | registrationWritten
  | ready | committing | committed | rollbackPending | aborted
  deriving DecidableEq, Repr

inductive Outcome where
  | pending | updated | noop | failed
  deriving DecidableEq, Repr

structure Transaction (Client Login Metadata Binding Statement Digest Timestamp Stable : Type) where
  request : Request Client Metadata Statement Digest Timestamp
  phase : Phase := .idle
  authenticated : Option (Row Client Login Metadata Binding Statement Digest Timestamp Stable) := none
  matched : Option (Row Client Login Metadata Binding Statement Digest Timestamp Stable) := none
  staged : List (Write Client Login Metadata Binding Statement Digest Timestamp Stable) := []
  outcome : Outcome := .pending
  response : RegistrationTransaction.Response := .pending

inductive Event (Client Login Metadata Binding Statement Digest Timestamp Stable Role : Type) where
  | register (owner : Nat)
      (writes : List (RegistrationTransaction.Write Client Login (Metadata × Binding)
        (Payload Statement Digest Timestamp Stable) Role))
  | update (owner : Nat) (writes : List (Write Client Login Metadata Binding Statement Digest Timestamp Stable))

structure State (C L M B S D T R P F : Type) where
  core : RegisterState C L M B S D T R P F
  updates : Nat → Transaction C L M B S D T R
  events : List (Event C L M B S D T R P) := []

/-- Relational labels for the ACTUAL old constructors. This eliminates only
Prop into Prop; no function computes an actor/event by inspecting a Step proof.
The index retains the original constructor, all native guards and exact targets. -/
inductive RegisterAction : {before after : RegisterState C L M B S D T R P F} →
    RegistrationTransaction.Step true before after → Option Nat →
    List (Event C L M B S D T R P) → Prop where
  | begin (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .idle) :
      RegisterAction (RegistrationTransaction.Step.begin state id phase) (some id) []
  | acquire (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .waiting) (free : state.lock = none)
      (required : true = true ∨ (state.transactions id).request.kind = .instanceReplacement) :
      RegisterAction (RegistrationTransaction.Step.acquire state id phase free required) (some id) []
  | read (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .locked)
      (ordinary : (state.transactions id).request.kind = .ordinary) :
      RegisterAction (RegistrationTransaction.Step.read state id phase ordinary) (some id) []
  | replica (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .read)
      (same : (state.transactions id).observed = some (state.transactions id).request.candidate.metadata) :
      RegisterAction (RegistrationTransaction.Step.replica state id phase same) (some id) []
  | replace (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .read)
      (different : (state.transactions id).observed ≠ some (state.transactions id).request.candidate.metadata) :
      RegisterAction (RegistrationTransaction.Step.replace state id phase different) (some id) []
  | instanceStart (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .locked)
      (kind : (state.transactions id).request.kind = .instanceReplacement) :
      RegisterAction (RegistrationTransaction.Step.instanceStart state id phase kind) (some id) []
  | upsert (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .upserting)
      (available : state.lock = none ∨ state.lock = some id) :
      RegisterAction (RegistrationTransaction.Step.upsert state id phase available) (some id) []
  | delete (state : RegisterState C L M B S D T R P F) (id : Nat) (victims) (phase : (state.transactions id).phase = .deleting)
      (snapshot : Listing (applyWrites state.db (state.transactions id).staged) victims) :
      RegisterAction (RegistrationTransaction.Step.delete state id victims phase snapshot) (some id) []
  | drop (state : RegisterState C L M B S D T R P F) (id : Nat) (login) (remaining) (phase : (state.transactions id).phase = .dropping)
      (next : (state.transactions id).drops = login :: remaining) :
      RegisterAction (RegistrationTransaction.Step.drop state id login remaining phase next) (some id) []
  | dropped (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .dropping)
      (done : (state.transactions id).drops = []) :
      RegisterAction (RegistrationTransaction.Step.dropped state id phase done) (some id) []
  | create (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .creating)
      (valid : ValidWrite (applyWrites state.db (state.transactions id).staged)
        (.createLogin (state.transactions id).request.candidate.login (state.transactions id).request.role)) :
      RegisterAction (RegistrationTransaction.Step.create state id phase valid) (some id) []
  | insert (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .inserting)
      (valid : ValidWrite (applyWrites state.db (state.transactions id).staged)
        (.insert (state.transactions id).request.candidate)) :
      RegisterAction (RegistrationTransaction.Step.insert state id phase valid) (some id) []
  | requestCommit (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .ready) :
      RegisterAction (RegistrationTransaction.Step.requestCommit state id phase) (some id) []
  | serverCommit (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .committing) :
      RegisterAction (RegistrationTransaction.Step.serverCommit state id phase) (some id) [.register id (state.transactions id).staged]
  | acknowledge (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .committed)
      (pending : (state.transactions id).response = .pending) :
      RegisterAction (RegistrationTransaction.Step.acknowledge state id phase pending) (some id) []
  | fail (state : RegisterState C L M B S D T R P F) (id : Nat) (beforeCommit : (state.transactions id).phase ≠ .committing)
      (notCommitted : (state.transactions id).phase ≠ .committed)
      (notAborted : (state.transactions id).phase ≠ .aborted) :
      RegisterAction (RegistrationTransaction.Step.fail state id beforeCommit notCommitted notAborted) (some id) []
  | serverAbort (state : RegisterState C L M B S D T R P F) (id : Nat) (phase : (state.transactions id).phase = .abortPending ∨
        (state.transactions id).phase = .committing) :
      RegisterAction (RegistrationTransaction.Step.serverAbort state id phase) (some id) []
  | frame (state : RegisterState C L M B S D T R P F) (value : F) :
      RegisterAction (RegistrationTransaction.Step.frame state value) none []

def updateTx (state : State C L M B S D T R P F) (id : Nat) (tx : Transaction C L M B S D T R) :=
  { state with updates := put state.updates id tx }

def initial (db : DB C L M B S D T R P F)
    (registrations : Nat → RegistrationTransaction.Request C L (M × B) (Payload S D T R) P)
    (updates : Nat → Request C M S D T) : State C L M B S D T R P F :=
  { core := RegistrationTransaction.initial db registrations
    updates := fun id => { request := updates id } }

def committedState (state : State C L M B S D T R P F) (id : Nat) : State C L M B S D T R P F :=
  { (updateTx state id { state.updates id with phase := .committed }) with
    core := { state.core with
      db := commit state.core.db (state.updates id).staged
      lock := release state.core.lock id }
    events := state.events ++ [.update id (state.updates id).staged] }

/-- Constraints enforced by the native deployment metadata CHECKs remain an
explicit statement-success relation. No lock or desired post-state is assumed by
this relation. Admission has already established the fixed deployment/namespace. -/
abbrev MetadataAccepted (M B : Type) := M → B → Prop

/-- fixed=true is the corrected deployment-first update. fixed=false models
only the earlier UPDATE order, against the SAME fixed register protocol. Its
missing-row branch is sufficient for the paired stale-update counterexample. -/
inductive Step (fixed : Bool) (methods : Nat → Method) (accepted : MetadataAccepted M B) :
    State C L M B S D T R P F → State C L M B S D T R P F → Prop where
  | register (state) (nextCore) (step : RegistrationTransaction.Step true state.core nextCore)
      (actor : Option Nat) (contribution : List (Event C L M B S D T R P))
      (label : RegisterAction step actor contribution)
      (method : ∀ id, actor = some id → methods id = .register) :
      Step fixed methods accepted state
        { state with core := nextCore, events := state.events ++ contribution }
  /- An actual preflight row/digest read, before the SQL transaction. Signature,
  authority, deployment/slug admission and service reply/audit are not extracted. -/
  | preflight (state) (id) (row)
      (method : methods id = .update)
      (phase : (state.updates id).phase = .idle)
      (found : state.core.db.registrations (state.updates id).request.client = some row)
      (digest : row.payload.tokenDigest = (state.updates id).request.presentedDigest)
      (changed : row.metadata.1 ≠ (state.updates id).request.metadata) :
      Step fixed methods accepted state (updateTx state id
        { state.updates id with phase := .preflight, authenticated := some row })
  | beginUpdate (state) (id) (phase : (state.updates id).phase = .preflight) :
      Step fixed methods accepted state (updateTx state id { state.updates id with phase := .begun })
  | metadata (state) (id)
      (phase : (fixed = true ∧ (state.updates id).phase = .begun) ∨
        (fixed = false ∧ (state.updates id).phase = .registrationWritten))
      (free : state.core.lock = none)
      (valid : accepted (state.updates id).request.metadata state.core.db.metadata.2) :
      Step fixed methods accepted state
        { (updateTx state id { state.updates id with
            phase := if fixed then .metadataWritten else .ready
            staged := (state.updates id).staged ++ [.deploymentMetadata (state.updates id).request.metadata] }) with
          core := { state.core with lock := some id } }
  | matchRow (state) (id) (row)
      (phase : fixed = true ∧ (state.updates id).phase = .metadataWritten)
      (found : state.core.db.registrations (state.updates id).request.client = some row) :
      Step fixed methods accepted state (updateTx state id
        { state.updates id with
          phase := .ready
          matched := some row
          outcome := .updated
          staged := (state.updates id).staged ++
            [.matchedRow (state.updates id).request.client (patched row (state.updates id).request)] })
  | missRow (state) (id)
      (phase : (fixed = true ∧ (state.updates id).phase = .metadataWritten) ∨
        (fixed = false ∧ (state.updates id).phase = .begun))
      (absent : state.core.db.registrations (state.updates id).request.client = none) :
      Step fixed methods accepted state (updateTx state id
        { state.updates id with
          phase := if fixed then .rollbackPending else .registrationWritten
          outcome := .noop })
  | requestCommit (state) (id) (phase : (state.updates id).phase = .ready) :
      Step fixed methods accepted state (updateTx state id { state.updates id with phase := .committing })
  | serverCommit (state) (id) (phase : (state.updates id).phase = .committing) :
      Step fixed methods accepted state (committedState state id)
  | rollbackNoop (state) (id) (phase : (state.updates id).phase = .rollbackPending)
      (outcome : (state.updates id).outcome = .noop) :
      Step fixed methods accepted state
        { (updateTx state id { state.updates id with phase := .aborted }) with
          core := { state.core with lock := release state.core.lock id } }
  | acknowledgeUpdate (state) (id)
      (phase : (state.updates id).phase = .committed)
      (pending : (state.updates id).response = .pending) :
      Step fixed methods accepted state (updateTx state id { state.updates id with response := .success })
  | acknowledgeNoop (state) (id) (phase : (state.updates id).phase = .aborted)
      (outcome : (state.updates id).outcome = .noop)
      (pending : (state.updates id).response = .pending) :
      Step fixed methods accepted state (updateTx state id { state.updates id with response := .success })
  | fail (state) (id) (method : methods id = .update)
      (notCommitted : (state.updates id).phase ≠ .committed)
      (notCommitting : (state.updates id).phase ≠ .committing)
      (notAborted : (state.updates id).phase ≠ .aborted) :
      Step fixed methods accepted state (updateTx state id
        { state.updates id with phase := .rollbackPending, outcome := .failed, response := .error })
  | serverAbort (state) (id)
      (phase : (state.updates id).phase = .committing ∨ (state.updates id).phase = .rollbackPending) :
      Step fixed methods accepted state
        { (updateTx state id { state.updates id with phase := .aborted }) with
          core := { state.core with lock := release state.core.lock id } }

inductive Execution (fixed : Bool) (methods : Nat → Method) (accepted : MetadataAccepted M B) :
    State C L M B S D T R P F → State C L M B S D T R P F → Prop where
  | refl (state) : Execution fixed methods accepted state state
  | tail {first middle last} : Execution fixed methods accepted first middle →
      Step fixed methods accepted middle last → Execution fixed methods accepted first last

end RegistrationUpdate
