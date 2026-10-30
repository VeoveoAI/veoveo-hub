import RefreshTransactionProof

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Abort-pending transactions may retain a lock but cannot commit. The phases
below need the stable family snapshot to decide/write/request commit. -/
def OwnsFamily (phase : Phase) : Prop :=
  phase = .locked ∨ phase = .read ∨ phase = .writing ∨ phase = .ready ∨ phase = .committing

def Locked (state : State) : Prop :=
  ∀ id, OwnsFamily (state.transactions id).phase →
    state.familyLock = some id ∧ (state.transactions id).familyRead = state.db.family

theorem initial_locked (db : Database) (requests : Id → Request) : Locked (initial db requests) := by
  intro id active
  simp [OwnsFamily, initial] at active

theorem locked_step {before after : State} (step : Step before after)
    (locked : Locked before) : Locked after := by
  intro subject active
  have prior := locked subject
  cases step
  case serverCommit id phase =>
    by_cases same : subject = id
    · simp [update, put, same, OwnsFamily] at active
    · have wasActive : OwnsFamily (before.transactions subject).phase := by
        simpa [update, put, same] using active
      have subjectOwner := (locked subject wasActive).1
      have commitOwner := (locked id (by simp [OwnsFamily, phase])).1
      have impossible : subject = id := Option.some.inj (subjectOwner.symm.trans commitOwner)
      exact False.elim (same impossible)
  case lock id family phase free target present =>
    by_cases same : subject = id
    · subst subject
      simp [update, put, present]
    · have wasActive : OwnsFamily (before.transactions subject).phase := by
        simpa [update, put, same] using active
      have owner := (locked subject wasActive).1
      simp [free] at owner
  case pruneFamily family now present expired free tokensFree =>
    have owner := (locked subject active).1
    simp [free] at owner
  all_goals clear locked
  all_goals
    simp only [update, put] at * <;> try split_ifs at *
  all_goals simp_all [OwnsFamily, decisionPhase]
  all_goals try (split_ifs <;> simp_all [OwnsFamily])

theorem execution_locked {before after : State} (execution : Execution before after)
    (locked : Locked before) : Locked after := by
  induction execution with
  | refl => exact locked
  | tail execution step ih => exact locked_step step ih

/-- Actual recorded family metadata is stable across lock wait completion and
ordinary token maintenance, until this transaction's server decision. -/
theorem committing_family_snapshot (db : Database) (requests : Id → Request) (state : State)
    (execution : Execution (initial db requests) state) (id : Id)
    (phase : (state.transactions id).phase = .committing) :
    state.familyLock = some id ∧ (state.transactions id).familyRead = state.db.family := by
  exact execution_locked execution (initial_locked db requests) id (by simp [OwnsFamily, phase])

end
end RefreshTransaction

#print axioms RefreshTransaction.committing_family_snapshot
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.execution_locked,
      ``RefreshTransaction.committing_family_snapshot] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh lock axiom in {declaration}: {found}"
