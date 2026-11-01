import FactorySavedParentFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.SavedParent.Fixture
open Watcher
open FactoryRetarget.Watcher.Fixture

def expectedCurrentB : Projection :=
  ⟨runnerKey, repositoryB, branchB, "release", some revision2, some frank, some linkedD, some dave⟩

def wrongReplayRow : Row := expectedRow repositoryA branchA "main" revision1 alice linkedC carol oldTree
def wrongReplayClient : Client := ⟨1, some alice, parentRepository, some repositoryA, some carol⟩
def oldResolvedInput : ParentInput :=
  ⟨runnerKey, uuid, repositoryA, parentBranchId, linkedC, false, revision1, alice, oldTree, oldEntries⟩

/-- The intact ID/name/live checks pass, but the native head is different.
The actual backing row and current SQL query remain the newer B values. The
head-omission fault then replays old parent1/Alice/A, with every modeled full
output independently specified and both submitted argv values exposed. -/
def WrongHeadReplay : Prop :=
  let first := normalB false
  let second := oldAfterB
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) none none ∧
  observedRow first = retargetedRow ∧
  (oldObservation first.state branch2).link.query = .ok (some expectedCurrentB) ∧
  branch2.id = input1.branchId ∧ branch2.name = input1.parent.branch ∧ branch2.deleted = false ∧
  branch2.latest ≠ input1.revision ∧
  ChildSucceeded second ∧
  second.state.watcher.factory.rows 1 = some wrongReplayRow ∧
  (second.state.watcher.factory.rows 1).map client = some wrongReplayClient ∧
  second.state.watcher.factory.sandboxes 1 = expectedNative oldRequest ∧
  second.state.watcher.factory.providers 1 = expectedProviders alice repositoryA carol ∧
  second.state.watcher.factory.createRequests 1 = [retargetRequest, oldRequest] ∧
  (second.state.watcher.factory.createRequests 1).map (fun request => request.spec.command) =
    [expectedArgv repositoryB branchB newEntries, expectedArgv repositoryA branchA oldEntries] ∧
  second.state.watcher.failures failureKey = none ∧
  second.state.watcher.failures unrelatedKey = some unrelatedFailure ∧
  second.state.preparations prepKey = none ∧
  second.state.preparations otherPrepKey = some otherPrepFailure ∧
  Event.sourceAttempt input1 ∈ second.trace ∧
  Event.nativeReply (.ok branch2) ∈ second.trace ∧
  Trace.logicalResolve oldResolvedInput (some expectedCurrentB) ∈
    (second.child.map (fun child => child.trace)).getD [] ∧
  wrongReplayRow ≠ retargetedRow ∧ wrongReplayClient ≠ retargetedClient ∧
  oldRequest.spec.command ≠ retargetRequest.spec.command

set_option maxRecDepth 4096 in
/-- False on baseline. Only the isolated head-omission model should establish
this concrete full wrong result with an independently checked axiom closure. -/
theorem head_omission_replays_old_complete_output : WrongHeadReplay := by
  unfold WrongHeadReplay BValues ChildSucceeded
  decide

#print axioms head_omission_replays_old_complete_output
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``head_omission_replays_old_complete_output
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected saved-head omission witness axiom: {unexpected}"

end FactoryRetarget.SavedParent.Fixture
