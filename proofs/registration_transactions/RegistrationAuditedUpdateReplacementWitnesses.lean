import RegistrationAuditedUpdateWitnesses

/-! Concrete ordinary replacement in one admitted already-claimed namespace,
then a previously authenticated UPDATE becomes an affected0 no-op. The namespace
pin and native admission are interpretation boundaries, not a new policy/guard.
The chosen [first,second] Listing is one valid order,not a SQL ordering claim. -/
namespace RegistrationAuditedUpdate.ReplacementWitnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationAuditedUpdate.Witnesses
open RegistrationTransaction (put)

abbrev RegTx := RegistrationTransaction.Transaction Nat Nat (Metadata × String) Payload String
abbrev RegCore := RegistrationUpdate.RegisterState Nat Nat Metadata String String String Nat Stable String (String × Nat)
abbrev RegWrite := RegistrationTransaction.Write Nat Nat (Metadata × String) Payload String

def replacementMetadata : Metadata :=
  ⟨"fixture-service", "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v3;version=3"⟩
def candidate : Row :=
  ⟨50, 60, (replacementMetadata, binding),
    ⟨"opaque-signed-statement:replacement", "opaque-token-digest:replacement", 501, 501,
      ⟨"retained-deployment", "fixture-service", none, "complete-stable:replacement;created=500;remainder=new"⟩⟩⟩
def registerRequest : RegistrationTransaction.Request Nat Nat (Metadata × String) Payload String :=
  { candidate, role := "replacement-registration-role", kind := .ordinary }
def registerRequests (id : Nat) := if id = 1 then registerRequest else registrations id
def start : World := RegistrationAuditedUpdate.initial seedDB registerRequests updates priorAudit

def updateAt (s : World) (tx : Tx) (lock : Option Nat) : World :=
  { s with
    core := { RegistrationUpdate.updateTx s.core 0 tx with core := { s.core.core with lock } } }
def stale : World := updateAt start
  { start.core.updates 0 with phase := .preflight, authenticated := some first } none
/-- Endpoint helper only. Every use is justified by actual old register Step and
RegisterAction,not an assumed atomic correct replacement. -/
def registerAt (s : World) (tx : RegTx) (lock : Option Nat) : World :=
  { s with
    core := { s.core with core := { RegistrationTransaction.update s.core.core 1 tx with lock } } }
def waiting : World := registerAt stale { stale.core.core.transactions 1 with phase := .waiting } none
def selected : World := registerAt waiting { waiting.core.core.transactions 1 with phase := .locked } (some 1)
def observed : World := registerAt selected
  { selected.core.core.transactions 1 with phase := .read, observed := some selected.core.core.db.metadata } (some 1)
def replacing : World := registerAt observed
  { observed.core.core.transactions 1 with phase := .upserting, replacing := true } (some 1)
def upserted : World := registerAt replacing
  { replacing.core.core.transactions 1 with
    phase := .deleting
    staged := (replacing.core.core.transactions 1).staged ++ [.deployment candidate.metadata] } (some 1)
def captured : World := registerAt upserted
  { upserted.core.core.transactions 1 with
    phase := .dropping
    captured := [first, second]
    drops := [40, 41]
    staged := (upserted.core.core.transactions 1).staged ++ [.deleteRows [30, 31]] } (some 1)
def droppedFirst : World := registerAt captured
  { captured.core.core.transactions 1 with
    drops := [41]
    staged := (captured.core.core.transactions 1).staged ++ [.dropLogin 40] } (some 1)
def droppedSecond : World := registerAt droppedFirst
  { droppedFirst.core.core.transactions 1 with
    drops := []
    staged := (droppedFirst.core.core.transactions 1).staged ++ [.dropLogin 41] } (some 1)
def creating : World := registerAt droppedSecond
  { droppedSecond.core.core.transactions 1 with phase := .creating } (some 1)
def created : World := registerAt creating
  { creating.core.core.transactions 1 with
    phase := .inserting
    staged := (creating.core.core.transactions 1).staged ++ [.createLogin 60 "replacement-registration-role"] } (some 1)
def ready : World := registerAt created
  { created.core.core.transactions 1 with
    phase := .ready
    staged := (created.core.core.transactions 1).staged ++ [.insert candidate] } (some 1)
def committing : World := registerAt ready { ready.core.core.transactions 1 with phase := .committing } (some 1)

