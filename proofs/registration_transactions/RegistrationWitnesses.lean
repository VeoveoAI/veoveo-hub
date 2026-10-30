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

private theorem execution_trans {locking : Bool} {first middle last : State Nat Nat M R P F}
    (left : Execution locking first middle) (right : Execution locking middle last) : Execution locking first last := by
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

theorem staged_abort_is_reachable (old next : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) (secondKind : Kind) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old next firstPayload secondPayload firstRole secondRole secondKind
    let staged := replicaPrefix db rs
    let pending := update staged 0 { staged.transactions 0 with phase := .abortPending, response := .error }
    let aborted := { (update pending 0 { pending.transactions 0 with phase := .aborted }) with
      lock := release pending.lock 0 }
    WellFormed db ∧ Execution true (initial db rs) aborted ∧
      (staged.transactions 0).staged ≠ [] ∧ aborted.db = db ∧
      (aborted.transactions 0).phase = .aborted ∧
      (aborted.transactions 0).response = .error ∧ aborted.lock = none ∧ aborted.commits = [] := by
  dsimp only
  let db := seed old seedPayload seedRole unrelatedRole frame
  let rs := requests old next firstPayload secondPayload firstRole secondRole secondKind
  let staged := replicaPrefix db rs
  let pending := update staged 0 { staged.transactions 0 with phase := .abortPending, response := .error }
  let aborted := { (update pending 0 { pending.transactions 0 with phase := .aborted }) with
    lock := release pending.lock 0 }
  have prefixExecution := replicaPrefix_execution db rs
    (by simp [rs, requests]) (by simp [db, seed, rs, requests])
    (by simp [ValidWrite, db, seed, rs, requests, put])
  have failStep : Step true staged pending := Step.fail staged 0
    (by simp [staged, replicaPrefix, update, put])
    (by simp [staged, replicaPrefix, update, put])
    (by simp [staged, replicaPrefix, update, put])
  have abortStep : Step true pending aborted := Step.serverAbort pending 0
    (Or.inl (by simp [pending, update, put]))
  refine ⟨seed_wellFormed _ _ _ _ _, Execution.tail (Execution.tail prefixExecution failStep) abortStep,
    ?_, ?_, ?_, ?_, ?_, ?_⟩
  all_goals simp [aborted, pending, staged, replicaPrefix, initial, update, put, release]

private def upserted (state : State Nat Nat M R P F) (id : Nat) :=
  { (update state id { state.transactions id with
      phase := .deleting
      staged := (state.transactions id).staged ++ [.deployment (state.transactions id).request.candidate.metadata] }) with
    lock := some id }

private def deleted (state : State Nat Nat M R P F) (id : Nat)
    (rows : List (Registration Nat Nat M R)) :=
  update state id { state.transactions id with
    phase := .dropping
    captured := rows
    drops := rows.map (·.login)
    staged := (state.transactions id).staged ++ [.deleteRows (rows.map (·.client))] }

private def dropped (state : State Nat Nat M R P F) (id login : Nat) (remaining : List Nat) :=
  update state id { state.transactions id with
    drops := remaining
    staged := (state.transactions id).staged ++ [.dropLogin login] }

private def replacementCreating (state : State Nat Nat M R P F) (id : Nat)
    (first second : Registration Nat Nat M R) :=
  let captured := deleted (upserted state id) id [first, second]
  let droppedFirst := dropped captured id first.login [second.login]
  let droppedSecond := dropped droppedFirst id second.login []
  update droppedSecond id { droppedSecond.transactions id with phase := .creating }

private def replacementToInsert (state : State Nat Nat M R P F) (id : Nat)
    (first second : Registration Nat Nat M R) :=
  let creating := replacementCreating state id first second
  update creating id { creating.transactions id with
    phase := .inserting
    staged := (creating.transactions id).staged ++
      [.createLogin (creating.transactions id).request.candidate.login (creating.transactions id).request.role] }

