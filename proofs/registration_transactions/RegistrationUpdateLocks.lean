import RegistrationUpdateProof

namespace RegistrationUpdate
open RegistrationTransaction
variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- Lift the original ORDERED register prefix invariant without treating its
complete effect as an atomic primitive. Update steps leave those programs alone. -/
theorem registration_alignment_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after)
    (aligned : RegistrationTransaction.Alignment before.core) :
    RegistrationTransaction.Alignment after.core := by
  cases step
  case register nextCore actual actor contribution label method =>
    exact RegistrationTransaction.alignment_step actual aligned
  all_goals exact aligned

theorem registration_choices_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after)
    (aligned : RegistrationTransaction.Alignment before.core)
    (choices : Choices before.core) : Choices after.core := by
  cases step
  case register nextCore actual actor contribution label method =>
    exact choices_step actual aligned choices
  all_goals exact choices

/-- A non-idle physical update must be classified as an update. In particular,
its rollback cannot release a registration attempt's lock under the same ID. -/
def UpdateMethods (methods : Nat → Method) (state : State C L M B S D T R P F) : Prop :=
  ∀ id, (state.updates id).phase ≠ .idle → methods id = .update

def RegisterMethods (methods : Nat → Method) (state : State C L M B S D T R P F) : Prop :=
  ∀ id, (state.core.transactions id).phase ≠ .idle → methods id = .register

/-- Failure can occur before or after acquiring the lock. The nonempty staged
program distinguishes a locked rollbackPending path from an early error. -/
def UpdateHolding (tx : Transaction C L M B S D T R) : Prop :=
  tx.phase = .metadataWritten ∨ tx.phase = .ready ∨ tx.phase = .committing ∨
    (tx.phase = .rollbackPending ∧ tx.staged ≠ [])

def Ownership (state : State C L M B S D T R P F) : Prop :=
  RegistrationTransaction.Ownership state.core ∧
    ∀ id, UpdateHolding (state.updates id) → state.core.lock = some id

/-- Any REAL register step preserves the lock of a differently classified
update. Acquire/upsert cannot cross it, and release only releases its own ID. -/
theorem register_preserves_update_lock (methods : Nat → Method)
    {before after : RegisterState C L M B S D T R P F}
    {actual : RegistrationTransaction.Step true before after}
    {actor : Option Nat} {contribution : List (Event C L M B S D T R P)}
    (label : RegisterAction actual actor contribution)
    (method : ∀ id, actor = some id → methods id = .register)
    (subject : Nat) (typed : methods subject = .update)
    (owns : before.lock = some subject) : after.lock = some subject := by
  cases label <;> simp only [RegistrationTransaction.update,
    RegistrationTransaction.committedState, release] at * <;> try split_ifs at *
  all_goals simp_all

theorem update_methods_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (typed : UpdateMethods methods before) :
    UpdateMethods methods after := by
  intro subject active
  have atSubject := typed subject
  clear typed
  cases step <;> simp only [updateTx, put, committedState] at * <;> try split_ifs at *
  all_goals simp_all
  all_goals
    apply atSubject
    intro idle
    simp_all

theorem register_methods_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (typed : RegisterMethods methods before) :
    RegisterMethods methods after := by
  intro subject active
  have atSubject := typed subject
  clear typed
  cases step
  case register nextCore actual actor contribution label method =>
    cases label <;> simp only [RegistrationTransaction.update,
      RegistrationTransaction.committedState, put] at * <;> try split_ifs at *
    all_goals simp_all
  all_goals exact atSubject active

theorem update_holding_not_idle (tx : Transaction C L M B S D T R)
    (active : UpdateHolding tx) : tx.phase ≠ .idle := by
  rcases active with active | active | active | ⟨active, _⟩ <;> simp [active]

theorem register_holding_not_idle (phase : RegistrationTransaction.Phase)
    (active : Holding phase) : phase ≠ .idle := by
  intro idle
  subst phase
  simp [Holding] at active

theorem release_register_lock (methods : Nat → Method) (lock : Option Nat)
    (subject actor : Nat) (owns : lock = some subject)
    (registered : methods subject = .register) (updating : methods actor = .update) :
    release lock actor = some subject := by
  have different : subject ≠ actor := by
    intro same
    rw [same, updating] at registered
    cases registered
  simp [release, owns, different]

/-- The original registration lock theorem lifts across update transitions.
In particular an update rollback cannot release a same-ID registration lock. -/
theorem registration_ownership_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after)
    (registerTyped : RegisterMethods methods before) (updateTyped : UpdateMethods methods before)
    (ownership : RegistrationTransaction.Ownership before.core) :
    RegistrationTransaction.Ownership after.core := by
  cases step
  case register nextCore actual actor contribution label method =>
    exact RegistrationTransaction.ownership_step actual ownership
  case metadata id phase free valid =>
    intro subject holding
    have owner := ownership subject holding
    rw [free] at owner
    cases owner
  case serverCommit id phase =>
    intro subject holding
    exact release_register_lock methods before.core.lock subject id (ownership subject holding)
      (registerTyped subject (register_holding_not_idle _ holding))
      (updateTyped id (by simp [phase]))
  case rollbackNoop id phase outcome =>
    intro subject holding
    exact release_register_lock methods before.core.lock subject id (ownership subject holding)
      (registerTyped subject (register_holding_not_idle _ holding))
      (updateTyped id (by simp [phase]))
  case serverAbort id phase =>
    intro subject holding
    have updating : methods id = .update := updateTyped id (by
      rcases phase with phase | phase <;> simp [phase])
    exact release_register_lock methods before.core.lock subject id (ownership subject holding)
      (registerTyped subject (register_holding_not_idle _ holding)) updating
  all_goals exact ownership

