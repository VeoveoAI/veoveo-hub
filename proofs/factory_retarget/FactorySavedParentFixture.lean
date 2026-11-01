import FactorySavedParentModel
import FactoryWatcherFixture

/-! Synthetic existing-row histories of the manual saved-parent layer. This import
graph contains executable models and public fixture data only, no correctness
Foundation/History leaf. Old diagnostics are initial represented data; their
creation and missing-row allocation are not claimed. Prepared/source/native/SQL
observations retain all inherited interpretation contracts. In particular a later
ready read is separate from the prior provisioning acknowledgement, and submitted
branch@LATEST argv proves no checkout bytes, remote readiness or model inference.
- BOTH source/data and expected full records are separate from the executors.
- New-ID replacement reuses a real represented row; it is not initial allocation.
- Public raw/digest/script data reuse the earlier audited synthetic fixture. -/
namespace FactoryRetarget.SavedParent.Fixture
open Watcher
open FactoryRetarget.Watcher.Fixture

/-- Structural row-evidence plumbing only. No expected output appears in its
premises; it remains true if the admission rule is deliberately faulted later. -/
theorem ready_keeps_watcher (attempt : Attempt) (o : Observations Unit)
    (s : SavedState Unit) (work : Ready Unit) (selected : enter attempt o s = .ready work) :
    work.state.watcher = s.watcher := by
  cases origin : attempt.origin <;> cases active : o.afterLockCancelled <;>
    cases native : o.nativeBranch <;> cases source : attempt.source <;>
    simp [enter, origin, active, native, handleSource, source] at selected
  all_goals try split_ifs at selected
  all_goals simp_all [publishPreparation]
  all_goals rw [← selected]
  all_goals rfl

/-- Uses actual before-row evidence, and invokes the unchanged enter/complete
model. No desired Ready/Request/after-state is supplied. -/
def runAtOne (attempt : Attempt) (o : Observations Unit) (s : SavedState Unit)
    (backing : Row) (bound : s.watcher.factory.rows 1 = some backing) : SavedOutcome Unit :=
  finishExisting 1 backing attempt o s (by
    intro work selected
    rw [ready_keeps_watcher attempt o s work selected]
    exact bound)

def parentKey : ParentKey := ⟨parentRepository, "proof/retarget"⟩
def prepKey : PreparationKey := ⟨uuid, parentKey⟩
def otherPrepKey : PreparationKey := ⟨uuid, ⟨parentRepository, "proof/unrelated"⟩⟩
def oldPrepFailure : PreparationFailure := ⟨revision1, alice, "initial represented parent read failure", 7⟩
def otherPrepFailure : PreparationFailure := ⟨revision2, frank, "unrelated parent preparation failed", 9⟩

def start : SavedState Unit where
  watcher := initialState
  preparations := fun key => if key = prepKey then some oldPrepFailure else
    if key = otherPrepKey then some otherPrepFailure else none

def input1 : Input := ⟨uuid, parentKey, parentBranchId, revision1, alice⟩
def input2 : Input := ⟨uuid, parentKey, parentBranchId, revision2, frank⟩
def input3 : Input := ⟨uuid, parentKey, parentBranchId, String.ofList (List.replicate 64 '3'), frank⟩
def input9 : Input := ⟨uuid, parentKey, String.ofList (List.replicate 32 '9'), revision2, frank⟩

def source1 : LinkedSource := ⟨"team-a", repositoryA, linkedC, false, oldTree, oldEntries⟩
def source2 : LinkedSource := ⟨"team-a", repositoryB, linkedD, false, newTree, newEntries⟩

def branch2 : NativeBranch := ⟨parentBranchId, "proof/retarget", false, revision2⟩
def branch3 : NativeBranch := ⟨parentBranchId, "proof/retarget", false, input3.revision⟩
def branch9 : NativeBranch := ⟨input9.branchId, "proof/retarget", false, revision2⟩
def deletedOldBranch : NativeBranch := ⟨parentBranchId, "proof/retarget", true, revision1⟩

def clearError : Error := ⟨"fixture-clear", "the actual link-status clear was refused"⟩
def lostPreparationReply : Error := ⟨"fixture-preparation-sql", "preparation publication reply was lost"⟩
def error3 : String := "reading parent revision3 as Frank failed: fixture source transport error"
def oldError : String := "reading old parent revision1 as Alice failed: obsolete fixture read error"