/-- This constructor trace uses exactly the statement-time two-row capture and
two actual drop steps. The final fixtures below discharge every premise. -/
private theorem replacementToInsert_execution (state : State Nat Nat M R P F) (id : Nat)
    (first second : Registration Nat Nat M R)
    (phase : (state.transactions id).phase = .upserting)
    (available : state.lock = none ∨ state.lock = some id)
    (empty : (state.transactions id).staged = []) (listing : Listing state.db [first, second])
    (valid : ValidWrite (applyWrites (replacementCreating state id first second).db
      ((replacementCreating state id first second).transactions id).staged)
      (.createLogin ((replacementCreating state id first second).transactions id).request.candidate.login
        ((replacementCreating state id first second).transactions id).request.role)) :
    Execution true state (replacementToInsert state id first second) := by
  let written := upserted state id
  let captured := deleted written id [first, second]
  let droppedFirst := dropped captured id first.login [second.login]
  let droppedSecond := dropped droppedFirst id second.login []
  let creating := update droppedSecond id { droppedSecond.transactions id with phase := .creating }
  have upsertStep : Step true state written := Step.upsert state id phase available
  have deleteStep : Step true written captured := Step.delete written id [first, second]
    (by simp [written, upserted, update, put])
    (by simpa [Listing, written, upserted, update, put, empty, applyWrites, applyWrite] using listing)
  have firstStep : Step true captured droppedFirst := Step.drop captured id first.login [second.login]
    (by simp [captured, deleted, update, put])
    (by simp [captured, deleted, update, put])
  have secondStep : Step true droppedFirst droppedSecond := Step.drop droppedFirst id second.login []
    (by simp [droppedFirst, dropped, captured, deleted, update, put])
    (by simp [droppedFirst, dropped, update, put])
  have doneStep : Step true droppedSecond creating := Step.dropped droppedSecond id
    (by simp [droppedSecond, dropped, droppedFirst, captured, deleted, update, put])
    (by simp [droppedSecond, dropped, update, put])
  have createStep : Step true creating (replacementToInsert state id first second) := Step.create creating id
    (by simp [creating, update, put]) valid
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.refl state) upsertStep) deleteStep) firstStep) secondStep) doneStep) createStep

private def replacementStart (state : State Nat Nat M R P F) (id : Nat) (kind : Kind) :=
  let locked := { (update state id { state.transactions id with phase := .locked }) with lock := some id }
  let base := if kind = .ordinary then
      update locked id { locked.transactions id with phase := .read, observed := some state.db.metadata }
    else locked
  update base id { base.transactions id with phase := .upserting, replacing := true }

private theorem replacementStart_execution (state : State Nat Nat M R P F) (id : Nat) (kind : Kind)
    (phase : (state.transactions id).phase = .waiting) (free : state.lock = none)
    (selected : (state.transactions id).request.kind = kind)
    (different : kind = .ordinary → state.db.metadata ≠ (state.transactions id).request.candidate.metadata) :
    Execution true state (replacementStart state id kind) := by
  let locked := { (update state id { state.transactions id with phase := .locked }) with lock := some id }
  have lockStep : Step true state locked := Step.acquire state id phase free (Or.inl rfl)
  cases kind
  case ordinary =>
    let read := update locked id { locked.transactions id with phase := .read, observed := some state.db.metadata }
    have readStep : Step true locked read := Step.read locked id
      (by simp [locked, update, put]) (by simpa [locked, update, put] using selected)
    have replaceStep : Step true read (replacementStart state id .ordinary) := Step.replace read id
      (by simp [read, update, put])
      (by simpa [read, locked, update, put] using different rfl)
    exact Execution.tail (Execution.tail (Execution.tail (Execution.refl state) lockStep) readStep) replaceStep
  case instanceReplacement =>
    have replaceStep : Step true locked (replacementStart state id .instanceReplacement) := Step.instanceStart locked id
      (by simp [locked, update, put]) (by simpa [locked, update, put] using selected)
    exact Execution.tail (Execution.tail (Execution.refl state) lockStep) replaceStep

