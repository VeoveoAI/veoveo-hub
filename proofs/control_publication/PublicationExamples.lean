import PublicationProof

namespace ControlPublication

def exampleContext : ContextRows where
  id := 11
  tenant := 7
  tenantRow := 70
  row := 110
  grants := [21, 22]
  memberships := [31, 32]

def examplePlan (revision : Id) (expected : Option Id) : Plan where
  revision
  document := revision + 100
  expected
  objects := [41, 42]
  enterprise := 50
  contexts := [exampleContext]
  batch := [{ id := revision + 60, content := revision + 600 }]

/-- Genuine primitive counterexample: a premature commit persists the revision
without objects, Work Contexts, pointer or audit batch. Completeness cannot be a
precondition baked into commit if this fault is to remain expressible. -/
theorem premature_commit_is_partial :
    let plan := examplePlan 1 none
    let after := commit emptyDatabase [.revision plan.revision plan.document]
    after.revisions 1 = some 101 ∧ after.objects 1 = [] ∧ after.contexts 11 = none ∧
      after.active = none ∧ after.audit 61 = none := by
  simp [commit, applyWrites, applyWrite, examplePlan, emptyDatabase, put]

/-- A transaction that commits activation before calling the appender can leave
an active revision with no successful publication batch when that append fails. -/
theorem split_commit_loses_audit_atomicity :
    let plan := examplePlan 1 none
    let after := commit emptyDatabase (plan.writes.dropLast)
    after.active = some 1 ∧ after.audit 61 = none ∧ after.contexts 11 = some 110 := by
  simp [commit, applyWrites, applyWrite, Plan.writes, Plan.contextWrites, contextWrites,
    examplePlan, exampleContext, emptyDatabase, keepOrInsert, put]

/-- Omitting the locked predecessor check is a distinct fault: the new revision
can be complete yet overwrite a winner despite an obsolete expected predecessor. -/
theorem omitted_cas_overwrites_stale_predecessor :
    let winner := commit emptyDatabase (examplePlan 1 none).writes
    let stale := examplePlan 2 none
    winner.active ≠ stale.expected ∧
      (commit winner stale.writes).active = some 2 ∧
      (commit winner stale.writes).revisions 1 = some 101 := by
  simp [commit, applyWrites, applyWrite, Plan.writes, Plan.contextWrites, contextWrites,
    examplePlan, exampleContext, emptyDatabase, keepOrInsert, put]

theorem conflicting_incoming_audit_id_refuses :
    ¬ ValidBatch emptyDatabase.audit [{ id := 3, content := 4 }, { id := 3, content := 5 }] := by
  simp [ValidBatch, emptyDatabase, put]

theorem identical_incoming_audit_id_is_idempotent :
    ValidBatch emptyDatabase.audit [{ id := 3, content := 4 }, { id := 3, content := 4 }] ∧
      appendAudit emptyDatabase.audit [{ id := 3, content := 4 }, { id := 3, content := 4 }] 3 = some 4 := by
  simp [ValidBatch, appendAudit, emptyDatabase, put]

/-- A client error while committing leaves both server outcomes possible.
Neither transition depends on a successful response being delivered. These are
primitive-state witnesses, not yet claimed reachable program executions. -/
def uncertainExample : State where
  db := emptyDatabase
  transactions := fun _ => {
    plan := examplePlan 1 none
    phase := .committing
    staged := (examplePlan 1 none).writes
    response := .error }
  lock := some 0
  commits := []

theorem uncertain_server_may_commit :
    Step uncertainExample
      { updateTransaction uncertainExample 0
          { uncertainExample.transactions 0 with phase := .committed } with
        db := commit uncertainExample.db (uncertainExample.transactions 0).staged
        lock := release uncertainExample.lock 0
        commits := [{ publisher := 0, predecessor := none, revision := 1 }] } := by
  exact Step.serverCommit uncertainExample 0 rfl

theorem uncertain_server_may_abort :
    Step uncertainExample
      { updateTransaction uncertainExample 0
          { uncertainExample.transactions 0 with phase := .aborted, staged := [], remaining := [] } with
        lock := release uncertainExample.lock 0 } := by
  exact Step.serverAbortUncertain uncertainExample 0 rfl rfl

#print axioms premature_commit_is_partial
#print axioms split_commit_loses_audit_atomicity
#print axioms omitted_cas_overwrites_stale_predecessor
#print axioms conflicting_incoming_audit_id_refuses
#print axioms identical_incoming_audit_id_is_idempotent
#print axioms uncertain_server_may_commit
#print axioms uncertain_server_may_abort

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``premature_commit_is_partial, ``split_commit_loses_audit_atomicity,
      ``omitted_cas_overwrites_stale_predecessor, ``conflicting_incoming_audit_id_refuses,
      ``identical_incoming_audit_id_is_idempotent, ``uncertain_server_may_commit,
      ``uncertain_server_may_abort] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication example axiom in {declaration}: {found}"

end ControlPublication