def replaceFirst : Record Draft Nat :=
  ⟨⟨200, ⟨.replace, "fixture-service"⟩, 223456789,
    "complete-replace-envelope:first;class=hub.registration;outcome=success;context=background"⟩, 223456⟩
def replaceSecond : Record Draft Nat :=
  ⟨⟨201, ⟨.replace, "fixture-service"⟩, 223457789,
    "complete-replace-envelope:second;class=hub.registration;outcome=success;context=background"⟩, 223457⟩
def registerRecord : Record Draft Nat :=
  ⟨⟨202, ⟨.register, "fixture-service"⟩, 223458789,
    "complete-register-envelope:class=hub.registration;outcome=success;context=background"⟩, 223458⟩
def externalRecords : List (Record Draft Nat) := [replaceFirst, replaceSecond, registerRecord]
def externalAudit : Audit :=
  put (put (put priorAudit 200 (some replaceFirst)) 201 (some replaceSecond)) 202 (some registerRecord)
def registered : World :=
  { committing with
    core := { committing.core with
      core := RegistrationTransaction.committedState committing.core.core 1
      events := committing.core.events ++ [.register 1 (committing.core.core.transactions 1).staged] }
    audit := externalAudit }
def registerAcknowledged : World := registerAt registered
  { registered.core.core.transactions 1 with response := .success } none

def updateBegun : World := updateAt registerAcknowledged
  { registerAcknowledged.core.updates 0 with phase := .begun } none
def metadataWritten : World := updateAt updateBegun
  { updateBegun.core.updates 0 with
    phase := .metadataWritten
    staged := (updateBegun.core.updates 0).staged ++ [.deploymentMetadata (updateBegun.core.updates 0).request.metadata] }
  (some 0)
def missing : World := updateAt metadataWritten
  { metadataWritten.core.updates 0 with phase := .rollbackPending, outcome := .noop } (some 0)
def rolledBack : World := updateAt missing { missing.core.updates 0 with phase := .aborted } none
def acknowledged : World := updateAt rolledBack { rolledBack.core.updates 0 with response := .success } none

/-- Complete independent expected values,no commit/overlay/sequential executor. -/
def expectedCandidate : Row :=
  ⟨50, 60,
    (⟨"fixture-service", "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v3;version=3"⟩,
      "installation:retained-deployment-binding"),
    ⟨"opaque-signed-statement:replacement", "opaque-token-digest:replacement", 501, 501,
      ⟨"retained-deployment", "fixture-service", none,
        "complete-stable:replacement;created=500;remainder=new"⟩⟩⟩
def expectedDB : DB :=
  { metadata :=
      (⟨"fixture-service", "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v3;version=3"⟩,
        "installation:retained-deployment-binding")
    registrations := fun key => if key = 50 then some expectedCandidate else none
    logins := fun login => if login = 60 then some "replacement-registration-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("complete-unrelated-application", 73) }
def expectedAudit : Audit := fun id =>
  if id = 202 then some registerRecord else if id = 201 then some replaceSecond
  else if id = 200 then some replaceFirst else if id = 900 then some priorRecord else none
def expectedWrites : List RegWrite :=
  [.deployment (replacementMetadata, binding), .deleteRows [30, 31], .dropLogin 40,
    .dropLogin 41, .createLogin 60 "replacement-registration-role", .insert expectedCandidate]
def expectedRegisterTx : RegTx :=
  { request := registerRequest
    phase := .committed
    observed := some (oldMetadata, binding)
    replacing := true
    captured := [first, second]
    drops := []
    staged := expectedWrites
    response := .success }
def expectedUpdateTx : Tx :=
  { request := request
    phase := .aborted
    authenticated := some first
    matched := none
    staged := [.deploymentMetadata nextMetadata]
    outcome := .noop
    response := .success }

def ReplacementNoopAccepted : Prop :=
  acknowledged.core.core.db = expectedDB ∧ acknowledged.audit = expectedAudit ∧
    acknowledged.core.core.lock = none ∧
    acknowledged.core.updates = put (fun _ => ({ request } : Tx)) 0 expectedUpdateTx ∧
    acknowledged.core.core.transactions = put (fun id => ({ request := registerRequests id } : RegTx))
      1 expectedRegisterTx ∧
    acknowledged.core.core.commits = [(1, expectedWrites)] ∧
    acknowledged.core.events = [.register 1 expectedWrites] ∧ acknowledged.cursors = (fun _ => none)

end RegistrationAuditedUpdate.ReplacementWitnesses
