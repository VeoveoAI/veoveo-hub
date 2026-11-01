import RegistrationAuditedLifetimeWitnessExecution

/-! Independent complete audit/acceptance assertions, separated from the actual
execution proof so a behavioral fault can still qualify its legal wrong path. -/
namespace RegistrationAuditedLifetime.Witnesses
open RegistrationAuditedLifetime
open RegistrationTransaction (put rowMap)

theorem replacement_audit : acknowledged.audit = expectedAudit := by
  have registerSlot : acknowledged.audit 102 = some registerRecord := by
    letI : DecidableEq (Record Draft Nat) := fun left right =>
      decidable_of_iff (left.draft = right.draft ∧ left.occurredAt = right.occurredAt) (by
        cases left
        cases right
        simp only [Record.mk.injEq])
    decide
  funext id
  by_cases first : id = 100
  · subst id
    rfl
  by_cases second : id = 101
  · subst id
    rfl
  by_cases last : id = 102
  · subst id
    exact registerSlot
  simp [acknowledged, committed, committing, cursor3, appendedRegister, appendedSecond,
    appendedFirst, appendPending, begun, setCursor, cursor, advance, sourceProgram,
    source, ready, created, creating, droppedSecond, droppedFirst, captured, upserted,
    prepared, selected, waiting, cached, writing, start, initial, RegistrationLifetime.initial,
    RegistrationLifetime.update, requests, proposal, oldFirst, oldSecond, candidate,
    overlay, encoding, replaceFirst, replaceSecond, registerRecord, priorAudit, expectedAudit,
    put, first, second, last]

/-- Independent complete output plus explicit execution, preventing a disconnected
endpoint computation or unconditional refusal from qualifying this witness. -/
theorem replacement_complete : Execution source encoding start acknowledged ∧ ReplacementAccepted := by
  refine ⟨replacement_execution, replacement_database, replacement_audit,
    rfl, rfl, rfl, rfl, rfl, rfl, rfl, ?_, ?_, ?_⟩
  all_goals decide

#print axioms replacement_audit
#print axioms replacement_complete

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``replacement_audit, ``replacement_complete] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected audited replacement assertion axiom in {declaration}: {unexpected}"

end RegistrationAuditedLifetime.Witnesses
