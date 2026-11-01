import RegistrationLifetimeProof

/-! Exact registration effects and actual finite commit histories for the fixed
instance-only protocol. Environmental frame changes remain explicit history
steps. Response tags are not a specification of complete API response payloads. -/
namespace RegistrationLifetime
open RegistrationTransaction (put rowMap)
variable {C L M R P I D F : Type}
variable [DecidableEq C] [DecidableEq L]

/-- Complete captured deletion/role-drop effects, before removal's instance DELETE.
Unrelated roles, immutable deployment payload and application frame are retained. -/
theorem deletion_program_exact (db : Database C L M R P I D F)
    (rows : List (RegistrationTransaction.Registration C L M R)) (listing : Listing db rows) :
    applyWrites db (deletionProgram rows) =
      { removal db with instanceRow := db.instanceRow } := by
  classical
  have clears := delete_listing_clears db rows listing
  unfold deletionProgram
  rw [writes_append, drop_logins_exact]
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

/-- The registration program handles BOTH first insertion and an existing
immutable deployment. Its result is the independent full replacement contract,
not a fold supplied as a desired-output premise. -/
theorem registration_program_exact (db : Database C L M R P I D F)
    (proposal : Proposal C L M R P D)
    (rows : List (RegistrationTransaction.Registration C L M R)) (listing : Listing db rows) :
    applyWrites db (completeProgram (.register proposal) rows) = replacement db proposal := by
  classical
  let prepared := applyWrites db (registerPrelude proposal)
  have snapshot : Listing prepared rows := listing
  change applyWrites db (registerPrelude proposal ++ deletionProgram rows ++
    [.registration (.createLogin proposal.candidate.login proposal.role),
      .registration (.insert proposal.candidate)]) = replacement db proposal
  rw [writes_append, writes_append]
  change applyWrites (applyWrites prepared (deletionProgram rows)) _ = _
  rw [deletion_program_exact prepared rows snapshot]
  apply Database.ext
  · rfl
  · change upsert db.deployment proposal.candidate.metadata proposal.firstDeployment =
      some ⟨proposal.candidate.metadata,
        match db.deployment with | none => proposal.firstDeployment | some old => old.stable⟩
    cases db.deployment <;> rfl
  · rfl
  · funext login
    by_cases same : login = proposal.candidate.login <;>
      simp [applyWrites, applyWrite, registerPrelude, prepared, removal, replacement, put,
        registeredLogin, same]
    all_goals rfl
  · rfl

