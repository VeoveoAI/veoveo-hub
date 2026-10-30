import PublicationHistory

/- Constructive program executions for the next increment; not wired into the
initial checkpoint. The database still has no application-completeness guard. -/
namespace ControlPublication

theorem execution_trans {first middle last : State}
    (left : Execution first middle) (right : Execution middle last) : Execution first last := by
  induction right with
  | refl => exact left
  | tail previous step ih => exact Execution.tail ih step

theorem update_self (state : State) (id : Id) :
    updateTransaction state id (state.transactions id) = state := by
  cases state
  simp only [updateTransaction]
  congr 1
  funext other
  by_cases same : other = id <;> simp [put, same]

theorem update_twice (state : State) (id : Id) (first second : Transaction) :
    updateTransaction (updateTransaction state id first) id second =
      updateTransaction state id second := by
  cases state
  simp only [updateTransaction]
  congr 1
  funext other
  by_cases same : other = id <;> simp [updateTransaction, put, same]

def stageOne (state : State) (id : Id) (write : Write) (remaining : List Write) : State :=
  updateTransaction state id
    { state.transactions id with staged := (state.transactions id).staged ++ [write], remaining }

def stageList (state : State) (id : Id) : List Write → State
  | [] => state
  | write :: remaining => stageList (stageOne state id write remaining) id remaining

theorem stageList_result (state : State) (id : Id) (writes : List Write)
    (remaining : (state.transactions id).remaining = writes) :
    stageList state id writes = updateTransaction state id
      { state.transactions id with staged := (state.transactions id).staged ++ writes, remaining := [] } := by
  induction writes generalizing state with
  | nil =>
    have same : { state.transactions id with
        staged := (state.transactions id).staged ++ []
        remaining := [] } = state.transactions id := by
      cases transaction : state.transactions id
      simp_all
    simp only [stageList, same, update_self]
  | cons write writes ih =>
    rw [stageList, ih (stageOne state id write writes) (by simp [stageOne, updateTransaction, put])]
    simp only [stageOne, updateTransaction, put_same, List.append_assoc, List.singleton_append]
    exact update_twice _ _ _ _

theorem stageList_execution (state : State) (id : Id) (writes : List Write)
    (phase : (state.transactions id).phase = .writing)
    (remaining : (state.transactions id).remaining = writes)
    (valid : ValidWrites (applyWrites state.db (state.transactions id).staged) writes) :
    Execution state (stageList state id writes) := by
  induction writes generalizing state with
  | nil => exact Execution.refl state
  | cons write writes ih =>
    have first : Step state (stageOne state id write writes) :=
      Step.write state id write writes phase remaining valid.1
    have rest := ih (stageOne state id write writes)
      (by simpa [stageOne, updateTransaction, put] using phase)
      (by simp [stageOne, updateTransaction, put])
      (by simpa [stageOne, updateTransaction, put, applyWrites_append, applyWrites] using valid.2)
    exact execution_trans (Execution.tail (Execution.refl state) first) rest

/-- A caller whose current predecessor matches and whose statements satisfy
the modeled constraints has a concrete scheduled success. This supplies a valid
execution, not a fairness promise for all possible schedules or database failures. -/
theorem valid_publication_execution (db : Database) (plans : Id → Plan) (id : Id)
    (base : db.active = (plans id).expected) (valid : ValidWrites db (plans id).writes) :
    ∃ state, Execution (initial db plans) state ∧
      state.db = expectedPublication db (plans id) ∧
      (state.transactions id).response = .success ∧ state.lock = none := by
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
  let committed : State :=
    { updateTransaction committing id { committing.transactions id with phase := .committed } with
      db := commit committing.db (committing.transactions id).staged
      lock := release committing.lock id
      commits := committing.commits ++ [⟨id, (committing.transactions id).plan.expected,
        (committing.transactions id).plan.revision⟩] }
  have commitStep : Step committing committed := Step.serverCommit committing id
    (by simp [committing, updateTransaction, put])
  let result := updateTransaction committed id { committed.transactions id with response := .success }
  have acknowledgeStep : Step committed result := Step.acknowledge committed id
    (by simp [committed, updateTransaction, put])
    (by simp [committed, committing, ready, staged, stagedResult, writing, locked, begun, start,
      initial, updateTransaction, put])
  refine ⟨result, ?_, ?_, ?_, ?_⟩
  · exact Execution.tail (Execution.tail (Execution.tail (Execution.tail
      (execution_trans traversal stageExecution) readyStep) commitRequest) commitStep) acknowledgeStep
  · simp [result, committed, committing, ready, staged, stagedResult, writing, locked, begun, start,
      initial, updateTransaction, put, publication_commit_exact]
  · simp [result, updateTransaction, put]
  · simp [result, committed, committing, ready, staged, stagedResult, writing, locked, begun, start,
      initial, updateTransaction, put, release]

theorem valid_initial_publication (db : Database) (plans : Id → Plan) (id : Id)
    (empty : db.active = none) (initialBase : (plans id).expected = none)
    (valid : ValidWrites db (plans id).writes) :
    ∃ state, Execution (initial db plans) state ∧
      state.db = expectedPublication db (plans id) ∧
      (state.transactions id).response = .success ∧ state.lock = none := by
  apply valid_publication_execution db plans id _ valid
  exact empty.trans initialBase.symm

theorem valid_replacement_publication (db : Database) (plans : Id → Plan) (id predecessor : Id)
    (active : db.active = some predecessor) (expected : (plans id).expected = some predecessor)
    (valid : ValidWrites db (plans id).writes) :
    ∃ state, Execution (initial db plans) state ∧
      state.db = expectedPublication db (plans id) ∧
      (state.transactions id).response = .success ∧ state.lock = none := by
  apply valid_publication_execution db plans id _ valid
  exact active.trans expected.symm

#print axioms stageList_execution
#print axioms valid_publication_execution
#print axioms valid_initial_publication
#print axioms valid_replacement_publication

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``stageList_execution, ``valid_publication_execution,
      ``valid_initial_publication, ``valid_replacement_publication] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication execution axiom in {declaration}: {found}"

end ControlPublication
