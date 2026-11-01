import RegistrationAuditedLifetimeSpecification

/-! Populated synthetic STORE-model inputs and execution endpoints. This module
imports models/specifications only, so a behavioral mutation can still elaborate
its data without importing correctness proofs. The two-row seed is SQL-valid in
the modeled writer domain; its native creation history is NOT claimed. -/
namespace RegistrationAuditedLifetime.Witnesses
open RegistrationAuditedLifetime
open RegistrationTransaction (put)

structure Metadata where
  server : String
  complete : String
  deriving DecidableEq

structure RowPayload where
  server : String
  complete : String
  deriving DecidableEq

/-- Complete synthetic draft envelope; original time and stored column time stay
distinct. IDs/credentials are model fixture data, not native parser/crypto proofs. -/
structure Draft where
  id : Nat
  intent : Intent String
  originalNanos : Nat
  complete : String
  deriving DecidableEq

abbrev Row := RegistrationTransaction.Registration Nat Nat Metadata RowPayload
abbrev DB := RegistrationLifetime.Database Nat Nat Metadata RowPayload String String String (String × Nat)
abbrev Core := RegistrationLifetime.State Nat Nat Metadata RowPayload String String String (String × Nat)
abbrev Tx := RegistrationLifetime.Transaction Nat Nat Metadata RowPayload String String String
abbrev Req := RegistrationLifetime.Request Nat Nat Metadata RowPayload String String
abbrev World := State Nat Nat Metadata RowPayload String String String (String × Nat) Nat String Draft Nat
abbrev Audit := Ledger Nat Draft Nat

def oldServer := "server-a-fixture"
def newServer := "server-b-fixture"
def otherServer := "unrelated-server-fixture"
def instancePayload := "full-instance:deployment=fixture;database=installation;factory=retained"
def oldMetadata : Metadata := ⟨oldServer, "full-old-metadata:key=old;binding=shared;version=1"⟩
def newMetadata : Metadata := ⟨newServer, "full-new-metadata:key=new;binding=shared;version=2"⟩
def oldFirst : Row := ⟨30, 40, oldMetadata, ⟨oldServer, "full-old-row:first;issued=11;credential=synthetic-first"⟩⟩
def oldSecond : Row := ⟨31, 41, oldMetadata, ⟨oldServer, "full-old-row:second;issued=12;credential=synthetic-second"⟩⟩
def candidate : Row := ⟨10, 20, newMetadata, ⟨newServer, "full-new-row:endpoint=https://fixture.invalid/mcp;issued=23;credential=synthetic-new"⟩⟩
def proposal : RegistrationLifetime.Proposal Nat Nat Metadata RowPayload String String :=
  { candidate, role := "new-registration-role", firstDeployment := "unused-first-deployment-value"
    externalEnvironment := false }
def requests : Nat → Req := fun id => if id = 0 then .register proposal else .remove

def source : Source Metadata RowPayload String := ⟨Metadata.server, RowPayload.server⟩
def encoding : Encoding Nat String Draft Nat :=
  ⟨Draft.id, Draft.intent, fun draft => draft.originalNanos / 1000⟩

def priorRecord : Record Draft Nat :=
  ⟨⟨900, ⟨.update, otherServer⟩, 987654321,
    "complete-prior-envelope:class=hub.registration;outcome=success;context=prior"⟩, 987654⟩
def replaceFirst : Record Draft Nat :=
  ⟨⟨100, ⟨.replace, oldServer⟩, 123456789,
    "complete-replace-envelope:first;class=hub.registration;outcome=success;context=background"⟩, 123456⟩
def replaceSecond : Record Draft Nat :=
  ⟨⟨101, ⟨.replace, oldServer⟩, 123457789,
    "complete-replace-envelope:second;class=hub.registration;outcome=success;context=background"⟩, 123457⟩
def registerRecord : Record Draft Nat :=
  ⟨⟨102, ⟨.register, newServer⟩, 123458789,
    "complete-register-envelope:class=hub.registration;outcome=success;context=background"⟩, 123458⟩
def priorAudit : Audit := fun id => if id = 900 then some priorRecord else none

