import FactoryWatcherFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher.Fixture

set_option maxRecDepth 4096 in
/-- Unchanged standalone positive execution assertion. Only the fixture/model
import graph is used; this is not a reuse of a generic history proof artifact.
The incoming update must be accepted using the current applied parent program. -/
theorem current_parent_program_accepts : ProgramAdvanceHistory := by
  unfold ProgramAdvanceHistory FullValues
  decide

#print axioms current_parent_program_accepts
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``current_parent_program_accepts
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected standalone watcher acceptance axiom: {unexpected}"

end FactoryRetarget.Watcher.Fixture
