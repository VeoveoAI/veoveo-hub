import RegistrationLifetimeWitnesses

/-! Deliberate handwritten primitive-effect fault, not Rust extraction or native
bug evidence. The staged DROP entry remains; only its modeled effect is ignored.
No generic correctness module is imported. -/
namespace RegistrationLifetime.Witnesses
open RegistrationLifetime
open RegistrationTransaction (put)

def dropFaultDatabase : DB :=
  { instanceRow := none
    deployment := some ⟨metadata, "first-deployment-stable"⟩
    registrations := fun _ => none
    logins := fun login => if login = 20 then some "candidate-role"
      else if login = 99 then some "unrelated-role" else none
    frame := ("unrelated-application", 73) }

/-- Actual full fixed-path output under the no-op DROP: instance and row disappear,
but the exact admitted login survives. Source requests/events/response tags and
unrelated role/frame remain intact. This is false on the baseline model. -/
def DropLoginWrongOutput : Prop :=
  fixedFinal.db = dropFaultDatabase ∧
  (fixedFinal.transactions 0).request = .register proposal ∧
  (fixedFinal.transactions 1).request = .remove ∧
  (fixedFinal.transactions 0).response = .registered ∧
  (fixedFinal.transactions 1).response = .removed ∧
  (fixedFinal.transactions 0).error = none ∧ (fixedFinal.transactions 1).error = none ∧
  fixedFinal.instanceOwner = none ∧ fixedFinal.deploymentOwner = none ∧
  fixedFinal.commits =
    [(0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows []),
        .registration (.createLogin 20 "candidate-role"), .registration (.insert candidate)]),
      (1, [.registration (.deleteRows [10]), .registration (.dropLogin 20), .deleteInstance])] ∧
  fixedFinal.db.logins 20 = some "candidate-role"

theorem drop_login_wrong_complete_output : DropLoginWrongOutput := by
  refine ⟨?_, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩
  apply Database.ext <;> try rfl
  funext key
  by_cases same : key = 10 <;>
    simp [fixedFinal, fixedRegistration, finishOneRemoval, oneRemovalReady, erasedRemove, erasingRemove,
      droppedOne, capturedRows, selectedRemove, waitingRemove, finishRegister, insertedRegister,
      beforeInsert, createdRegister, creatingRegister, upsertedRegister, preparedRegister,
      selectedRegister, waitingRegister, cached, acknowledged, committedState, committing,
      applyWrites, applyWrite, start, initial, requests, seedDB, candidate, proposal, update, put,
      dropFaultDatabase, same]

#print axioms drop_login_wrong_complete_output

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``DropLoginWrongOutput, ``drop_login_wrong_complete_output] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected dropped-login diagnostic axiom in {declaration}: {unexpected}"

end RegistrationLifetime.Witnesses
