import RegistrationAuditedUpdateReplacementExecution

/-! Independent full DB/ledger/map acceptance after actual replacement and the
stale no-op. No generic audit vocabulary theorem is assumed for external commits. -/
namespace RegistrationAuditedUpdate.ReplacementWitnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationAuditedUpdate.Witnesses
open RegistrationTransaction (put)

theorem database_complete : acknowledged.core.core.db = expectedDB := by
  change RegistrationTransaction.applyWrites seedDB
    [.deployment candidate.metadata, .deleteRows [30, 31], .dropLogin 40, .dropLogin 41,
      .createLogin 60 "replacement-registration-role", .insert candidate] = expectedDB
  apply RegistrationTransaction.Database.ext
  · rfl
  · funext key
    by_cases newKey : key = 50
    · subst key
      rfl
    by_cases firstKey : key = 30
    · subst key
      rfl
    by_cases secondKey : key = 31
    · subst key
      rfl
    simp [RegistrationTransaction.applyWrites, RegistrationTransaction.applyWrite,
      seedDB, expectedDB, put, candidate, newKey, firstKey, secondKey]
  · funext login
    by_cases newLogin : login = 60
    · subst login
      rfl
    by_cases firstLogin : login = 40
    · subst login
      rfl
    by_cases secondLogin : login = 41
    · subst login
      rfl
    simp [RegistrationTransaction.applyWrites, RegistrationTransaction.applyWrite,
      seedDB, expectedDB, put, candidate, newLogin, firstLogin, secondLogin]
  · rfl

theorem audit_complete : acknowledged.audit = expectedAudit := by
  rfl

/-- The full prior record survives; every visible record is Replace/Register.
The no-op contributes neither an Update record nor an Update cursor. -/
theorem no_update_audit : acknowledged.audit 900 = some priorRecord ∧
    acknowledged.cursors = (fun _ => none) ∧
    ∀ id record, acknowledged.audit id = some record →
      (recordIntent encoding record).activity ≠ .update := by
  refine ⟨rfl, rfl, ?_⟩
  intro id record stored
  rw [audit_complete] at stored
  simp only [expectedAudit] at stored
  split_ifs at stored <;> cases stored <;> decide

/-- Explicit pending private write then completed rollback. Both response tags
are ordinary store success,not a full native ClientInformation response. -/
theorem noop_boundaries :
    (metadataWritten.core.updates 0).staged = [.deploymentMetadata nextMetadata] ∧
    metadataWritten.core.core.lock = some 0 ∧ metadataWritten.core.core.db = expectedDB ∧
    missing.core.core.db.registrations request.client = none ∧
    (missing.core.updates 0).phase = .rollbackPending ∧ (missing.core.updates 0).outcome = .noop ∧
    rolledBack.core.core.lock = none ∧ (acknowledged.core.updates 0).response = .success ∧
    (acknowledged.core.core.transactions 1).response = .success := by
  refine ⟨rfl, rfl, ?_, rfl, rfl, rfl, rfl, rfl, rfl⟩
  exact database_complete

theorem update_other (s : World) (tx : Tx) (lock : Option Nat) (id : Nat) (other : id ≠ 0) :
    (updateAt s tx lock).core.updates id = s.core.updates id := by
  simp only [updateAt, RegistrationUpdate.updateTx, put, if_neg other]

theorem register_other (s : World) (tx : RegTx) (lock : Option Nat) (id : Nat) (other : id ≠ 1) :
    (registerAt s tx lock).core.core.transactions id = s.core.core.transactions id := by
  simp only [registerAt, RegistrationTransaction.update, put, if_neg other]

theorem complete_outputs : ReplacementNoopAccepted := by
  refine ⟨database_complete, audit_complete, rfl, ?_, ?_, rfl, rfl, rfl⟩
  · funext id
    by_cases actor : id = 0
    · subst id
      rfl
    simp only [put, if_neg actor]
    calc
      acknowledged.core.updates id = rolledBack.core.updates id := update_other _ _ _ _ actor
      _ = missing.core.updates id := update_other _ _ _ _ actor
      _ = metadataWritten.core.updates id := update_other _ _ _ _ actor
      _ = updateBegun.core.updates id := update_other _ _ _ _ actor
      _ = registerAcknowledged.core.updates id := update_other _ _ _ _ actor
      _ = stale.core.updates id := rfl
      _ = start.core.updates id := update_other _ _ _ _ actor
  · funext id
    by_cases actor : id = 1
    · subst id
      rfl
    simp only [put, if_neg actor]
    calc
      acknowledged.core.core.transactions id = registerAcknowledged.core.core.transactions id := rfl
      _ = registered.core.core.transactions id := register_other _ _ _ _ actor
      _ = committing.core.core.transactions id := by
        simp only [registered, RegistrationTransaction.committedState,
          RegistrationTransaction.update, put, if_neg actor]
      _ = ready.core.core.transactions id := register_other _ _ _ _ actor
      _ = created.core.core.transactions id := register_other _ _ _ _ actor
      _ = creating.core.core.transactions id := register_other _ _ _ _ actor
      _ = droppedSecond.core.core.transactions id := register_other _ _ _ _ actor
      _ = droppedFirst.core.core.transactions id := register_other _ _ _ _ actor
      _ = captured.core.core.transactions id := register_other _ _ _ _ actor
      _ = upserted.core.core.transactions id := register_other _ _ _ _ actor
      _ = replacing.core.core.transactions id := register_other _ _ _ _ actor
      _ = observed.core.core.transactions id := register_other _ _ _ _ actor
      _ = selected.core.core.transactions id := register_other _ _ _ _ actor
      _ = waiting.core.core.transactions id := register_other _ _ _ _ actor
      _ = stale.core.core.transactions id := register_other _ _ _ _ actor
      _ = start.core.core.transactions id := rfl

/-- Complete20-step composition,input validity,concrete full external contribution
and independent complete output. The vocabulary proof is for this explicit batch
only;generic registration/namespace audit adaptation remains OPEN. -/
theorem replacement_noop_complete : RegistrationTransaction.WellFormed seedDB ∧
    Execution methods metadataAccepted source encoding start acknowledged ∧
    AppendBatch encoding committing.audit externalRecords externalAudit ∧
    ReplacementNoopAccepted := by
  exact ⟨input_valid.1, composed_execution, external_batch_valid, complete_outputs⟩

#print axioms database_complete
#print axioms audit_complete
#print axioms no_update_audit
#print axioms noop_boundaries
#print axioms replacement_noop_complete

run_cmd do
  for declaration in [``database_complete, ``audit_complete, ``no_update_audit,
      ``noop_boundaries, ``update_other, ``register_other, ``complete_outputs,
      ``replacement_noop_complete] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unexpected replacement/noop assertion axiom in {declaration}: {name}"

end RegistrationAuditedUpdate.ReplacementWitnesses
