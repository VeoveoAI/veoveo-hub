import ComputerLifecycleWitnesses
import Lean.Util.CollectAxioms

namespace ComputerLifecycle

theorem claim_admission (row : Row) (key : Key) (cutoff : Int) :
    claimAllowed row key cutoff = true ↔ ClaimPermitted row key cutoff := by
  cases operation : row.operation <;>
    simp [claimAllowed, ClaimPermitted, owned, operation, and_assoc]

theorem claim_exact (rows : Table) (key : Key) (op : Nat) (target : Provision) (now cutoff : Int)
    (row : Row) (found : rows key.id = some row) (permitted : ClaimPermitted row key cutoff) :
    (claim rows key op target now cutoff).rows = put rows key.id (some (expectedClaim row op target now)) ∧
      (claim rows key op target now cutoff).matched = true := by
  have allowed := (claim_admission row key cutoff).2 permitted
  simp [claim, found, allowed, expectedClaim]

theorem claim_refused (rows : Table) (key : Key) (op : Nat) (target : Provision) (now cutoff : Int)
    (row : Row) (found : rows key.id = some row) (refused : ¬ClaimPermitted row key cutoff) :
    (claim rows key op target now cutoff).rows = rows ∧
      (claim rows key op target now cutoff).matched = false := by
  have allowed : claimAllowed row key cutoff = false := by
    cases value : claimAllowed row key cutoff
    · rfl
    · exact False.elim (refused ((claim_admission row key cutoff).1 value))
  simp [claim, found, allowed]

theorem finish_exact (rows : Table) (key : Key) (op : Nat) (notice : Option Notice)
    (row : Row) (current : Operation) (found : rows key.id = some row)
    (pending : row.operation = some current) (tenant : row.tenant = key.tenant)
    (principal : row.principal = key.principal) (same : current.id = op) :
    (finish rows key op notice).rows = put rows key.id (some (expectedFinish row current.target notice)) ∧
      (finish rows key op notice).matched = true := by
  simp [finish, found, pending, owned, tenant, principal, same, expectedFinish]

theorem fail_exact (rows : Table) (key : Key) (op : Nat) (failure : Failure)
    (row : Row) (current : Operation) (found : rows key.id = some row)
    (pending : row.operation = some current) (tenant : row.tenant = key.tenant)
    (principal : row.principal = key.principal) (same : current.id = op) :
    (fail rows key op failure).rows = put rows key.id (some (expectedFailure row failure)) ∧
      (fail rows key op failure).matched = true := by
  simp [fail, completionAllowed, found, pending, owned, tenant, principal, same, expectedFailure]

theorem shown_exact (rows : Table) (key : Key) (op : Nat) (row : Row) (current : Operation)
    (found : rows key.id = some row) (pending : row.operation = some current)
    (tenant : row.tenant = key.tenant) (principal : row.principal = key.principal) (same : current.id = op) :
    (showNotice rows key op).rows = put rows key.id (some { row with noticeShown := true }) ∧
      (showNotice rows key op).matched = true := by
  simp [showNotice, completionAllowed, found, pending, owned, tenant, principal, same]

/-- Full durable-table rejection;no corresponding native-mutation fence follows. -/
theorem stale_completion (rows : Table) (key : Key) (oldOp : Nat) (notice : Option Notice)
    (failure : Failure) (row : Row) (newOp : Operation)
    (found : rows key.id = some row) (pending : row.operation = some newOp) (different : newOp.id ≠ oldOp) :
    RejectedCompletion rows (finish rows key oldOp notice) (fail rows key oldOp failure) := by
  simp [RejectedCompletion, finish, fail, found, pending, completionAllowed, different]

theorem ended_completion (rows : Table) (key : Key) (oldOp : Nat) (notice : Option Notice)
    (failure : Failure) (row : Row) (found : rows key.id = some row) (ended : row.operation = none) :
    RejectedCompletion rows (finish rows key oldOp notice) (fail rows key oldOp failure) := by
  simp [RejectedCompletion, finish, fail, found, ended, completionAllowed]

theorem owner_completion_refused (rows : Table) (key : Key) (op : Nat) (notice : Option Notice)
    (failure : Failure) (row : Row) (found : rows key.id = some row) (wrong : owned row key = false) :
    RejectedCompletion rows (finish rows key op notice) (fail rows key op failure) := by
  cases pending : row.operation <;>
    simp [RejectedCompletion, finish, fail, found, pending, completionAllowed, wrong]

