import RegistrationUpdateObservations

namespace RegistrationUpdate
open RegistrationTransaction
noncomputable section
variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- Independent operations in ACTUAL tagged commit order. Neither operation
specification evaluates the event's observed staged write list. No-op rollbacks
contribute no committed event; later acknowledgements do not freeze this history. -/
inductive SerialHistory
    (registrations : Nat → RegistrationTransaction.Request C L (M × B) (Payload S D T R) P)
    (updates : Nat → Request C M S D T) :
    DB C L M B S D T R P F → List (Event C L M B S D T R P) → DB C L M B S D T R P F → Prop where
  | nil (db) : SerialHistory registrations updates db [] db
  | register {first middle events} (history : SerialHistory registrations updates first events middle)
      (id : Nat) (writes) :
      SerialHistory registrations updates first (events ++ [.register id writes])
        (RegistrationTransaction.sequential middle (registrations id))
  | update {first middle events} (history : SerialHistory registrations updates first events middle)
      (id : Nat) (writes) :
      SerialHistory registrations updates first (events ++ [.update id writes])
        (sequentialUpdate middle (updates id))

theorem register_request_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (id : Nat) :
    (after.core.transactions id).request = (before.core.transactions id).request := by
  cases step
  case register nextCore actual actor contribution label method => exact request_step actual id
  all_goals rfl

theorem update_request_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (id : Nat) :
    (after.updates id).request = (before.updates id).request := by
  cases step <;> simp only [updateTx, put, committedState]
  all_goals try split_ifs
  all_goals simp_all

theorem reached_requests (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) (id : Nat) :
    (state.core.transactions id).request = registrations id ∧
      (state.updates id).request = updates id := by
  induction execution with
  | refl => exact ⟨rfl, rfl⟩
  | tail previous step ih =>
    exact ⟨(register_request_step methods accepted step id).trans ih.1,
      (update_request_step methods accepted step id).trans ih.2⟩

/-- Reuse the old component proofs against their ESTABLISHED mixed invariants.
This does not assume the real registration operation has an atomic correct effect. -/
theorem register_effect_from_invariants (state : RegisterState C L M B S D T R P F)
    (id : Nat) (phase : (state.transactions id).phase = .committing)
    (alignment : RegistrationTransaction.Alignment state) (choices : Choices state)
    (stable : StableObservations state) :
    RegistrationTransaction.commit state.db (state.transactions id).staged =
      RegistrationTransaction.sequential state.db (state.transactions id).request := by
  have aligned := alignment id
  have complete : RegistrationTransaction.Complete (state.transactions id) := by
    simp only [RegistrationTransaction.Aligned, phase] at aligned
    unfold RegistrationTransaction.Complete
    cases replacing : (state.transactions id).replacing <;>
      simpa [admissionPrefix, replacing, replacementPrefix, replaceWrites, admitWrites, List.append_assoc] using aligned
  have observations := stable id
  have selected := (choices id).2 (by simp [Chosen, phase])
  have observed := observations.1 (by simp [Observing, phase])
  unfold RegistrationTransaction.Complete at complete
  cases replacing : (state.transactions id).replacing
  · have sameChoice := selected.1 replacing
    have same : state.db.metadata = (state.transactions id).request.candidate.metadata := by
      exact Option.some.inj ((observed sameChoice.1).symm.trans sameChoice.2)
    simp only [replacing, Bool.false_eq_true, ↓reduceIte] at complete
    rw [complete]
    simp only [RegistrationTransaction.commit, RegistrationTransaction.sequential,
      sameChoice.1, same, and_self, ite_true]
    exact admit_exact state.db (state.transactions id).request
  · have listing := observations.2 (by simp [CapturedPhase, phase]) replacing
    have different : ¬((state.transactions id).request.kind = .ordinary ∧
        state.db.metadata = (state.transactions id).request.candidate.metadata) := by
      rintro ⟨ordinary, same⟩
      rcases selected.2 replacing with instanceKind | inequality
      · simp [ordinary] at instanceKind
      · exact inequality ((observed ordinary).trans (congrArg some same))
    simp only [replacing, ↓reduceIte] at complete
    rw [complete]
    simp only [RegistrationTransaction.commit, RegistrationTransaction.sequential, different, ite_false]
    exact replacement_exact state.db (state.transactions id).request _ listing

