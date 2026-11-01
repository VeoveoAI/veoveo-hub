import RegistrationAuditedLifetimeModel
import RegistrationLifetimeSpecification

/-! Independent STORE contract. Multiplicity is by activity and stored server,
not a set and not equality to an executor fold. Full-record extension is stated
pointwise; unrelated records and the complete data result remain visible. -/
namespace RegistrationAuditedLifetime
variable {C L M R P I D F A S J T : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq A] [DecidableEq S]

def expectedBag (source : Source M R S)
    (request : RegistrationLifetime.Request C L M R P D)
    (ended : List (RegistrationTransaction.Registration C L M R)) (intent : Intent S) : Nat :=
  match request with
  | .register proposal =>
      if intent.activity = .replace then
        (ended.map (fun row => source.endedServer row.payload)).count intent.server
      else if intent.activity = .register ∧
          source.candidateServer proposal.candidate.metadata = intent.server then 1 else 0
  | .remove =>
      if intent.activity = .deregister then
        (ended.map (fun row => source.endedServer row.payload)).count intent.server
      else 0

def ExactBag (source : Source M R S) (encoding : Encoding A S J T)
    (request : RegistrationLifetime.Request C L M R P D)
    (ended : List (RegistrationTransaction.Registration C L M R)) (records : List (Record J T)) : Prop :=
  ∀ intent, (records.map (recordIntent encoding)).count intent = expectedBag source request ended intent

/-- Preconditions for counting calls as distinct NEW records. Exact same-ID retry
is modeled separately and does not create an additional native record. -/
def NewIdentities (encoding : Encoding A S J T) (before : Ledger A J T)
    (records : List (Record J T)) : Prop :=
  (records.map (fun record => encoding.id record.draft)).Nodup ∧
    ∀ record ∈ records, before (encoding.id record.draft) = none

def FullRecordExtension (encoding : Encoding A S J T)
    (before after : Ledger A J T) (records : List (Record J T)) : Prop :=
  (∀ record ∈ records, after (encoding.id record.draft) = some record) ∧
    (∀ id, id ∉ records.map (fun record => encoding.id record.draft) → after id = before id)

/-- Full modeled data plus audit bag/records at the atomic STORE decision. No
service response, served update or continued candidate liveness is asserted. -/
def StoreDecision (source : Source M R S) (encoding : Encoding A S J T)
    (before after : State C L M R P I D F A S J T)
    (request : RegistrationLifetime.Request C L M R P D)
    (ended : List (RegistrationTransaction.Registration C L M R)) (records : List (Record J T)) : Prop :=
  after.core.db = RegistrationLifetime.sequential before.core.db request ∧
    ExactBag source encoding request ended records ∧
    FullRecordExtension encoding before.audit after.audit records

end RegistrationAuditedLifetime