theorem unrelated_rows (rows : Table) (key : Key) (op : Nat) (target : Provision)
    (now cutoff : Int) (notice : Option Notice) (failure : Failure) :
    Framed rows (claim rows key op target now cutoff).rows key ∧
    Framed rows (finish rows key op notice).rows key ∧
    Framed rows (fail rows key op failure).rows key ∧ Framed rows (showNotice rows key op).rows key := by
  refine ⟨?_, ?_, ?_, ?_⟩
  all_goals
    intro id other
    cases found : rows key.id
    case none => simp [claim, finish, fail, showNotice, found]
    case some row =>
      cases pending : row.operation <;>
        simp [claim, finish, fail, showNotice, found, pending, put, other] <;>
        split_ifs <;> simp [put, other]

theorem commit_error_not_rollback (before : Table) (effect : Effect) (message : String) :
    (deliver before effect (.commitError message)).rows = effect.rows ∧
      (deliver before effect (.commitError message)).response = .error message := ⟨rfl, rfl⟩

theorem abort_error_preserves (before : Table) (effect : Effect) (message : String) :
    (deliver before effect (.abortError message)).rows = before ∧
      (deliver before effect (.abortError message)).response = .error message := ⟨rfl, rfl⟩

theorem launch_claim (rows : Table) (key : Key) (op : Nat) (target : Provision) (now cutoff : Int)
    (row : Row) (news : Option String) (found : rows key.id = some row)
    (permitted : ClaimPermitted row key cutoff) :
    let effect := claim rows key op target now cutoff
    launch (deliver rows effect .commitAck) key op target news (.ok (effect.rows key.id)) =
      .started ⟨key, op, target, expectedClaim row op target now, news⟩ := by
  have cut := claim_exact rows key op target now cutoff row found permitted
  simp [deliver, launch, cut.1, cut.2, put]

theorem claim_error_no_launch (before : Table) (effect : Effect) (key : Key) (op : Nat)
    (target : Provision) (news : Option String) (read : Except String (Option Row)) (message : String) :
    launch (deliver before effect (.commitError message)) key op target news read = .error message := rfl

theorem reread_error_no_launch (rows : Table) (key : Key) (op : Nat) (target : Provision)
    (news : Option String) (message : String) :
    launch ⟨rows, .ok true⟩ key op target news (.error message) = .error message := rfl

/-- Returned-stage success derives the complete ordered attempts and argument
binding;outer timer interruption is expressly outside this loop theorem. -/
theorem stages_success {N : Type} (context : Context) (observe : Invocation → Observation N)
    (native : N) (stages : List Stage)
    (accepted : ∀ stage ∈ stages, (observe (invoke context stage)).result = .ok ()) :
    (runStages context observe native stages).result = .ok () ∧
      (runStages context observe native stages).attempts = stages.map (invoke context) := by
  induction stages generalizing native with
  | nil => exact ⟨rfl, rfl⟩
  | cons stage remaining ih =>
    have first := accepted stage (by simp)
    have later := ih (observe (invoke context stage)).afterNative (by
      intro next member
      exact accepted next (List.mem_cons_of_mem stage member))
    simp [runStages, first, later.1, later.2]

/-- The observed first stage error exactly determines attempted prefix,error
payload and last observed native effects. No rollback/cancellation is inferred. -/
theorem stages_first_error {N : Type} (context : Context) (observe : Invocation → Observation N)
    (native : N) (stages : List Stage) (cause : String)
    (failed : (runStages context observe native stages).result = .error cause) :
    ∃ before failing after, stages = before ++ failing :: after ∧
      (∀ stage ∈ before, (observe (invoke context stage)).result = .ok ()) ∧
      (observe (invoke context failing)).result = .error cause ∧
      (runStages context observe native stages).attempts = (before ++ [failing]).map (invoke context) ∧
      (runStages context observe native stages).native = (observe (invoke context failing)).afterNative := by
  induction stages generalizing native with
  | nil => simp [runStages] at failed
  | cons stage remaining ih =>
    cases result : (observe (invoke context stage)).result with
    | error error =>
      have same : error = cause := by simpa [runStages, result] using failed
      subst error
      exact ⟨[], stage, remaining, rfl, by simp, result,
        by simp [runStages, result], by simp [runStages, result]⟩
    | ok unit =>
      cases unit
      have laterFailure : (runStages context observe (observe (invoke context stage)).afterNative remaining).result =
          .error cause := by simpa [runStages, result] using failed
      rcases ih (observe (invoke context stage)).afterNative laterFailure with
        ⟨before, failing, after, decomposition, accepted, firstError, attempts, effects⟩
      refine ⟨stage :: before, failing, after, ?_, ?_, firstError, ?_, ?_⟩
      · simp [decomposition]
      · intro next member
        rcases List.mem_cons.mp member with same | tail
        · subst next
          exact result
        · exact accepted next tail
      · simpa [runStages, result] using congrArg (fun calls => invoke context stage :: calls) attempts
      · simpa [runStages, result] using effects

