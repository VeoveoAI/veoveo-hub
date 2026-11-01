import RegistrationLifetimeSpecification
import Lean.Util.CollectAxioms

/-! Constructive synthetic model executions. Native identifiers, SQL/crypto/input
admission and full API response payloads remain explicit interpretation boundaries.
No generic correctness leaf is imported. -/
namespace RegistrationLifetime.Witnesses
open RegistrationLifetime
open RegistrationTransaction (put rowMap)

abbrev Row := RegistrationTransaction.Registration Nat Nat String String
abbrev DB := Database Nat Nat String String String String String (String × Nat)
abbrev World := State Nat Nat String String String String String (String × Nat)
abbrev Req := Request Nat Nat String String String String

def instancePayload : String := "full-instance-fixture:deployment=01;database=installation;factory=retained"
def metadata : String := "full-metadata-fixture:slug=observatory-fixture;key=new-key;binding=shared;version=2"
def payload : String := "full-row-fixture:endpoint=https://fixture.invalid/mcp;credential=public-synthetic;issued=23"
def candidate : Row := ⟨10, 20, metadata, payload⟩
def proposal : Proposal Nat Nat String String String String :=
  { candidate, role := "candidate-role", firstDeployment := "first-deployment-stable", externalEnvironment := false }
def unrelatedRoles : Nat → Option String := fun login => if login = 99 then some "unrelated-role" else none
def seedDB : DB :=
  { instanceRow := some instancePayload
    deployment := none
    registrations := fun _ => none
    logins := unrelatedRoles
    frame := ("unrelated-application", 73) }
def requests : Nat → Req := fun id => if id = 0 then .register proposal else .remove
def start : World := initial seedDB requests

