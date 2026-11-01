import FactoryControlFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.FactoryControlFixture

set_option maxRecDepth 4096 in
/-- Standalone populated modeled execution assertion, not an extracted Rust or
native test. All expected row/provider/request/event values are independent. -/
theorem exact_capacity_accepts :
    ExpectedBoundary (apply 1 initial requested (boundaryObservations) (state render)) := by
  unfold ExpectedBoundary
  decide

#print axioms exact_capacity_accepts
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``exact_capacity_accepts
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected standalone Factory control axiom: {unexpected}"

end FactoryRetarget.FactoryControlFixture