def childObservation (s : SavedState Unit) (target : Target)
    (before after : CreateRequest Unit) (publication : Publication) : ParentObservations Unit where
  query := .ok ((s.watcher.factory.rows 1).bind Watcher.project)
  resolve := fun _ _ => .ok ⟨target, observations before after⟩
  recorded := fun _ _ => none
  cancellation := uncancelled
  publication := publication

def parentObservation (s : SavedState Unit) (failedClear : Bool) : Observations Unit where
  afterLockCancelled := false
  afterNativeCancelled := false
  nativeBranch := .ok branch2
  preparationSQL := ⟨true, .ok (), 19⟩
  link := childObservation s ⟨repositoryB, branchB, "release", dave, linkedD⟩ oldRequest retargetRequest
    (if failedClear then ⟨false, .error clearError, 23⟩ else ⟨true, .ok (), 23⟩)

def oldObservation (s : SavedState Unit) (native : NativeBranch) : Observations Unit where
  afterLockCancelled := false
  afterNativeCancelled := false
  nativeBranch := .ok native
  preparationSQL := ⟨true, .ok (), 31⟩
  link := childObservation s ⟨repositoryA, branchA, "main", carol, linkedC⟩ retargetRequest oldRequest ⟨true, .ok (), 31⟩

def errorObservation (s : SavedState Unit) : Observations Unit :=
  { parentObservation s false with
    nativeBranch := .ok branch3
    preparationSQL := ⟨true, .error lostPreparationReply, 43⟩ }

def duplicateRequest : CreateRequest Unit := expectedRequest repositoryB branchB newEntries (image 'c')
def duplicateObservation (s : SavedState Unit) : Observations Unit :=
  { parentObservation s false with
    link := childObservation s ⟨repositoryB, branchB, "release", dave, linkedD⟩
      retargetRequest duplicateRequest ⟨true, .ok (), 47⟩ }

/-- The exact first operation is run, including internally constructed ParentInput,
preparation-clear and child Factory output. It is not a desired B-state premise. -/
def normalB (failedClear : Bool) : SavedOutcome Unit :=
  runAtOne (begin input2 .notification (.ok source2)) (parentObservation start failedClear) start initial
    (by simp [start, initialState])

def observedRow (before : SavedOutcome Unit) : Row :=
  (before.state.watcher.factory.rows 1).getD initial

set_option maxRecDepth 4096 in
def delayedOk (first : SavedOutcome Unit) (native : NativeBranch)
    (present : first.state.watcher.factory.rows 1 = some (observedRow first)) : SavedOutcome Unit :=
  runAtOne (begin input1 (.savedRetry alice) (.ok source1)) (oldObservation first.state native)
    first.state (observedRow first) present

set_option maxRecDepth 4096 in
def delayedError (first : SavedOutcome Unit) (native : NativeBranch)
    (present : first.state.watcher.factory.rows 1 = some (observedRow first)) : SavedOutcome Unit :=
  runAtOne (begin input1 (.savedRetry alice) (.error oldError)) (oldObservation first.state native)
    first.state (observedRow first) present

set_option maxRecDepth 4096 in
def newPreparationError : SavedOutcome Unit :=
  let first := normalB false
  runAtOne (begin input3 (.savedRetry alice) (.error error3)) (errorObservation first.state)
    first.state (observedRow first) (by decide)

def normalReplacement : SavedOutcome Unit :=
  runAtOne (begin input9 .notification (.ok source2))
    { parentObservation start false with nativeBranch := .ok branch9 }
    start initial (by simp [start, initialState])

set_option maxRecDepth 4096 in
def sameHeadDuplicate : SavedOutcome Unit :=
  let first := normalB false
  runAtOne (begin input2 (.savedRetry alice) (.ok source2)) (duplicateObservation first.state)
    first.state (observedRow first) (by decide)

set_option maxRecDepth 4096 in
/-- Presence is checked only for these closed actual preceding executions,
never for arbitrary outcomes and never from an expected B-row premise. -/
def oldAfterB : SavedOutcome Unit := delayedOk (normalB false) branch2 (by decide)

set_option maxRecDepth 4096 in
def oldAfterUncleared : SavedOutcome Unit := delayedOk (normalB true) branch2 (by decide)

set_option maxRecDepth 4096 in
def oldErrorAfterUncleared : SavedOutcome Unit := delayedError oldAfterUncleared branch2 (by decide)

set_option maxRecDepth 4096 in
def oldAfterError : SavedOutcome Unit := delayedError newPreparationError branch3 (by decide)

