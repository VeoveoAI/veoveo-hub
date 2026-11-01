import RegistrationLifetimeSpecification
import RegistrationEffects
import Lean.Util.CollectAxioms

namespace RegistrationLifetime
open RegistrationTransaction (put rowMap)
variable {C L M R P I D F : Type}
variable [DecidableEq C] [DecidableEq L]

def Holding (phase : Phase) : Prop :=
  phase = .held ∨ phase = .upserting ∨ phase = .deleting ∨ phase = .dropping ∨
  phase = .creating ∨ phase = .inserting ∨ phase = .erasing ∨ phase = .ready ∨ phase = .committing

def Captured (phase : Phase) : Prop :=
  phase = .dropping ∨ phase = .creating ∨ phase = .inserting ∨
  phase = .erasing ∨ phase = .ready ∨ phase = .committing

def Ownership (s : State C L M R P I D F) : Prop :=
  ∀ id, Holding (s.transactions id).phase → s.instanceOwner = some id

/-- Ordered private SQL prefix, independent of the desired output contract. -/
def Aligned (tx : Transaction C L M R P I D) : Prop :=
  match tx.phase with
  | .idle | .preflight | .waiting | .held | .upserting =>
      tx.staged = [] ∧ tx.captured = [] ∧ tx.drops = []
  | .deleting => tx.staged = prelude tx.request ∧ tx.captured = [] ∧ tx.drops = []
  | .dropping => ∃ done : List L,
      done ++ tx.drops = tx.captured.map (·.login) ∧
      tx.staged = prelude tx.request ++ deletedRows tx.captured ++ droppedLogins done
  | .creating => ∃ proposal, tx.request = .register proposal ∧
      tx.staged = registerPrelude proposal ++ deletionProgram tx.captured
  | .inserting => ∃ proposal, tx.request = .register proposal ∧
      tx.staged = registerPrelude proposal ++ deletionProgram tx.captured ++
        [.registration (.createLogin proposal.candidate.login proposal.role)]
  | .erasing => tx.request = .remove ∧ tx.staged = deletionProgram tx.captured
  | .ready | .committing | .committed => Complete tx
  | .absent | .abortPending | .aborted => True

def Alignment (s : State C L M R P I D F) : Prop := ∀ id, Aligned (s.transactions id)

/-- Full instance equality is scoped to the represented two-writer domain. Native
Factory revision/pusher/served/failure updates are NOT silently included. DELETE
stability is current committed state until this attempt commits/aborts. -/
def StableObservations (s : State C L M R P I D F) : Prop :=
  ∀ id,
    (Holding (s.transactions id).phase →
      (s.transactions id).lockedInstance = s.db.instanceRow ∧
      (s.transactions id).lockedInstance.isSome = true) ∧
    (Captured (s.transactions id).phase → Listing s.db (s.transactions id).captured)

theorem initial_ownership (db : Database C L M R P I D F) requests :
    Ownership (initial db requests) := by
  intro id active
  simp [initial, Holding] at active

theorem ownership_step {before after : State C L M R P I D F}
    (step : Step true before after) (owned : Ownership before) : Ownership after := by
  intro subject active
  have previous := owned subject
  cases step <;> simp only [update, put, committedState, release] at * <;>
    try split_ifs at *
  all_goals simp_all [Holding]

theorem execution_ownership {before after : State C L M R P I D F}
    (execution : Execution true before after) (owned : Ownership before) : Ownership after := by
  induction execution with
  | refl => exact owned
  | tail previous step ih => exact ownership_step step ih

