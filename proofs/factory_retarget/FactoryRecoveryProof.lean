import FactoryNativeOutputProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget
variable {P : Type} [DecidableEq P]

/-- Every observation on this exact finite prefix succeeded. These are equations
for the existing operation, not an assertion about the desired after-state. -/
def ClearPrefix (d : Deployment) (r : Row) (q : Request) (o : Observations P) :
    List Step → State P → Prop
  | [], _ => True
  | first :: rest, s => observedError d o first s = none ∧
      ClearPrefix d r q o rest (effect d r q o first.action s)

def successEvents (steps : List Step) : List Event :=
  steps.map (fun step => .success step.action)

/-- Exact continuation and event composition for successful prefix observations. -/
theorem run_clear_append (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (before later : List Step) (s : State P)
    (clear : ClearPrefix d r q o before s) :
    run d r q o (before ++ later) s =
      let tail := run d r q o later (foldEffects d r q o before s)
      { tail with events := successEvents before ++ tail.events } := by
  induction before generalizing s with
  | nil => simp [run, foldEffects, successEvents]
  | cons first rest recursive =>
      rcases clear with ⟨observed, remaining⟩
      simp only [List.cons_append, run, observed]
      rw [recursive _ remaining]
      simp [foldEffects, successEvents]

/-- Once a prefix has returned an error, later modeled steps are not executed. -/
theorem run_error_append (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (before later : List Step) (s : State P) (error : Error)
    (failed : (run d r q o before s).error = some error) :
    run d r q o (before ++ later) s = run d r q o before s := by
  induction before generalizing s with
  | nil => simp [run] at failed
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some cause => simp [run, observed]
      | none =>
          have tailFailed : (run d r q o rest (effect d r q o first.action s)).error = some error := by
            simpa [run, observed] using failed
          simp only [List.cons_append, run, observed]
          rw [recursive _ tailFailed]

/-- Exact complete state and trace at a specified first refused observation.
`stopped` retains uncertain statement commit and independent failure-log outcomes. -/
theorem run_refuses_at (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (before later : List Step) (first : Step)
    (s : State P) (error : Error) (clear : ClearPrefix d r q o before s)
    (refused : observedError d o first (foldEffects d r q o before s) = some error) :
    run d r q o (before ++ first :: later) s =
      let halted := stopped d r q o first error (foldEffects d r q o before s)
      { halted with events := successEvents before ++ halted.events } := by
  rw [run_clear_append d r q o before (first :: later) s clear]
  simp [run, refused]

/-- Every failed normal return has an actual successful prefix and first error;
all original errors, whole state and event ordering are retained exactly. -/
theorem failed_run_prefix (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (steps : List Step) (s : State P) (error : Error)
    (failed : (run d r q o steps s).error = some error) :
    ∃ before first later, steps = before ++ first :: later ∧
      ClearPrefix d r q o before s ∧
      observedError d o first (foldEffects d r q o before s) = some error ∧
      (run d r q o steps s).state =
        (stopped d r q o first error (foldEffects d r q o before s)).state ∧
      (run d r q o steps s).events = successEvents before ++
        (stopped d r q o first error (foldEffects d r q o before s)).events := by
  induction steps generalizing s with
  | nil => simp [run] at failed
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some cause =>
          have same : cause = error := by simpa [run, observed] using failed
          subst cause
          exact ⟨[], first, rest, rfl, trivial, observed, by simp [run, observed, foldEffects],
            by simp [run, observed, successEvents, foldEffects]⟩
      | none =>
          have tailFailed : (run d r q o rest (effect d r q o first.action s)).error = some error := by
            simpa [run, observed] using failed
          obtain ⟨before, refused, later, shape, clear, rejected, state, events⟩ :=
            recursive _ tailFailed
          refine ⟨first :: before, refused, later, ?_, ⟨observed, clear⟩, ?_, ?_, ?_⟩
          · simp [shape]
          · simpa [foldEffects] using rejected
          · simpa [run, observed, foldEffects] using state
          · simpa [run, observed, foldEffects, successEvents] using events

/-- Exact complete row at that first error, including successful SQL writes,
unknown commit of the failed SQL and best-effort failure logging. -/
theorem failed_row_prefix (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (steps : List Step) (s : State P) (error : Error)
    (existing : s.rows d = some r) (failed : (run d r q o steps s).error = some error) :
    ∃ before first later, steps = before ++ first :: later ∧
      ClearPrefix d r q o before s ∧
      observedError d o first (foldEffects d r q o before s) = some error ∧
      (run d r q o steps s).state.rows d =
        some (refusedRow o first error (rowFold before r)) := by
  obtain ⟨before, first, later, shape, clear, refused, state, _⟩ :=
    failed_run_prefix d r q o steps s error failed
  refine ⟨before, first, later, shape, clear, refused, ?_⟩
  rw [state]
  exact stopped_row_exact d r _ q o first error _ (fold_row d r r q o before s existing)

@[simp] theorem updateRow_runner (sql : Sql) (r : Row) (runner : Runner r) :
    Runner (updateRow sql r) := by
  cases sql <;> simp_all [Runner, updateRow]
  all_goals split <;> simp_all [Runner]

theorem rowFold_runner (steps : List Step) (r : Row) (runner : Runner r) :
    Runner (rowFold steps r) := by
  induction steps generalizing r with
  | nil => exact runner
  | cons first rest recursive =>
      apply recursive
      cases action : first.action <;> simp_all [rowFold, rowAction]

theorem refusedRow_runner (o : Observations P) (step : Step) (error : Error)
    (r : Row) (runner : Runner r) : Runner (refusedRow o step error r) := by
  cases committed : o.sqlCommittedOnError <;> cases record : step.recorded <;>
    cases logged : o.failureLogCommitted <;> cases action : step.action <;>
      simp_all [refusedRow, rowAction]

/-- Existence and Runner are derived from every return, including failures. Full
immutable identity is derived separately from the operation's existing frame. -/
theorem run_surviving_runner (d : Deployment) (original : Row) (q : Request)
    (o : Observations P) (steps : List Step) (s : State P) (r : Row)
    (existing : s.rows d = some r) (runner : Runner r) :
    ∃ final, (run d original q o steps s).state.rows d = some final ∧ Runner final := by
  induction steps generalizing s r with
  | nil => exact ⟨r, existing, runner⟩
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some error =>
          refine ⟨refusedRow o first error r, ?_, refusedRow_runner o first error r runner⟩
          simpa [run, observed] using stopped_row_exact d original r q o first error s existing
      | none =>
          have next : Runner (rowAction first.action r) := by
            cases action : first.action <;> simp_all [rowAction]
          simpa [run, observed] using recursive _ _
            (effect_row d original r q o first.action s existing) next

theorem apply_surviving_runner (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (runner : Runner r) :
    ∃ final, (apply d r q o s).state.rows d = some final ∧ Runner final ∧
      identity final = identity r := by
  obtain ⟨final, stored, valid⟩ := run_surviving_runner d r q o _ s r existing runner
  change (apply d r q o s).state.rows d = some final at stored
  refine ⟨final, stored, valid, ?_⟩
  have same := apply_identity d r q o s d
  apply Option.some.inj
  simpa only [stored, existing, Option.map_some] using same

/-- Target-field writes are uniquely identified, with all payload data retained. -/
def TargetWrite (a : Action) : Prop := ∃ target, a = .sql (.target target)

theorem non_target_row (a : Action) (r : Row) (notTarget : ¬ TargetWrite a) :
    targetFields (rowAction a r) = targetFields r := by
  cases a with
  | sql statement =>
      cases statement <;> simp_all [TargetWrite, rowAction, updateRow, targetFields]
      all_goals cases linked : r.link.isSome <;> simp [linked]
  | rpc operation => rfl
  | abort error => rfl

theorem non_target_fold (steps : List Step) (r : Row)
    (noTarget : ∀ step ∈ steps, ¬ TargetWrite step.action) :
    targetFields (rowFold steps r) = targetFields r := by
  induction steps generalizing r with
  | nil => rfl
  | cons first rest recursive =>
      rw [rowFold, recursive _ (fun step member => noTarget step (by simp [member]))]
      exact non_target_row first.action r (noTarget first (by simp))

theorem non_target_refusal (o : Observations P) (step : Step) (error : Error)
    (r : Row) (notTarget : ¬ TargetWrite step.action) :
    targetFields (refusedRow o step error r) = targetFields r := by
  have same := non_target_row step.action r notTarget
  have logging : ∀ row stage message,
      targetFields (updateRow (.failed stage message) row) = targetFields row := by
    intro row stage message
    rfl
  cases committed : o.sqlCommittedOnError <;> cases record : step.recorded <;>
    cases logged : o.failureLogCommitted <;>
      simp_all [refusedRow, logging]

/-- Without a target write, every return preserves all four old linked-target
fields, including uncertain commit and failure-log effects. No native claim. -/
theorem run_non_target (d : Deployment) (original : Row) (q : Request)
    (o : Observations P) (steps : List Step) (s : State P) (r : Row)
    (existing : s.rows d = some r)
    (noTarget : ∀ step ∈ steps, ¬ TargetWrite step.action) :
    ∃ final, (run d original q o steps s).state.rows d = some final ∧
      targetFields final = targetFields r := by
  induction steps generalizing s r with
  | nil => exact ⟨r, existing, rfl⟩
  | cons first rest recursive =>
      have firstSafe := noTarget first (by simp)
      have tailSafe : ∀ step ∈ rest, ¬ TargetWrite step.action :=
        fun step member => noTarget step (by simp [member])
      cases observed : observedError d o first s with
      | some error =>
          refine ⟨refusedRow o first error r, ?_, non_target_refusal o first error r firstSafe⟩
          simpa [run, observed] using stopped_row_exact d original r q o first error s existing
      | none =>
          obtain ⟨final, stored, same⟩ := recursive _ _
            (effect_row d original r q o first.action s existing) tailSafe
          refine ⟨final, ?_, same.trans (non_target_row first.action r firstSafe)⟩
          simpa [run, observed] using stored

/-- Actual prefix payload includes the requested parent pusher; every other field
is taken from the same observations as `program`. -/
def retargetFront (q : Request) (o : Observations P) : List Step :=
  (if o.installationDatabase then [] else [recorded .database (.rpc .ensureDatabase)]) ++
  [recorded .build (.rpc .build), recorded .create (.rpc .policies),
   recorded .create (.rpc .template), recorded .create (.rpc .requestedProviders),
   direct (.sql (.ungranted o.prepared.ungranted)), direct (.sql (.push q.pusher)),
   recorded .create (.rpc .readSandbox), direct (.rpc .runnerApplied),
   direct (.sql .unserved)] ++
  match o.readNative with
  | some _ => [recorded .create (.rpc .deleteSandbox)]
  | none => admission o.admission

/-- Both changed-target paths commit the complete target only after this prefix;
for the present path the successful delete is in that prefix. -/
theorem changed_program_decomposition (r : Row) (q : Request) (o : Observations P)
    (retarget : changed r q = true) :
    program r q o o.readNative = retargetFront q o ++
      recorded .create (.sql (.target q.target)) :: creation o.prepared ++
        [direct (.sql (.linkedApplied q.target.revision)), direct (.sql (.ownTree q.ownTree)),
         direct (.sql (.applied q.revision))] := by
  cases native : o.readNative with
  | none => cases installed : o.installationDatabase <;>
      simp [program, retargetFront, choice, retarget, native, installed, List.append_assoc]
  | some old =>
      cases spec : old.spec <;> cases installed : o.installationDatabase <;>
        simp [program, retargetFront, choice, retarget, native, spec, installed, List.append_assoc]

theorem retarget_front_no_target (q : Request) (o : Observations P) :
    ∀ step ∈ retargetFront q o, ¬ TargetWrite step.action := by
  cases native : o.readNative <;> cases installed : o.installationDatabase <;>
    simp [retargetFront, native, installed, admission]
  all_goals try split_ifs
  all_goals simp_all [TargetWrite, direct, recorded]
  all_goals aesop

/-- Failure anywhere in the actual preparation/read/delete/admission segment
returns before target SQL. The row may have other updates, and delete failure may
leave no native worker. Only the old linked-target tuple is promised here. -/
theorem retarget_precommit_failure (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (error : Error) (existing : s.rows d = some r)
    (runner : Runner r) (retarget : changed r q = true)
    (failed : (run d r q o (retargetFront q o) s).error = some error) :
    apply d r q o s = run d r q o (retargetFront q o) s ∧
    ∃ final, (apply d r q o s).state.rows d = some final ∧
      targetFields final = targetFields r ∧ Runner final ∧ identity final = identity r := by
  have stoppedEarly : apply d r q o s = run d r q o (retargetFront q o) s := by
    unfold apply
    rw [changed_program_decomposition r q o retarget]
    simpa only [List.append_assoc] using
      run_error_append d r q o (retargetFront q o)
        ((recorded .create (.sql (.target q.target)) :: creation o.prepared) ++
          [direct (.sql (.linkedApplied q.target.revision)), direct (.sql (.ownTree q.ownTree)),
           direct (.sql (.applied q.revision))]) s error failed
  refine ⟨stoppedEarly, ?_⟩
  obtain ⟨final, stored, same⟩ := run_non_target d r q o (retargetFront q o) s r existing
    (retarget_front_no_target q o)
  obtain ⟨survivor, bound, valid, immutable⟩ := apply_surviving_runner d r q o s existing runner
  have rowEq : final = survivor := by
    have sameSome : some final = some survivor := by simpa [stoppedEarly, stored] using bound
    exact Option.some.inj sameSome
  subst survivor
  exact ⟨final, by simpa [stoppedEarly] using stored, same, valid, immutable⟩

/-- Preserving the full target tuple also preserves the source retarget decision. -/
theorem changed_of_same_target (before after : Row) (q : Request)
    (same : targetFields after = targetFields before) : changed after q = changed before q := by
  have fields : after.linkedRepository = before.linkedRepository ∧
      after.trackedBranch = before.trackedBranch ∧ after.trackedName = before.trackedName ∧
      after.linkedPusher = before.linkedPusher := by simpa [targetFields] using same
  simp [changed, fields.1, fields.2.1]

/-- Independent complete retry output, relative to the actual row/state left by
failure. No result is assumed, and prior partial native effects are retained. -/
def RetryOutput (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (before : State P) : Prop :=
  let final := (apply d r q o before).state
  final.rows d = some (successRow r q o (choice r q o.prepared o.readNative)) ∧
  final.sandboxes d = nativeOutput r q o ∧
  final.providers d = providerOutput r q o (before.providers d) ∧
  final.createRequests d = requestOutput r q o (before.createRequests d) ∧
  Frame d before final

set_option maxHeartbeats 600000 in
/-- No-error observations plus the source's independent capacity facts construct
acceptance, including absence following a delete whose error lost its reply. -/
theorem no_fault_capacity_accepts (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (noFault : o.fault = none) (capacity : CapacityAllowed o o.readNative) :
    (apply d r q o s).error = none := by
  have createAbsent : choice r q o.prepared o.readNative = .create → o.readNative = none := by
    intro selected
    cases native : o.readNative with
    | none => rfl
    | some old =>
        cases spec : old.spec with
        | none => simp [choice, native, spec] at selected
        | some config =>
            simp only [choice, native, spec] at selected
            split_ifs at selected <;> contradiction
  have noAbort : ∀ step ∈ program r q o o.readNative,
      ∀ error, step.action ≠ .abort error := by
    -- Only the selected program branch is unfolded, not policy/configuration
    -- comparisons inside choice. The creation list contains no abort itself.
    cases selected : choice r q o.prepared o.readNative with
    | keep =>
        simp only [program, selected]
        split_ifs <;> simp [direct, recorded]
    | restart =>
        simp only [program, selected]
        split_ifs <;> simp [direct, recorded]
    | replace =>
        simp only [program, selected, creation]
        split_ifs <;> simp [direct, recorded]
    | create =>
        have absent := createAbsent selected
        simp only [program, selected, admission, creation]
        split_ifs <;> simp_all [CapacityAllowed, direct, recorded]
        all_goals omega
  exact no_fault_run d r q o _ s r existing noFault noAbort

/-- Any actual failed return leaves an existing Runner for conditional recovery.
No-error and capacity observations construct the second call's success rather
than assuming it. This includes unknown target commits and failed creation, and
both present and absent native retry observations. No automatic retry is claimed. -/
theorem retry_after_failure (d : Deployment) (r : Row) (q : Request)
    (first retry : Observations P) (s : State P) (error : Error)
    (existing : s.rows d = some r) (key : r.deployment = d) (runner : Runner r)
    (failed : (apply d r q first s).error = some error)
    (noFault : retry.fault = none) (capacity : CapacityAllowed retry retry.readNative) :
    ∃ intermediate, (apply d r q first s).state.rows d = some intermediate ∧
      Runner intermediate ∧ identity intermediate = identity r ∧
      (apply d intermediate q retry (apply d r q first s).state).error = none ∧
      RetryOutput d intermediate q retry (apply d r q first s).state ∧
      Frame d s (apply d intermediate q retry (apply d r q first s).state).state := by
  obtain ⟨intermediate, stored, valid, immutable⟩ :=
    apply_surviving_runner d r q first s existing runner
  have nextKey : intermediate.deployment = d := by
    have firstField := congrArg (fun value => value.1) immutable
    simpa [identity, key] using firstField
  have accepted := no_fault_capacity_accepts d intermediate q retry _ stored noFault capacity
  refine ⟨intermediate, stored, valid, immutable, accepted, ?_, ?_⟩
  · exact successful_outputs_complete d intermediate q retry _ stored nextKey valid accepted
  · exact frame_trans (apply_frame d r q first s) (apply_frame d intermediate q retry _)

/-- Precommit recovery additionally derives unchanged old target and a still
changed request. A missing worker is permitted; capacity is the same source fact
as for any native-absent creation, never a desired-success premise. -/
theorem retarget_retry_after_precommit_failure (d : Deployment) (r : Row) (q : Request)
    (first retry : Observations P) (s : State P) (error : Error)
    (existing : s.rows d = some r) (key : r.deployment = d) (runner : Runner r)
    (retarget : changed r q = true)
    (failed : (run d r q first (retargetFront q first) s).error = some error)
    (noFault : retry.fault = none) (capacity : CapacityAllowed retry retry.readNative) :
    ∃ intermediate, (apply d r q first s).state.rows d = some intermediate ∧
      Runner intermediate ∧ identity intermediate = identity r ∧
      targetFields intermediate = targetFields r ∧ changed intermediate q = true ∧
      (apply d intermediate q retry (apply d r q first s).state).error = none ∧
      RetryOutput d intermediate q retry (apply d r q first s).state ∧
      Frame d s (apply d intermediate q retry (apply d r q first s).state).state := by
  obtain ⟨stoppedEarly, targetRow, targetStored, same, _, _⟩ :=
    retarget_precommit_failure d r q first s error existing runner retarget failed
  have fullFailed : (apply d r q first s).error = some error := by
    simpa [stoppedEarly] using failed
  obtain ⟨intermediate, stored, valid, immutable, accepted, outputs, frame⟩ :=
    retry_after_failure d r q first retry s error existing key runner fullFailed noFault capacity
  have rowEq : targetRow = intermediate := Option.some.inj (targetStored.symm.trans stored)
  subst targetRow
  exact ⟨intermediate, stored, valid, immutable, same,
    (changed_of_same_target r intermediate q same).trans retarget, accepted, outputs, frame⟩

#print axioms failed_run_prefix
#print axioms failed_row_prefix
#print axioms apply_surviving_runner
#print axioms changed_program_decomposition
#print axioms retarget_precommit_failure
#print axioms no_fault_capacity_accepts
#print axioms retry_after_failure
#print axioms retarget_retry_after_precommit_failure

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``run_clear_append, ``run_error_append, ``run_refuses_at,
      ``failed_run_prefix, ``failed_row_prefix, ``updateRow_runner, ``rowFold_runner,
      ``refusedRow_runner, ``run_surviving_runner, ``apply_surviving_runner,
      ``non_target_row, ``non_target_fold, ``non_target_refusal, ``run_non_target,
      ``changed_program_decomposition, ``retarget_front_no_target,
      ``retarget_precommit_failure, ``changed_of_same_target,
      ``no_fault_capacity_accepts, ``retry_after_failure, ``retarget_retry_after_precommit_failure] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected recovery protocol axiom in {declaration}: {unexpected}"

end FactoryRetarget
