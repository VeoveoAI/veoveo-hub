import RegistrationProof

namespace RegistrationTransaction
variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]

def Holding (phase : Phase) : Prop :=
  phase = .locked ∨ phase = .read ∨ phase = .upserting ∨ phase = .deleting ∨
  phase = .dropping ∨ phase = .creating ∨ phase = .inserting ∨
  phase = .ready ∨ phase = .committing

def Ownership (state : State C L M R P F) : Prop :=
  ∀ id, Holding (state.transactions id).phase → state.lock = some id

theorem initial_ownership (db : Database C L M R P F) requests :
    Ownership (initial db requests) := by
  intro id holds
  simp [initial, Holding] at holds

/-- Actual upsert locks apply to both protocols, but the original ordinary
same-metadata path has no such upsert. Hence only fixed executions have this
property for every active read/admission phase. -/
theorem ownership_step {before after : State C L M R P F} (step : Step true before after)
    (ownership : Ownership before) : Ownership after := by
  intro subject holding
  have atSubject := ownership subject
  cases step <;> simp only [update, put, committedState, release] at * <;>
    try split_ifs at *
  all_goals simp_all [Holding]

theorem execution_ownership {before after : State C L M R P F}
    (execution : Execution true before after) (ownership : Ownership before) : Ownership after := by
  induction execution with
  | refl => exact ownership
  | tail previous step ih => exact ownership_step step ih

theorem active_owner_unique (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) (left right : Nat)
    (leftActive : Holding (state.transactions left).phase)
    (rightActive : Holding (state.transactions right).phase) : left = right := by
  have invariant := execution_ownership execution (initial_ownership db requests)
  have leftOwner := invariant left leftActive
  have rightOwner := invariant right rightActive
  simpa only [leftOwner, Option.some.injEq] using rightOwner

#print axioms execution_ownership
#print axioms active_owner_unique
run_cmd do
  for declaration in [``execution_ownership, ``active_owner_unique] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration lock axiom: {name}"

end RegistrationTransaction