/-- Symmetric shared-lock property for updates, including failed partial writes.
Only source phase/order and the actual register constructors establish it. -/
theorem update_ownership_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after)
    (typed : UpdateMethods methods before) (alignment : Alignment before)
    (ownership : ∀ id, UpdateHolding (before.updates id) → before.core.lock = some id) :
    ∀ id, UpdateHolding (after.updates id) → after.core.lock = some id := by
  cases step
  case register nextCore actual actor contribution label method =>
    intro subject active
    exact register_preserves_update_lock methods label method subject
      (typed subject (update_holding_not_idle _ active)) (ownership subject active)
  case fail id method notCommitted notCommitting notAborted =>
    intro subject active
    by_cases same : subject = id
    · subst subject
      have nonempty : (before.updates id).staged ≠ [] := by
        simpa [updateTx, put, UpdateHolding] using active
      have aligned := alignment id
      have wasHolding : UpdateHolding (before.updates id) := by
        cases previous : (before.updates id).phase <;>
          simp_all [Aligned, UpdateHolding, Complete, NoopProgram]
      exact ownership id wasHolding
    · apply ownership subject
      simpa [updateTx, put, same] using active
  all_goals
    intro subject active
    have atSubject := ownership subject
    clear ownership
    simp only [updateTx, put, committedState, release] at *
    try split_ifs at *
  all_goals simp_all [UpdateHolding]

/-- Simultaneously maintained invariants; the old register program/choice
lemmas are lifted, not replaced by an assumed complete registration operation. -/
def Invariant (methods : Nat → Method) (state : State C L M B S D T R P F) : Prop :=
  Alignment state ∧ UpdateMethods methods state ∧ RegisterMethods methods state ∧
    Ownership state ∧ RegistrationTransaction.Alignment state.core ∧ Choices state.core

theorem initial_invariant (methods : Nat → Method) (db : DB C L M B S D T R P F)
    registrations updates : Invariant methods (initial db registrations updates) := by
  refine ⟨initial_alignment db registrations updates, ?_, ?_, ?_,
    RegistrationTransaction.initial_alignment db registrations,
    initial_choices db registrations⟩
  · intro id active
    simp [initial] at active
  · intro id active
    simp [initial, RegistrationTransaction.initial] at active
  · refine ⟨RegistrationTransaction.initial_ownership db registrations, ?_⟩
    intro id active
    simp [initial, UpdateHolding] at active

theorem invariant_step (methods : Nat → Method) (accepted : MetadataAccepted M B)
    {before after : State C L M B S D T R P F}
    (step : Step true methods accepted before after) (invariant : Invariant methods before) :
    Invariant methods after := by
  rcases invariant with ⟨aligned, updateTyped, registerTyped, ownership, registerAligned, choices⟩
  exact ⟨alignment_step methods accepted step aligned,
    update_methods_step methods accepted step updateTyped,
    register_methods_step methods accepted step registerTyped,
    ⟨registration_ownership_step methods accepted step registerTyped updateTyped ownership.1,
      update_ownership_step methods accepted step updateTyped aligned ownership.2⟩,
    registration_alignment_step methods accepted step registerAligned,
    registration_choices_step methods accepted step registerAligned choices⟩

/-- Shared deployment ownership for arbitrary finite MIXED histories, while
retaining both methods' ordered programs and the real register branch choice. -/
theorem reached_invariant (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state) :
    Invariant methods state := by
  induction execution with
  | refl => exact initial_invariant methods db registrations updates
  | tail previous step ih => exact invariant_step methods accepted step ih

theorem active_methods_exclude_each_other (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (db : DB C L M B S D T R P F) registrations updates state
    (execution : Execution true methods accepted (initial db registrations updates) state)
    (registerId updateId : Nat)
    (registerActive : Holding (state.core.transactions registerId).phase)
    (updateActive : UpdateHolding (state.updates updateId)) : False := by
  rcases reached_invariant methods accepted db registrations updates state execution with
    ⟨_, updateTyped, registerTyped, ⟨registerOwns, updateOwns⟩, _, _⟩
  have same : registerId = updateId := Option.some.inj
    ((registerOwns registerId registerActive).symm.trans (updateOwns updateId updateActive))
  have registered := registerTyped registerId (register_holding_not_idle _ registerActive)
  have updating := updateTyped updateId (update_holding_not_idle _ updateActive)
  rw [same, updating] at registered
  cases registered

#print axioms reached_invariant
#print axioms active_methods_exclude_each_other
run_cmd do
  for declaration in [``reached_invariant, ``active_methods_exclude_each_other] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed mixed registration lock axiom: {name}"

end RegistrationUpdate
