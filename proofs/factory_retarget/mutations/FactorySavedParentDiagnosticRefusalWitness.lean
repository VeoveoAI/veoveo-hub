import FactorySavedParentFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.SavedParent.Fixture
open Watcher
open FactoryRetarget.Watcher.Fixture

/-- All native freshness coordinates match and immutable preparation succeeds.
The preceding normal operation actually cleared BOTH diagnostic keys. Requiring
its matching LINK diagnostic invents authority: the valid duplicate is refused,
no child is invoked, and the complete first B state remains unmodified. -/
def WrongDiagnosticRefusal : Prop :=
  let first := normalB false
  let second := sameHeadDuplicate
  ChildSucceeded first ∧ BValues first (expectedNative retargetRequest) none none ∧
  branch2.id = input2.branchId ∧ branch2.name = input2.parent.branch ∧
  branch2.deleted = false ∧ branch2.latest = input2.revision ∧
  first.state.watcher.failures
    ⟨input2.watchedDeployment, ⟨input2.parent.repository, input2.parent.branch, source2.linkName⟩⟩ = none ∧
  first.state.preparations prepKey = none ∧
  Discarded second ∧ (second.child.bind (fun child => child.factoryOutcome)).isNone = true ∧
  BValues second (expectedNative retargetRequest) none none ∧
  Event.sourceAttempt input2 ∈ second.trace ∧ Event.sourceReply (.ok ()) ∈ second.trace ∧
  Event.nativeAttempt alice parentRepository parentBranchId ∈ second.trace ∧
  Event.nativeReply (.ok branch2) ∈ second.trace ∧
  second.trace.all (fun event => match event with | .linkRegion _ => false | _ => true) = true ∧
  (retargetRequest.spec.template.map (fun template => template.image)) ≠
    (duplicateRequest.spec.template.map (fun template => template.image))

set_option maxRecDepth 4096 in
/-- False on baseline. The isolated diagnostic-required model proves actual
wrong refusal, not a missing import, theorem rewrite or compiler failure. -/
theorem diagnostic_presence_refuses_valid_complete_duplicate : WrongDiagnosticRefusal := by
  unfold WrongDiagnosticRefusal BValues ChildSucceeded Discarded
  decide

#print axioms diagnostic_presence_refuses_valid_complete_duplicate
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``diagnostic_presence_refuses_valid_complete_duplicate
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected saved diagnostic-refusal witness axiom: {unexpected}"

end FactoryRetarget.SavedParent.Fixture
