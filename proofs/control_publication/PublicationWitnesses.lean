import PublicationExecution
import PublicationExamples
import PublicationUncertainty

namespace ControlPublication

def initialPlan : Plan :=
  { examplePlan 1 none with
    contexts := [exampleContext,
      { exampleContext with id := 12, row := 120, grants := [25], memberships := [35] }] }

def replacementPlan : Plan :=
  { examplePlan 2 (some 1) with
    contexts := [
      { exampleContext with row := 111, grants := [23, 24], memberships := [33] },
      { exampleContext with id := 13, row := 130, grants := [], memberships := [36, 37] }] }

def initialRows : Database := commit emptyDatabase initialPlan.writes

/-- Kernel-computed satisfiability of the operation constraints, with actual
context/grant/membership/retention statements and an audit batch. -/
theorem initial_statements_valid : ValidWrites emptyDatabase initialPlan.writes := by
  simp [ValidWrites, ValidWrite, ValidBatch, initialPlan, examplePlan, exampleContext,
    Plan.writes, Plan.contextWrites, contextWrites, emptyDatabase, applyWrite, keepOrInsert, put]

theorem replacement_statements_valid : ValidWrites initialRows replacementPlan.writes := by
  simp [ValidWrites, ValidWrite, ValidBatch, initialRows, initialPlan, replacementPlan,
    examplePlan, exampleContext, Plan.writes, Plan.contextWrites, contextWrites,
    emptyDatabase, commit, applyWrites, applyWrite, appendAudit, keepOrInsert, put]

theorem initial_execution_succeeds :
    ∃ state, Execution (initial emptyDatabase (fun _ => initialPlan)) state ∧
      state.db = expectedPublication emptyDatabase initialPlan ∧
      (state.transactions 0).response = .success ∧ state.lock = none := by
  exact valid_initial_publication emptyDatabase (fun _ => initialPlan) 0 rfl rfl initial_statements_valid

theorem replacement_execution_succeeds :
    ∃ state, Execution (initial initialRows (fun _ => replacementPlan)) state ∧
      state.db = expectedPublication initialRows replacementPlan ∧
      (state.transactions 0).response = .success ∧ state.lock = none := by
  exact valid_replacement_publication initialRows (fun _ => replacementPlan) 0 1
    (by decide) rfl replacement_statements_valid

/-- The replacement changes a retained mutable row and both ordinal lists,
removes an old context with its children, creates a new context with empty grants,
retains immutable prior history and preserves the prior audit alongside its own.
These are mathematical row-projection witnesses, not fabricated native results. -/
theorem replacement_rows_are_complete :
    let after := commit initialRows replacementPlan.writes
    after.active = some 2 ∧ after.revisions 1 = some 101 ∧ after.revisions 2 = some 102 ∧
      after.contexts 11 = some 111 ∧ after.contexts 12 = none ∧ after.contexts 13 = some 130 ∧
      after.grants 11 = [23, 24] ∧ after.grants 12 = [] ∧ after.grants 13 = [] ∧
      after.memberships 11 = [33] ∧ after.memberships 12 = [] ∧ after.memberships 13 = [36, 37] ∧
      after.audit 61 = some 601 ∧ after.audit 62 = some 602 := by decide

/-- Nonvacuous lost-acknowledgement execution: despite an error response, the
previously absent revision row is committed by the actual model program. -/
theorem concrete_lost_ack_commits_revision :
    ∃ state, Execution (initial emptyDatabase (fun _ => initialPlan)) state ∧
      (state.transactions 0).phase = .committed ∧
      (state.transactions 0).response = .error ∧ state.db.revisions 1 = some 101 := by
  obtain ⟨state, execution, database, committed, error⟩ :=
    reachable_server_commit_loses_acknowledgement emptyDatabase (fun _ => initialPlan) 0
      rfl initial_statements_valid
  refine ⟨state, execution, committed, error, ?_⟩
  rw [database]
  decide

#print axioms initial_statements_valid
#print axioms replacement_statements_valid
#print axioms initial_execution_succeeds
#print axioms replacement_execution_succeeds
#print axioms replacement_rows_are_complete
#print axioms concrete_lost_ack_commits_revision

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``initial_statements_valid, ``replacement_statements_valid,
      ``initial_execution_succeeds, ``replacement_execution_succeeds, ``replacement_rows_are_complete,
      ``concrete_lost_ack_commits_revision] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication witness axiom in {declaration}: {found}"

end ControlPublication
