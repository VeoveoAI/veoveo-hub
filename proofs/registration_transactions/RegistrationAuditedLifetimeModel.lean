import RegistrationLifetimeModel

/-! Manual audited STORE-only extension. The complete data protocol is reused;
audit append acknowledgement, SQL server decision and reply are separate events.
See AUDITED_STORE_SOURCE.md for the native contracts and intentionally open scope. -/
namespace RegistrationAuditedLifetime
open RegistrationTransaction (put)

inductive Activity
  | claimNamespace | register | update | deregister | expire | replace
  deriving DecidableEq, Repr

structure Intent (Server : Type) where
  activity : Activity
  server : Server
  deriving DecidableEq

/-- `Draft` retains the entire native JSON draft, including its original time.
The column timestamp is separate; Core compares BOTH on duplicate identity. -/
structure Record (Draft StoredTime : Type) where
  draft : Draft
  occurredAt : StoredTime

structure Encoding (AuditId Server Draft StoredTime : Type) where
  id : Draft → AuditId
  intent : Draft → Intent Server
  storedTime : Draft → StoredTime

abbrev Ledger (AuditId Draft StoredTime : Type) := AuditId → Option (Record Draft StoredTime)

variable {C L M R P I D F A S J T : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq A]

def recordIntent (encoding : Encoding A S J T) (record : Record J T) :=
  encoding.intent record.draft

/-- One native append. This does not start or commit a transaction. Exact retries
are accepted, conflicting IDs are not; no global ID-freshness axiom is present. -/
def CoreAppend (encoding : Encoding A S J T) (before : Ledger A J T)
    (record : Record J T) (after : Ledger A J T) : Prop :=
  record.occurredAt = encoding.storedTime record.draft ∧
    (before (encoding.id record.draft) = none ∨
      before (encoding.id record.draft) = some record) ∧
    after = put before (encoding.id record.draft) (some record)

