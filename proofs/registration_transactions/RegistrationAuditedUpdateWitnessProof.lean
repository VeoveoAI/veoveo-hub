import RegistrationAuditedUpdateWitnessExecution

/-! Independent full output assertions. Actual Step proofs remain importable
without this module or the generic audited UPDATE correctness module. -/
namespace RegistrationAuditedUpdate.Witnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationTransaction (put)

theorem update_audit : acknowledged.audit = expectedAudit := by
  /- A future omitted-Update control reaches this actual false slot assertion,
  after its independently qualified legal execution; not a broken generic import. -/
  have updateSlot : acknowledged.audit 100 = some updateRecord := by
    letI : DecidableEq (Record Draft Nat) := fun left right =>
      decidable_of_iff (left.draft = right.draft ∧ left.occurredAt = right.occurredAt) (by
        cases left
        cases right
        simp only [Record.mk.injEq])
    decide
  have visible : acknowledged.audit = put priorAudit 100 (some updateRecord) := rfl
  funext id
  by_cases updateId : id = 100
  · subst id
    exact updateSlot
  by_cases priorId : id = 900
  · subst id
    rfl
  rw [visible]
  simp [expectedAudit, put, priorAudit, updateId, priorId]

/-- Private writes/acknowledged audit are not yet committed. This populated
boundary complements end-state acceptance; it does not change commit semantics. -/
theorem private_before_commit : committing.core.core.db = seedDB ∧
    committing.audit = priorAudit ∧ committing.core.core.lock = some 0 ∧
    committing.cursors 0 = some expectedCursor ∧
    (committing.core.updates 0).phase = .committing ∧
    (committing.core.updates 0).staged = expectedWrites ∧
    expectedCursor.acknowledged = [updateRecord] := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- The full expected CURRENT row makes retained identity and credentials explicit;
only statement, metadata and updated_at change, with issuance/lifetime untouched. -/
theorem retained_fields : acknowledged.core.core.db.registrations 30 = some expectedFirst ∧
    expectedFirst.client = first.client ∧ expectedFirst.login = first.login ∧
    expectedFirst.payload.tokenDigest = first.payload.tokenDigest ∧
    expectedFirst.payload.issuedAt = first.payload.issuedAt ∧
    expectedFirst.payload.stable = first.payload.stable ∧
    expectedFirst.metadata.2 = first.metadata.2 ∧
    acknowledged.core.core.db.registrations 31 = some second ∧
    acknowledged.core.core.db.logins = seedDB.logins ∧
    acknowledged.core.core.db.frame = seedDB.frame := by
  rw [update_database]
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- Complete output is paired with actual execution, so refusal or a disconnected
expected state cannot qualify this synthetic store-success witness. -/
theorem update_complete : RegistrationTransaction.WellFormed seedDB ∧
    Execution methods metadataAccepted source encoding start acknowledged ∧ UpdateAccepted := by
  refine ⟨seed_wellFormed, update_execution, update_database, update_audit, rfl, ?_, rfl, rfl, rfl, ?_⟩
  · funext id
    by_cases actor : id = 0
    · subst id
      rfl
    have otherWriting (s : World) (tx : Tx) (lock : Option Nat) :
        (writing s tx lock).core.updates id = s.core.updates id := by
      simp only [writing, RegistrationUpdate.updateTx, put, if_neg actor]
    have appendedUpdates : appended.core.updates = ready.core.updates := rfl
    have retained : acknowledged.core.updates id = start.core.updates id := by
      calc
        acknowledged.core.updates id = committed.core.updates id := otherWriting _ _ _
        _ = committing.core.updates id := by
          simp only [committed, RegistrationUpdate.committedState,
            RegistrationUpdate.updateTx, put, if_neg actor]
        _ = appended.core.updates id := otherWriting _ _ _
        _ = ready.core.updates id := congrFun appendedUpdates id
        _ = metadataWritten.core.updates id := otherWriting _ _ _
        _ = begun.core.updates id := otherWriting _ _ _
        _ = preflight.core.updates id := otherWriting _ _ _
        _ = start.core.updates id := otherWriting _ _ _
    simp only [put, if_neg actor]
    rw [retained]
    rfl
  · funext id
    by_cases actor : id = 0
    · subst id
      rfl
    have otherAppend (s : World) (record : Record Draft Nat) :
        (appendPending s record).cursors id = s.cursors id := by
      unfold appendPending
      split
      · rfl
      · split
        · rfl
        · simp only [RegistrationAuditedUpdate.setCursor, put, if_neg actor]
    simp only [put, if_neg actor]
    calc
      acknowledged.cursors id = appended.cursors id := rfl
      _ = auditBegun.cursors id := otherAppend _ _
      _ = ready.cursors id := by
        simp only [auditBegun, RegistrationAuditedUpdate.setCursor, put, if_neg actor]
      _ = none := rfl

#print axioms update_audit
#print axioms private_before_commit
#print axioms retained_fields
#print axioms update_complete

run_cmd do
  for declaration in [``update_audit, ``private_before_commit, ``retained_fields, ``update_complete] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unexpected populated UPDATE acceptance axiom in {declaration}: {name}"

end RegistrationAuditedUpdate.Witnesses
