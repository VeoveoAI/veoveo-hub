import RegistrationObservations
import RegistrationEffects

namespace RegistrationTransaction
variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]

def Chosen (phase : Phase) : Prop :=
  phase = .upserting ∨ phase = .deleting ∨ phase = .dropping ∨ phase = .creating ∨
  phase = .inserting ∨ phase = .ready ∨ phase = .committing ∨ phase = .committed

/-- The source's full comparison decides the program branch. This is derived
from real control transitions, not assumed by PostgreSQL at commit. -/
def Choice (tx : Transaction C L M R P) : Prop :=
  (tx.phase = .read → tx.request.kind = .ordinary) ∧
  (Chosen tx.phase →
    (tx.replacing = false → tx.request.kind = .ordinary ∧
      tx.observed = some tx.request.candidate.metadata) ∧
    (tx.replacing = true → tx.request.kind = .instanceReplacement ∨
      tx.observed ≠ some tx.request.candidate.metadata))

def Choices (state : State C L M R P F) : Prop := ∀ id, Choice (state.transactions id)

theorem initial_choices (db : Database C L M R P F) requests : Choices (initial db requests) := by
  intro id
  simp [initial, Choice, Chosen]

theorem choices_step {before after : State C L M R P F} (step : Step true before after)
    (alignment : Alignment before) (choices : Choices before) : Choices after := by
  intro subject
  have alignedSubject := alignment subject
  have chosenSubject := choices subject
  clear alignment choices
  cases step <;> simp only [update, put, committedState] at * <;>
    try split_ifs at *
  all_goals simp_all [Choice, Chosen, Aligned]

theorem reached_choices (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) : Choices state := by
  have general {before after : State C L M R P F} (steps : Execution true before after)
      (alignment : Alignment before) (choices : Choices before) : Choices after := by
    induction steps with
    | refl => exact choices
    | tail previous step ih => exact choices_step step (execution_alignment previous alignment) ih
  exact general execution (initial_alignment db requests) (initial_choices db requests)

theorem request_step {locking : Bool} {before after : State C L M R P F}
    (step : Step locking before after) (id : Nat) :
    (after.transactions id).request = (before.transactions id).request := by
  cases step <;> simp only [update, put, committedState]
  all_goals try split_ifs
  all_goals simp_all

theorem reached_request (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) (id : Nat) :
    (state.transactions id).request = requests id := by
  induction execution with
  | refl => rfl
  | tail previous step ih => exact (request_step step id).trans ih

/-- An actual reached commit request has the independent sequential effect.
Complete staging, branch choice, and current locked DELETE snapshot are all
derived premises; the primitive server commit itself remains unguarded. -/
theorem committing_effect (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) (id : Nat)
    (phase : (state.transactions id).phase = .committing) :
    commit state.db (state.transactions id).staged = sequential state.db (requests id) := by
  rw [← reached_request db requests state execution id]
  have complete := committing_program_complete db requests state execution id phase
  have observations := reached_observations db requests state execution id
  have choices := (reached_choices db requests state execution id).2 (by simp [Chosen, phase])
  have observed := observations.1 (by simp [Observing, phase])
  unfold Complete at complete
  cases replacing : (state.transactions id).replacing
  · have selected := choices.1 replacing
    have same : state.db.metadata = (state.transactions id).request.candidate.metadata := by
      have equality := (observed selected.1).symm.trans selected.2
      exact Option.some.inj equality
    simp only [replacing, Bool.false_eq_true, ↓reduceIte] at complete
    rw [complete]
    simp only [commit, sequential, selected.1, same, and_self, ite_true]
    exact admit_exact state.db (state.transactions id).request
  · have listing := observations.2 (by simp [CapturedPhase, phase]) replacing
    have different : ¬((state.transactions id).request.kind = .ordinary ∧
        state.db.metadata = (state.transactions id).request.candidate.metadata) := by
      rintro ⟨ordinary, same⟩
      rcases choices.2 replacing with instanceKind | inequality
      · simp [ordinary] at instanceKind
      · exact inequality ((observed ordinary).trans (congrArg some same))
    simp only [replacing, ↓reduceIte] at complete
    rw [complete]
    simp only [commit, sequential, different, ite_false]
    exact replacement_exact state.db (state.transactions id).request _ listing

