import RefreshTransactionProof

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

theorem execution_trans {first middle last : State}
    (left : Execution first middle) (right : Execution middle last) : Execution first last := by
  induction right with
  | refl => exact left
  | tail previous step ih => exact Execution.tail ih step

def ValidWrites : Database → List Write → Prop
  | _, [] => True
  | db, head :: tail => ValidWrite db head ∧ ValidWrites (applyWrite db head) tail

theorem applyWrites_append (db : Database) (left right : List Write) :
    applyWrites db (left ++ right) = applyWrites (applyWrites db left) right := by
  simp [applyWrites, List.foldl_append]

def LocksAvailable (state : State) (id : Id) : Prop :=
  (∀ key, state.tokenLocks key = none ∨ state.tokenLocks key = some id) ∧
  (∀ key, state.auditLocks key = none ∨ state.auditLocks key = some id)

theorem acquired_own {α : Type} [DecidableEq α] (locks : α → Option Id) (owner : Id) (keys : List α)
    (available : ∀ key, locks key = none ∨ locks key = some owner) :
    ∀ key, acquire locks owner keys key = none ∨ acquire locks owner keys key = some owner := by
  intro key
  unfold acquire
  split_ifs
  · exact Or.inr rfl
  · exact available key

def stageOne (state : State) (id : Id) (head : Write) (tail : List Write) : State :=
  { update state id { state.transactions id with
      staged := (state.transactions id).staged ++ [head], remaining := tail } with
    tokenLocks := acquire state.tokenLocks id (TokenFootprint (applyWrites state.db (state.transactions id).staged) head)
    auditLocks := acquire state.auditLocks id (AuditFootprint head) }

def stageList (state : State) (id : Id) : List Write → State
  | [] => state
  | head :: tail => stageList (stageOne state id head tail) id tail

theorem stageList_rows (state : State) (id : Id) (writes : List Write) :
    (stageList state id writes).db = state.db ∧
      (stageList state id writes).familyLock = state.familyLock := by
  induction writes generalizing state with
  | nil => exact ⟨rfl, rfl⟩
  | cons head tail ih => simpa [stageList, stageOne, update] using ih (stageOne state id head tail)

theorem stageList_transaction (state : State) (id : Id) (writes : List Write)
    (remaining : (state.transactions id).remaining = writes) :
    (stageList state id writes).transactions id =
      { state.transactions id with staged := (state.transactions id).staged ++ writes, remaining := [] } := by
  induction writes generalizing state with
  | nil =>
    have same : { state.transactions id with staged := (state.transactions id).staged ++ [], remaining := [] } =
        state.transactions id := by
      cases transaction : state.transactions id
      simp_all
    simpa [stageList] using same.symm
  | cons head tail ih =>
    rw [stageList, ih (stageOne state id head tail) (by simp [stageOne, update, put])]
    simp [stageOne, update, put, List.append_assoc]

theorem stageList_other (state : State) (id other : Id) (writes : List Write) (different : other ≠ id) :
    (stageList state id writes).transactions other = state.transactions other := by
  induction writes generalizing state with
  | nil => rfl
  | cons head tail ih =>
    simpa [stageList, stageOne, update, put, different] using ih (stageOne state id head tail)

theorem stageList_available (state : State) (id : Id) (writes : List Write)
    (available : LocksAvailable state id) : LocksAvailable (stageList state id writes) id := by
  induction writes generalizing state with
  | nil => exact available
  | cons head tail ih =>
    apply ih
    exact ⟨acquired_own _ _ _ available.1, acquired_own _ _ _ available.2⟩

theorem stageList_execution (state : State) (id : Id) (writes : List Write)
    (phase : (state.transactions id).phase = .writing)
    (remaining : (state.transactions id).remaining = writes)
    (valid : ValidWrites (applyWrites state.db (state.transactions id).staged) writes)
    (available : LocksAvailable state id) : Execution state (stageList state id writes) := by
  induction writes generalizing state with
  | nil => exact Execution.refl state
  | cons head tail ih =>
    have first : Step state (stageOne state id head tail) :=
      Step.write state id head tail phase remaining valid.1
        (fun key member => available.1 key) (fun key member => available.2 key)
    have rest := ih (stageOne state id head tail)
      (by simpa [stageOne, update, put] using phase)
      (by simp [stageOne, update, put])
      (by simpa [stageOne, update, put, applyWrites_append, applyWrites] using valid.2)
      ⟨acquired_own _ _ _ available.1, acquired_own _ _ _ available.2⟩
    exact execution_trans (Execution.tail (Execution.refl state) first) rest

