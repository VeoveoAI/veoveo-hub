import RegistrationLifetimeWitnessProof
import RegistrationLifetimeHistoryProof

/-! Generic history theorems are imported ONLY after independently constructing
concrete Step chains and expected complete outputs in the models-only witness
leaf. They are not used to manufacture executable reachability. -/
namespace RegistrationLifetime.Witnesses
open RegistrationLifetime

/-- The exact original execution has no explanation in the SAME independent
mixed SerialHistory contract (including its frame-only environment constructor). -/
theorem original_has_no_serial_history :
    Execution false start originalFinal ∧
    ¬SerialHistory requests seedDB originalFinal.commits originalFinal.db := by
  refine ⟨original_orphan_execution, ?_⟩
  intro history
  have safe := serial_no_orphan history seed_valid.2
  have absent : originalFinal.db.instanceRow = none := rfl
  have missing := safe absent 10
  change some candidate = none at missing
  contradiction

/-- The accepted fixed path actually satisfies the same full-DB/order contract;
its reachability still comes from the independent concrete Step-chain proof. -/
theorem fixed_has_serial_history :
    Execution true start fixedFinal ∧
    SerialHistory requests seedDB fixedFinal.commits fixedFinal.db :=
  ⟨fixed_accepted_then_removed_execution,
    execution_serial seedDB requests fixedFinal fixed_accepted_then_removed_execution⟩

#print axioms original_has_no_serial_history
#print axioms fixed_has_serial_history

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``original_has_no_serial_history, ``fixed_has_serial_history] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected registration lifetime consequence axiom in {declaration}: {unexpected}"

end RegistrationLifetime.Witnesses
