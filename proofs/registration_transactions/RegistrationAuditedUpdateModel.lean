import RegistrationUpdateModel
import RegistrationAuditedLifetimeModel

/-! Manual STORE adapter. Existing mixed data steps are reused unchanged;
external registration audit contributions are explicit but not vocabulary-certified. -/
namespace RegistrationAuditedUpdate
open RegistrationTransaction (put)
open RegistrationAuditedLifetime
variable {C L M B St D T R P F A S J U : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq A]

/-- Native metadata.slug(), not a projection from stale authenticated metadata. -/
structure Source (Metadata Server : Type) where
  server : Metadata → Server

def sourceProgram (source : Source M S) (request : RegistrationUpdate.Request C M St D T) :
    List (Intent S) := [⟨.update, source.server request.metadata⟩]

/-- Finite actual append acknowledgements inside an external registration's
transaction. No desired activity, target or minimum list length is imposed. -/
inductive AppendBatch (encoding : Encoding A S J U) :
    Ledger A J U → List (Record J U) → Ledger A J U → Prop where
  | nil (ledger) : AppendBatch encoding ledger [] ledger
  | cons {before middle after record remaining} :
      CoreAppend encoding before record middle →
      AppendBatch encoding middle remaining after →
      AppendBatch encoding before (record :: remaining) after

structure State (C L M B St D T R P F A S J U : Type) where
  core : RegistrationUpdate.State C L M B St D T R P F
  audit : Ledger A J U
  cursors : Nat → Option (Cursor S J U) := fun _ => none

def initial (db : RegistrationUpdate.DB C L M B St D T R P F)
    (registrations : Nat → RegistrationTransaction.Request C L (M × B)
      (RegistrationUpdate.Payload St D T R) P)
    (updates : Nat → RegistrationUpdate.Request C M St D T) (audit : Ledger A J U) :
    State C L M B St D T R P F A S J U :=
  { core := RegistrationUpdate.initial db registrations updates, audit }

def setCursor (s : State C L M B St D T R P F A S J U) (id : Nat) (value : Cursor S J U) :=
  { s with cursors := put s.cursors id (some value) }

/-- Exclude real commit events and UPDATE commit entry from the generic path.
No desired stored result/audit program is a premise here. -/
def Ordinary (before after : RegistrationUpdate.State C L M B St D T R P F) : Prop :=
  after.events = before.events ∧
    ∀ id, (before.updates id).phase ≠ .committing → (after.updates id).phase ≠ .committing

def ActivePrivate (phase : RegistrationUpdate.Phase) : Prop :=
  phase = .ready ∨ phase = .committing ∨ phase = .rollbackPending

/-- Scoped native unique-key exclusion. Retired cursors are not reservations. -/
def DisjointPrivate (encoding : Encoding A S J U)
    (s : State C L M B St D T R P F A S J U) (records : List (Record J U)) : Prop :=
  ∀ id current, s.cursors id = some current → ActivePrivate (s.core.updates id).phase →
    ∀ privateRecord ∈ current.acknowledged, ∀ record ∈ records,
      encoding.id privateRecord.draft ≠ encoding.id record.draft

inductive Step (methods : Nat → RegistrationUpdate.Method)
    (accepted : RegistrationUpdate.MetadataAccepted M B)
    (source : Source M S) (encoding : Encoding A S J U) :
    State C L M B St D T R P F A S J U → State C L M B St D T R P F A S J U → Prop where
  | ordinary (s) (next)
      (base : RegistrationUpdate.Step true methods accepted s.core next)
      (notCommit : Ordinary s.core next) :
      Step methods accepted source encoding s { s with core := next }
  | beginAudit (s) (id) (ready : (s.core.updates id).phase = .ready)
      (unused : s.cursors id = none) :
      Step methods accepted source encoding s (setCursor s id
        (cursor (sourceProgram source (s.core.updates id).request)))
  | append (s) (id) (current) (wanted) (remaining) (record) (visibleAfter)
      (ready : (s.core.updates id).phase = .ready)
      (selected : s.cursors id = some current)
      (head : current.remaining = wanted :: remaining)
      (constructed : recordIntent encoding record = wanted)
      (acceptedAppend : CoreAppend encoding (overlay encoding s.audit current.acknowledged)
        record visibleAfter) :
      Step methods accepted source encoding s (setCursor s id (advance current remaining record))
  | requestCommit (s) (id) (current)
      (ready : (s.core.updates id).phase = .ready)
      (selected : s.cursors id = some current) (exhausted : current.remaining = []) :
      Step methods accepted source encoding s
        { s with
          core := (RegistrationUpdate.updateTx s.core id
            { s.core.updates id with phase := .committing }) }
  | serverCommit (s) (id) (current)
      (committing : (s.core.updates id).phase = .committing)
      (selected : s.cursors id = some current) (exhausted : current.remaining = [])
      (compatible : Compatible encoding s.audit current.acknowledged) :
      Step methods accepted source encoding s
        { s with
          core := RegistrationUpdate.committedState s.core id
          audit := overlay encoding s.audit current.acknowledged }
  /- Existing register data execution plus explicit atomic audit contribution.
  Its Register/Replace/ClaimNamespace program correctness is deliberately OPEN. -/
  | externalRegisterCommit (s) (next) (id) (writes) (records) (nextAudit)
      (base : RegistrationUpdate.Step true methods accepted s.core next)
      (event : next.events = s.core.events ++ [.register id writes])
      (appends : AppendBatch encoding s.audit records nextAudit)
      (disjoint : DisjointPrivate encoding s records) :
      Step methods accepted source encoding s { s with core := next, audit := nextAudit }
  | foreignAppend (s) (record) (next)
      (acceptedAppend : CoreAppend encoding s.audit record next)
      (disjoint : DisjointPrivate encoding s [record]) :
      Step methods accepted source encoding s { s with audit := next }

inductive Execution (methods : Nat → RegistrationUpdate.Method)
    (accepted : RegistrationUpdate.MetadataAccepted M B)
    (source : Source M S) (encoding : Encoding A S J U) :
    State C L M B St D T R P F A S J U → State C L M B St D T R P F A S J U → Prop where
  | refl (s) : Execution methods accepted source encoding s s
  | tail {first middle last} : Execution methods accepted source encoding first middle →
      Step methods accepted source encoding middle last →
      Execution methods accepted source encoding first last

def Progress (encoding : Encoding A S J U) (s : State C L M B St D T R P F A S J U) : Prop :=
  ∀ id current, s.cursors id = some current →
    current.acknowledged.map (recordIntent encoding) ++ current.remaining = current.program

def SourceBinding (source : Source M S) (s : State C L M B St D T R P F A S J U) : Prop :=
  ∀ id current, s.cursors id = some current →
    current.program = sourceProgram source (s.core.updates id).request

end RegistrationAuditedUpdate