theorem sequential_owned (db : Database C L M R P F) (request : Request C L M R P) (frame : F) :
    sequential (owned db frame) request = owned (sequential db request) frame := by
  by_cases same : request.kind = .ordinary ∧ db.metadata = request.candidate.metadata
  · simp only [sequential, owned, same, and_self, ite_true]
    simp only [expectedReplica, same.2]
  · simp only [sequential, owned, same, ite_false]
    rfl

/-- Arbitrary finite interleavings are equivalent to the independent operations
in ACTUAL server-commit order, after erasing only disjoint frame changes. Response
order and current survival are deliberately not claimed by this theorem. -/
theorem execution_serial (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) :
    SerialHistory requests db state.commits (owned state.db db.frame) := by
  induction execution with
  | refl => simpa only [initial, owned] using (SerialHistory.nil (requests := requests) db)
  | tail previous step ih =>
    cases step
    case serverCommit id phase =>
      change SerialHistory requests db (_ ++ [(id, _)])
        (owned (commit _ _) db.frame)
      rw [committing_effect db requests _ previous id phase, ← sequential_owned]
      exact SerialHistory.snoc ih id _
    all_goals simpa only [update, owned] using ih

/-- A single attempt cannot later become aborted after a server commit, even
if its caller has not yet received an acknowledgement. -/
theorem committed_terminal_step {locking : Bool} {before after : State C L M R P F}
    (step : Step locking before after) (id : Nat)
    (committed : (before.transactions id).phase = .committed) :
    (after.transactions id).phase = .committed := by
  cases step <;> simp only [update, put, committedState] at * <;>
    try split_ifs at *
  all_goals simp_all

def EventsCommitted (state : State C L M R P F) : Prop :=
  ∀ id writes, (id, writes) ∈ state.commits → (state.transactions id).phase = .committed

theorem initial_events (db : Database C L M R P F) requests : EventsCommitted (initial db requests) := by
  intro id writes member
  simp [initial] at member

theorem events_step {locking : Bool} {before after : State C L M R P F}
    (step : Step locking before after) (consistent : EventsCommitted before) : EventsCommitted after := by
  intro subject writes member
  have old (wasPresent : (subject, writes) ∈ before.commits) :
      (after.transactions subject).phase = .committed :=
    committed_terminal_step step subject (consistent subject writes wasPresent)
  cases step
  case serverCommit id phase =>
    rcases List.mem_append.mp member with fromBefore | newest
    · exact old fromBefore
    · have pair := List.mem_singleton.mp newest
      have same : subject = id := congrArg Prod.fst pair
      simp [committedState, update, put, same]
  all_goals exact old member

theorem execution_events {locking : Bool} {before after : State C L M R P F}
    (execution : Execution locking before after) (consistent : EventsCommitted before) : EventsCommitted after := by
  induction execution with
  | refl => exact consistent
  | tail previous step ih => exact events_step step ih

/-- Excludes the aborted attempt's actual contribution, not pre-existing equal
rows or later contributions by other attempts. Combined with execution_serial,
the final owned state is precisely a serial history containing no such event. -/
theorem aborted_event_excluded (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) (id : Nat)
    (aborted : (state.transactions id).phase = .aborted) :
    ∀ writes, (id, writes) ∉ state.commits := by
  intro writes member
  have committed := execution_events execution (initial_events db requests) id writes member
  simp [aborted] at committed

theorem serial_history_and_abort (db : Database C L M R P F) requests state
    (execution : Execution true (initial db requests) state) (id : Nat)
    (aborted : (state.transactions id).phase = .aborted) :
    SerialHistory requests db state.commits (owned state.db db.frame) ∧
      ∀ writes, (id, writes) ∉ state.commits :=
  ⟨execution_serial db requests state execution,
    aborted_event_excluded db requests state execution id aborted⟩

#print axioms committing_effect
#print axioms execution_serial
#print axioms aborted_event_excluded
#print axioms serial_history_and_abort
run_cmd do
  for declaration in [``committing_effect, ``execution_serial, ``aborted_event_excluded, ``serial_history_and_abort] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration history axiom: {name}"

end RegistrationTransaction