private theorem replacementCreating_fields (state : State Nat Nat M R P F) (id : Nat)
    (first second : Registration Nat Nat M R) (empty : (state.transactions id).staged = []) :
    (replacementCreating state id first second).db = state.db ∧
    ((replacementCreating state id first second).transactions id).request = (state.transactions id).request ∧
    ((replacementCreating state id first second).transactions id).staged =
      [.deployment (state.transactions id).request.candidate.metadata,
       .deleteRows [first.client, second.client], .dropLogin first.login, .dropLogin second.login] := by
  simp [replacementCreating, upserted, deleted, dropped, update, put, empty]

private theorem replacementToInsert_fields (state : State Nat Nat M R P F) (id : Nat)
    (first second : Registration Nat Nat M R) (empty : (state.transactions id).staged = []) :
    (replacementToInsert state id first second).db = state.db ∧
    ((replacementToInsert state id first second).transactions id).request = (state.transactions id).request ∧
    ((replacementToInsert state id first second).transactions id).phase = .inserting ∧
    ((replacementToInsert state id first second).transactions id).staged =
      [.deployment (state.transactions id).request.candidate.metadata,
       .deleteRows [first.client, second.client], .dropLogin first.login, .dropLogin second.login,
       .createLogin (state.transactions id).request.candidate.login (state.transactions id).request.role] ∧
    ((replacementToInsert state id first second).transactions id).response = (state.transactions id).response ∧
    (replacementToInsert state id first second).lock = some id := by
  simp [replacementToInsert, replacementCreating, upserted, deleted, dropped, update, put, empty]