def seedDB : DB :=
  { instanceRow := some instancePayload
    deployment := some ⟨oldMetadata, "retained-complete-deployment-stable"⟩
    registrations := fun key => if key = 30 then some oldFirst else if key = 31 then some oldSecond else none
    logins := fun login => if login = 40 then some "old-first-role"
      else if login = 41 then some "old-second-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("full-unrelated-application", 73) }
def start : World := initial seedDB requests priorAudit

/-- Source-state helpers retain the entire World, changing only the actual
transaction and named lock fields. They never call the independent result spec. -/
def writing (s : World) (tx : Tx) (instanceOwner deploymentOwner : Option Nat) : World :=
  { s with
    core := { RegistrationLifetime.update s.core 0 tx with instanceOwner, deploymentOwner } }

def cached : World := writing start
  { start.core.transactions 0 with phase := .preflight, cachedAuthority := some instancePayload } none none
def waiting : World := writing cached { cached.core.transactions 0 with phase := .waiting } none none
def selected : World := writing waiting
  { waiting.core.transactions 0 with phase := .held, lockedInstance := some instancePayload } (some 0) none
def prepared : World := writing selected { selected.core.transactions 0 with phase := .upserting } (some 0) none
def upserted : World := writing prepared
  { prepared.core.transactions 0 with
    phase := .deleting
    staged := [.upsertDeployment newMetadata "unused-first-deployment-value"] } (some 0) (some 0)
def captured : World := writing upserted
  { upserted.core.transactions 0 with
    phase := .dropping
    captured := [oldFirst, oldSecond]
    drops := [40, 41]
    staged := (upserted.core.transactions 0).staged ++ [.registration (.deleteRows [30, 31])] }
  (some 0) (some 0)
def droppedFirst : World := writing captured
  { captured.core.transactions 0 with
    drops := [41]
    staged := (captured.core.transactions 0).staged ++ [.registration (.dropLogin 40)] } (some 0) (some 0)
def droppedSecond : World := writing droppedFirst
  { droppedFirst.core.transactions 0 with
    drops := []
    staged := (droppedFirst.core.transactions 0).staged ++ [.registration (.dropLogin 41)] } (some 0) (some 0)
def creating : World := writing droppedSecond
  { droppedSecond.core.transactions 0 with phase := .creating } (some 0) (some 0)
def created : World := writing creating
  { creating.core.transactions 0 with
    phase := .inserting
    staged := (creating.core.transactions 0).staged ++ [.registration (.createLogin 20 "new-registration-role")] }
  (some 0) (some 0)
def ready : World := writing created
  { created.core.transactions 0 with
    phase := .ready
    staged := (created.core.transactions 0).staged ++ [.registration (.insert candidate)] } (some 0) (some 0)

def begun : World := setCursor ready 0
  (cursor (sourceProgram source (ready.core.transactions 0).request (ready.core.transactions 0).captured))
def cursor0 : Cursor String Draft Nat :=
  cursor (sourceProgram source (ready.core.transactions 0).request (ready.core.transactions 0).captured)
def cursor1 : Cursor String Draft Nat :=
  advance cursor0 cursor0.remaining.tail replaceFirst
def cursor2 : Cursor String Draft Nat := advance cursor1 cursor1.remaining.tail replaceSecond
/-- Bounded fixture operation, not a source guard: an exhausted actual cursor has
no instruction to append. This keeps the later dropped-instruction control from
silently executing an extra hardcoded Register anyway. Actual Step proofs check
the record against the selected head in every nonempty baseline call. -/
def appendPending (s : World) (record : Record Draft Nat) : World :=
  match s.cursors 0 with
  | none => s
  | some current =>
      match current.remaining with
      | [] => s
      | _ :: remaining => setCursor s 0 (advance current remaining record)
def appendedFirst : World := appendPending begun replaceFirst
def appendedSecond : World := appendPending appendedFirst replaceSecond
def appendedRegister : World := appendPending appendedSecond registerRecord
def cursor3 : Cursor String Draft Nat := (appendedRegister.cursors 0).getD (cursor [])
def committing : World :=
  { appendedRegister with
    core := (RegistrationLifetime.update appendedRegister.core 0
      { appendedRegister.core.transactions 0 with phase := .committing }) }
def committed : World :=
  { committing with
    core := RegistrationLifetime.committedState committing.core 0
    audit := overlay encoding committing.audit cursor3.acknowledged }
def acknowledged : World :=
  { committed with
    core := (RegistrationLifetime.update committed.core 0
      { committed.core.transactions 0 with response := .registered }) }

/-- These expected values are spelled independently of applyWrites, overlay,
sourceProgram or any sequential executor. All unrelated roles/records survive. -/
def expectedDB : DB :=
  { instanceRow := some instancePayload
    deployment := some ⟨⟨newServer, "full-new-metadata:key=new;binding=shared;version=2"⟩,
      "retained-complete-deployment-stable"⟩
    registrations := fun key => if key = 10 then
      some ⟨10, 20, ⟨newServer, "full-new-metadata:key=new;binding=shared;version=2"⟩,
        ⟨newServer, "full-new-row:endpoint=https://fixture.invalid/mcp;issued=23;credential=synthetic-new"⟩⟩ else none
    logins := fun login => if login = 20 then some "new-registration-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("full-unrelated-application", 73) }
def expectedAudit : Audit := fun id =>
  if id = 100 then some replaceFirst else if id = 101 then some replaceSecond
  else if id = 102 then some registerRecord else if id = 900 then some priorRecord else none
def expectedCursor : Cursor String Draft Nat :=
  ⟨[⟨.replace, oldServer⟩, ⟨.replace, oldServer⟩, ⟨.register, newServer⟩], [],
    [replaceFirst, replaceSecond, registerRecord]⟩

def ReplacementAccepted : Prop :=
  acknowledged.core.db = expectedDB ∧ acknowledged.audit = expectedAudit ∧
    (acknowledged.core.transactions 0).request = .register proposal ∧
    (acknowledged.core.transactions 0).response = .registered ∧
    (acknowledged.core.transactions 0).phase = .committed ∧
    acknowledged.core.instanceOwner = none ∧ acknowledged.core.deploymentOwner = none ∧
    acknowledged.cursors 0 = some expectedCursor ∧
    acknowledged.core.commits =
      [(0, [.upsertDeployment newMetadata "unused-first-deployment-value",
        .registration (.deleteRows [30, 31]), .registration (.dropLogin 40), .registration (.dropLogin 41),
        .registration (.createLogin 20 "new-registration-role"), .registration (.insert candidate)])] ∧
    (expectedCursor.acknowledged.map (recordIntent encoding)).count ⟨.replace, oldServer⟩ = 2 ∧
    (expectedCursor.acknowledged.map (recordIntent encoding)).count ⟨.register, newServer⟩ = 1 ∧
    (expectedCursor.acknowledged.map (recordIntent encoding)).count ⟨.replace, newServer⟩ = 0

end RegistrationAuditedLifetime.Witnesses
