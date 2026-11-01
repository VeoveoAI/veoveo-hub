import FactoryRetargetModel
import Lean.Util.CollectAxioms

namespace FactoryRetarget

variable {P : Type} [DecidableEq P]

/-- Model keys are complete deployment IDs. Physical native framing additionally
requires distinct (workspace, truncated sandboxName) keys for the compared objects;
no global UUID/truncation injectivity or freshness is asserted. -/
def Frame (d : Deployment) (before after : State P) : Prop :=
  ∀ key, key ≠ d → after.rows key = before.rows key ∧
    after.sandboxes key = before.sandboxes key ∧
    after.providers key = before.providers key ∧
    after.createRequests key = before.createRequests key

def IdentityFrame (before after : State P) : Prop :=
  ∀ key, (after.rows key).map identity = (before.rows key).map identity

def targetFields (r : Row) :=
  (r.linkedRepository, r.trackedBranch, r.trackedName, r.linkedPusher)

def requestedFields (q : Request) :=
  (some q.target.repository, some q.target.trackedBranch,
    some q.target.trackedName, some q.target.pusher)

@[simp] theorem update_same (f : α → α) (d : Deployment) (values : Deployment → α) :
    update f d values d = f (values d) := by simp [update]

@[simp] theorem update_other (f : α → α) (d key : Deployment) (values : Deployment → α)
    (different : key ≠ d) : update f d values key = values key := by
  simp [update, different]

theorem updateRow_identity (sql : Sql) (r : Row) :
    identity (updateRow sql r) = identity r := by
  cases sql <;> simp [updateRow, identity]
  all_goals split <;> simp

theorem sql_zero_rows (d : Deployment) (sql : Sql) (s : State P)
    (missing : s.rows d = none) : (sqlEffect d sql s).rows d = none := by
  simp [sqlEffect, missing]

theorem target_sql_exact (r : Row) (q : Request) (runner : Runner r) :
    updateRow (.target q.target) r =
      { r with
        linkedRepository := some q.target.repository,
        trackedBranch := some q.target.trackedBranch,
        trackedName := some q.target.trackedName,
        linkedPusher := some q.target.pusher } := by
  have present : r.link.isSome = true := runner.1
  simp [updateRow, present]