/-- Derive current snapshot, complete writes and instance presence from a reached
execution. No assumption of a desired intermediate/final database is needed. -/
theorem reached_registration_effect (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (proposal : Proposal C L M R P D)
    (method : (s.transactions id).request = .register proposal)
    (phase : (s.transactions id).phase = .committing) :
    (committedState s id).db = replacement s.db proposal ∧ s.db.instanceRow.isSome = true := by
  have observations := execution_observations execution (initial_ownership db requests)
    (initial_alignment db requests) (initial_observations db requests) id
  have listing := observations.2 (by simp [Captured, phase])
  have present := observations.1 (by simp [Holding, phase])
  have complete := committing_program_complete db requests s execution id phase
  constructor
  · change applyWrites s.db (s.transactions id).staged = replacement s.db proposal
    rw [show (s.transactions id).staged =
      completeProgram (.register proposal) (s.transactions id).captured from by
        simpa only [Complete, method] using complete]
    exact registration_program_exact s.db proposal (s.transactions id).captured listing
  · rw [← present.1]
    exact present.2

/-- Every attempt keeps its ORIGINAL supplied request, even across cached preflight,
private SQL staging, rollback, reply loss and unrelated environmental activity. -/
def OriginalRequests (requests : Nat → Request C L M R P D) (s : State C L M R P I D F) : Prop :=
  ∀ id, (s.transactions id).request = requests id

theorem initial_original_requests (db : Database C L M R P I D F) requests :
    OriginalRequests requests (initial db requests) := by
  intro id
  rfl

theorem original_requests_step {fixed : Bool} {before after : State C L M R P I D F}
    (step : Step fixed before after) (stable : OriginalRequests requests before) :
    OriginalRequests requests after := by
  cases step <;> intro subject <;>
    simp only [update, put, committedState] <;> try split_ifs
  all_goals simp_all [OriginalRequests]

theorem execution_original_requests {fixed : Bool} {before after : State C L M R P I D F}
    (execution : Execution fixed before after) (stable : OriginalRequests requests before) :
    OriginalRequests requests after := by
  induction execution with
  | refl => exact stable
  | tail previous step ih => exact original_requests_step step ih

/-- Both kinds of reached server commit implement the independent operation of
that attempt's original request, against the CURRENT locked database. -/
theorem reached_commit_effect (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (phase : (s.transactions id).phase = .committing) :
    (committedState s id).db = sequential s.db (requests id) := by
  have original := execution_original_requests execution (initial_original_requests db requests) id
  rw [← original]
  cases method : (s.transactions id).request with
  | register proposal =>
    have effect := reached_registration_effect db requests s execution id proposal method phase
    cases present : s.db.instanceRow with
    | none => simp [present] at effect
    | some inst => simpa only [sequential, present] using effect.1
  | remove =>
    have effect := reached_removal_effect db requests s execution id method phase
    cases present : s.db.instanceRow with
    | none => simp [present] at effect
    | some inst => simpa only [sequential, present] using effect.1

/-- One concrete native-model step either contributes its actual commit event,
changes only the explicitly disjoint frame, or leaves both database and events
unchanged. Staging, aborts and response delivery do not masquerade as commits. -/
theorem reached_serial_step (db : Database C L M R P I D F) requests
    {before after : State C L M R P I D F}
    (execution : Execution true (initial db requests) before)
    (step : Step true before after)
    (history : SerialHistory requests db before.commits before.db) :
    SerialHistory requests db after.commits after.db := by
  cases step
  case serverCommit id phase =>
    have effect := reached_commit_effect db requests before execution id phase
    change SerialHistory requests db
      (before.commits ++ [(id, (before.transactions id).staged)]) (committedState before id).db
    rw [effect]
    exact SerialHistory.snoc history id (before.transactions id).staged
  case frame value => exact SerialHistory.frame history value
  all_goals exact history

/-- Full current DB equality, with its actual server commit order, for EVERY finite
fixed execution. Frame interference is represented, not assumed absent. -/
theorem execution_serial (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) :
    SerialHistory requests db s.commits s.db := by
  induction execution with
  | refl => exact SerialHistory.nil db
  | tail previous step ih => exact reached_serial_step db requests previous step ih

theorem sequential_no_orphan (db : Database C L M R P I D F)
    (request : Request C L M R P D) (safe : NoOrphanRows db) :
    NoOrphanRows (sequential db request) := by
  cases present : db.instanceRow with
  | none => simpa only [sequential, present] using safe
  | some inst => cases request <;> simp [sequential, present, NoOrphanRows, replacement, removal]

theorem serial_no_orphan {requests : Nat → Request C L M R P D}
    {first last : Database C L M R P I D F} {events}
    (history : SerialHistory requests first events last) (safe : NoOrphanRows first) :
    NoOrphanRows last := by
  induction history with
  | nil => exact safe
  | frame previous value ih => exact ih
  | snoc previous id actual ih => exact sequential_no_orphan _ _ ih

/-- Initial no-orphan state is explicit. SQL constraints alone deliberately do not
imply this lifetime invariant. Unrelated login roles need not disappear. -/
theorem execution_no_orphan (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (safe : NoOrphanRows db) :
    NoOrphanRows s.db := serial_no_orphan (execution_serial db requests s execution) safe

/-- Complete removal specifically drops every login of the actual captured
registration set; external/shared/bootstrap roles are not claimed to vanish. -/
theorem reached_removal_drops_registered_login (db : Database C L M R P I D F) requests s
    (execution : Execution true (initial db requests) s) (id : Nat)
    (method : (s.transactions id).request = .remove)
    (phase : (s.transactions id).phase = .committing)
    (key : C) (row : RegistrationTransaction.Registration C L M R)
    (stored : s.db.registrations key = some row) :
    (committedState s id).db.logins row.login = none := by
  classical
  rw [(reached_removal_effect db requests s execution id method phase).1]
  change (if registeredLogin s.db row.login then none else s.db.logins row.login) = none
  exact if_pos ⟨key, row, stored, rfl⟩

/-- An observed commit event corresponds to a terminal committed transaction;
subsequent response errors keep that decision rather than roll it back. -/
def CommittedEvents (s : State C L M R P I D F) : Prop :=
  ∀ id writes, (id, writes) ∈ s.commits → (s.transactions id).phase = .committed

theorem initial_committed_events (db : Database C L M R P I D F) requests :
    CommittedEvents (initial db requests) := by
  intro id writes member
  simp [initial] at member

theorem committed_events_step {fixed : Bool} {before after : State C L M R P I D F}
    (step : Step fixed before after) (recorded : CommittedEvents before) : CommittedEvents after := by
  cases step
  case serverCommit actor phase =>
    intro subject writes member
    change (subject, writes) ∈ before.commits ++ [(actor, (before.transactions actor).staged)] at member
    rcases List.mem_append.mp member with old | latest
    · have previous := recorded subject writes old
      by_cases same : subject = actor
      · subst subject
        simp [committedState, update, put]
      · simpa [committedState, update, put, same] using previous
    · have same : subject = actor := congrArg Prod.fst (List.mem_singleton.mp latest)
      subst subject
      simp [committedState, update, put]
  case frame value => exact recorded
  all_goals
    intro subject writes member
    have previous := recorded subject writes member
    simp only [update, put]
    split_ifs with same
    · subst subject
      simp_all
    · exact previous

theorem execution_committed_events {fixed : Bool} {before after : State C L M R P I D F}
    (execution : Execution fixed before after) (recorded : CommittedEvents before) : CommittedEvents after := by
  induction execution with
  | refl => exact recorded
  | tail previous step ih => exact committed_events_step step ih

/-- CONFIRMED server abort is excluded from the actual commit sequence. A mere
service/error response is deliberately insufficient: it may follow a real commit. -/
theorem confirmed_abort_has_no_event {fixed : Bool} (db : Database C L M R P I D F) requests s
    (execution : Execution fixed (initial db requests) s) (id : Nat)
    (aborted : (s.transactions id).phase = .aborted) (writes : List (Write C L M R P D)) :
    (id, writes) ∉ s.commits := by
  intro member
  have committed := execution_committed_events execution (initial_committed_events db requests) id writes member
  simp [aborted] at committed

#print axioms registration_program_exact
#print axioms reached_registration_effect
#print axioms execution_original_requests
#print axioms reached_commit_effect
#print axioms execution_serial
#print axioms execution_no_orphan
#print axioms reached_removal_drops_registered_login
#print axioms execution_committed_events
#print axioms confirmed_abort_has_no_event

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``deletion_program_exact, ``registration_program_exact,
      ``reached_registration_effect, ``OriginalRequests, ``initial_original_requests,
      ``original_requests_step, ``execution_original_requests, ``reached_commit_effect,
      ``reached_serial_step, ``execution_serial, ``sequential_no_orphan,
      ``serial_no_orphan, ``execution_no_orphan, ``reached_removal_drops_registered_login,
      ``CommittedEvents, ``initial_committed_events, ``committed_events_step,
      ``execution_committed_events, ``confirmed_abort_has_no_event, ``SerialHistory] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected registration lifetime history axiom in {declaration}: {unexpected}"

end RegistrationLifetime