theorem active_owner_unique (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (left right : Nat)
    (leftActive : Holding (s.transactions left).phase)
    (rightActive : Holding (s.transactions right).phase) : left = right := by
  have owned := execution_ownership execution (initial_ownership db requests)
  have same := (owned left leftActive).symm.trans (owned right rightActive)
  exact Option.some.inj same

theorem initial_alignment (db : Database C L M R P I D F) requests :
    Alignment (initial db requests) := by
  intro id
  simp [initial, Aligned]

theorem aligned_drop (tx : Transaction C L M R P I D) (login : L) (remaining : List L)
    (phase : tx.phase = .dropping) (next : tx.drops = login :: remaining)
    (aligned : Aligned tx) :
    Aligned { tx with
      drops := remaining
      staged := tx.staged ++ [.registration (.dropLogin login)] } := by
  simp only [Aligned, phase] at aligned ⊢
  obtain ⟨done, progress, staged⟩ := aligned
  refine ⟨done ++ [login], ?_, ?_⟩
  · simpa only [List.append_assoc, List.singleton_append, next] using progress
  · simp [staged, droppedLogins, List.map_append, List.append_assoc]

/-- Resolve the request-dependent post-acquisition phase before inspecting the
source prefix invariant. lockedInstance itself is not an alignment component. -/
theorem aligned_after_lock (tx : Transaction C L M R P I D)
    (phase : tx.phase = .waiting) (aligned : Aligned tx) :
    Aligned { tx with phase := afterLock tx.request } := by
  have empty : tx.staged = [] ∧ tx.captured = [] ∧ tx.drops = [] := by
    simpa only [Aligned, phase] using aligned
  cases method : tx.request <;>
    simpa [Aligned, afterLock, method, prelude] using empty

theorem alignment_step {fixed : Bool} {before after : State C L M R P I D F}
    (step : Step fixed before after) (aligned : Alignment before) : Alignment after := by
  cases step
  case acquire id inst mode phase free present =>
    intro subject
    by_cases same : subject = id
    · subst subject
      have acquired := aligned_after_lock (before.transactions id) phase (aligned id)
      cases method : (before.transactions id).request <;>
        simpa [Aligned, afterLock, method, update, put] using acquired
    · simpa [update, put, same] using aligned subject
  case originalNoSelect id mode phase =>
    intro subject
    by_cases same : subject = id
    · subst subject
      simpa [update, put] using aligned_after_lock (before.transactions id) phase (aligned id)
    · simpa [update, put, same] using aligned subject
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
  all_goals
    simp_all [Aligned, prelude, registerPrelude, Complete, completeProgram,
      deletionProgram, deletedRows, droppedLogins,
      List.append_assoc]
  all_goals exact ⟨[], by simp, by simp_all⟩

theorem execution_alignment {fixed : Bool} {before after : State C L M R P I D F}
    (execution : Execution fixed before after) (aligned : Alignment before) : Alignment after := by
  induction execution with
  | refl => exact aligned
  | tail previous step ih => exact alignment_step step ih

theorem committing_program_complete (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (phase : (s.transactions id).phase = .committing) : Complete (s.transactions id) := by
  have aligned := execution_alignment execution (initial_alignment db requests) id
  simpa only [Aligned, phase] using aligned

/-- Full removal program is derived for every reached commit request. It is not
a premise of primitive serverCommit and does not assume the desired no-orphan DB. -/
theorem complete_removal_program (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (method : (s.transactions id).request = .remove)
    (phase : (s.transactions id).phase = .committing) :
    (s.transactions id).staged =
      deletedRows (s.transactions id).captured ++
      droppedLogins ((s.transactions id).captured.map (·.login)) ++ [.deleteInstance] := by
  have aligned := execution_alignment execution (initial_alignment db requests) id
  simpa [Aligned, phase, Complete, completeProgram, method, deletionProgram] using aligned

theorem prelude_preserves_rows (db : Database C L M R P I D F) (request : Request C L M R P D) :
    (applyWrites db (prelude request)).registrations = db.registrations := by
  cases request <;> rfl

theorem initial_observations (db : Database C L M R P I D F) requests :
    StableObservations (initial db requests) := by
  intro id
  simp [initial, Holding, Captured]

theorem captured_holding (phase : Phase) (active : Captured phase) : Holding phase := by
  simp only [Captured] at active
  rcases active with active | active | active | active | active | active <;> simp_all [Holding]

/-- Stable observation preservation uses actual shared lock ownership. The only
committed-map mutation is a serverCommit, which cannot belong to another active
attempt; frame steps affect no owned fields. Source DELETE's staged prelude has
no registration-map effects, derived from source-order alignment. -/
theorem observations_step {before after : State C L M R P I D F}
    (step : Step true before after) (owned : Ownership before)
    (aligned : Alignment before) (stable : StableObservations before) : StableObservations after := by
  cases step
  case acquire id inst mode phase free present =>
    intro subject
    by_cases same : subject = id
    · subst subject
      cases method : (before.transactions id).request <;>
        simp [update, put, afterLock, method, Holding, Captured, present]
    · simpa [update, put, same] using stable subject
  case originalNoSelect id mode phase => cases mode
  case delete id victims phase snapshot =>
    intro subject
    by_cases same : subject = id
    · subst subject
      have sourcePrefix := aligned id
      simp only [Aligned, phase] at sourcePrefix
      have current : Listing before.db victims := by
        rcases snapshot with ⟨unique, values⟩
        rw [sourcePrefix.1, prelude_preserves_rows] at values
        exact ⟨unique, values⟩
      constructor
      · intro _
        simpa [update, put] using (stable id).1 (by simp [Holding, phase])
      · intro _
        simpa [update, put] using current
    · simpa [update, put, same] using stable subject
  case erase id inst phase method available present =>
    intro subject
    by_cases same : subject = id
    · subst subject
      simp only [update, put, if_pos rfl]
      constructor
      · intro _
        exact ⟨present.symm, rfl⟩
      · intro _
        simpa [update, put] using (stable id).2 (by simp [Captured, phase])
    · simpa [update, put, same] using stable subject
  case serverCommit id phase =>
    intro subject
    by_cases same : subject = id
    · subst subject
      simp [committedState, update, put, Holding, Captured]
    · have impossible (active : Holding (before.transactions subject).phase) : False := by
        have subjectOwner := owned subject active
        have actorOwner := owned id (by simp [Holding, phase])
        exact same (Option.some.inj (subjectOwner.symm.trans actorOwner))
      constructor
      · intro active
        exact False.elim (impossible (by simpa [committedState, update, put, same] using active))
      · intro captured
        exact False.elim (impossible (captured_holding _ (by
          simpa [committedState, update, put, same] using captured)))
  case frame value => exact stable
  all_goals
    intro subject
    have observed := stable subject
    simp only [update, put, release]
    split_ifs with same
    · subst subject
      constructor
      · intro active
        apply observed.1
        simp_all [Holding]
      · intro captured
        apply observed.2
        simp_all [Captured]
    · exact observed

theorem execution_observations {before after : State C L M R P I D F}
    (execution : Execution true before after) (owned : Ownership before)
    (aligned : Alignment before) (stable : StableObservations before) : StableObservations after := by
  induction execution with
  | refl => exact stable
  | tail previous step ih =>
      exact observations_step step (execution_ownership previous owned)
        (execution_alignment previous aligned) ih

theorem reached_current_delete_snapshot (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (phase : Captured (s.transactions id).phase) : Listing s.db (s.transactions id).captured := by
  exact (execution_observations execution (initial_ownership db requests)
    (initial_alignment db requests) (initial_observations db requests) id).2 phase

/-- Reuse the existing row-map theorem, without inventing a metadata value to
project an absent deployment into the old retained-deployment Database. -/
theorem delete_listing_clears (db : Database C L M R P I D F)
    (rows : List (RegistrationTransaction.Registration C L M R)) (listing : Listing db rows) :
    (applyWrites db (deletedRows rows)).registrations = fun _ => none := by
  classical
  funext key
  change (if key ∈ rows.map (·.client) then none else db.registrations key) = none
  by_cases member : key ∈ rows.map (·.client)
  · rw [if_pos member]
  · rw [if_neg member, listing.2]
    exact RegistrationTransaction.rowMap_none_of_not_mem rows key member

theorem registered_login_listing (db : Database C L M R P I D F)
    (rows : List (RegistrationTransaction.Registration C L M R)) (listing : Listing db rows) (login : L) :
    registeredLogin db login ↔ login ∈ rows.map (·.login) := by
  classical
  constructor
  · rintro ⟨key, row, stored, equal⟩
    rw [listing.2] at stored
    have member := ((RegistrationTransaction.rowMap_some_iff rows listing.1 key row).mp stored).1
    exact List.mem_map.mpr ⟨row, member, equal⟩
  · intro member
    obtain ⟨row, inRows, equal⟩ := List.mem_map.mp member
    refine ⟨row.client, row, ?_, equal⟩
    rw [listing.2]
    exact (RegistrationTransaction.rowMap_some_iff rows listing.1 row.client row).mpr ⟨inRows, rfl⟩

theorem drop_logins_exact (db : Database C L M R P I D F) (logins : List L) :
    applyWrites db (droppedLogins logins) =
      { db with logins := fun login => if login ∈ logins then none else db.logins login } := by
  induction logins generalizing db with
  | nil => rfl
  | cons head tail ih =>
    change applyWrites (applyWrite db (.registration (.dropLogin head))) (droppedLogins tail) = _
    rw [ih]
    apply Database.ext <;> try rfl
    funext login
    by_cases same : login = head <;> by_cases member : login ∈ tail <;>
      simp [applyWrite, put, same, member]

theorem writes_append (db : Database C L M R P I D F) (left right : List (Write C L M R P D)) :
    applyWrites db (left ++ right) = applyWrites (applyWrites db left) right := List.foldl_append

/-- Independent complete removal effects, including unrelated-login and full
immutable deployment/instance/frame payloads. Native shared group state is not
this owned login map. Later the current listing premise is derived from locking. -/
theorem removal_program_exact (db : Database C L M R P I D F)
    (rows : List (RegistrationTransaction.Registration C L M R)) (listing : Listing db rows) :
    applyWrites db (deletionProgram rows ++ [.deleteInstance]) = removal db := by
  classical
  let base := applyWrites db (deletedRows rows)
  have clears : base.registrations = fun _ => none := delete_listing_clears db rows listing
  unfold deletionProgram
  rw [writes_append, writes_append, drop_logins_exact]
  apply Database.ext
  · rfl
  · rfl
  · exact clears
  · funext login
    have correspondence := registered_login_listing db rows listing login
    change (if login ∈ rows.map (·.login) then none else db.logins login) =
      (if registeredLogin db login then none else db.logins login)
    by_cases member : login ∈ rows.map (·.login)
    · rw [if_pos member, if_pos (correspondence.mpr member)]
    · have absent : ¬registeredLogin db login := fun present => member (correspondence.mp present)
      rw [if_neg member, if_neg absent]
  · rfl

/-- A reached fixed commit derives BOTH its current snapshot and complete staged
program; the resulting full database equals the independent removal operation.
No desired intermediate state or no-orphan guard occurs among the premises. -/
theorem reached_removal_effect (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (method : (s.transactions id).request = .remove)
    (phase : (s.transactions id).phase = .committing) :
    (committedState s id).db = removal s.db ∧ (s.db.instanceRow).isSome = true := by
  have observations := execution_observations execution (initial_ownership db requests)
    (initial_alignment db requests) (initial_observations db requests) id
  have listing := observations.2 (by simp [Captured, phase])
  have present := observations.1 (by simp [Holding, phase])
  constructor
  · change applyWrites s.db (s.transactions id).staged = removal s.db
    rw [complete_removal_program db requests s execution id method phase]
    exact removal_program_exact s.db (s.transactions id).captured listing
  · rw [← present.1]
    exact present.2

#print axioms execution_ownership
#print axioms active_owner_unique
#print axioms execution_alignment
#print axioms committing_program_complete
#print axioms complete_removal_program
#print axioms execution_observations
#print axioms reached_current_delete_snapshot
#print axioms removal_program_exact
#print axioms reached_removal_effect

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``initial_ownership, ``ownership_step, ``execution_ownership,
      ``active_owner_unique, ``initial_alignment, ``aligned_drop, ``aligned_after_lock, ``alignment_step,
      ``execution_alignment, ``committing_program_complete, ``complete_removal_program, ``prelude_preserves_rows,
      ``initial_observations, ``captured_holding, ``observations_step,
      ``execution_observations, ``reached_current_delete_snapshot, ``delete_listing_clears,
      ``registered_login_listing, ``drop_logins_exact, ``writes_append,
      ``removal_program_exact, ``reached_removal_effect,
      ``applyWrite, ``applyWrites, ``ValidWrite, ``committedState,
      ``replacement, ``removal, ``sequential] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected registration lifetime proof axiom in {declaration}: {unexpected}"

end RegistrationLifetime
