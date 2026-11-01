import FactorySavedParentFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.SavedParent.Fixture
open FactoryRetarget.Watcher.Fixture

/-- Actual existing-row input, not initial allocation or an expected final row. -/
theorem initial_row_present : start.watcher.factory.rows 1 = some initial := by
  simp [start, initialState]

set_option maxRecDepth 4096 in
/-- A real normal parent2/B application precedes the delayed old source success.
The explicit complete B records survive the rejected retry. -/
theorem populated_newer_then_old_success : AcceptedThenOldSuccess := by
  unfold AcceptedThenOldSuccess BValues ChildSucceeded Discarded
  decide

set_option maxRecDepth 4096 in
/-- A refused best-effort diagnostic clear leaves the represented full old row.
Neither obsolete source outcome can use that row to authorize replay. -/
theorem populated_failed_clear_then_old_results : FailedClearThenOldResults := by
  unfold FailedClearThenOldResults BValues ChildSucceeded Discarded
  decide

set_option maxRecDepth 4096 in
/-- Actual B success and current error3 publication precede the obsolete error1.
The committed full preparation payload remains despite its lost reply. -/
theorem populated_current_error_then_old_error : AcceptedErrorThenOldError := by
  unfold AcceptedErrorThenOldError BValues ChildSucceeded Discarded
  decide

set_option maxRecDepth 4096 in
/-- New parent ID9 really applies on the existing deployment at the reused name.
The old ID's deleted native observation prevents all saved work from replaying. -/
theorem populated_replacement_then_deleted_old : ReplacementThenDeletedOld := by
  unfold ReplacementThenDeletedOld BValues ChildSucceeded Discarded
  decide

set_option maxRecDepth 4096 in
/-- Both statuses are absent after normal success, yet the same-head retry still
continues and its child succeeds. KEEP retains the prior native image/request,
even though this second preparation built a different image. -/
theorem populated_cleared_then_valid_duplicate : ClearedThenValidDuplicate := by
  unfold ClearedThenValidDuplicate BValues ChildSucceeded
  decide

#print axioms populated_newer_then_old_success
#print axioms populated_failed_clear_then_old_results
#print axioms populated_current_error_then_old_error
#print axioms populated_replacement_then_deleted_old
#print axioms populated_cleared_then_valid_duplicate

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``ready_keeps_watcher, ``runAtOne, ``initial_row_present,
      ``populated_newer_then_old_success, ``populated_failed_clear_then_old_results,
      ``populated_current_error_then_old_error, ``populated_replacement_then_deleted_old,
      ``populated_cleared_then_valid_duplicate] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected populated saved-parent axiom in {declaration}: {unexpected}"

end FactoryRetarget.SavedParent.Fixture