private theorem replacement_overlap_succeeds (old next : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) (kind : Kind)
    (different : kind = .ordinary → old ≠ next) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old next firstPayload secondPayload firstRole secondRole kind
    let first := finishInsert (replicaPrefix db rs) 0
    let before := replacementStart first 1 kind
    let tail := replacementToInsert before 1 ⟨0, 10, old, seedPayload⟩ (rs 0).candidate
    let result := finishInsert tail 1
    WellFormed db ∧ Execution true (initial db rs) result ∧
      result.db = expectedReplacement (expectedReplica db (rs 0)) (rs 1) ∧
      (result.transactions 0).response = .success ∧
      (result.transactions 1).response = .success ∧ result.lock = none ∧
      result.commits.map Prod.fst = [0, 1] := by
  dsimp only
  let db := seed old seedPayload seedRole unrelatedRole frame
  let rs := requests old next firstPayload secondPayload firstRole secondRole kind
  let first := finishInsert (replicaPrefix db rs) 0
  let before := replacementStart first 1 kind
  let seedRow : Registration Nat Nat M R := ⟨0, 10, old, seedPayload⟩
  let tail := replacementToInsert before 1 seedRow (rs 0).candidate
  have admitted := seeded_replica_admitted old next seedPayload firstPayload secondPayload
    seedRole firstRole secondRole unrelatedRole frame kind
  have firstExecution : Execution true (initial db rs) first := admitted.2.1
  have firstDB : first.db = expectedReplica db (rs 0) := admitted.2.2.1
  have start := replacementStart_execution first 1 kind admitted.2.2.2.2.1 admitted.2.2.2.2.2
    (by simp [first, finishInsert, replicaPrefix, inserted, commitRequested, committedState, initial, update, put, rs, requests])
    (by simpa [first, finishInsert, replicaPrefix, inserted, commitRequested, committedState, initial, update, put,
      commit, applyWrites, applyWrite, db, seed, rs, requests] using different)
  have reached : Execution true (initial db rs) before := execution_trans firstExecution start
  have phase : (before.transactions 1).phase = .upserting := by
    cases kind <;> simp [before, replacementStart, update, put]
  have empty : (before.transactions 1).staged = [] := by
    have aligned := execution_alignment reached (initial_alignment db rs) 1
    simp only [Aligned, phase] at aligned
    exact aligned.1
  have own : before.lock = some 1 := by cases kind <;> simp [before, replacementStart, update, put]
  have beforeDB : before.db = first.db := by cases kind <;> rfl
  have beforeRequest : (before.transactions 1).request = rs 1 := reached_request db rs before reached 1
  have listing : Listing first.db [seedRow, (rs 0).candidate] := by
    rw [firstDB]
    constructor
    · simp [rs, requests, seedRow]
    · funext key
      by_cases zero : key = 0 <;> by_cases one : key = 1 <;>
        simp [expectedReplica, db, seed, rs, requests, seedRow, rowMap, put, zero, one]
  have currentListing : Listing before.db [seedRow, (rs 0).candidate] := by simpa only [beforeDB] using listing
  have creatingFields := replacementCreating_fields before 1 seedRow (rs 0).candidate empty
  have validCreate : ValidWrite (applyWrites (replacementCreating before 1 seedRow (rs 0).candidate).db
      ((replacementCreating before 1 seedRow (rs 0).candidate).transactions 1).staged)
      (.createLogin ((replacementCreating before 1 seedRow (rs 0).candidate).transactions 1).request.candidate.login
        ((replacementCreating before 1 seedRow (rs 0).candidate).transactions 1).request.role) := by
    rw [creatingFields.1, creatingFields.2.1, creatingFields.2.2, beforeDB, beforeRequest, firstDB]
    simp [ValidWrite, applyWrites, applyWrite, expectedReplica, db, seed, seedRow, rs, requests, put]
  have traversal := replacementToInsert_execution before 1 seedRow (rs 0).candidate phase (Or.inr own)
    empty currentListing validCreate
  have fields := replacementToInsert_fields before 1 seedRow (rs 0).candidate empty
  have cleared : (applyWrites before.db
      [.deployment (before.transactions 1).request.candidate.metadata,
       .deleteRows [seedRow.client, (rs 0).candidate.client],
       .dropLogin seedRow.login, .dropLogin (rs 0).candidate.login,
       .createLogin (before.transactions 1).request.candidate.login (before.transactions 1).request.role]).registrations =
      fun _ => none := by
    have clear := delete_listing_clears (applyWrite before.db
      (.deployment (before.transactions 1).request.candidate.metadata)) [seedRow, (rs 0).candidate] currentListing
    simpa only [List.map_cons, List.map_nil, applyWrite, applyWrites, List.foldl_cons, List.foldl_nil] using clear
  have validInsert : ValidWrite (applyWrites tail.db (tail.transactions 1).staged) (.insert (tail.transactions 1).request.candidate) := by
    rw [fields.1, fields.2.1, fields.2.2.2.1]
    refine ⟨?_, ?_, ?_⟩
    · rw [cleared]
    · simp [applyWrites, applyWrite, put]
    · intro key row found
      rw [cleared] at found
      cases found
  have pending : (before.transactions 1).response = .pending := by
    cases kind <;> simp [before, replacementStart, first, finishInsert, replicaPrefix, inserted,
      commitRequested, committedState, update, put, initial]
  have finished := finishInsert_execution true tail 1 fields.2.2.1 (fields.2.2.2.2.1.trans pending) validInsert
  have finalFields := finishInsert_fields tail 1
  have exactDB : (finishInsert tail 1).db = expectedReplacement first.db (rs 1) := by
    rw [finalFields.1, fields.1, fields.2.1, fields.2.2.2.1]
    change applyWrites before.db (replaceWrites (before.transactions 1).request [seedRow, (rs 0).candidate]) = _
    rw [beforeDB, beforeRequest]
    exact replacement_exact first.db (rs 1) _ listing
  refine ⟨admitted.1, execution_trans (execution_trans reached traversal) finished, ?_, ?_,
    finalFields.2.2.2, ?_, ?_⟩
  · simpa only [firstDB] using exactDB
  · have keeps : ((finishInsert tail 1).transactions 0).response = (first.transactions 0).response := by
      cases kind <;> simp [tail, finishInsert, replacementToInsert, replacementCreating, upserted, deleted, dropped,
        before, replacementStart, inserted, commitRequested, committedState, update, put]
    exact keeps.trans admitted.2.2.2.1
  · rw [finalFields.2.1, fields.2.2.2.2.2]
    rfl
  · cases kind <;> simp [tail, first, finishInsert, replicaPrefix, replacementToInsert, replacementCreating,
      upserted, deleted, dropped, before, replacementStart, inserted, commitRequested, committedState, update, put, initial]