/-- Independent complete first-registration result, including opaque whole payloads. -/
def admittedDB : DB :=
  { instanceRow := some instancePayload
    deployment := some ⟨metadata, "first-deployment-stable"⟩
    registrations := fun key => if key = 10 then some ⟨10, 20, metadata, payload⟩ else none
    logins := fun login => if login = 20 then some "candidate-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("unrelated-application", 73) }
def originalOrphanDB : DB := { admittedDB with instanceRow := none }
def emptyRemovedDB : DB :=
  { instanceRow := none
    deployment := none
    registrations := fun _ => none
    logins := unrelatedRoles
    frame := ("unrelated-application", 73) }
def admittedThenRemovedDB : DB :=
  { instanceRow := none
    deployment := some ⟨metadata, "first-deployment-stable"⟩
    registrations := fun _ => none
    logins := unrelatedRoles
    frame := ("unrelated-application", 73) }

/-- Fixture-only named source states, each corresponding to one existing Step.
These definitions do not execute an independent spec or add a safety guard. -/
def cached (s : World) := update s 0 { s.transactions 0 with phase := .preflight, cachedAuthority := some instancePayload }
def waitingRegister (s : World) := update s 0 { s.transactions 0 with phase := .waiting }
def selectedRegister (fixed : Bool) (s : World) : World :=
  if fixed then
    { update s 0 { s.transactions 0 with
        phase := afterLock (s.transactions 0).request
        lockedInstance := some instancePayload } with instanceOwner := some 0 }
  else update s 0 { s.transactions 0 with phase := afterLock (s.transactions 0).request }
def preparedRegister (s : World) := update s 0 { s.transactions 0 with phase := .upserting }
def upsertedRegister (s : World) : World :=
  { update s 0 { s.transactions 0 with
      phase := .deleting
      staged := (s.transactions 0).staged ++ [.upsertDeployment metadata "first-deployment-stable"] }
    with deploymentOwner := some 0 }
def capturedRows (s : World) (id : Nat) (rows : List Row) :=
  update s id { s.transactions id with
    phase := .dropping
    captured := rows
    drops := rows.map (·.login)
    staged := (s.transactions id).staged ++ [.registration (.deleteRows (rows.map (·.client)))] }
def droppedOne (s : World) (id login : Nat) :=
  update s id { s.transactions id with
    drops := []
    staged := (s.transactions id).staged ++ [.registration (.dropLogin login)] }
def creatingRegister (s : World) := update s 0 { s.transactions 0 with phase := .creating }
def createdRegister (s : World) := update s 0 { s.transactions 0 with
  phase := .inserting
  staged := (s.transactions 0).staged ++ [.registration (.createLogin 20 "candidate-role")] }
def insertedRegister (s : World) := update s 0 { s.transactions 0 with
  phase := .ready
  staged := (s.transactions 0).staged ++ [.registration (.insert candidate)] }
def committing (s : World) (id : Nat) := update s id { s.transactions id with phase := .committing }
def acknowledged (s : World) (id : Nat) := update s id { s.transactions id with response := successResponse (s.transactions id).request }
def waitingRemove (s : World) := update s 1 { s.transactions 1 with phase := .waiting }
def selectedRemove (fixed : Bool) (s : World) : World :=
  if fixed then
    { update s 1 { s.transactions 1 with
        phase := afterLock (s.transactions 1).request
        lockedInstance := some instancePayload } with instanceOwner := some 1 }
  else update s 1 { s.transactions 1 with phase := afterLock (s.transactions 1).request }
def erasingRemove (s : World) := update s 1 { s.transactions 1 with phase := .erasing }
def erasedRemove (s : World) : World :=
  { update s 1 { s.transactions 1 with
      phase := .ready
      lockedInstance := some instancePayload
      staged := (s.transactions 1).staged ++ [.deleteInstance] }
    with instanceOwner := some 1 }
def absentRegister (s : World) := update s 0 { s.transactions 0 with phase := .absent }
def returnedAbsent (s : World) : World :=
  { update s 0 { s.transactions 0 with phase := .aborted, response := .invalidToken }
    with instanceOwner := release s.instanceOwner 0, deploymentOwner := release s.deploymentOwner 0 }
def failedRemove (s : World) := update s 1 { s.transactions 1 with
  phase := .abortPending, response := .error, error := some "synthetic server refusal before commit" }
def abortedRemove (s : World) : World :=
  { update s 1 { s.transactions 1 with phase := .aborted }
    with instanceOwner := release s.instanceOwner 1, deploymentOwner := release s.deploymentOwner 1 }
def deliveryError (s : World) := update s 0 { s.transactions 0 with
  response := .error, error := some "synthetic lost commit response" }

/-- Registration pauses AFTER private upsert/DELETE/CREATE, BEFORE INSERT. -/
def beforeInsert (fixed : Bool) (s : World) : World :=
  createdRegister (creatingRegister (capturedRows
    (upsertedRegister (preparedRegister (selectedRegister fixed (waitingRegister (cached s))))) 0 []))
def finishRegister (s : World) : World := committedState (committing (insertedRegister s) 0) 0
/-- Empty removal still obtains the source's real late instance DELETE row lock. -/
def emptyRemovalReady (fixed : Bool) (s : World) : World :=
  erasedRemove (erasingRemove (capturedRows (selectedRemove fixed (waitingRemove s)) 1 []))
def finishEmptyRemoval (fixed : Bool) (s : World) := committedState (committing (emptyRemovalReady fixed s) 1) 1
def oneRemovalReady (s : World) : World :=
  erasedRemove (erasingRemove (droppedOne (capturedRows (selectedRemove true (waitingRemove s)) 1 [candidate]) 1 20))
def finishOneRemoval (s : World) := committedState (committing (oneRemovalReady s) 1) 1

/-- Both public requests and unrelated fixture data are actual populated inputs. -/
theorem seed_valid : InitialValid seedDB := by
  simp [InitialValid, SQLValid, NoOrphanRows, seedDB]

theorem execution_trans {fixed : Bool} {a b c : World}
    (left : Execution fixed a b) (right : Execution fixed b c) : Execution fixed a c := by
  induction right with
  | refl => exact left
  | tail previous step ih => exact Execution.tail ih step

/-- Closed scenario endpoints: the expected databases above are independent of
these operation-state definitions and are not premises of their executions. -/
def originalRemoval := finishEmptyRemoval false (beforeInsert false start)
def originalFinal := acknowledged (acknowledged (finishRegister originalRemoval) 1) 0
def fixedRegistration := finishRegister (beforeInsert true start)
def fixedFinal := acknowledged (acknowledged (finishOneRemoval fixedRegistration) 1) 0
def cachedBeforeRemoval := waitingRegister (cached start)
def removedBeforeAdmission := finishEmptyRemoval true cachedBeforeRemoval
def absentFinal := returnedAbsent (absentRegister (acknowledged removedBeforeAdmission 1))
def removalAborted := abortedRemove (failedRemove (emptyRemovalReady true start))
def rollbackFinal := acknowledged (finishRegister (beforeInsert true removalAborted)) 0
def lostReplyFinal := deliveryError fixedRegistration

/-- Assertions will retain complete stored effects, both independent reply tags
and actual commit order, rather than test only a success Boolean. -/
def OriginalOrphan : Prop :=
  originalFinal.db = originalOrphanDB ∧
  (originalFinal.transactions 0).response = .registered ∧
  (originalFinal.transactions 1).response = .removed ∧
  originalFinal.instanceOwner = none ∧ originalFinal.deploymentOwner = none ∧
  originalFinal.commits =
    [(1, [.registration (.deleteRows []), .deleteInstance]),
      (0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows []),
        .registration (.createLogin 20 "candidate-role"), .registration (.insert candidate)])]
