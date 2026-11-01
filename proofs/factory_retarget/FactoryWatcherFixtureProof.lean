import FactoryWatcherFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher.Fixture

/-- Current-read/captured values start from the actual complete original row. -/
theorem initial_projection : project initial = some captured := by decide

theorem initial_runner : Runner initial := by simp [Runner, initial]

/-- The singleton represented SQL state satisfies the actual point-key uniqueness
contract. This is not a blanket assumption about every desired after-state. -/
theorem initial_unique_key : UniqueKey initialState.factory := by
  intro key d₁ d₂ row₁ row₂ first second
  have firstId : d₁ = 1 := by
    by_contra different
    simpa [initialState, different] using first.1
  have secondId : d₂ = 1 := by
    by_contra different
    simpa [initialState, different] using second.1
  have same : d₁ = d₂ := firstId.trans secondId.symm
  have atSame : initialState.factory.rows d₂ = some row₁ := by simpa only [same] using first.1
  exact ⟨same, Option.some.inj (atSame.symm.trans second.1)⟩

/-- Both parent-revision AND parent-pusher coordinates are used by the source
oracle. The crossed combinations refuse rather than quietly returning old data. -/
theorem preparation_is_indexed_by_revision_and_pusher :
    let args := Arguments.mk runnerKey revision1 alice uuid repositoryA linkedE erin branchA
    (prepare args).map (fun source => (source.ownTree, source.ownEntries, source.factory.prepared.image)) =
      .ok (oldTree, oldEntries, image 'd') ∧
    (prepare { args with parentRevision := revision2, parentPusher := frank }).map
      (fun source => (source.ownTree, source.ownEntries, source.factory.prepared.image)) =
      .ok (newTree, newEntries, image 'c') ∧
    (match prepare { args with parentRevision := revision2 } with | .error _ => true | .ok _ => false) = true ∧
    (match prepare { args with parentPusher := frank } with | .error _ => true | .ok _ => false) = true := by
  decide

set_option maxRecDepth 4096 in
/-- Actual A→B parent operation followed by queued old A, retaining complete B
outputs and full client/request values. This is a synthetic modeled history. -/
theorem populated_retarget_history : RetargetHistory := by
  unfold RetargetHistory FullValues
  decide

set_option maxRecDepth 4096 in
/-- Actual same-target parent2/Frank then captured parent1/Alice. The second call
accepts, requests current program entries and exposes its full client/argv. -/
theorem populated_program_advance_history : ProgramAdvanceHistory := by
  unfold ProgramAdvanceHistory FullValues
  decide

set_option maxRecDepth 4096 in
/-- Factory error is constructed by selecting deleteSandbox as the failed native
observation. Watcher preparation still succeeds and clears its own status key. -/
theorem populated_factory_failure_history : FailedFactoryHistory := by
  unfold FailedFactoryHistory FullValues
  decide

set_option maxRecDepth 4096 in
/-- Actual preparation-refusal/lost-status-reply then actual acknowledged parent
success. The full first failure and unrelated watched-deployment status survive
or clear according to their exact operation order. -/
theorem populated_refusal_then_parent_history : RefusalThenParentHistory := by
  unfold RefusalThenParentHistory FullValues
  decide

set_option maxRecDepth 4096 in
/-- A parent operation can change only the tracked-ID within A. The captured
old branch event is then skipped without disturbing complete new outputs. -/
theorem populated_same_repository_branch_history : BranchChangedHistory := by
  unfold BranchChangedHistory FullValues
  decide

set_option maxRecDepth 4096 in
/-- Requested B fails, recorded A applies successfully, and B's full timestamped
failure remains even though its publication acknowledgement is lost. -/
theorem populated_recorded_fallback_history : RecordedFallbackHistory := by
  unfold RecordedFallbackHistory
  decide

#print axioms populated_same_repository_branch_history
#print axioms populated_recorded_fallback_history
#print axioms populated_retarget_history
#print axioms populated_program_advance_history
#print axioms populated_factory_failure_history
#print axioms populated_refusal_then_parent_history

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``initial_projection, ``initial_runner, ``initial_unique_key, ``preparation_is_indexed_by_revision_and_pusher,
      ``populated_retarget_history, ``populated_program_advance_history,
      ``populated_factory_failure_history, ``populated_refusal_then_parent_history,
      ``populated_same_repository_branch_history, ``populated_recorded_fallback_history] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected populated watcher history axiom in {declaration}: {unexpected}"

end FactoryRetarget.Watcher.Fixture