def Conflict (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (record : Record J T) : Prop :=
  ∃ previous, ledger (encoding.id record.draft) = some previous ∧ previous ≠ record

def overlay (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (records : List (Record J T)) : Ledger A J T :=
  records.foldl (fun current record =>
    put current (encoding.id record.draft) (some record)) ledger

/-- Source projections from complete candidate metadata and captured ended rows.
They remain explicit serialization/query correspondence boundaries. -/
structure Source (Metadata Row Server : Type) where
  candidateServer : Metadata → Server
  endedServer : Row → Server

def sourceProgram (source : Source M R S)
    (request : RegistrationLifetime.Request C L M R P D)
    (ended : List (RegistrationTransaction.Registration C L M R)) : List (Intent S) :=
  match request with
  | .register proposal =>
      ended.map (fun row => ⟨.replace, source.endedServer row.payload⟩) ++
        [⟨.register, source.candidateServer proposal.candidate.metadata⟩]
  | .remove => ended.map (fun row => ⟨.deregister, source.endedServer row.payload⟩)

structure Cursor (Server Draft StoredTime : Type) where
  /-- Source program captured at the actual ready transaction, not a desired batch. -/
  program : List (Intent Server)
  remaining : List (Intent Server)
  acknowledged : List (Record Draft StoredTime) := []

def cursor (program : List (Intent S)) : Cursor S J T := ⟨program, program, []⟩

def advance (current : Cursor S J T) (remaining : List (Intent S)) (record : Record J T) :
    Cursor S J T :=
  { current with remaining, acknowledged := current.acknowledged ++ [record] }

structure State (Client Login Metadata Row Role Instance Stable Frame AuditId Server Draft StoredTime : Type) where
  core : RegistrationLifetime.State Client Login Metadata Row Role Instance Stable Frame
  audit : Ledger AuditId Draft StoredTime
  cursors : Nat → Option (Cursor Server Draft StoredTime) := fun _ => none

def initial (db : RegistrationLifetime.Database C L M R P I D F)
    (requests : Nat → RegistrationLifetime.Request C L M R P D) (audit : Ledger A J T) :
    State C L M R P I D F A S J T :=
  { core := RegistrationLifetime.initial db requests, audit }

def setCursor (s : State C L M R P I D F A S J T) (id : Nat) (value : Cursor S J T) :=
  { s with cursors := put s.cursors id (some value) }

/-- Only the old commit-entry and commit-decision transitions are intercepted.
This is a control-flow exclusion, not a desired database/audit-output guard. -/
def Ordinary (before after : RegistrationLifetime.State C L M R P I D F) : Prop :=
  after.commits = before.commits ∧
    ∀ id, (before.transactions id).phase ≠ .committing →
      (after.transactions id).phase ≠ .committing

/-- Native unique-key locking prevents a committed foreign conflict after an
acknowledged private append. Foreign append steps below enforce the scoped
disjoint-ID case; identical committed records are also compatible. -/
def Compatible (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (records : List (Record J T)) : Prop :=
  ∀ record ∈ records, ledger (encoding.id record.draft) = none ∨
    ledger (encoding.id record.draft) = some record

/-- Retired ghost cursors do not reserve native record IDs forever. A statement
error still has private effects until PostgreSQL confirms its abort. -/
def ActivePrivate (phase : RegistrationLifetime.Phase) : Prop :=
  phase = .ready ∨ phase = .committing ∨ phase = .abortPending

inductive Step (source : Source M R S) (encoding : Encoding A S J T) :
    State C L M R P I D F A S J T → State C L M R P I D F A S J T → Prop
  | ordinary (s) (next)
      (base : RegistrationLifetime.Step true s.core next)
      (notCommit : Ordinary s.core next) :
      Step source encoding s { s with core := next }
  | beginAudit (s) (id)
      (ready : (s.core.transactions id).phase = .ready)
      (unused : s.cursors id = none) :
      Step source encoding s (setCursor s id
        (cursor (sourceProgram source (s.core.transactions id).request
          (s.core.transactions id).captured)))
  | append (s) (id) (current) (wanted) (remaining) (record) (visibleAfter)
      (ready : (s.core.transactions id).phase = .ready)
      (selected : s.cursors id = some current)
      (head : current.remaining = wanted :: remaining)
      (constructed : recordIntent encoding record = wanted)
      (accepted : CoreAppend encoding (overlay encoding s.audit current.acknowledged)
        record visibleAfter) :
      Step source encoding s (setCursor s id (advance current remaining record))
  | requestCommit (s) (id) (current)
      (ready : (s.core.transactions id).phase = .ready)
      (selected : s.cursors id = some current)
      (exhausted : current.remaining = []) :
      Step source encoding s
        { s with
          core := (RegistrationLifetime.update s.core id
            { s.core.transactions id with phase := .committing }) }
  | serverCommit (s) (id) (current)
      (committing : (s.core.transactions id).phase = .committing)
      (selected : s.cursors id = some current)
      (exhausted : current.remaining = [])
      (compatible : Compatible encoding s.audit current.acknowledged) :
      Step source encoding s
        { s with
          core := RegistrationLifetime.committedState s.core id
          audit := overlay encoding s.audit current.acknowledged }
  | foreignAppend (s) (record) (next)
      (accepted : CoreAppend encoding s.audit record next)
      (disjoint : ∀ id current, s.cursors id = some current →
        ActivePrivate (s.core.transactions id).phase →
        ∀ privateRecord ∈ current.acknowledged,
          encoding.id privateRecord.draft ≠ encoding.id record.draft) :
      Step source encoding s { s with audit := next }

inductive Execution (source : Source M R S) (encoding : Encoding A S J T) :
    State C L M R P I D F A S J T → State C L M R P I D F A S J T → Prop
  | refl (s) : Execution source encoding s s
  | tail {first middle last} : Execution source encoding first middle →
      Step source encoding middle last → Execution source encoding first last

/-- Progress is a theorem target, deliberately absent from commit rules. -/
def Progress (encoding : Encoding A S J T) (s : State C L M R P I D F A S J T) : Prop :=
  ∀ id current, s.cursors id = some current →
    current.acknowledged.map (recordIntent encoding) ++ current.remaining = current.program

end RegistrationAuditedLifetime