theorem ordered_success {N : Type} (context : Context) (observe : Invocation → Observation N)
    (native : N) (accepted : ∀ stage ∈ program, (observe (invoke context stage)).result = .ok ()) :
    (runStages context observe native program).result = .ok () ∧
      (runStages context observe native program).attempts = expectedAttempts context := by
  simpa only [program, expectedAttempts, invoke, List.map_cons, List.map_nil] using
    stages_success context observe native program accepted

/-- No stale worker can change current durable rows despite its own historical
native success/failure. Caller supplies the ACTUAL completion linearization table. -/
theorem stale_worker {N : Type} (rows : Table) (context : Context) (observe : Invocation → Observation N)
    (native : N) (endedAt : Int) (row : Row) (newOp : Operation)
    (found : rows context.key.id = some row) (pending : row.operation = some newOp)
    (different : newOp.id ≠ context.operation) :
    (runWorker rows context observe native endedAt .commitAck).recorded.rows = rows ∧
      (runWorker rows context observe native endedAt .commitAck).recorded.response = .ok false := by
  have rejected := stale_completion rows context.key context.operation (updateNotice context)
    ⟨"unused", endedAt, context.target.version⟩ row newOp found pending different
  unfold runWorker
  cases result : (runStages context observe native program).result with
  | ok unit =>
    cases unit
    simp [deliver, rejected.1, rejected.2.1, result]
  | error cause =>
    have actual := stale_completion rows context.key context.operation (updateNotice context)
      ⟨cause, endedAt, context.target.version⟩ row newOp found pending different
    simp [deliver, actual.2.2.1, actual.2.2.2, result]

/- Fixture inputs are populated synthetic admitted snapshots,not native UUID,
version-hash or provisioning executions. Native is merely an opaque effect marker. -/
namespace Fixture


theorem put_same (rows : Table) (id : Nat) (first last : Option Row) :
    put (put rows id first) id last = put rows id last := by
  funext subject
  by_cases same : subject = id <;> simp [put, same]

theorem put_expected (row : Row) : put base 7 (some row) = expectedRows row := by
  funext id
  by_cases same : id = 7 <;> simp [put, base, expectedRows, same]

theorem accepted_success : claimed.matched = true ∧
    launch (deliver base claimed .commitAck) key 70 next (some "new release") (.ok (claimed.rows 7)) =
      .started context ∧
    succeeded.native.attempts = expectedAttempts context ∧ succeeded.native.result = .ok () ∧
    succeeded.recorded.response = .ok true ∧ succeeded.recorded.rows = expectedRows expectedSuccess ∧
    succeeded.native.native = 9 ∧ succeeded.loggedRecordingError = none := by
  refine ⟨rfl, rfl, rfl, rfl, rfl, ?_, rfl, rfl⟩
  change put (put base 7 (some (expectedClaim seed 70 next 0))) 7 (some expectedSuccess) = _
  rw [put_same]
  exact put_expected expectedSuccess

/-- Instantiate the generic first-error theorem on the ACTUAL partial native
run,then identify its failing stage from the supplied concrete observation. -/
theorem observed_failure_prefix :
    ∃ before after, program = before ++ Stage.createSandbox :: after ∧
      (∀ stage ∈ before, (createError (invoke context stage)).result = .ok ()) ∧
      failed.native.attempts = expectedFailureAttempts context ∧ failed.native.native = 5 := by
  rcases stages_first_error context createError 0 program "synthetic create refusal" rfl with
    ⟨before, failing, after, decomposition, accepted, refused, attempts, effects⟩
  have same : failing = .createSandbox := by
    by_contra different
    simp [createError, invoke, different] at refused
  subst failing
  exact ⟨before, after, decomposition, accepted, rfl, rfl⟩