theorem effect_frame (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (a : Action) (s : State P) : Frame d s (effect d r q o a s) := by
  intro key different
  cases a with
  | sql statement => simp [effect, sqlEffect, update, different]
  | rpc operation =>
      cases operation <;> simp [effect, rpcEffect, providerEffect, update, different]
  | abort error => simp [effect]

theorem failure_frame (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (a : Action) (s : State P) : Frame d s (failureEffect d r q o a s) := by
  intro key different
  cases a with
  | sql statement =>
      cases committed : o.sqlCommittedOnError <;>
        simp [failureEffect, sqlEffect, update, different, committed]
  | rpc operation =>
      cases submitted : o.createSubmittedOnError <;> cases operation <;>
        simp [failureEffect, update, different, submitted]
  | abort error => simp [failureEffect]

theorem frame_trans {d : Deployment} {a b c : State P}
    (first : Frame d a b) (second : Frame d b c) : Frame d a c := by
  intro key different
  rcases first key different with ⟨r₁, n₁, p₁, c₁⟩
  rcases second key different with ⟨r₂, n₂, p₂, c₂⟩
  exact ⟨r₂.trans r₁, n₂.trans n₁, p₂.trans p₁, c₂.trans c₁⟩

theorem sql_identity (d : Deployment) (sql : Sql) (s : State P) :
    IdentityFrame s (sqlEffect d sql s) := by
  intro key
  by_cases same : key = d
  · subst key
    cases value : s.rows d with
    | none => simp [sqlEffect, value]
    | some r => simp [sqlEffect, value, updateRow_identity]
  · simp [sqlEffect, update, same]

theorem effect_identity (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (a : Action) (s : State P) : IdentityFrame s (effect d r q o a s) := by
  cases a with
  | sql statement => exact sql_identity d statement s
  | rpc operation => cases operation <;> intro key <;> rfl
  | abort error => intro key; rfl

theorem failure_identity (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (a : Action) (s : State P) : IdentityFrame s (failureEffect d r q o a s) := by
  cases a with
  | sql statement =>
      cases committed : o.sqlCommittedOnError
      · intro key; simp [failureEffect, committed]
      · simpa [failureEffect, committed] using sql_identity d statement s
  | rpc operation => cases operation <;> intro key <;> rfl
  | abort error => intro key; rfl

@[simp] theorem stopped_original_error (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (step : Step) (error : Error) (s : State P) :
    (stopped d r q o step error s).error = some error := rfl

theorem stopped_frame (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (step : Step) (error : Error) (s : State P) :
    Frame d s (stopped d r q o step error s).state := by
  unfold stopped
  cases recordedEq : step.recorded with
  | none => exact failure_frame d r q o step.action s
  | some recordStage =>
      cases log : o.failureLogCommitted
      · exact failure_frame d r q o step.action s
      · exact frame_trans (failure_frame d r q o step.action s)
          (effect_frame d r q o (.sql (.failed recordStage error.message)) _)

theorem stopped_identity (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (step : Step) (error : Error) (s : State P) :
    IdentityFrame s (stopped d r q o step error s).state := by
  unfold stopped
  cases recordedEq : step.recorded with
  | none => exact failure_identity d r q o step.action s
  | some recordStage =>
      cases log : o.failureLogCommitted
      · exact failure_identity d r q o step.action s
      · intro key
        exact (sql_identity d (.failed recordStage error.message) _ key).trans
          (failure_identity d r q o step.action s key)

theorem run_frame (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (steps : List Step) (s : State P) : Frame d s (run d r q o steps s).state := by
  induction steps generalizing s with
  | nil => intro key different; exact ⟨rfl, rfl, rfl, rfl⟩
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some error => simpa [run, observed] using stopped_frame d r q o first error s
      | none =>
          simpa [run, observed] using frame_trans (effect_frame d r q o first.action s)
            (recursive (effect d r q o first.action s))

theorem run_identity (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (steps : List Step) (s : State P) : IdentityFrame s (run d r q o steps s).state := by
  induction steps generalizing s with
  | nil => intro key; rfl
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some error => simpa [run, observed] using stopped_identity d r q o first error s
      | none =>
          intro key
          simpa [run, observed] using (recursive (effect d r q o first.action s) key).trans
            (effect_identity d r q o first.action s key)

/-- Complete state at every normal return, including refused/unknown-commit paths,
frames unrelated whole objects in the model. No successful-after-state premise. -/
theorem apply_frame (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (s : State P) : Frame d s (apply d r q o s).state :=
  run_frame d r q o _ s

theorem apply_identity (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (s : State P) : IdentityFrame s (apply d r q o s).state :=
  run_identity d r q o _ s

def foldEffects (d : Deployment) (r : Row) (q : Request) (o : Observations P) :
    List Step → State P → State P
  | [], s => s
  | first :: rest, s => foldEffects d r q o rest (effect d r q o first.action s)

def rowAction (a : Action) (r : Row) : Row :=
  match a with | .sql sql => updateRow sql r | _ => r

def rowFold : List Step → Row → Row
  | [], r => r
  | first :: rest, r => rowFold rest (rowAction first.action r)

theorem rowFold_append (a b : List Step) (r : Row) :
    rowFold (a ++ b) r = rowFold b (rowFold a r) := by
  induction a generalizing r with
  | nil => rfl
  | cons first rest recursive => simpa [rowFold] using recursive (rowAction first.action r)

theorem effect_row (d : Deployment) (original r : Row) (q : Request) (o : Observations P)
    (a : Action) (s : State P) (existing : s.rows d = some r) :
    (effect d original q o a s).rows d = some (rowAction a r) := by
  cases a with
  | sql statement => simp [effect, rowAction, sqlEffect, existing]
  | rpc operation => cases operation <;> simpa [effect, rpcEffect, providerEffect, rowAction] using existing
  | abort error => simpa [effect, rowAction] using existing

theorem fold_row (d : Deployment) (original r : Row) (q : Request) (o : Observations P)
    (steps : List Step) (s : State P) (existing : s.rows d = some r) :
    (foldEffects d original q o steps s).rows d = some (rowFold steps r) := by
  induction steps generalizing s r with
  | nil => exact existing
  | cons first rest recursive =>
      simpa [foldEffects, rowFold] using recursive (rowAction first.action r)
        (effect d original q o first.action s) (effect_row d original r q o first.action s existing)

theorem run_success_fold (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (steps : List Step) (s : State P) (normal : (run d r q o steps s).error = none) :
    (run d r q o steps s).state = foldEffects d r q o steps s := by
  induction steps generalizing s with
  | nil => rfl
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some error => simp [run, observed] at normal
      | none =>
          have tailNormal : (run d r q o rest (effect d r q o first.action s)).error = none := by
            simpa [run, observed] using normal
          simpa [run, observed, foldEffects] using recursive _ tailNormal

/-- Existing capacity acceptance is an independent count/config fact, never a
premise asserting the desired resulting row. Later lemma derives it from success. -/
def CapacityAllowed (o : Observations P) (native : Option (Sandbox P)) : Prop :=
  native.isSome = true ∨ o.admission.configured = false ∨
    o.admission.count ≤ o.admission.limit ∨ o.admission.enforce = false

theorem complete_row_algebra (r : Row) (q : Request) (o : Observations P)
    (native : Option (Sandbox P)) (runner : Runner r) (capacity : CapacityAllowed o native) :
    rowFold (program r q o native) r = successRow r q o (choice r q o.prepared native) := by
  have linked : r.link.isSome = true := runner.1
  cases native with
  | none =>
      simp only [program, choice, admission, creation]
      split_ifs <;>
        simp_all [rowFold_append, rowFold, rowAction, updateRow, successRow, CapacityAllowed]
      all_goals omega
  | some old =>
      cases spec : old.spec with
      | none =>
          simp only [program, choice, spec, admission, creation]
          split_ifs <;>
            simp_all [rowFold_append, rowFold, rowAction, updateRow, successRow, CapacityAllowed]
      | some config =>
          simp only [program, choice, spec, admission, creation]
          split_ifs <;>
            simp_all [rowFold_append, rowFold, rowAction, updateRow, successRow, CapacityAllowed]

/-- Whole successful row, derived from actual modeled steps and independent input
facts. Explicit initial binding/key/Runner conditions are interpretation premises. -/
theorem successful_apply_exact (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (capacity : CapacityAllowed o o.readNative)
    (normal : (apply d r q o s).error = none) :
    (apply d r q o s).state.rows d =
      some (successRow r q o (choice r q o.prepared o.readNative)) := by
  rw [show (apply d r q o s).state =
      foldEffects d r q o (program r q o o.readNative) s from
        run_success_fold d r q o _ s normal]
  rw [fold_row d r r q o _ s existing, complete_row_algebra r q o _ runner capacity]

theorem successful_target_client_selection (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (capacity : CapacityAllowed o o.readNative)
    (normal : (apply d r q o s).error = none) :
    ∃ final, (apply d r q o s).state.rows d = some final ∧
      identity final = identity r ∧ targetFields final = requestedFields q ∧
      (client final).linkedRepository = some q.target.repository ∧
      (client final).linkedPusher = some q.target.pusher ∧
      selectedByPush final q.target.repository q.target.trackedBranch := by
  refine ⟨successRow r q o _, successful_apply_exact d r q o s existing key runner capacity normal, ?_⟩
  simp [successRow, identity, targetFields, requestedFields, client, selectedByPush]

/-- Full issued request, not a claim of remote receipt, readiness or checkout hash. -/
theorem constructed_request_exact (r : Row) (q : Request) (p : Prepared P) :
    (createRequest r q p).name = sandboxName r p ∧
    (createRequest r q p).workspace = p.workspace ∧
    (createRequest r q p).spec = configuration r q p ∧
    (createRequest r q p).spec.command =
      ["/bin/sh", "-c", p.runnerScript, "runner",
       (p.loreBase.setPath ("/" ++ r.repository)).render, r.branch,
       (p.loreBase.setPath ("/" ++ q.target.repository)).render,
       q.target.trackedBranch] ++ q.ownEntries := by exact ⟨rfl, rfl, rfl, rfl⟩

/-- Current row after an arbitrary failed action, including an atomic committed
SQL write followed by a lost reply and independently committed failure logging. -/
def refusedRow (o : Observations P) (step : Step) (error : Error) (before : Row) : Row :=
  let committed := if o.sqlCommittedOnError then rowAction step.action before else before
  match step.recorded with
  | none => committed
  | some stage => if o.failureLogCommitted then
      updateRow (.failed stage error.message) committed else committed

theorem failure_effect_row (d : Deployment) (original before : Row) (q : Request)
    (o : Observations P) (a : Action) (s : State P) (existing : s.rows d = some before) :
    (failureEffect d original q o a s).rows d =
      some (if o.sqlCommittedOnError then rowAction a before else before) := by
  cases a with
  | sql statement =>
      cases committed : o.sqlCommittedOnError <;>
        simp [failureEffect, sqlEffect, rowAction, existing, committed]
  | rpc operation =>
      cases operation <;> cases committed : o.sqlCommittedOnError <;>
        simp [failureEffect, rowAction, existing, committed]
  | abort error =>
      cases committed : o.sqlCommittedOnError <;> simp [failureEffect, rowAction, existing, committed]

theorem stopped_row_exact (d : Deployment) (original before : Row) (q : Request)
    (o : Observations P) (step : Step) (error : Error) (s : State P)
    (existing : s.rows d = some before) :
    (stopped d original q o step error s).state.rows d = some (refusedRow o step error before) := by
  unfold stopped refusedRow
  cases saved : step.recorded with
  | none => simpa [saved] using failure_effect_row d original before q o step.action s existing
  | some stage =>
      cases log : o.failureLogCommitted
      · simpa [saved, log] using failure_effect_row d original before q o step.action s existing
      · simp [saved, log, sqlEffect,
          failure_effect_row d original before q o step.action s existing]

/-- On the changed-target SQL failure, both committed and uncommitted outcomes
remain represented; the optional logging write cannot change the linked target. -/
theorem uncertain_target_write (r : Row) (q : Request) (o : Observations P)
    (runner : Runner r) (error : Error) :
    targetFields (refusedRow o (recorded .create (.sql (.target q.target))) error r) =
      if o.sqlCommittedOnError then requestedFields q else targetFields r := by
  have linked : r.link.isSome = true := runner.1
  cases committed : o.sqlCommittedOnError <;> cases logged : o.failureLogCommitted <;>
    simp [refusedRow, rowAction, updateRow, targetFields, requestedFields, linked, committed, logged]

/-- Raw custody material is distinct from the stored digest; this corollary uses
only the explicit native SHA relation, not an assumed successful client result. -/
theorem created_client_projection (r : Row) (q : Request) (o : Observations P)
    (path : Choice) (creating : path = .replace ∨ path = .create)
    (hash : String → String)
    (digest : hash o.prepared.clientSecretRaw = o.prepared.clientSecretDigest) :
    clientForSecret hash (refreshMaterial r o.prepared).clientSecretRaw
        (successRow r q o path) = some (client (successRow r q o path)) ∧
      (refreshMaterial r o.prepared).clientId = o.prepared.deploymentText r.deployment ∧
      (refreshMaterial r o.prepared).credentialKey = "LORE_ACCESS_TOKEN" := by
  simp [clientForSecret, refreshMaterial, successRow, creating, digest]

theorem other_target_excluded (r : Row) (q : Request) (o : Observations P)
    (path : Choice) (repository branch : String)
    (different : repository ≠ q.target.repository ∨ branch ≠ q.target.trackedBranch) :
    ¬ selectedByPush (successRow r q o path) repository branch := by
  rcases different with repositoryDifferent | branchDifferent
  · simp [selectedByPush, successRow, Ne.symm repositoryDifferent]
  · simp [selectedByPush, successRow, Ne.symm branchDifferent]

/-- Historical source defect: the original linked.sql leaves both target-ID
columns untouched while Source::command already constructs the requested target.
This theorem does not pretend that old source was Aeneas-extracted/verified. -/
theorem historical_omission (r : Row) (q : Request) (p : Prepared P)
    (oldRepository oldBranch : String)
    (repository : r.linkedRepository = some oldRepository)
    (branch : r.trackedBranch = some oldBranch)
    (different : oldRepository ≠ q.target.repository) :
    (client (historicalLinked q.target r)).linkedRepository = some oldRepository ∧
    selectedByPush (historicalLinked q.target r) oldRepository oldBranch ∧
    ¬ selectedByPush (historicalLinked q.target r) q.target.repository q.target.trackedBranch ∧
    (command r q p)[6]? = some ((p.loreBase.setPath ("/" ++ q.target.repository)).render) ∧
    (command r q p)[7]? = some q.target.trackedBranch := by
  simp [historicalLinked, client, selectedByPush, command, repository, branch, different]

#print axioms successful_apply_exact
#print axioms successful_target_client_selection
#print axioms constructed_request_exact
#print axioms stopped_row_exact
#print axioms uncertain_target_write
#print axioms created_client_projection
#print axioms historical_omission

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``updateRow, ``program, ``apply, ``clientForSecret,
      ``updateRow_identity, ``sql_zero_rows, ``target_sql_exact,
      ``effect_frame, ``failure_frame, ``run_frame, ``run_identity,
      ``apply_frame, ``apply_identity, ``run_success_fold,
      ``complete_row_algebra, ``successful_apply_exact, ``successful_target_client_selection,
      ``constructed_request_exact, ``stopped_original_error, ``stopped_row_exact,
      ``uncertain_target_write, ``created_client_projection, ``other_target_excluded,
      ``historical_omission] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected manual Factory protocol axiom in {declaration}: {unexpected}"

end FactoryRetarget
