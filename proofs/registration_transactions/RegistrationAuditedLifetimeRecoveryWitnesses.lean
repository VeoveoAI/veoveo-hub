import RegistrationAuditedLifetimeWitnesses

/-! Synthetic two-row removal/rollback/retry executions. Imports contain no
correctness leaves. The source refuses a later audit call before commit; the
observed failure/rollback is explicit, not inferred from absence of an append.
The qualified native refusal fixture removes one row, not this exact schedule. -/
namespace RegistrationAuditedLifetime.RemovalRecovery
open RegistrationAuditedLifetime
open RegistrationAuditedLifetime.Witnesses

def refusalMessage := "synthetic append refusal: Core VV501 conflicting record"
def lostReplyMessage := "synthetic removal commit reply lost"

def firstDeregister : Record Draft Nat :=
  ⟨⟨100, ⟨.deregister, oldServer⟩, 223456789,
    "complete-deregister-envelope:attempt1-first;class=hub.registration;outcome=success;context=background"⟩, 223456⟩
/-- Same committed ID as priorRecord, but different complete draft AND stored
column time. No global UUID-freshness claim is made by this model fixture. -/
def conflictingDeregister : Record Draft Nat :=
  ⟨⟨900, ⟨.deregister, oldServer⟩, 223457789,
    "complete-deregister-envelope:attempt1-second;class=hub.registration;outcome=success;context=background"⟩, 223457⟩
def retryDeregisterFirst : Record Draft Nat :=
  ⟨⟨200, ⟨.deregister, oldServer⟩, 323456789,
    "complete-deregister-envelope:attempt2-first;class=hub.registration;outcome=success;context=background"⟩, 323456⟩
def retryDeregisterSecond : Record Draft Nat :=
  ⟨⟨201, ⟨.deregister, oldServer⟩, 323457789,
    "complete-deregister-envelope:attempt2-second;class=hub.registration;outcome=success;context=background"⟩, 323457⟩

/-- A narrow source-state helper for two removal attempts. Deployment lock,
complete DB/audit, all other transactions and cursors are preserved. -/
def writeAttempt (s : World) (actor : Nat) (tx : Tx) (instanceOwner : Option Nat) : World :=
  { s with
    core := { RegistrationLifetime.update s.core actor tx with instanceOwner } }
def beginRemoval (s : World) (actor : Nat) : World :=
  writeAttempt s actor { s.core.transactions actor with phase := .waiting } s.core.instanceOwner
def selectRemoval (s : World) (actor : Nat) : World :=
  writeAttempt s actor
    { s.core.transactions actor with
      phase := RegistrationLifetime.afterLock (s.core.transactions actor).request
      lockedInstance := some instancePayload } (some actor)
def captureRemoval (s : World) (actor : Nat) : World :=
  writeAttempt s actor
    { s.core.transactions actor with
      phase := .dropping
      captured := [oldFirst, oldSecond]
      drops := [40, 41]
      staged := (s.core.transactions actor).staged ++ [.registration (.deleteRows [30, 31])] }
    s.core.instanceOwner
def dropRemoval (s : World) (actor login : Nat) (remaining : List Nat) : World :=
  writeAttempt s actor
    { s.core.transactions actor with
      drops := remaining
      staged := (s.core.transactions actor).staged ++ [.registration (.dropLogin login)] }
    s.core.instanceOwner
def eraseRemoval (s : World) (actor : Nat) : World :=
  writeAttempt s actor { s.core.transactions actor with phase := .erasing } s.core.instanceOwner
def readyRemoval (s : World) (actor : Nat) : World :=
  writeAttempt s actor
    { s.core.transactions actor with
      phase := .ready
      lockedInstance := some instancePayload
      staged := (s.core.transactions actor).staged ++ [.deleteInstance] } (some actor)
def removalPrefix (s : World) (actor : Nat) : World :=
  readyRemoval (eraseRemoval (dropRemoval (dropRemoval
    (captureRemoval (selectRemoval (beginRemoval s actor) actor) actor) actor 40 [41]) actor 41 []) actor) actor

def beginAuditAt (s : World) (actor : Nat) : World :=
  setCursor s actor (cursor (sourceProgram source (s.core.transactions actor).request
    (s.core.transactions actor).captured))
def cursorAt (s : World) (actor : Nat) : Cursor String Draft Nat :=
  (s.cursors actor).getD (cursor [])
def appendPendingAt (s : World) (actor : Nat) (record : Record Draft Nat) : World :=
  match s.cursors actor with
  | none => s
  | some current =>
      match current.remaining with
      | [] => s
      | _ :: remaining => setCursor s actor (advance current remaining record)

def firstReady : World := removalPrefix start 1
def firstBegun : World := beginAuditAt firstReady 1
def partialRemoval : World := appendPendingAt firstBegun 1 firstDeregister
def refused : World :=
  { partialRemoval with
    core := (RegistrationLifetime.update partialRemoval.core 1
      { partialRemoval.core.transactions 1 with
        phase := .abortPending
        response := .error
        error := some refusalMessage }) }
def aborted : World :=
  { refused with
    core := { (RegistrationLifetime.update refused.core 1
      { refused.core.transactions 1 with phase := .aborted }) with
      instanceOwner := RegistrationLifetime.release refused.core.instanceOwner 1
      deploymentOwner := RegistrationLifetime.release refused.core.deploymentOwner 1 } }

def retryReady : World := removalPrefix aborted 2
def retryBegun : World := beginAuditAt retryReady 2
def retryOnce : World := appendPendingAt retryBegun 2 retryDeregisterFirst
def retryTwice : World := appendPendingAt retryOnce 2 retryDeregisterSecond
def retryCommitting : World :=
  { retryTwice with
    core := (RegistrationLifetime.update retryTwice.core 2
      { retryTwice.core.transactions 2 with phase := .committing }) }
