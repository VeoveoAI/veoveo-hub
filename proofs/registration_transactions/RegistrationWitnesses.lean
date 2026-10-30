import RegistrationHistory

/- Constructive executions of original Step constructors, not an atomic
registration oracle. Payload/metadata/role/frame types remain parametric; Nat
only supplies distinct mathematical client/login representatives in the fixture.
These witnesses do not encode native JSON, passwords, JWTs or PostgreSQL roles. -/
namespace RegistrationTransaction.Witnesses
variable {M R P F : Type} [DecidableEq M]

private def inserted (state : State Nat Nat M R P F) (id : Nat) :=
  update state id { state.transactions id with
    phase := .ready
    staged := (state.transactions id).staged ++ [.insert (state.transactions id).request.candidate] }

private def commitRequested (state : State Nat Nat M R P F) (id : Nat) :=
  update state id { state.transactions id with phase := .committing }

private def finishInsert (state : State Nat Nat M R P F) (id : Nat) :=
  let committing := commitRequested (inserted state id) id
  let committed := committedState committing id
  update committed id { committed.transactions id with response := .success }

private theorem finishInsert_execution (locking : Bool) (state : State Nat Nat M R P F) (id : Nat)
    (phase : (state.transactions id).phase = .inserting)
    (pending : (state.transactions id).response = .pending)
    (valid : ValidWrite (applyWrites state.db (state.transactions id).staged)
      (.insert (state.transactions id).request.candidate)) :
    Execution locking state (finishInsert state id) := by
  have first : Step locking state (inserted state id) := Step.insert state id phase valid
  have request : Step locking (inserted state id) (commitRequested (inserted state id) id) :=
    Step.requestCommit _ id (by simp [inserted, update, put])
  have commitStep : Step locking (commitRequested (inserted state id) id)
      (committedState (commitRequested (inserted state id) id) id) :=
    Step.serverCommit (commitRequested (inserted state id) id) id
    (by simp [commitRequested, update, put])
  have acknowledge : Step locking (committedState (commitRequested (inserted state id) id) id)
      (finishInsert state id) :=
    Step.acknowledge (committedState (commitRequested (inserted state id) id) id) id
    (by simp [committedState, update, put])
    (by simpa [committedState, commitRequested, inserted, update, put] using pending)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.refl state) first) request) commitStep) acknowledge

private theorem finishInsert_fields (state : State Nat Nat M R P F) (id : Nat) :
    (finishInsert state id).db =
      commit state.db ((state.transactions id).staged ++ [.insert (state.transactions id).request.candidate]) ∧
    (finishInsert state id).lock = release state.lock id ∧
    ((finishInsert state id).transactions id).phase = .committed ∧
    ((finishInsert state id).transactions id).response = .success := by
  simp [finishInsert, inserted, commitRequested, committedState, update, put]

private def seed (old : M) (payload : R) (role unrelatedRole : P) (frame : F) : Database Nat Nat M R P F :=
  { metadata := old
    registrations := put (fun _ => none) 0 (some ⟨0, 10, old, payload⟩)
    logins := put (put (fun _ => none) 10 (some role)) 99 (some unrelatedRole)
    frame }

private def requests (old next : M) (firstPayload secondPayload : R) (firstRole secondRole : P)
    (secondKind : Kind) : Nat → Request Nat Nat M R P :=
  fun id => if id = 0 then ⟨⟨1, 11, old, firstPayload⟩, firstRole, .ordinary⟩
    else ⟨⟨2, 12, next, secondPayload⟩, secondRole, secondKind⟩

/-- The second actual transaction has begun while the first owns the deployment
lock. No successful acquire for it is invented before that lock is released. -/
private def replicaPrefix (db : Database Nat Nat M R P F) (requests : Nat → Request Nat Nat M R P) :=
  let start := initial db requests
  let begun := update start 0 { start.transactions 0 with phase := .waiting }
  let locked := { (update begun 0 { begun.transactions 0 with phase := .locked }) with lock := some 0 }
  let read := update locked 0 { locked.transactions 0 with phase := .read, observed := some db.metadata }
  let waiting := update read 1 { read.transactions 1 with phase := .waiting }
  let creating := update waiting 0 { waiting.transactions 0 with phase := .creating }
  update creating 0 { creating.transactions 0 with
    phase := .inserting
    staged := [.createLogin (requests 0).candidate.login (requests 0).role] }

