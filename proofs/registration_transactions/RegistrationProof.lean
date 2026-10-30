import RegistrationSpecification
import Lean.Util.CollectAxioms

namespace RegistrationTransaction
variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]

/-- Already completed replacement writes, before admitting the new candidate. -/
def replacementPrefix (tx : Transaction C L M R P) : List (Write C L M R P) :=
  [.deployment tx.request.candidate.metadata, .deleteRows (tx.captured.map (·.client))] ++
    tx.captured.map (fun row => .dropLogin row.login)

def admissionPrefix (tx : Transaction C L M R P) := if tx.replacing then replacementPrefix tx else []

/-- Complete admissionPrefix/loop invariant of the source program. Aborted transactions
may retain partial private writes; they are not allowed to serverCommit. -/
def Aligned (tx : Transaction C L M R P) : Prop :=
  match tx.phase with
  | .idle | .waiting | .locked | .read =>
      tx.staged = [] ∧ tx.captured = [] ∧ tx.drops = [] ∧ tx.replacing = false
  | .upserting => tx.staged = [] ∧ tx.replacing = true
  | .deleting => tx.staged = [.deployment tx.request.candidate.metadata] ∧ tx.replacing = true
  | .dropping => tx.replacing = true ∧ ∃ done : List L,
      done ++ tx.drops = tx.captured.map (·.login) ∧
      tx.staged = [.deployment tx.request.candidate.metadata,
        .deleteRows (tx.captured.map (·.client))] ++ done.map Write.dropLogin
  | .creating => tx.staged = admissionPrefix tx
  | .inserting => tx.staged = admissionPrefix tx ++
      [.createLogin tx.request.candidate.login tx.request.role]
  | .ready | .committing | .committed => tx.staged = admissionPrefix tx ++ admitWrites tx.request
  | .abortPending | .aborted => True

def Alignment (state : State C L M R P F) : Prop := ∀ id, Aligned (state.transactions id)

theorem initial_alignment (db : Database C L M R P F) (requests : Nat → Request C L M R P) :
    Alignment (initial db requests) := by
  intro id
  simp [initial, Aligned]

theorem aligned_drop (tx : Transaction C L M R P) (login : L) (remaining : List L)
    (phase : tx.phase = .dropping) (next : tx.drops = login :: remaining)
    (aligned : Aligned tx) :
    Aligned { tx with drops := remaining, staged := tx.staged ++ [.dropLogin login] } := by
  simp only [Aligned, phase] at aligned ⊢
  obtain ⟨replacing, done, progress, staged⟩ := aligned
  refine ⟨replacing, done ++ [login], ?_, ?_⟩
  · simpa only [List.append_assoc, List.singleton_append, next] using progress
  · simp [staged, List.map_append, List.append_assoc]

theorem alignment_step {locking : Bool} {before after : State C L M R P F}
    (step : Step locking before after) (aligned : Alignment before) : Alignment after := by
  cases step
  case drop id login remaining phase next =>
    intro subject
    by_cases same : subject = id
    · subst subject
      simpa [update, put] using aligned_drop (before.transactions id) login remaining phase next (aligned id)
    · simpa [update, put, same] using aligned subject
  all_goals
    intro subject
    have atSubject := aligned subject
    clear aligned
    simp only [update, put, committedState] at *
    try split_ifs at *
  all_goals simp_all [Aligned, admissionPrefix, replacementPrefix, admitWrites,
    List.append_assoc, List.map_append]
  all_goals exact ⟨[], by simp, by simp_all⟩

theorem execution_alignment {locking : Bool} {before after : State C L M R P F}
    (execution : Execution locking before after) (aligned : Alignment before) : Alignment after := by
  induction execution with
  | refl => exact aligned
  | tail previous step ih => exact alignment_step step ih

/-- The primitive commit has no correctness guard. This completeness fact is
derived solely from actual source-order transitions, for every finite schedule. -/
theorem committing_program_complete (db : Database C L M R P F)
    (requests : Nat → Request C L M R P) (state : State C L M R P F)
    (execution : Execution true (initial db requests) state) (id : Nat)
    (phase : (state.transactions id).phase = .committing) : Complete (state.transactions id) := by
  have aligned := execution_alignment execution (initial_alignment db requests) id
  simp only [Aligned, phase] at aligned
  unfold Complete
  cases replacing : (state.transactions id).replacing <;>
    simpa [admissionPrefix, replacing, replacementPrefix, replaceWrites, admitWrites, List.append_assoc] using aligned

#print axioms committing_program_complete
run_cmd do
  let axioms ← Lean.collectAxioms ``committing_program_complete
  for name in axioms do
    unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
      throwError "unreviewed registration program axiom: {name}"

end RegistrationTransaction
