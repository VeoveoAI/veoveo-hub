import FactoryControlFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.FactoryControlFixture

set_option maxRecDepth 4096 in
/-- Standalone populated modeled execution assertion, not an extracted Rust or
native test. All expected row/provider/request/event values are independent. -/
theorem retarget_complete_result :
    ExpectedRetarget (apply 1 initial requested (observations render) (state render)) := by
  unfold ExpectedRetarget
  decide

#print axioms retarget_complete_result
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``retarget_complete_result
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected standalone Factory control axiom: {unexpected}"

end FactoryRetarget.FactoryControlFixture