private theorem replicaPrefix_execution (db : Database Nat Nat M R P F)
    (requests : Nat → Request Nat Nat M R P)
    (ordinary : (requests 0).kind = .ordinary)
    (same : db.metadata = (requests 0).candidate.metadata)
    (valid : ValidWrite db (.createLogin (requests 0).candidate.login (requests 0).role)) :
    Execution true (initial db requests) (replicaPrefix db requests) := by
  let start := initial db requests
  let begun := update start 0 { start.transactions 0 with phase := .waiting }
  let locked := { (update begun 0 { begun.transactions 0 with phase := .locked }) with lock := some 0 }
  let read := update locked 0 { locked.transactions 0 with phase := .read, observed := some db.metadata }
  let waiting := update read 1 { read.transactions 1 with phase := .waiting }
  let creating := update waiting 0 { waiting.transactions 0 with phase := .creating }
  have beginStep : Step true start begun := Step.begin start 0 rfl
  have lockStep : Step true begun locked := Step.acquire begun 0
    (by simp [begun, update, put]) rfl (Or.inl rfl)
  have readStep : Step true locked read := Step.read locked 0
    (by simp [locked, update, put])
    (by simpa [locked, begun, start, initial, update, put] using ordinary)
  have waitStep : Step true read waiting := Step.begin read 1
    (by simp [read, locked, begun, start, initial, update, put])
  have replicaStep : Step true waiting creating := Step.replica waiting 0
    (by simp [waiting, update, put, read])
    (by simp [waiting, read, locked, begun, start, initial, update, put, same])
  have createStep : Step true creating (replicaPrefix db requests) := Step.create creating 0
    (by simp [creating, update, put])
    (by simpa [creating, waiting, read, locked, begun, start, initial, update, put, applyWrites] using valid)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.refl start) beginStep) lockStep) readStep) waitStep) replicaStep) createStep

/-- The mathematical fixture satisfies the raw selected row/role constraints;
metadata, remaining full row payloads and role attributes have arbitrary values. -/
private theorem seed_wellFormed (old : M) (payload : R) (role unrelatedRole : P) (frame : F) :
    WellFormed (seed old payload role unrelatedRole frame) := by
  constructor
  · intro key row stored
    by_cases same : key = 0 <;> simp [seed, put, same] at stored
    subst row
    simp [seed, put, same]
  · intro first second left right firstStored secondStored _
    by_cases firstZero : first = 0 <;> by_cases secondZero : second = 0 <;>
      simp_all [seed, put]

theorem seeded_replica_admitted (old next : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) (secondKind : Kind) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old next firstPayload secondPayload firstRole secondRole secondKind
    let result := finishInsert (replicaPrefix db rs) 0
    WellFormed db ∧ Execution true (initial db rs) result ∧
      result.db = expectedReplica db (rs 0) ∧
      (result.transactions 0).response = .success ∧
      (result.transactions 1).phase = .waiting ∧ result.lock = none := by
  dsimp only
  let db := seed old seedPayload seedRole unrelatedRole frame
  let rs := requests old next firstPayload secondPayload firstRole secondRole secondKind
  have first := replicaPrefix_execution db rs
    (by simp [rs, requests]) (by simp [db, seed, rs, requests])
    (by simp [ValidWrite, db, seed, rs, requests, put])
  have finish := finishInsert_execution true (replicaPrefix db rs) 0
    (by simp [replicaPrefix, update, put])
    (by simp [replicaPrefix, update, put, initial])
    (by simp [replicaPrefix, update, put, initial, ValidWrite, applyWrites, applyWrite, db, seed, rs, requests])
  refine ⟨seed_wellFormed _ _ _ _ _, ?_, ?_, ?_, ?_, ?_⟩
  · have trans {first middle last : State Nat Nat M R P F}
        (left : Execution true first middle) (right : Execution true middle last) : Execution true first last := by
      induction right with
      | refl => exact left
      | tail _ step ih => exact Execution.tail ih step
    exact trans first finish
  · simp [finishInsert, replicaPrefix, inserted, commitRequested, committedState, update, put,
      initial, commit, applyWrites, applyWrite, admitWrites, expectedReplica, db, rs, requests]
  · simp [finishInsert, update, put]
  · simp [finishInsert, replicaPrefix, inserted, commitRequested, committedState, update, put, initial]
  · simp [finishInsert, replicaPrefix, inserted, commitRequested, committedState, update, put, initial, release]

private def replicaFromWaiting (state : State Nat Nat M R P F) (id : Nat) :=
  let locked := { (update state id { state.transactions id with phase := .locked }) with lock := some id }
  let read := update locked id { locked.transactions id with phase := .read, observed := some state.db.metadata }
  let creating := update read id { read.transactions id with phase := .creating }
  update creating id { creating.transactions id with
    phase := .inserting
    staged := (creating.transactions id).staged ++
      [.createLogin (creating.transactions id).request.candidate.login (creating.transactions id).request.role] }

