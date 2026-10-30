import PublicationExecution

namespace ControlPublication

/-- Construct the actual program prefix up to sending commit, using the same
per-statement staging lemma as the success proof. This is an execution witness,
not a commit oracle or a liveness claim about every scheduler. -/
theorem valid_committing_execution (db : Database) (plans : Id → Plan) (id : Id)
    (base : db.active = (plans id).expected) (valid : ValidWrites db (plans id).writes) :
    ∃ state, Execution (initial db plans) state ∧ state.db = db ∧
      (state.transactions id).phase = .committing ∧
      (state.transactions id).response = .pending ∧
      (state.transactions id).staged = (plans id).writes := by
  let start := initial db plans
  let begun := updateTransaction start id { start.transactions id with phase := .begun }
  let locked := { updateTransaction begun id
    { begun.transactions id with phase := .locked, observed := begun.db.active } with lock := some id }
  let writing := updateTransaction locked id
    { locked.transactions id with phase := .writing, remaining := (locked.transactions id).plan.writes }
  have beginStep : Step start begun := Step.begin start id rfl
  have lockStep : Step begun locked := Step.lock begun id (by simp [begun, updateTransaction, put]) rfl
  have compareStep : Step locked writing := Step.compare locked id
    (by simp [locked, updateTransaction, put])
    (by simpa [locked, begun, start, initial, updateTransaction, put] using base)
  have traversal : Execution start writing :=
    Execution.tail (Execution.tail (Execution.tail (Execution.refl start) beginStep) lockStep) compareStep
  let staged := stageList writing id (plans id).writes
  have stageExecution : Execution writing staged := stageList_execution writing id (plans id).writes
    (by simp [writing, updateTransaction, put])
    (by simp [writing, locked, begun, start, initial, updateTransaction, put])
    (by simpa [writing, locked, begun, start, initial, updateTransaction, put, applyWrites] using valid)
  have stagedResult := stageList_result writing id (plans id).writes
    (by simp [writing, locked, begun, start, initial, updateTransaction, put])
  let ready := updateTransaction staged id { staged.transactions id with phase := .ready }
  have readyStep : Step staged ready := Step.ready staged id
    (by simp [staged, stagedResult, writing, updateTransaction, put])
    (by simp [staged, stagedResult, updateTransaction, put])
  let committing := updateTransaction ready id { ready.transactions id with phase := .committing }
  have commitRequest : Step ready committing := Step.requestCommit ready id
    (by simp [ready, updateTransaction, put])
  refine ⟨committing, ?_, ?_, ?_, ?_, ?_⟩
  · exact Execution.tail (Execution.tail (execution_trans traversal stageExecution) readyStep) commitRequest
  · simp [committing, ready, staged, stagedResult, writing, locked, begun, start, initial, updateTransaction]
  · simp [committing, updateTransaction, put]
  · simp [committing, ready, staged, stagedResult, writing, locked, begun, start, initial, updateTransaction, put]
  · simp [committing, ready, staged, stagedResult, writing, locked, begun, start, initial, updateTransaction, put]

/-- A single reachable state with a client commit error permits either server
outcome. Both resulting client responses remain errors; only one database has
the publication. In particular a returned error cannot imply rollback. -/
theorem reachable_commit_error_is_ambiguous (db : Database) (plans : Id → Plan) (id : Id)
    (base : db.active = (plans id).expected) (valid : ValidWrites db (plans id).writes) :
    ∃ common committed aborted,
      Execution (initial db plans) common ∧
      (common.transactions id).response = .error ∧
      Step common committed ∧ Step common aborted ∧
      committed.db = expectedPublication db (plans id) ∧
      (committed.transactions id).phase = .committed ∧
      (committed.transactions id).response = .error ∧
      aborted.db = db ∧ (aborted.transactions id).phase = .aborted ∧
      (aborted.transactions id).response = .error := by
  obtain ⟨state, execution, database, phase, pending, staged⟩ :=
    valid_committing_execution db plans id base valid
  let common := updateTransaction state id { state.transactions id with response := .error }
  have errorStep : Step state common := Step.commitUncertain state id phase pending
  have commonPhase : (common.transactions id).phase = .committing := by
    simpa [common, updateTransaction, put] using phase
  have commonError : (common.transactions id).response = .error := by simp [common, updateTransaction, put]
  let committed := committedState common id
  have committedStep : Step common committed := Step.serverCommit common id commonPhase
  let aborted : State :=
    { updateTransaction common id
        { common.transactions id with phase := .aborted, staged := [], remaining := [] } with
      lock := release common.lock id }
  have abortedStep : Step common aborted := Step.serverAbortUncertain common id commonPhase commonError
  refine ⟨common, committed, aborted, Execution.tail execution errorStep, commonError,
    committedStep, abortedStep, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · change commit common.db (common.transactions id).staged = _
    simpa [common, updateTransaction, put, database, staged] using publication_commit_exact db (plans id)
  · simp [committed, committedState, updateTransaction, put]
  · simp [committed, committedState, common, updateTransaction, put]
  · simpa [aborted, common, updateTransaction] using database
  · simp [aborted, updateTransaction, put]
  · simp [aborted, common, updateTransaction, put]

theorem reachable_server_commit_loses_acknowledgement (db : Database) (plans : Id → Plan) (id : Id)
    (base : db.active = (plans id).expected) (valid : ValidWrites db (plans id).writes) :
    ∃ state, Execution (initial db plans) state ∧
      state.db = expectedPublication db (plans id) ∧
      (state.transactions id).phase = .committed ∧ (state.transactions id).response = .error := by
  obtain ⟨state, execution, database, phase, pending, staged⟩ :=
    valid_committing_execution db plans id base valid
  let committed := committedState state id
  have commitStep : Step state committed := Step.serverCommit state id phase
  let result := updateTransaction committed id { committed.transactions id with response := .error }
  have lost : Step committed result := Step.loseAcknowledgement committed id
    (by simp [committed, committedState, updateTransaction, put])
    (by simpa [committed, committedState, updateTransaction, put] using pending)
  refine ⟨result, Execution.tail (Execution.tail execution commitStep) lost, ?_, ?_, ?_⟩
  · change commit state.db (state.transactions id).staged = _
    rw [database, staged, publication_commit_exact]
  · simp [result, committed, committedState, updateTransaction, put]
  · simp [result, updateTransaction, put]

#print axioms valid_committing_execution
#print axioms reachable_commit_error_is_ambiguous
#print axioms reachable_server_commit_loses_acknowledgement

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``valid_committing_execution, ``reachable_commit_error_is_ambiguous,
      ``reachable_server_commit_loses_acknowledgement] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication uncertainty axiom in {declaration}: {found}"

end ControlPublication
