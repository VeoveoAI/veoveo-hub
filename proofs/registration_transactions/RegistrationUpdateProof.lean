import RegistrationUpdateEffects

namespace RegistrationUpdate
open RegistrationTransaction
variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- Ordered application prefixes. Failure may discard ANY partial program; only
an actual updated path can reach ready/commit, and no-op discards just metadata.
These constraints are NOT built into the primitive commit function. -/
def Aligned (tx : Transaction C L M B S D T R) : Prop :=
  match tx.phase with
  | .idle | .preflight | .begun =>
      tx.staged = [] ∧ tx.matched = none ∧ tx.outcome = .pending
  | .metadataWritten =>
      tx.staged = [.deploymentMetadata tx.request.metadata] ∧
        tx.matched = none ∧ tx.outcome = .pending
  | .ready | .committing | .committed => Complete tx
  | .registrationWritten => False
  | .rollbackPending | .aborted => tx.outcome = .noop → NoopProgram tx

def Alignment (state : State C L M B S D T R P F) : Prop :=
  ∀ id, Aligned (state.updates id)

theorem initial_alignment (db : DB C L M B S D T R P F) registrations updates :
    Alignment (initial db registrations updates) := by
  intro id
  simp [initial, Aligned]

theorem alignment_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (aligned : Alignment before) : Alignment after := by
  intro subject
  have atSubject := aligned subject
  clear aligned
  cases step
  case serverAbort id phase =>
    rcases phase with phase | phase <;>
      simp only [updateTx, put] at * <;> try split_ifs at *
    all_goals simp_all [Aligned, Complete, NoopProgram]
    all_goals
      rcases atSubject with ⟨row, matched, updated, writes⟩
      simp [updated]
  all_goals
    simp only [updateTx, put, committedState] at *
    try split_ifs at *
  all_goals simp_all [Aligned, Complete, NoopProgram]

theorem reached_alignment (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) :
    Alignment state := by
  induction execution with
  | refl => exact initial_alignment db registrations updates
  | tail previous step ih => exact alignment_step methods accepted step ih

/-- Every actual commit request has exactly deployment-then-matched-row writes,
with the CURRENT matched row and updated outcome. No assumed SQL commit guard. -/
theorem committing_program_complete (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (id : Nat) (phase : (state.updates id).phase = .committing) :
    Complete (state.updates id) := by
  have aligned := reached_alignment methods accepted db registrations updates state execution id
  simpa only [Aligned, phase] using aligned

/-- The affected0 branch is not a successful publication path. It carries only
the private metadata write, which rollback_noop_exact proves is discarded. -/
theorem noop_program_discarded (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (id : Nat)
    (phase : (state.updates id).phase = .rollbackPending ∨ (state.updates id).phase = .aborted)
    (outcome : (state.updates id).outcome = .noop) :
    NoopProgram (state.updates id) := by
  have aligned := reached_alignment methods accepted db registrations updates state execution id
  rcases phase with phase | phase
  all_goals
    have discarded : (state.updates id).outcome = .noop → NoopProgram (state.updates id) := by
      simpa only [Aligned, phase] using aligned
    exact discarded outcome

/-- A matched successful program publishes the independently specified complete
field transformation. Current-row stability until commit is a later mixed-lock
invariant; this result exposes the actual captured row rather than assuming it. -/
theorem committing_exact_fields (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (id : Nat) (phase : (state.updates id).phase = .committing) :
    ∃ row, (state.updates id).matched = some row ∧ (state.updates id).outcome = .updated ∧
      commit state.core.db (state.updates id).staged =
        expectedMatched state.core.db (state.updates id).request row := by
  rcases committing_program_complete methods accepted db registrations updates state execution id phase with
    ⟨row, matched, outcome, writes⟩
  refine ⟨row, matched, outcome, ?_⟩
  rw [writes]
  exact matched_program_exact _ _ _

#print axioms committing_program_complete
#print axioms noop_program_discarded
#print axioms committing_exact_fields

run_cmd do
  for declaration in [``committing_program_complete, ``noop_program_discarded,
      ``committing_exact_fields] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration update program axiom: {name}"

end RegistrationUpdate
