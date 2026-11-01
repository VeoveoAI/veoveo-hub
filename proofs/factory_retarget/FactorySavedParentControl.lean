import FactorySavedParentFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.SavedParent.Fixture

set_option maxRecDepth 4096 in
/-- Standalone positive execution assertion: preserve newer B against obsolete
successful preparation AND accept a valid same-head duplicate after both clears.
No correctness proof artifact participates in this fixture/model import graph. -/
theorem saved_parent_preserves_and_accepts :
    AcceptedThenOldSuccess ∧ ClearedThenValidDuplicate := by
  constructor
  · unfold AcceptedThenOldSuccess BValues ChildSucceeded Discarded
    decide
  · unfold ClearedThenValidDuplicate BValues ChildSucceeded
    decide

#print axioms saved_parent_preserves_and_accepts
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``saved_parent_preserves_and_accepts
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected standalone saved-parent control axiom: {unexpected}"

end FactoryRetarget.SavedParent.Fixture