theorem committing_sequential_register (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (id : Nat) (phase : (state.core.transactions id).phase = .committing) :
    RegistrationTransaction.commit state.core.db (state.core.transactions id).staged =
      RegistrationTransaction.sequential state.core.db (registrations id) := by
  have invariant := reached_invariant methods accepted db registrations updates state execution
  have stable := (reached_stable methods accepted db registrations updates state execution).1
  rw [← (reached_requests methods accepted db registrations updates state execution id).1]
  exact register_effect_from_invariants state.core id phase invariant.2.2.2.2.1 invariant.2.2.2.2.2 stable

theorem sequential_update_owned (db : DB C L M B S D T R P F) (request : Request C M S D T) (frame : F) :
    sequentialUpdate (owned db frame) request = owned (sequentialUpdate db request) frame := by
  unfold sequentialUpdate owned
  cases found : db.registrations request.client <;> rfl

/-- All finite mixed schedules agree with independently specified per-client
update and register/replace operations in their real commit order. Service reply
order, HTTP authentication and physical SQL refinement are separate boundaries. -/
theorem execution_serial (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) :
    SerialHistory registrations updates db state.events (owned state.core.db db.frame) := by
  induction execution with
  | refl => simpa only [initial, RegistrationTransaction.initial, owned] using
      (SerialHistory.nil (registrations := registrations) (updates := updates) db)
  | tail previous step ih =>
    cases step
    case register nextCore actual actor contribution label method =>
      cases label
      case serverCommit id phase =>
        change SerialHistory registrations updates db (_ ++ [.register id _])
          (owned (RegistrationTransaction.commit _ _) db.frame)
        rw [committing_sequential_register methods accepted db registrations updates _ previous id phase,
          ← sequential_owned]
        exact SerialHistory.register ih id _
      all_goals simpa only [RegistrationTransaction.update, List.append_nil, owned] using ih
    case serverCommit id phase =>
      change SerialHistory registrations updates db (_ ++ [.update id _]) (owned (commit _ _) db.frame)
      rw [committing_sequential_update methods accepted db registrations updates _ previous id phase,
        (reached_requests methods accepted db registrations updates _ previous id).2,
        ← sequential_update_owned]
      exact SerialHistory.update ih id _
    all_goals simpa only [updateTx, owned] using ih

/-- Terminal phase, not a transport-error inference. -/
theorem register_committed_terminal (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (id : Nat)
    (committed : (before.core.transactions id).phase = .committed) :
    (after.core.transactions id).phase = .committed := by
  cases step
  case register nextCore actual actor contribution label method =>
    exact committed_terminal_step actual id committed
  all_goals exact committed

theorem update_committed_terminal (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (id : Nat)
    (committed : (before.updates id).phase = .committed) : (after.updates id).phase = .committed := by
  cases step <;> simp only [updateTx, put, committedState] at * <;> try split_ifs at *
  all_goals simp_all

def EventCommitted (state : State C L M B S D T R P F) : Event C L M B S D T R P → Prop
  | .register id _ => (state.core.transactions id).phase = .committed
  | .update id _ => (state.updates id).phase = .committed

def EventsCommitted (state : State C L M B S D T R P F) : Prop :=
  ∀ event, event ∈ state.events → EventCommitted state event

theorem event_terminal (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F} (step : Step true methods accepted before after)
    (event : Event C L M B S D T R P) (committed : EventCommitted before event) : EventCommitted after event := by
  cases event
  case register id writes => exact register_committed_terminal methods accepted step id committed
  case update id writes => exact update_committed_terminal methods accepted step id committed

theorem events_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F} (step : Step true methods accepted before after)
    (consistent : EventsCommitted before) : EventsCommitted after := by
  intro event member
  have preserved (wasPresent : event ∈ before.events) : EventCommitted after event :=
    event_terminal methods accepted step event (consistent event wasPresent)
  cases step
  case register nextCore actual actor contribution label method =>
    cases label
    case serverCommit id phase =>
      rcases List.mem_append.mp member with earlier | newest
      · exact preserved earlier
      · have equal := List.mem_singleton.mp newest
        subst event
        simp [EventCommitted, RegistrationTransaction.committedState, RegistrationTransaction.update, put]
    all_goals exact preserved (by simpa only [List.append_nil] using member)
  case serverCommit id phase =>
    rcases List.mem_append.mp member with earlier | newest
    · exact preserved earlier
    · have equal := List.mem_singleton.mp newest
      subst event
      simp [EventCommitted, committedState, updateTx, put]
  all_goals exact preserved member

theorem reached_events (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) : EventsCommitted state := by
  induction execution with
  | refl => intro event member; simp [initial] at member
  | tail previous step ih => exact events_step methods accepted step ih

/-- Aborted updates (including successful no-ops after rollback) and aborted
register attempts contribute no ACTUAL commit event. This does not remove effects
of other writers or claim that equivalent row values never existed previously. -/
theorem aborted_events_excluded (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) (id : Nat) :
    ((state.core.transactions id).phase = .aborted →
      ∀ writes, Event.register id writes ∉ state.events) ∧
    ((state.updates id).phase = .aborted → ∀ writes, Event.update id writes ∉ state.events) := by
  have events := reached_events methods accepted db registrations updates state execution
  constructor
  · intro aborted writes member
    have committed := events (.register id writes) member
    simp [EventCommitted, aborted] at committed
  · intro aborted writes member
    have committed := events (.update id writes) member
    simp [EventCommitted, aborted] at committed

#print axioms committing_sequential_register
#print axioms execution_serial
#print axioms aborted_events_excluded
run_cmd do
  for declaration in [``committing_sequential_register, ``execution_serial, ``aborted_events_excluded] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed mixed registration history axiom: {name}"

end
end RegistrationUpdate