theorem ordinary_replacement_succeeds (old next : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) (different : old ≠ next) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old next firstPayload secondPayload firstRole secondRole .ordinary
    let first := finishInsert (replicaPrefix db rs) 0
    let before := replacementStart first 1 .ordinary
    let result := finishInsert (replacementToInsert before 1 ⟨0, 10, old, seedPayload⟩ (rs 0).candidate) 1
    WellFormed db ∧ Execution true (initial db rs) result ∧
      result.db = expectedReplacement (expectedReplica db (rs 0)) (rs 1) ∧
      (result.transactions 0).response = .success ∧
      (result.transactions 1).response = .success ∧ result.lock = none ∧
      result.commits.map Prod.fst = [0, 1] :=
  replacement_overlap_succeeds old next seedPayload firstPayload secondPayload
    seedRole firstRole secondRole unrelatedRole frame .ordinary (fun _ => different)

/-- Instance replacement is unconditional: this includes old=next, with no
inequality premise hiding the equal-metadata replacement case. -/
theorem instance_replacement_succeeds (old next : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old next firstPayload secondPayload firstRole secondRole .instanceReplacement
    let first := finishInsert (replicaPrefix db rs) 0
    let before := replacementStart first 1 .instanceReplacement
    let result := finishInsert (replacementToInsert before 1 ⟨0, 10, old, seedPayload⟩ (rs 0).candidate) 1
    WellFormed db ∧ Execution true (initial db rs) result ∧
      result.db = expectedReplacement (expectedReplica db (rs 0)) (rs 1) ∧
      (result.transactions 0).response = .success ∧
      (result.transactions 1).response = .success ∧ result.lock = none ∧
      result.commits.map Prod.fst = [0, 1] :=
  replacement_overlap_succeeds old next seedPayload firstPayload secondPayload
    seedRole firstRole secondRole unrelatedRole frame .instanceReplacement (by simp)

/- The original path below takes NO ordinary read lock. Replacement still
acquires its real upsert lock. A pauses before INSERT with a private login write;
B captures/deletes seed and drops its login before A commits. The remaining B
CREATE/INSERT steps are scheduled later, matching the native catalog wait. -/
private def originalAReady (db : Database Nat Nat M R P F) (rs : Nat → Request Nat Nat M R P) :=
  let start := initial db rs
  let begun := update start 0 { start.transactions 0 with phase := .waiting }
  let read := update begun 0 { begun.transactions 0 with phase := .read, observed := some begun.db.metadata }
  let creating := update read 0 { read.transactions 0 with phase := .creating }
  update creating 0 { creating.transactions 0 with
    phase := .inserting
    staged := (creating.transactions 0).staged ++
      [.createLogin (creating.transactions 0).request.candidate.login (creating.transactions 0).request.role] }

private theorem originalAReady_fields (db : Database Nat Nat M R P F) (rs : Nat → Request Nat Nat M R P) :
    (originalAReady db rs).db = db ∧ (originalAReady db rs).lock = none ∧
    (originalAReady db rs).transactions 1 = { request := rs 1 } ∧
    (originalAReady db rs).transactions 0 = {
      request := rs 0
      phase := .inserting
      observed := some db.metadata
      staged := [.createLogin (rs 0).candidate.login (rs 0).role] } ∧
    (originalAReady db rs).commits = [] := by
  simp [originalAReady, initial, update, put]

private theorem originalAReady_execution (db : Database Nat Nat M R P F)
    (rs : Nat → Request Nat Nat M R P)
    (ordinary : (rs 0).kind = .ordinary) (same : db.metadata = (rs 0).candidate.metadata)
    (valid : ValidWrite db (.createLogin (rs 0).candidate.login (rs 0).role)) :
    Execution false (initial db rs) (originalAReady db rs) := by
  let start := initial db rs
  let begun := update start 0 { start.transactions 0 with phase := .waiting }
  let read := update begun 0 { begun.transactions 0 with phase := .read, observed := some begun.db.metadata }
  let creating := update read 0 { read.transactions 0 with phase := .creating }
  have beginStep : Step false start begun := Step.begin start 0 rfl
  have readStep : Step false begun read := Step.unlockedRead begun 0 rfl
    (by simp [begun, update, put]) (by simpa [begun, start, initial, update, put] using ordinary)
  have chooseStep : Step false read creating := Step.replica read 0
    (by simp [read, update, put]) (by simp [read, begun, start, initial, update, put, same])
  have createStep : Step false creating (originalAReady db rs) := Step.create creating 0
    (by simp [creating, update, put])
    (by simpa [creating, read, begun, start, initial, update, put, applyWrites] using valid)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.refl start) beginStep) readStep) chooseStep) createStep

