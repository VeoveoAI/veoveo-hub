import RegistrationAuditedUpdateWitnesses

/-! Synthetic single-deployment STORE-model recovery. Two distinct modeled attempts have
distinct actor IDs. No native collision execution, lost reply or new transition is
claimed. Expected maps reuse the independently literal populated seed/result. -/
namespace RegistrationAuditedUpdate.RecoveryWitnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationAuditedUpdate.Witnesses
open RegistrationTransaction (put)

def methods (id : Nat) : RegistrationUpdate.Method :=
  if id = 0 ∨ id = 1 then .update else .register
def start : World := RegistrationAuditedUpdate.initial seedDB registrations updates priorAudit

/-- Fixture endpoint helper, not a data executor or desired-result check. -/
def writingAt (s : World) (actor : Nat) (tx : Tx) (lock : Option Nat) : World :=
  { s with
    core := { RegistrationUpdate.updateTx s.core actor tx with
      core := { s.core.core with lock } } }
def preflightAt (s : World) (actor : Nat) : World := writingAt s actor
  { s.core.updates actor with phase := .preflight, authenticated := some first } s.core.core.lock
def beginAt (s : World) (actor : Nat) : World := writingAt (preflightAt s actor) actor
  { (preflightAt s actor).core.updates actor with phase := .begun } s.core.core.lock
def metadataAt (s : World) (actor : Nat) : World := writingAt (beginAt s actor) actor
  { (beginAt s actor).core.updates actor with
    phase := .metadataWritten
    staged := ((beginAt s actor).core.updates actor).staged ++
      [.deploymentMetadata ((beginAt s actor).core.updates actor).request.metadata] } (some actor)
def readyAt (s : World) (actor : Nat) : World := writingAt (metadataAt s actor) actor
  { (metadataAt s actor).core.updates actor with
    phase := .ready
    matched := some first
    outcome := .updated
    staged := ((metadataAt s actor).core.updates actor).staged ++
      [.matchedRow ((metadataAt s actor).core.updates actor).request.client
        (RegistrationUpdate.patched first ((metadataAt s actor).core.updates actor).request)] } (some actor)

def ready0 : World := readyAt start 0
def auditing0 : World := RegistrationAuditedUpdate.setCursor ready0 0
  (cursor (RegistrationAuditedUpdate.sourceProgram source (ready0.core.updates 0).request))
/-- Correct instruction and timestamp, but this ID is already committed to a
DIFFERENT complete record (prior900). This is not the native trigger mechanism. -/
def refusedRecord : Record Draft Nat :=
  ⟨⟨900, ⟨.update, "fixture-service"⟩, 223456789,
    "complete-refused-update-envelope:class=hub.registration;outcome=success;context=background"⟩, 223456⟩
def failed : World := writingAt auditing0 0
  { auditing0.core.updates 0 with phase := .rollbackPending, outcome := .failed, response := .error } (some 0)
def aborted : World := writingAt failed 0 { failed.core.updates 0 with phase := .aborted } none

def retryReady : World := readyAt aborted 1
def retryAuditing : World := RegistrationAuditedUpdate.setCursor retryReady 1
  (cursor (RegistrationAuditedUpdate.sourceProgram source (retryReady.core.updates 1).request))
def retryCursor0 : Cursor String Draft Nat :=
  cursor (RegistrationAuditedUpdate.sourceProgram source (retryReady.core.updates 1).request)
def retryCursor1 : Cursor String Draft Nat := advance retryCursor0 [] updateRecord
def retryAppended : World := RegistrationAuditedUpdate.setCursor retryAuditing 1 retryCursor1
def retryCommitting : World := writingAt retryAppended 1
  { retryAppended.core.updates 1 with phase := .committing } (some 1)
def retryCommitted : World :=
  { retryCommitting with
    core := RegistrationUpdate.committedState retryCommitting.core 1
    audit := overlay encoding retryCommitting.audit retryCursor1.acknowledged }
def retryAcknowledged : World := writingAt retryCommitted 1
  { retryCommitted.core.updates 1 with response := .success } none

/-- These values use no staged-write/audit executor or source-program evaluation. -/
def expectedFailureTx (phase : RegistrationUpdate.Phase) : Tx :=
  { request := request
    phase := phase
    authenticated := some first
    matched := some first
    staged := expectedWrites
    outcome := .failed
    response := .error }
def expectedPendingCursor : Cursor String Draft Nat :=
  ⟨[⟨.update, "fixture-service"⟩], [⟨.update, "fixture-service"⟩], []⟩
def expectedFailureUpdates (phase : RegistrationUpdate.Phase) : Nat → Tx :=
  put (fun _ => ({ request } : Tx)) 0 (expectedFailureTx phase)
def expectedFailureCursors : Nat → Option (Cursor String Draft Nat) :=
  put (fun _ => none) 0 (some expectedPendingCursor)

def FailureOutputs (s : World) (phase : RegistrationUpdate.Phase) (lock : Option Nat) : Prop :=
  s.core.core.db = seedDB ∧ s.audit = priorAudit ∧ s.core.core.lock = lock ∧
    s.core.updates = expectedFailureUpdates phase ∧
    s.core.core.transactions = (fun id =>
      ({ request := registrations id } : RegistrationTransaction.Transaction Nat Nat (Metadata × String) Payload String)) ∧
    s.core.core.commits = [] ∧ s.core.events = [] ∧ s.cursors = expectedFailureCursors

def expectedRetryUpdates : Nat → Tx := fun id =>
  if id = 1 then expectedTx else if id = 0 then expectedFailureTx .aborted else { request }
def expectedRetryCursors : Nat → Option (Cursor String Draft Nat) := fun id =>
  if id = 1 then some expectedCursor else if id = 0 then some expectedPendingCursor else none

def RetryOutputs : Prop :=
  retryAcknowledged.core.core.db = expectedDB ∧ retryAcknowledged.audit = expectedAudit ∧
    retryAcknowledged.core.core.lock = none ∧
    retryAcknowledged.core.updates = expectedRetryUpdates ∧
    retryAcknowledged.core.core.transactions = (fun id =>
      ({ request := registrations id } : RegistrationTransaction.Transaction Nat Nat (Metadata × String) Payload String)) ∧
    retryAcknowledged.core.core.commits = [] ∧
    retryAcknowledged.core.events = [.update 1 expectedWrites] ∧
    retryAcknowledged.cursors = expectedRetryCursors

end RegistrationAuditedUpdate.RecoveryWitnesses
