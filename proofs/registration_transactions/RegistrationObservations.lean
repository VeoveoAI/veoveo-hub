import RegistrationLocks

namespace RegistrationTransaction
variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]

def Observing (phase : Phase) : Prop :=
  phase = .read ∨ phase = .upserting ∨ phase = .deleting ∨ phase = .dropping ∨
  phase = .creating ∨ phase = .inserting ∨ phase = .ready ∨ phase = .committing

def CapturedPhase (phase : Phase) : Prop :=
  phase = .dropping ∨ phase = .creating ∨ phase = .inserting ∨
  phase = .ready ∨ phase = .committing

theorem observing_holding (phase : Phase) (observing : Observing phase) : Holding phase :=
  Or.inr observing

theorem captured_holding (phase : Phase) (captured : CapturedPhase phase) : Holding phase :=
  observing_holding phase (Or.inr (Or.inr (Or.inr captured)))

/-- Assertions refer to CURRENT committed state while this attempt retains the
lock, not to a stale database snapshot stored in a proof oracle. Instance writes
have no invented read/compare observation. DELETE order remains arbitrary. -/
def StableObservations (state : State C L M R P F) : Prop :=
  ∀ id,
    (Observing (state.transactions id).phase →
      (state.transactions id).request.kind = .ordinary →
      (state.transactions id).observed = some state.db.metadata) ∧
    (CapturedPhase (state.transactions id).phase →
      (state.transactions id).replacing = true → Listing state.db (state.transactions id).captured)

theorem initial_observations (db : Database C L M R P F) requests :
    StableObservations (initial db requests) := by
  intro id
  simp [initial, Observing, CapturedPhase]

theorem observations_step {before after : State C L M R P F} (step : Step true before after)
    (ownership : Ownership before) (alignment : Alignment before)
    (stable : StableObservations before) : StableObservations after := by
  intro subject
  have observedSubject := stable subject
  have alignedSubject := alignment subject
  have ownedSubject := ownership subject
  clear stable alignment
  cases step <;> simp only [update, put, committedState, release] at * <;>
    try split_ifs at *
  all_goals simp_all [Observing, CapturedPhase, Holding, Aligned, Listing, applyWrites, applyWrite]
  all_goals
    rename_i id phase different
    have owner : before.lock = some id := ownership id (by simp [Holding, phase])
    have impossible (active : Holding (before.transactions subject).phase) : False := by
      have equality : some subject = some id := (ownership subject active).symm.trans owner
      exact different (Option.some.inj equality)
    constructor
    · intro observing _
      exact False.elim (impossible (observing_holding _ observing))
    · intro captured _
      exact False.elim (impossible (captured_holding _ captured))

theorem execution_observations {before after : State C L M R P F}
    (execution : Execution true before after) (ownership : Ownership before)
    (alignment : Alignment before) (stable : StableObservations before) : StableObservations after := by
  induction execution with
  | refl => exact stable
  | tail previous step ih =>
    exact observations_step step (execution_ownership previous ownership)
      (execution_alignment previous alignment) ih

theorem reached_observations (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) : StableObservations state :=
  execution_observations execution (initial_ownership db requests)
    (initial_alignment db requests) (initial_observations db requests)

#print axioms reached_observations
run_cmd do
  let axioms ← Lean.collectAxioms ``reached_observations
  for name in axioms do
    unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
      throwError "unreviewed registration observation axiom: {name}"

end RegistrationTransaction
