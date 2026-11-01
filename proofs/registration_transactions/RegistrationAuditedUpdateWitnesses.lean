import RegistrationAuditedUpdateSpecification

/-! Populated synthetic admitted STORE-model input. Models/specifications only;
no generic UPDATE correctness or output-acceptance module is imported. Creation of
the two-replica seed, native parsing/signatures and SQL refinement are not proved. -/
namespace RegistrationAuditedUpdate.Witnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationTransaction (put)

structure Metadata where
  server : String
  complete : String
  deriving DecidableEq

structure Stable where
  deployment : String
  server : String
  orphanedAt : Option Nat
  complete : String
  deriving DecidableEq

structure Draft where
  id : Nat
  intent : Intent String
  originalNanos : Nat
  complete : String
  deriving DecidableEq

abbrev Payload := RegistrationUpdate.Payload String String Nat Stable
abbrev Row := RegistrationUpdate.Row Nat Nat Metadata String String String Nat Stable
abbrev DB := RegistrationUpdate.DB Nat Nat Metadata String String String Nat Stable String (String × Nat)
abbrev Core := RegistrationUpdate.State Nat Nat Metadata String String String Nat Stable String (String × Nat)
abbrev Tx := RegistrationUpdate.Transaction Nat Nat Metadata String String String Nat Stable
abbrev Req := RegistrationUpdate.Request Nat Metadata String String Nat
abbrev World := RegistrationAuditedUpdate.State Nat Nat Metadata String String String Nat Stable String
  (String × Nat) Nat String Draft Nat
abbrev Audit := Ledger Nat Draft Nat

def server := "fixture-service"
def binding := "installation:retained-deployment-binding"
def oldMetadata : Metadata := ⟨server, "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v1;version=1"⟩
def nextMetadata : Metadata := ⟨server, "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v2;version=2"⟩
def first : Row :=
  ⟨30, 40, (oldMetadata, binding),
    ⟨"opaque-signed-statement:first-before", "opaque-token-digest:first", 11, 12,
      ⟨"retained-deployment", server, none, "complete-stable:first;created=5;remainder=retained"⟩⟩⟩
def second : Row :=
  ⟨31, 41, (oldMetadata, binding),
    ⟨"opaque-signed-statement:second-before", "opaque-token-digest:second", 21, 22,
      ⟨"retained-deployment", server, some 23, "complete-stable:second;created=6;remainder=retained"⟩⟩⟩
def request : Req :=
  { client := 30, metadata := nextMetadata, statement := "opaque-signed-statement:first-after"
    presentedDigest := "opaque-token-digest:first", now := 99 }
def registrations : Nat → RegistrationTransaction.Request Nat Nat (Metadata × String) Payload String :=
  fun _ => { candidate := first, role := "first-registration-role", kind := .ordinary }
def updates : Nat → Req := fun _ => request
def methods (id : Nat) : RegistrationUpdate.Method := if id = 0 then .update else .register
/-- Selected statement/admission interpretation, not a correct-output oracle. -/
def metadataAccepted (metadata : Metadata) (storage : String) : Prop :=
  metadata.server = server ∧ storage = binding

def source : RegistrationAuditedUpdate.Source Metadata String := ⟨Metadata.server⟩
def encoding : Encoding Nat String Draft Nat :=
  ⟨Draft.id, Draft.intent, fun draft => draft.originalNanos / 1000⟩
def priorRecord : Record Draft Nat :=
  ⟨⟨900, ⟨.register, "unrelated-fixture-server"⟩, 987654321,
    "complete-prior-envelope:class=hub.registration;outcome=success;context=prior"⟩, 987654⟩
def updateRecord : Record Draft Nat :=
  ⟨⟨100, ⟨.update, "fixture-service"⟩, 123456789,
    "complete-update-envelope:class=hub.registration;outcome=success;context=background"⟩, 123456⟩
def priorAudit : Audit := fun id => if id = 900 then some priorRecord else none