theorem failure_and_retry : failed.native.result = .error "synthetic create refusal" ∧
    failed.native.attempts = expectedFailureAttempts context ∧
    failed.recorded.response = .ok true ∧ failed.recorded.rows = expectedRows expectedFailed ∧
    failed.native.native = 5 ∧ (failed.recorded.rows 7).map (·.ready) = some true ∧
    retryClaim.matched = true ∧
    launch (deliver failed.recorded.rows retryClaim .commitAck) key 71 next (some "new release")
      (.ok (retryClaim.rows 7)) = .started retryContext ∧
    retried.native.attempts = expectedAttempts retryContext ∧ retried.recorded.response = .ok true ∧
    retried.recorded.rows = expectedRows expectedSuccess := by
  refine ⟨rfl, rfl, rfl, ?_, rfl, rfl, rfl, rfl, rfl, rfl, ?_⟩
  · change put (put base 7 (some (expectedClaim seed 70 next 0))) 7 (some expectedFailed) = _
    rw [put_same]
    exact put_expected expectedFailed
  · change put (put (put (put base 7 (some (expectedClaim seed 70 next 0)))
      7 (some expectedFailed)) 7 (some (expectedClaim expectedFailed 71 next 30)))
      7 (some expectedSuccess) = _
    repeat rw [put_same]
    exact put_expected expectedSuccess

theorem actual_stale_history : newerClaim.matched = true ∧
    newerClaim.rows = expectedRows expectedNewerClaim ∧
    RejectedCompletion newerClaim.rows (finish newerClaim.rows key 70 none)
      (fail newerClaim.rows key 70 ⟨"obsolete", 700, "v2"⟩) ∧
    newerDone.recorded.response = .ok true ∧
    newerDone.recorded.rows = expectedRows expectedSuccess ∧
    RejectedCompletion newerDone.recorded.rows (finish newerDone.recorded.rows key 70 none)
      (fail newerDone.recorded.rows key 70 ⟨"obsolete", 701, "v2"⟩) := by
  refine ⟨rfl, ?_, ?_, rfl, ?_, ?_⟩
  · change put (put base 7 (some (expectedClaim seed 70 next 0)))
      7 (some expectedNewerClaim) = _
    rw [put_same]
    exact put_expected expectedNewerClaim
  · exact stale_completion newerClaim.rows key 70 none ⟨"obsolete", 700, "v2"⟩
      ((newerClaim.rows 7).getD seed) ⟨71, 661, next⟩ rfl rfl (by decide)
  · change put (put (put base 7 (some (expectedClaim seed 70 next 0)))
      7 (some expectedNewerClaim)) 7 (some expectedSuccess) = _
    repeat rw [put_same]
    exact put_expected expectedSuccess
  · exact ended_completion newerDone.recorded.rows key 70 none ⟨"obsolete", 701, "v2"⟩
      ((newerDone.recorded.rows 7).getD seed) rfl rfl

theorem notice_suppression :
    (finish (showNotice claimed.rows key 70).rows key 70 (some ⟨"v0", "v2", "new release"⟩)).matched = true ∧
    ((finish (showNotice claimed.rows key 70).rows key 70 (some ⟨"v0", "v2", "new release"⟩)).rows 7).map
      (·.notice) = some none := by
  exact ⟨rfl, rfl⟩

/-- Exact-cutoff gap,not an invented complement between observation and claim. -/
theorem strict_boundary :
    running { seed with operation := some ⟨70, 0, next⟩ } 660 = none ∧
    claimAllowed { seed with operation := some ⟨70, 0, next⟩ } key 0 = false := by
  decide

end Fixture

#print axioms claim_exact
#print axioms finish_exact
#print axioms fail_exact
#print axioms stale_completion
#print axioms stages_first_error
#print axioms ordered_success
#print axioms stale_worker
#print axioms Fixture.accepted_success
#print axioms Fixture.observed_failure_prefix
#print axioms Fixture.failure_and_retry
#print axioms Fixture.actual_stale_history

run_cmd do
  for declaration in [``claim_admission, ``claim_exact, ``claim_refused, ``finish_exact, ``fail_exact,
      ``shown_exact, ``stale_completion, ``ended_completion, ``owner_completion_refused,
      ``unrelated_rows, ``commit_error_not_rollback, ``abort_error_preserves, ``launch_claim,
      ``claim_error_no_launch, ``reread_error_no_launch, ``stages_success, ``stages_first_error,
      ``ordered_success, ``stale_worker, ``Fixture.put_same, ``Fixture.put_expected,
      ``Fixture.accepted_success, ``Fixture.observed_failure_prefix, ``Fixture.failure_and_retry, ``Fixture.actual_stale_history,
      ``Fixture.notice_suppression, ``Fixture.strict_boundary] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unexpected Computer lifecycle axiom in {declaration}: {name}"

end ComputerLifecycle