set_option maxRecDepth 4096 in
def oldAfterReplacement : SavedOutcome Unit := delayedOk normalReplacement deletedOldBranch (by decide)

/-- Independent complete modeled values: explicit public fixture records, never
successRow/configuration/parentRequest or an executor's expected-state fold. -/
def BValues (output : SavedOutcome Unit) (native : Option (Sandbox Unit))
    (diagnostic : Option Failure) (preparation : Option PreparationFailure) : Prop :=
  output.state.watcher.factory.rows 1 = some retargetedRow ∧
  output.state.watcher.factory.sandboxes 1 = native ∧
  output.state.watcher.factory.providers 1 = expectedProviders frank repositoryB dave ∧
  output.state.watcher.factory.createRequests 1 = [retargetRequest] ∧
  (output.state.watcher.factory.rows 1).map client = some retargetedClient ∧
  (output.state.watcher.factory.createRequests 1).map (fun request => request.spec.command) =
    [expectedArgv repositoryB branchB newEntries] ∧
  output.state.watcher.failures failureKey = diagnostic ∧
  output.state.watcher.failures unrelatedKey = some unrelatedFailure ∧
  output.state.preparations prepKey = preparation ∧
  output.state.preparations otherPrepKey = some otherPrepFailure

def ChildSucceeded (output : SavedOutcome Unit) : Prop :=
  output.exit = .continued ∧ output.child.isSome = true ∧
  (output.child.bind (fun child => child.factoryOutcome)).isSome = true ∧
  (output.child.bind (fun child => child.factoryOutcome)).bind (fun applied => applied.error) = none

def Discarded (output : SavedOutcome Unit) : Prop :=
  output.exit = .discarded ∧ output.child.isNone = true

/-- Actual parent2/B then delayed old success. All B values are independent. -/
def AcceptedThenOldSuccess : Prop :=
  let first := normalB false
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) none none ∧
  Discarded oldAfterB ∧
  BValues oldAfterB (expectedNative retargetRequest) none none

/-- The initial old diagnostic is explicitly represented data, not newly created
by this fixture. An explicit noncommitted clear observation retains its whole payload. -/
def FailedClearThenOldResults : Prop :=
  let first := normalB true
  let afterOk := oldAfterUncleared
  let afterError := oldErrorAfterUncleared
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) (some oldFailure) none ∧
  Discarded afterOk ∧ BValues afterOk (expectedNative retargetRequest) (some oldFailure) none ∧
  Discarded afterError ∧ BValues afterError (expectedNative retargetRequest) (some oldFailure) none

def expectedError3 : PreparationFailure := ⟨input3.revision, frank, error3, 43⟩

/-- B2 success -> actual current error3 publication -> old error1 discard. The
full newer payload survives despite its lost SQL reply; B's outputs also remain. -/
def AcceptedErrorThenOldError : Prop :=
  let first := normalB false
  let next := newPreparationError
  let last := oldAfterError
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) none none ∧
  next.exit = .sourceRefused error3 ∧ next.child.isNone = true ∧
  BValues next (expectedNative retargetRequest) none (some expectedError3) ∧
  Event.preparationStatusReply (.error lostPreparationReply) ∈ next.trace ∧
  Event.statusWarning lostPreparationReply ∈ next.trace ∧
  Discarded last ∧ BValues last (expectedNative retargetRequest) none (some expectedError3)

/-- Replacement is ACTUALLY applied using new parent ID9 under the same existing
row/name. The old deleted branch still has name+head1, and is discarded. -/
def ReplacementThenDeletedOld : Prop :=
  let first := normalReplacement
  let last := oldAfterReplacement
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) none none ∧
  Event.sourceAttempt input9 ∈ first.trace ∧
  Discarded last ∧ BValues last (expectedNative retargetRequest) none none ∧
  Event.nativeAttempt alice parentRepository parentBranchId ∈ last.trace ∧
  Event.nativeReply (.ok deletedOldBranch) ∈ last.trace

/-- Same source/head, distinct rebuilt image. KEEP retains the first native image
and request/providers, but an actual child application still succeeds. -/
def ClearedThenValidDuplicate : Prop :=
  let first := normalB false
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) none none ∧
  ChildSucceeded sameHeadDuplicate ∧
  BValues sameHeadDuplicate (some ⟨some retargetRequest.spec, .ready, 0, ()⟩) none none ∧
  (retargetRequest.spec.template.map (fun template => template.image)) ≠
    (duplicateRequest.spec.template.map (fun template => template.image))

end FactoryRetarget.SavedParent.Fixture