private def originalBReady (state : State Nat Nat M R P F) :=
  let begun := update state 1 { state.transactions 1 with phase := .waiting }
  let read := update begun 1 { begun.transactions 1 with phase := .read, observed := some begun.db.metadata }
  update read 1 { read.transactions 1 with phase := .upserting, replacing := true }

private theorem originalBReady_fields (state : State Nat Nat M R P F) :
    (originalBReady state).db = state.db ∧ (originalBReady state).lock = state.lock ∧
    (originalBReady state).transactions 1 = { state.transactions 1 with
      phase := .upserting, observed := some state.db.metadata, replacing := true } ∧
    (originalBReady state).transactions 0 = state.transactions 0 ∧
    (originalBReady state).commits = state.commits := by
  simp [originalBReady, update, put]

private theorem originalBReady_execution (state : State Nat Nat M R P F)
    (phase : (state.transactions 1).phase = .idle)
    (ordinary : (state.transactions 1).request.kind = .ordinary)
    (different : state.db.metadata ≠ (state.transactions 1).request.candidate.metadata) :
    Execution false state (originalBReady state) := by
  let begun := update state 1 { state.transactions 1 with phase := .waiting }
  let read := update begun 1 { begun.transactions 1 with phase := .read, observed := some begun.db.metadata }
  have beginStep : Step false state begun := Step.begin state 1 phase
  have readStep : Step false begun read := Step.unlockedRead begun 1 rfl
    (by simp [begun, update, put]) (by simpa [begun, update, put] using ordinary)
  have replaceStep : Step false read (originalBReady state) := Step.replace read 1
    (by simp [read, update, put]) (by simpa [read, begun, update, put] using different)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.refl state) beginStep) readStep) replaceStep

private def originalPrefix (db : Database Nat Nat M R P F) (rs : Nat → Request Nat Nat M R P)
    (seedRow : Registration Nat Nat M R) :=
  let base := originalBReady (originalAReady db rs)
  let captured := deleted (upserted base 1) 1 [seedRow]
  let removed := dropped captured 1 seedRow.login []
  update removed 1 { removed.transactions 1 with phase := .creating }