private theorem replicaFromWaiting_execution (state : State Nat Nat M R P F) (id : Nat)
    (phase : (state.transactions id).phase = .waiting) (free : state.lock = none)
    (ordinary : (state.transactions id).request.kind = .ordinary)
    (same : state.db.metadata = (state.transactions id).request.candidate.metadata)
    (empty : (state.transactions id).staged = [])
    (valid : ValidWrite state.db
      (.createLogin (state.transactions id).request.candidate.login (state.transactions id).request.role)) :
    Execution true state (replicaFromWaiting state id) := by
  let locked := { (update state id { state.transactions id with phase := .locked }) with lock := some id }
  let read := update locked id { locked.transactions id with phase := .read, observed := some state.db.metadata }
  let creating := update read id { read.transactions id with phase := .creating }
  have lockStep : Step true state locked := Step.acquire state id phase free (Or.inl rfl)
  have readStep : Step true locked read := Step.read locked id
    (by simp [locked, update, put]) (by simpa [locked, update, put] using ordinary)
  have replicaStep : Step true read creating := Step.replica read id
    (by simp [read, update, put]) (by simp [read, locked, update, put, same])
  have createStep : Step true creating (replicaFromWaiting state id) := Step.create creating id
    (by simp [creating, update, put])
    (by simpa [creating, read, locked, update, put, empty, applyWrites] using valid)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.refl state) lockStep) readStep) replicaStep) createStep

private theorem execution_trans {first middle last : State Nat Nat M R P F}
    (left : Execution true first middle) (right : Execution true middle last) : Execution true first last := by
  induction right with
  | refl => exact left
  | tail _ step ih => exact Execution.tail ih step

/-- Both actual transactions were begun before the first released its lock;
both finish successfully. The full initial row and both new candidates/logins
survive, as do the unrelated role and frame, with actual commit order [0,1]. -/
theorem same_metadata_overlap_succeeds (old : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old old firstPayload secondPayload firstRole secondRole .ordinary
    let first := finishInsert (replicaPrefix db rs) 0
    let result := finishInsert (replicaFromWaiting first 1) 1
    WellFormed db ∧ Execution true (initial db rs) result ∧
      result.db = expectedReplica (expectedReplica db (rs 0)) (rs 1) ∧
      (result.transactions 0).response = .success ∧
      (result.transactions 1).response = .success ∧ result.lock = none ∧
      result.commits.map Prod.fst = [0, 1] := by
  dsimp only
  let db := seed old seedPayload seedRole unrelatedRole frame
  let rs := requests old old firstPayload secondPayload firstRole secondRole .ordinary
  let first := finishInsert (replicaPrefix db rs) 0
  have admitted := seeded_replica_admitted old old seedPayload firstPayload secondPayload
    seedRole firstRole secondRole unrelatedRole frame .ordinary
  have start : Execution true (initial db rs) first := admitted.2.1
  have create := replicaFromWaiting_execution first 1 admitted.2.2.2.2.1 admitted.2.2.2.2.2
    (by simp [first, finishInsert, inserted, commitRequested, committedState, replicaPrefix, initial, update, put, rs, requests])
    (by simp [first, finishInsert, inserted, commitRequested, committedState, replicaPrefix, initial, update, put,
      commit, applyWrites, applyWrite, db, seed, rs, requests])
    (by simp [first, finishInsert, inserted, commitRequested, committedState, replicaPrefix, initial, update, put])
    (by simp [first, finishInsert, inserted, commitRequested, committedState, replicaPrefix, initial, update, put,
      ValidWrite, commit, applyWrites, applyWrite, db, seed, rs, requests])
  have finish := finishInsert_execution true (replicaFromWaiting first 1) 1
    (by simp [replicaFromWaiting, update, put])
    (by simp [replicaFromWaiting, first, finishInsert, inserted, commitRequested, committedState,
      replicaPrefix, initial, update, put])
    (by
      simp [replicaFromWaiting, first, finishInsert, inserted, commitRequested, committedState,
        replicaPrefix, initial, update, put, ValidWrite, commit, applyWrites, applyWrite, db, seed, rs, requests]
      intro key stored lookup
      by_cases one : key = 1 <;> by_cases zero : key = 0 <;> simp_all
      all_goals subst stored
      all_goals simp)
  refine ⟨admitted.1, execution_trans (execution_trans start create) finish, ?_, ?_, ?_, ?_, ?_⟩
  · simp [first, finishInsert, replicaFromWaiting, inserted, commitRequested, committedState,
      replicaPrefix, initial, update, put, commit, applyWrites, applyWrite, expectedReplica, rs, requests]
  · simp [first, finishInsert, replicaFromWaiting, inserted, commitRequested, committedState, update, put]
  · simp [finishInsert, update, put]
  · simp [finishInsert, replicaFromWaiting, inserted, commitRequested, committedState, update, put, release]
  · simp [first, finishInsert, replicaFromWaiting, inserted, commitRequested, committedState,
      replicaPrefix, initial, update, put]

#print axioms seeded_replica_admitted
#print axioms same_metadata_overlap_succeeds
run_cmd do
  for declaration in [``seeded_replica_admitted, ``same_metadata_overlap_succeeds] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration witness axiom: {name}"

end RegistrationTransaction.Witnesses
