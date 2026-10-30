import RegistrationUpdateLocks

namespace RegistrationUpdate
open RegistrationTransaction
variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- An updated row before commit, or the absent row before confirmed no-op
rollback. Completed/aborted phases have no claimed current-row stability. -/
def ObservingUpdate (tx : Transaction C L M B S D T R) : Prop :=
  tx.phase = .ready ∨ tx.phase = .committing ∨
    (tx.phase = .rollbackPending ∧ tx.outcome = .noop)

def CurrentRows (state : State C L M B S D T R P F) : Prop :=
  ∀ id, ObservingUpdate (state.updates id) →
    state.core.db.registrations (state.updates id).request.client = (state.updates id).matched

theorem observing_update_holding (tx : Transaction C L M B S D T R)
    (aligned : Aligned tx) (observing : ObservingUpdate tx) : UpdateHolding tx := by
  rcases observing with ready | committing | ⟨pending, noop⟩
  · exact Or.inr (Or.inl ready)
  · exact Or.inr (Or.inr (Or.inl committing))
  · refine Or.inr (Or.inr (Or.inr ⟨pending, ?_⟩))
    have discards : tx.outcome = .noop → NoopProgram tx := by
      simpa only [Aligned, pending] using aligned
    have staged := (discards noop).2
    simp [staged]

theorem invariant_excludes_cross (methods : Nat → Method)
    (state : State C L M B S D T R P F) (invariant : Invariant methods state)
    (registerId updateId : Nat)
    (registered : Holding (state.core.transactions registerId).phase)
    (updating : UpdateHolding (state.updates updateId)) : False := by
  rcases invariant with ⟨_, updateTyped, registerTyped, ⟨registerOwns, updateOwns⟩, _, _⟩
  have same := Option.some.inj
    ((registerOwns registerId registered).symm.trans (updateOwns updateId updating))
  have registerMethod := registerTyped registerId (register_holding_not_idle _ registered)
  have updateMethod := updateTyped updateId (update_holding_not_idle _ updating)
  rw [same, updateMethod] at registerMethod
  cases registerMethod

/-- Existing full-metadata/DELETE observations remain true in the MIXED
protocol. A real update commit cannot run while any registering observer holds
that deployment. The previous constructor-level theorem still handles register. -/
theorem registration_observations_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (invariant : Invariant methods before)
    (stable : StableObservations before.core) : StableObservations after.core := by
  cases step
  case register nextCore actual actor contribution label method =>
    exact observations_step actual invariant.2.2.2.1.1 invariant.2.2.2.2.1 stable
  case serverCommit id phase =>
    intro subject
    have updating : UpdateHolding (before.updates id) := by simp [UpdateHolding, phase]
    constructor
    · intro observing _
      exact False.elim (invariant_excludes_cross methods before invariant subject id
        (observing_holding _ observing) updating)
    · intro captured _
      exact False.elim (invariant_excludes_cross methods before invariant subject id
        (captured_holding _ captured) updating)
  all_goals exact stable

theorem current_rows_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (invariant : Invariant methods before)
    (current : CurrentRows before) : CurrentRows after := by
  cases step
  case register nextCore actual actor contribution label method =>
    cases actual
    case serverCommit id phase =>
      intro subject observing
      have updating := observing_update_holding _ (invariant.1 subject) observing
      exact False.elim (invariant_excludes_cross methods before invariant id subject
        (by simp [Holding, phase]) updating)
    all_goals exact current
  case serverCommit id phase =>
    intro subject observing
    by_cases same : subject = id
    · subst subject
      simp [committedState, updateTx, put, ObservingUpdate] at observing
    · have previous : ObservingUpdate (before.updates subject) := by
        simpa [committedState, updateTx, put, same] using observing
      have left := invariant.2.2.2.1.2 id (by simp [UpdateHolding, phase])
      have right := invariant.2.2.2.1.2 subject
        (observing_update_holding _ (invariant.1 subject) previous)
      have equal : subject = id := Option.some.inj (right.symm.trans left)
      exact (same equal).elim
  all_goals
    intro subject observing
    have atSubject := current subject
    have alignedSubject := invariant.1 subject
    clear current invariant
    simp only [updateTx, put] at *
    try split_ifs at *
  all_goals simp_all [ObservingUpdate, Aligned, Complete, NoopProgram]

def Stable (state : State C L M B S D T R P F) : Prop :=
  StableObservations state.core ∧ CurrentRows state

theorem initial_stable (db : DB C L M B S D T R P F) registrations updates :
    Stable (initial db registrations updates) := by
  refine ⟨initial_observations db registrations, ?_⟩
  intro id observing
  simp [initial, ObservingUpdate] at observing

/-- Arbitrary finite mixed interleavings preserve the CURRENT register metadata,
statement-time captured deletion rows, and matched/absent update row while held. -/
theorem reached_stable (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) : Stable state := by
  induction execution with
  | refl => exact initial_stable db registrations updates
  | tail previous step ih =>
    have invariant := reached_invariant methods accepted db registrations updates _ previous
    exact ⟨registration_observations_step methods accepted step invariant ih.1,
      current_rows_step methods accepted step invariant ih.2⟩

/-- The captured-row effect is now connected to the CURRENT independent
per-client operation, using the reached shared-lock observations. -/
theorem committing_sequential_update (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (id : Nat) (phase : (state.updates id).phase = .committing) :
    commit state.core.db (state.updates id).staged =
      sequentialUpdate state.core.db (state.updates id).request := by
  rcases committing_program_complete methods accepted db registrations updates state execution id phase with
    ⟨row, matched, outcome, staged⟩
  have found := (reached_stable methods accepted db registrations updates state execution).2 id
    (by simp [ObservingUpdate, phase])
  rw [matched] at found
  rw [staged]
  exact matched_sequential_exact _ _ _ found

/-- The no-op decision still refers to an absent CURRENT row at rollbackPending.
Immediate rollback preservation follows by composing rollback_noop_exact; this
statement does not cover arbitrary aborted states or the later acknowledgement. -/
theorem pending_noop_absence (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (id : Nat) (phase : (state.updates id).phase = .rollbackPending)
    (noop : (state.updates id).outcome = .noop) :
    state.core.db.registrations (state.updates id).request.client = none ∧
      sequentialUpdate state.core.db (state.updates id).request = state.core.db := by
  have discarded := noop_program_discarded methods accepted db registrations updates state execution id
    (Or.inl phase) noop
  have current := (reached_stable methods accepted db registrations updates state execution).2 id
    (Or.inr (Or.inr ⟨phase, noop⟩))
  rw [discarded.1] at current
  exact ⟨current, by simp only [sequentialUpdate, current]⟩

#print axioms reached_stable
#print axioms committing_sequential_update
#print axioms pending_noop_absence
run_cmd do
  for declaration in [``reached_stable, ``committing_sequential_update, ``pending_noop_absence] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed mixed registration observation axiom: {name}"

end RegistrationUpdate