/-- A compatible no-cleanup execution can finish the actual staged program.
This is an existential schedule, not fairness or unconditional native success. -/
theorem finish_writing (state : State) (id : Id) (writes : List Write)
    (phase : (state.transactions id).phase = .writing)
    (remaining : (state.transactions id).remaining = writes)
    (pending : (state.transactions id).response = .pending)
    (valid : ValidWrites (applyWrites state.db (state.transactions id).staged) writes)
    (available : LocksAvailable state id) (owner : state.familyLock = some id) :
    ∃ result, Execution state result ∧
      result.db = commit state.db ((state.transactions id).staged ++ writes) ∧
      (result.transactions id).outcome = (state.transactions id).outcome ∧
      (result.transactions id).response = .success ∧ result.familyLock = none ∧
      (∀ key, result.tokenLocks key = none) ∧ (∀ key, result.auditLocks key = none) ∧
      (∀ other, other ≠ id → result.transactions other = state.transactions other) := by
  let staged := stageList state id writes
  have stagedTx := stageList_transaction state id writes remaining
  have rows := stageList_rows state id writes
  have locks := stageList_available state id writes available
  have stagedOwner : staged.familyLock = some id := by
    simpa only [staged] using rows.2.trans owner
  let ready := update staged id { staged.transactions id with phase := .ready }
  have readyStep : Step staged ready := Step.ready staged id
    (by simpa [staged, stagedTx] using phase) (by simp [staged, stagedTx])
  let committing := update ready id { ready.transactions id with phase := .committing }
  have request : Step ready committing := Step.requestCommit ready id (by simp [ready, update, put])
  let committed : State :=
    { update committing id { committing.transactions id with phase := .committed } with
      db := commit committing.db (committing.transactions id).staged
      familyLock := if committing.familyLock = some id then none else committing.familyLock
      tokenLocks := release committing.tokenLocks id
      auditLocks := release committing.auditLocks id
      rotations := match (committing.transactions id).outcome, (committing.transactions id).tokenRead with
        | .rotated successor, some predecessor => committing.rotations ++ [⟨id, predecessor.id, successor.id, successor.generation⟩]
        | _, _ => committing.rotations }
  have commitStep : Step committing committed := Step.serverCommit committing id (by simp [committing, update, put])
  let result := update committed id { committed.transactions id with response := .success }
  have response : Step committed result := Step.respond committed id
    (by simp [committed, update, put])
    (by simp [committed, committing, ready, staged, stagedTx, update, put, pending]) .success (Or.inl rfl)
  refine ⟨result, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · exact Execution.tail (Execution.tail (Execution.tail (Execution.tail
      (stageList_execution state id writes phase remaining valid available) readyStep) request) commitStep) response
  · simp [result, committed, committing, ready, staged, stagedTx, rows.1, update, put]
  · simp [result, committed, committing, ready, staged, stagedTx, update, put]
  · simp [result, update, put]
  · simp [result, committed, committing, ready, stagedOwner, update]
  · intro key
    have available : staged.tokenLocks key = none ∨ staged.tokenLocks key = some id := locks.1 key
    have released : release staged.tokenLocks id key = none := by
      rcases available with missing | held
      · simp [release, missing]
      · simp [release, held]
    simpa [result, committed, committing, ready, update] using released
  · intro key
    have available : staged.auditLocks key = none ∨ staged.auditLocks key = some id := locks.2 key
    have released : release staged.auditLocks id key = none := by
      rcases available with missing | held
      · simp [release, missing]
      · simp [release, held]
    simpa [result, committed, committing, ready, update] using released
  · intro other different
    simpa [result, committed, committing, ready, update, put, different] using stageList_other state id other writes different

end
end RefreshTransaction

#print axioms RefreshTransaction.finish_writing
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.stageList_execution, ``RefreshTransaction.finish_writing] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh execution axiom in {declaration}: {found}"