def FixedAcceptedThenRemoved : Prop :=
  fixedRegistration.db = admittedDB ∧ fixedFinal.db = admittedThenRemovedDB ∧
  (fixedFinal.transactions 0).response = .registered ∧
  (fixedFinal.transactions 1).response = .removed ∧
  fixedFinal.instanceOwner = none ∧ fixedFinal.deploymentOwner = none ∧
  fixedFinal.commits =
    [(0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows []),
        .registration (.createLogin 20 "candidate-role"), .registration (.insert candidate)]),
      (1, [.registration (.deleteRows [10]), .registration (.dropLogin 20), .deleteInstance])]
def CachedAuthorityCannotAdmit : Prop :=
  absentFinal.db = emptyRemovedDB ∧ (absentFinal.transactions 0).cachedAuthority = some instancePayload ∧
  (absentFinal.transactions 0).response = .invalidToken ∧
  (absentFinal.transactions 0).staged = [] ∧
  (absentFinal.transactions 1).response = .removed ∧
  absentFinal.instanceOwner = none ∧ absentFinal.deploymentOwner = none ∧
  absentFinal.commits = [(1, [.registration (.deleteRows []), .deleteInstance])]
def RollbackThenAccepted : Prop :=
  removalAborted.db = seedDB ∧ removalAborted.commits = [] ∧
  removalAborted.instanceOwner = none ∧ removalAborted.deploymentOwner = none ∧
  rollbackFinal.db = admittedDB ∧ (rollbackFinal.transactions 1).phase = .aborted ∧
  (rollbackFinal.transactions 1).response = .error ∧
  (rollbackFinal.transactions 1).error = some "synthetic server refusal before commit" ∧
  rollbackFinal.instanceOwner = none ∧ rollbackFinal.deploymentOwner = none ∧
  (rollbackFinal.transactions 0).response = .registered ∧
  rollbackFinal.commits =
    [(0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows []),
      .registration (.createLogin 20 "candidate-role"), .registration (.insert candidate)])]
def LostReplyStillCommitted : Prop :=
  lostReplyFinal.db = admittedDB ∧ (lostReplyFinal.transactions 0).phase = .committed ∧
  (lostReplyFinal.transactions 0).response = .error ∧
  (lostReplyFinal.transactions 0).error = some "synthetic lost commit response" ∧
  lostReplyFinal.instanceOwner = none ∧ lostReplyFinal.deploymentOwner = none ∧
  lostReplyFinal.commits =
    [(0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows []),
      .registration (.createLogin 20 "candidate-role"), .registration (.insert candidate)])]

/-- A real replacement has a committed prior row/login and immutable deployment
remainder; the proposal's first-insertion value must NOT replace that remainder. -/
def priorRow : Row := ⟨30, 40, "old-full-metadata", "old-full-registration-row"⟩
def replacementSeed : DB :=
  { instanceRow := some instancePayload
    deployment := some ⟨"old-full-metadata", "retained-old-stable"⟩
    registrations := put (fun _ => none) 30 (some priorRow)
    logins := fun login => if login = 40 then some "prior-role" else unrelatedRoles login
    frame := ("unrelated-application", 73) }
def replacementStart : World := initial replacementSeed requests
def beforeReplacement : World :=
  createdRegister (creatingRegister (droppedOne (capturedRows
    (upsertedRegister (preparedRegister (selectedRegister true (waitingRegister (cached replacementStart)))))
      0 [priorRow]) 0 40))
def replacementFinal := acknowledged (finishRegister beforeReplacement) 0
def expectedReplacement : DB :=
  { instanceRow := some instancePayload
    deployment := some ⟨metadata, "retained-old-stable"⟩
    registrations := fun key => if key = 10 then some ⟨10, 20, metadata, payload⟩ else none
    logins := fun login => if login = 20 then some "candidate-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("unrelated-application", 73) }
def ReplacementAccepted : Prop :=
  replacementFinal.db = expectedReplacement ∧
  (replacementFinal.transactions 0).request = .register proposal ∧
  (replacementFinal.transactions 0).response = .registered ∧
  replacementFinal.instanceOwner = none ∧ replacementFinal.deploymentOwner = none ∧
  replacementFinal.commits =
    [(0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows [30]),
      .registration (.dropLogin 40), .registration (.createLogin 20 "candidate-role"),
      .registration (.insert candidate)])]

end RegistrationLifetime.Witnesses