def seedDB : DB :=
  { metadata := (oldMetadata, binding)
    registrations := fun key => if key = 30 then some first else if key = 31 then some second else none
    logins := fun login => if login = 40 then some "first-registration-role"
      else if login = 41 then some "second-registration-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("complete-unrelated-application", 73) }
def start : World := RegistrationAuditedUpdate.initial seedDB registrations updates priorAudit

/-- Complete fixture state helper; only the actual actor transaction and shared
lock fields change. It does not evaluate any expected output specification. -/
def writing (s : World) (tx : Tx) (lock : Option Nat) : World :=
  { s with
    core := { RegistrationUpdate.updateTx s.core 0 tx with
      core := { s.core.core with lock } } }
def preflight : World := writing start
  { start.core.updates 0 with phase := .preflight, authenticated := some first } none
def begun : World := writing preflight { preflight.core.updates 0 with phase := .begun } none
def metadataWritten : World := writing begun
  { begun.core.updates 0 with
    phase := .metadataWritten
    staged := (begun.core.updates 0).staged ++ [.deploymentMetadata (begun.core.updates 0).request.metadata] }
  (some 0)
def ready : World := writing metadataWritten
  { metadataWritten.core.updates 0 with
    phase := .ready
    matched := some first
    outcome := .updated
    staged := (metadataWritten.core.updates 0).staged ++
      [.matchedRow (metadataWritten.core.updates 0).request.client
        (RegistrationUpdate.patched first (metadataWritten.core.updates 0).request)] }
  (some 0)
def auditBegun : World := RegistrationAuditedUpdate.setCursor ready 0
  (cursor (RegistrationAuditedUpdate.sourceProgram source (ready.core.updates 0).request))
def cursor0 : Cursor String Draft Nat :=
  cursor (RegistrationAuditedUpdate.sourceProgram source (ready.core.updates 0).request)
/-- Bounded fixture helper reads the ACTUAL cursor. Empty means no append, which
lets an omitted-program control retain a legal execution instead of hardcoding an
extra Update call that the mutated source did not request. -/
def appendPending (s : World) (record : Record Draft Nat) : World :=
  match s.cursors 0 with
  | none => s
  | some current =>
    match current.remaining with
    | [] => s
    | _ :: remaining => RegistrationAuditedUpdate.setCursor s 0 (advance current remaining record)
def appended : World := appendPending auditBegun updateRecord
def finalCursor : Cursor String Draft Nat := (appended.cursors 0).getD (cursor [])
def committing : World := writing appended { appended.core.updates 0 with phase := .committing } (some 0)
def committed : World :=
  { committing with
    core := RegistrationUpdate.committedState committing.core 0
    audit := overlay encoding committing.audit finalCursor.acknowledged }
def acknowledged : World := writing committed { committed.core.updates 0 with response := .success } none

/-- Independent complete expected values: no patched/commit/overlay/sourceProgram
or sequentialUpdate executor is used in their definitions. -/
def expectedFirst : Row :=
  ⟨30, 40,
    (⟨"fixture-service", "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v2;version=2"⟩,
      "installation:retained-deployment-binding"),
    ⟨"opaque-signed-statement:first-after", "opaque-token-digest:first", 11, 99,
      ⟨"retained-deployment", "fixture-service", none,
        "complete-stable:first;created=5;remainder=retained"⟩⟩⟩
def expectedDB : DB :=
  { metadata :=
      (⟨"fixture-service", "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v2;version=2"⟩,
        "installation:retained-deployment-binding")
    registrations := fun key => if key = 30 then some expectedFirst else if key = 31 then
      some ⟨31, 41,
        (⟨"fixture-service", "complete-metadata:key=fixture;endpoint=https://fixture.invalid/v1;version=1"⟩,
          "installation:retained-deployment-binding"),
        ⟨"opaque-signed-statement:second-before", "opaque-token-digest:second", 21, 22,
          ⟨"retained-deployment", "fixture-service", some 23,
            "complete-stable:second;created=6;remainder=retained"⟩⟩⟩ else none
    logins := fun login => if login = 40 then some "first-registration-role"
      else if login = 41 then some "second-registration-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("complete-unrelated-application", 73) }
def expectedAudit : Audit := fun id =>
  if id = 100 then some updateRecord else if id = 900 then some priorRecord else none
def expectedCursor : Cursor String Draft Nat := ⟨[⟨.update, "fixture-service"⟩], [], [updateRecord]⟩
def expectedWrites : List (RegistrationUpdate.Write Nat Nat Metadata String String String Nat Stable) :=
  [.deploymentMetadata nextMetadata, .matchedRow 30 expectedFirst]
def expectedTx : Tx :=
  { request := request
    phase := .committed
    authenticated := some first
    matched := some first
    staged := expectedWrites
    outcome := .updated
    response := .success }

/-- Every modeled committed-data field plus full audit, actor/other transaction
maps, exact response, lock, commit-event and cursor outputs are visible. -/
def UpdateAccepted : Prop :=
  acknowledged.core.core.db = expectedDB ∧ acknowledged.audit = expectedAudit ∧
    acknowledged.core.core.lock = none ∧
    acknowledged.core.updates = put (fun _ => ({ request } : Tx)) 0 expectedTx ∧
    acknowledged.core.core.transactions =
      (fun id => ({ request := registrations id } :
        RegistrationTransaction.Transaction Nat Nat (Metadata × String) Payload String)) ∧
    acknowledged.core.core.commits = [] ∧
    acknowledged.core.events = [.update 0 expectedWrites] ∧
    acknowledged.cursors = put (fun _ => none) 0 (some expectedCursor)

end RegistrationAuditedUpdate.Witnesses