private theorem originalPrefix_execution (db : Database Nat Nat M R P F)
    (rs : Nat → Request Nat Nat M R P) (seedRow : Registration Nat Nat M R)
    (firstKind : (rs 0).kind = .ordinary) (firstSame : db.metadata = (rs 0).candidate.metadata)
    (secondKind : (rs 1).kind = .ordinary) (secondDifferent : db.metadata ≠ (rs 1).candidate.metadata)
    (listing : Listing db [seedRow])
    (valid : ValidWrite db (.createLogin (rs 0).candidate.login (rs 0).role)) :
    Execution false (initial db rs) (originalPrefix db rs seedRow) := by
  let a := originalAReady db rs
  let base := originalBReady a
  let written := upserted base 1
  let captured := deleted written 1 [seedRow]
  let removed := dropped captured 1 seedRow.login []
  have aFields := originalAReady_fields db rs
  have bFields := originalBReady_fields a
  have aExecution := originalAReady_execution db rs firstKind firstSame valid
  have bExecution := originalBReady_execution a
    (by rw [aFields.2.2.1])
    (by simpa only [a, aFields.2.2.1] using secondKind)
    (by simpa only [a, aFields.1, aFields.2.2.1] using secondDifferent)
  have baseDB : base.db = db := bFields.1.trans aFields.1
  have baseLock : base.lock = none := bFields.2.1.trans aFields.2.1
  have basePhase : (base.transactions 1).phase = .upserting := by rw [bFields.2.2.1]
  have baseEmpty : (base.transactions 1).staged = [] := by rw [bFields.2.2.1, aFields.2.2.1]
  have upsertStep : Step false base written := Step.upsert base 1 basePhase (Or.inl baseLock)
  have baseListing : Listing base.db [seedRow] := by rw [baseDB]; exact listing
  have deleteStep : Step false written captured := Step.delete written 1 [seedRow]
    (by simp [written, upserted, update, put])
    (by simpa only [Listing, written, upserted, update, put, baseEmpty, applyWrites, applyWrite,
      List.nil_append, List.foldl_cons, List.foldl_nil, ite_true] using baseListing)
  have dropStep : Step false captured removed := Step.drop captured 1 seedRow.login []
    (by simp [captured, deleted, update, put]) (by simp [captured, deleted, update, put])
  have doneStep : Step false removed (originalPrefix db rs seedRow) := Step.dropped removed 1
    (by simp [removed, dropped, captured, deleted, update, put]) (by simp [removed, dropped, update, put])
  exact execution_trans (execution_trans aExecution bExecution)
    (Execution.tail (Execution.tail (Execution.tail (Execution.tail
      (Execution.refl base) upsertStep) deleteStep) dropStep) doneStep)

private theorem originalPrefix_fields (db : Database Nat Nat M R P F)
    (rs : Nat → Request Nat Nat M R P) (seedRow : Registration Nat Nat M R) :
    (originalPrefix db rs seedRow).db = db ∧
    (originalPrefix db rs seedRow).lock = some 1 ∧
    (originalPrefix db rs seedRow).transactions 0 = {
      request := rs 0
      phase := .inserting
      observed := some db.metadata
      staged := [.createLogin (rs 0).candidate.login (rs 0).role] } ∧
    (originalPrefix db rs seedRow).transactions 1 = {
      request := rs 1
      phase := .creating
      observed := some db.metadata
      replacing := true
      captured := [seedRow]
      staged := [.deployment (rs 1).candidate.metadata, .deleteRows [seedRow.client], .dropLogin seedRow.login] } ∧
    (originalPrefix db rs seedRow).commits = [] := by
  have aFields := originalAReady_fields db rs
  have bFields := originalBReady_fields (originalAReady db rs)
  simp [originalPrefix, upserted, deleted, dropped, update, put,
    bFields.1, bFields.2.2.1, bFields.2.2.2.1, bFields.2.2.2.2,
    aFields.1, aFields.2.2.1, aFields.2.2.2.1, aFields.2.2.2.2]

