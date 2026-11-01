import RegistrationAuditedUpdateModel
import RegistrationUpdateSpecification
import RegistrationAuditedLifetimeSpecification

namespace RegistrationAuditedUpdate
open RegistrationAuditedLifetime
variable {C L M B St D T R P F A S J U : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq A]

/-- Independently stated exact certified UPDATE cut, relative to the ACTUAL
precommit database and ledger. External registrations' vocabulary is not claimed. -/
def UpdateDecision (source : Source M S) (encoding : Encoding A S J U)
    (before after : State C L M B St D T R P F A S J U)
    (request : RegistrationUpdate.Request C M St D T) (records : List (Record J U)) : Prop :=
  after.core.core.db = RegistrationUpdate.sequentialUpdate before.core.core.db request ∧
    (∃ record, records = [record] ∧
      recordIntent encoding record = ⟨.update, source.server request.metadata⟩) ∧
    FullRecordExtension encoding before.audit after.audit records ∧
    (∀ id record, before.audit id = some record → after.audit id = some record)

/-- This is per-own-step preservation, not a claim that concurrent external
writers could not have committed before or after this rollback. -/
def Discarded (before after : State C L M B St D T R P F A S J U) : Prop :=
  after.core.core.db = before.core.core.db ∧ after.audit = before.audit ∧
    after.cursors = before.cursors ∧ after.core.events = before.core.events

end RegistrationAuditedUpdate
