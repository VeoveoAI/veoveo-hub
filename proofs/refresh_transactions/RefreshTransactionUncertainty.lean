import RefreshTransactionWitnesses
import RefreshTransactionReplay

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- The uncertainty examples start at a reached committing state, with all four
legitimate writes staged and no public row changed yet. They are not arbitrary
records whose phase was manually set to committing. -/
theorem reached_committing (template : Family) :
    ∃ pending, Execution (initial (witnessDB template) (witnessRequests template)) pending ∧
      pending.db = witnessDB template ∧ (pending.transactions 0).phase = .committing ∧
      (pending.transactions 0).response = .pending ∧
      (pending.transactions 0).staged = rotationWrites template := by
  let waiting := waitingState template
  rcases waiting_properties template with ⟨rows, owner, firstTx, retryTx, tokensFree, auditFree⟩
  let staged := stageList waiting 0 (rotationWrites template)
  have stageExecution := stageList_execution waiting 0 (rotationWrites template)
    (by rw [firstTx]) (by rw [firstTx])
    (by simpa [waiting, rows, firstTx, applyWrites] using rotation_constraints_satisfiable template)
    ⟨fun key => Or.inl (tokensFree key), fun key => Or.inl (auditFree key)⟩
  have stagedTx : staged.transactions 0 =
      { waiting.transactions 0 with
        staged := (waiting.transactions 0).staged ++ rotationWrites template
        remaining := [] } :=
    stageList_transaction waiting 0 (rotationWrites template) (by rw [firstTx])
  have stagedRows : staged.db = witnessDB template :=
    (stageList_rows waiting 0 (rotationWrites template)).1.trans rows
  let ready := update staged 0 { staged.transactions 0 with phase := .ready }
  have readyStep : Step staged ready := Step.ready staged 0
    (by simp [stagedTx, waiting, firstTx]) (by simp [stagedTx])
  let pending := update ready 0 { ready.transactions 0 with phase := .committing }
  have request : Step ready pending := Step.requestCommit ready 0 (by simp [ready, update, put])
  refine ⟨pending, ?_, ?_, ?_, ?_, ?_⟩
  · exact Execution.tail (Execution.tail
      (execution_trans (waiting_execution template) stageExecution) readyStep) request
  · simpa [pending, ready, update] using stagedRows
  · simp [pending, update, put]
  · simp [pending, ready, update, put, stagedTx, waiting, firstTx]
  · simp [pending, ready, update, put, stagedTx, waiting, firstTx]

/-- One reached commit request can report an error before the server decision.
Both a complete commit and a complete abort remain possible, with the same
observed error. This is design-model uncertainty, not a native fault-injection
claim or a proof about physical durability. -/
theorem uncertain_commit_outcomes (template : Family) :
    ∃ pending failed persisted aborted,
      Execution (initial (witnessDB template) (witnessRequests template)) pending ∧
      (pending.transactions 0).phase = .committing ∧
      Step pending failed ∧ (failed.transactions 0).response = .error ∧
      Step failed persisted ∧ Step failed aborted ∧
      (persisted.transactions 0).phase = .committed ∧ (aborted.transactions 0).phase = .aborted ∧
      (persisted.transactions 0).response = .error ∧ (aborted.transactions 0).response = .error ∧
      persisted.db = rotatedDB template ∧ aborted.db = witnessDB template := by
  rcases reached_committing template with ⟨pending, reached, beforeRows, phase, response, staged⟩
  let failed := update pending 0 { pending.transactions 0 with response := .error }
  have errorStep : Step pending failed := Step.uncertain pending 0 phase
  have failedPhase : (failed.transactions 0).phase = .committing := by
    simpa [failed, update, put] using phase
  let persisted := committedState failed 0
  have commitStep : Step failed persisted := Step.serverCommit failed 0 failedPhase
  let aborted : State :=
    { update failed 0 { failed.transactions 0 with phase := .aborted, staged := [], remaining := [] } with
      familyLock := if failed.familyLock = some 0 then none else failed.familyLock
      tokenLocks := release failed.tokenLocks 0
      auditLocks := release failed.auditLocks 0 }
  have abortStep : Step failed aborted := Step.abort failed 0
    (Or.inr ⟨failedPhase, by simp [failed, update, put]⟩)
  refine ⟨pending, failed, persisted, aborted, reached, phase, errorStep, ?_, commitStep, abortStep, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · simp [failed, update, put]
  · simp [persisted, committedState, update, put]
  · simp [aborted, update, put]
  · simp [persisted, committedState, failed, update, put]
  · simp [aborted, failed, update, put]
  · simp [persisted, committedState, failed, update, put, beforeRows, staged, rotatedDB]
  · simpa [aborted, failed, update] using beforeRows

/-- The server may also commit first and then lose its acknowledgement. The
error does not roll back its full family/token/audit delta. -/
theorem committed_then_lost_ack (template : Family) :
    ∃ pending committed failed,
      Execution (initial (witnessDB template) (witnessRequests template)) pending ∧
      Step pending committed ∧ Step committed failed ∧
      (failed.transactions 0).phase = .committed ∧ (failed.transactions 0).response = .error ∧ failed.db = rotatedDB template := by
  rcases reached_committing template with ⟨pending, reached, beforeRows, phase, response, staged⟩
  let committed := committedState pending 0
  have commitStep : Step pending committed := Step.serverCommit pending 0 phase
  have committedPhase : (committed.transactions 0).phase = .committed := by
    simp [committed, committedState, update, put]
  have waiting : (committed.transactions 0).response = .pending := by
    simpa [committed, committedState, update, put] using response
  let failed := update committed 0 { committed.transactions 0 with response := .error }
  have lost : Step committed failed := Step.respond committed 0 committedPhase waiting .error (Or.inr rfl)
  refine ⟨pending, committed, failed, reached, commitStep, lost, ?_, ?_, ?_⟩
  · simpa [failed, update, put] using committedPhase
  · simp [failed, update, put]
  · simp [failed, committed, committedState, update, put, beforeRows, staged, rotatedDB]

end
end RefreshTransaction

#print axioms RefreshTransaction.reached_committing
#print axioms RefreshTransaction.uncertain_commit_outcomes
#print axioms RefreshTransaction.committed_then_lost_ack
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.reached_committing,
      ``RefreshTransaction.uncertain_commit_outcomes, ``RefreshTransaction.committed_then_lost_ack] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh uncertainty axiom in {declaration}: {found}"