private theorem originalAfterA_fields (db : Database Nat Nat M R P F)
    (rs : Nat → Request Nat Nat M R P) (seedRow : Registration Nat Nat M R) :
    let afterA := finishInsert (originalPrefix db rs seedRow) 0
    afterA.db = expectedReplica db (rs 0) ∧ afterA.lock = some 1 ∧
    (afterA.transactions 0).response = .success ∧
    afterA.transactions 1 = {
      request := rs 1
      phase := .creating
      observed := some db.metadata
      replacing := true
      captured := [seedRow]
      staged := [.deployment (rs 1).candidate.metadata, .deleteRows [seedRow.client], .dropLogin seedRow.login] } ∧
    afterA.commits.map Prod.fst = [0] := by
  dsimp only
  let paused := originalPrefix db rs seedRow
  have fields := originalPrefix_fields db rs seedRow
  constructor
  · rw [(finishInsert_fields paused 0).1, fields.1, fields.2.2.1]
    exact admit_exact db (rs 0)
  · simp [finishInsert, inserted, commitRequested, committedState, update, put, release,
      fields.2.1, fields.2.2.1, fields.2.2.2.1, fields.2.2.2.2]

private theorem original_midpoint_reachable (old next : M) (seedPayload firstPayload secondPayload : R)
    (seedRole firstRole secondRole unrelatedRole : P) (frame : F) (different : old ≠ next) :
    let db := seed old seedPayload seedRole unrelatedRole frame
    let rs := requests old next firstPayload secondPayload firstRole secondRole .ordinary
    let seedRow : Registration Nat Nat M R := ⟨0, 10, old, seedPayload⟩
    let afterA := finishInsert (originalPrefix db rs seedRow) 0
    WellFormed db ∧ Execution false (initial db rs) afterA ∧
      afterA.db = expectedReplica db (rs 0) ∧ afterA.lock = some 1 ∧
      (afterA.transactions 0).response = .success ∧
      (afterA.transactions 1).phase = .creating ∧
      ((afterA.transactions 1).captured.map (·.client)) = [0] := by
  dsimp only
  let db := seed old seedPayload seedRole unrelatedRole frame
  let rs := requests old next firstPayload secondPayload firstRole secondRole .ordinary
  let seedRow : Registration Nat Nat M R := ⟨0, 10, old, seedPayload⟩
  let paused := originalPrefix db rs seedRow
  have listing : Listing db [seedRow] := by
    constructor
    · simp [seedRow]
    · rfl
  have execution := originalPrefix_execution db rs seedRow
    (by simp [rs, requests]) (by simp [db, seed, rs, requests])
    (by simp [rs, requests]) (by simpa [db, seed, rs, requests] using different)
    listing (by simp [ValidWrite, db, seed, rs, requests, put])
  have fields := originalPrefix_fields db rs seedRow
  have finish := finishInsert_execution false paused 0
    (by rw [fields.2.2.1]) (by rw [fields.2.2.1])
    (by rw [fields.1, fields.2.2.1]
        simp [ValidWrite, applyWrites, applyWrite, db, seed, rs, requests, put])
  have result := originalAfterA_fields db rs seedRow
  refine ⟨seed_wellFormed _ _ _ _ _, execution_trans execution finish, result.1,
    result.2.1, result.2.2.1, ?_, ?_⟩
  · rw [result.2.2.2.1]
  · rw [result.2.2.2.1]
    simp [seedRow]

#print axioms original_midpoint_reachable
#print axioms seeded_replica_admitted
#print axioms same_metadata_overlap_succeeds
#print axioms staged_abort_is_reachable
#print axioms ordinary_replacement_succeeds
#print axioms instance_replacement_succeeds
run_cmd do
  for declaration in [``original_midpoint_reachable, ``seeded_replica_admitted, ``same_metadata_overlap_succeeds, ``staged_abort_is_reachable,
      ``ordinary_replacement_succeeds, ``instance_replacement_succeeds] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration witness axiom: {name}"

end RegistrationTransaction.Witnesses