def retryCommitted : World :=
  { retryCommitting with
    core := RegistrationLifetime.committedState retryCommitting.core 2
    audit := overlay encoding retryCommitting.audit (cursorAt retryCommitting 2).acknowledged }
def retryAcknowledged : World :=
  { retryCommitted with
    core := (RegistrationLifetime.update retryCommitted.core 2
      { retryCommitted.core.transactions 2 with response := .removed }) }

/-- Alternative suffix from the SAME commit-pending state; not an automatic retry
after an unknown commit outcome. No successful served_at write is represented. -/
def replyLost : World :=
  { retryCommitting with
    core := (RegistrationLifetime.update retryCommitting.core 2
      { retryCommitting.core.transactions 2 with response := .error, error := some lostReplyMessage }) }
def replyLostCommitted : World :=
  { replyLost with
    core := RegistrationLifetime.committedState replyLost.core 2
    audit := overlay encoding replyLost.audit (cursorAt replyLost 2).acknowledged }

/-- Independent outputs, not calls to removal/applyWrites/overlay. -/
def expectedRemovedDB : DB :=
  { instanceRow := none
    deployment := some ⟨oldMetadata, "retained-complete-deployment-stable"⟩
    registrations := fun _ => none
    logins := fun login => if login = 99 then some "unrelated-role" else none
    frame := ("full-unrelated-application", 73) }
def expectedRemovedAudit : Audit := fun id =>
  if id = 200 then some retryDeregisterFirst else if id = 201 then some retryDeregisterSecond
  else if id = 900 then some priorRecord else none
def expectedPartialCursor : Cursor String Draft Nat :=
  ⟨[⟨.deregister, oldServer⟩, ⟨.deregister, oldServer⟩], [⟨.deregister, oldServer⟩], [firstDeregister]⟩
def expectedRetryCursor : Cursor String Draft Nat :=
  ⟨[⟨.deregister, oldServer⟩, ⟨.deregister, oldServer⟩], [], [retryDeregisterFirst, retryDeregisterSecond]⟩
def removalWrites : List (RegistrationLifetime.Write Nat Nat Metadata RowPayload String String) :=
  [.registration (.deleteRows [30, 31]), .registration (.dropLogin 40),
    .registration (.dropLogin 41), .deleteInstance]

def ConfirmedRollback : Prop :=
  aborted.core.db = seedDB ∧ aborted.audit = priorAudit ∧ aborted.core.commits = [] ∧
    aborted.core.instanceOwner = none ∧ aborted.core.deploymentOwner = none ∧
    (aborted.core.transactions 1).request = .remove ∧
    (aborted.core.transactions 1).phase = .aborted ∧
    (aborted.core.transactions 1).response = .error ∧
    (aborted.core.transactions 1).error = some refusalMessage ∧
    (aborted.core.transactions 1).staged = removalWrites ∧
    aborted.cursors 1 = some expectedPartialCursor ∧ aborted.audit 100 = none ∧
    aborted.audit 900 = some priorRecord ∧
    aborted.core.transactions 0 = start.core.transactions 0 ∧ aborted.cursors 0 = none

def RemovalRetried : Prop :=
  retryAcknowledged.core.db = expectedRemovedDB ∧ retryAcknowledged.audit = expectedRemovedAudit ∧
    retryAcknowledged.core.instanceOwner = none ∧ retryAcknowledged.core.deploymentOwner = none ∧
    retryAcknowledged.core.commits = [(2, removalWrites)] ∧
    (retryAcknowledged.core.transactions 2).request = .remove ∧
    (retryAcknowledged.core.transactions 2).phase = .committed ∧
    (retryAcknowledged.core.transactions 2).response = .removed ∧
    (retryAcknowledged.core.transactions 2).error = none ∧
    retryAcknowledged.core.transactions 1 = aborted.core.transactions 1 ∧
    retryAcknowledged.cursors 1 = some expectedPartialCursor ∧
    retryAcknowledged.cursors 2 = some expectedRetryCursor ∧
    retryAcknowledged.audit 100 = none ∧ retryAcknowledged.audit 900 = some priorRecord ∧
    retryAcknowledged.core.transactions 0 = start.core.transactions 0 ∧ retryAcknowledged.cursors 0 = none ∧
    ((cursorAt retryAcknowledged 2).acknowledged.map (recordIntent encoding)).count ⟨.deregister, oldServer⟩ = 2

def ErrorReplyCommitted : Prop :=
  replyLostCommitted.core.db = expectedRemovedDB ∧ replyLostCommitted.audit = expectedRemovedAudit ∧
    replyLostCommitted.core.instanceOwner = none ∧ replyLostCommitted.core.deploymentOwner = none ∧
    replyLostCommitted.core.commits = [(2, removalWrites)] ∧
    (replyLostCommitted.core.transactions 2).request = .remove ∧
    (replyLostCommitted.core.transactions 2).phase = .committed ∧
    (replyLostCommitted.core.transactions 2).response = .error ∧
    (replyLostCommitted.core.transactions 2).error = some lostReplyMessage ∧
    replyLostCommitted.core.transactions 1 = aborted.core.transactions 1 ∧
    replyLostCommitted.cursors 1 = some expectedPartialCursor ∧
    replyLostCommitted.cursors 2 = some expectedRetryCursor ∧
    replyLostCommitted.core.transactions 0 = start.core.transactions 0 ∧ replyLostCommitted.cursors 0 = none

end RegistrationAuditedLifetime.RemovalRecovery
