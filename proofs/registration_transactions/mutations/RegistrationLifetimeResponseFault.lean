import RegistrationLifetimeWitnesses

/-! Deliberate wrong SUCCESS RESPONSE TAG after actual registration commit. This
is neither a pre-admission rejection nor native endpoint behavior. No generic
correctness module is imported. -/
namespace RegistrationLifetime.Witnesses
open RegistrationLifetime
open RegistrationTransaction (put)

/-- The current valid replacement is fully admitted, including its old immutable
deployment data, old-role cleanup, new row/role and unrelated values, but receives
invalidToken rather than registered. False on the baseline model. -/
def WrongRegistrationResponse : Prop :=
  replacementFinal.db = expectedReplacement ∧
  (replacementFinal.transactions 0).request = .register proposal ∧
  (replacementFinal.transactions 0).phase = .committed ∧
  (replacementFinal.transactions 0).response = .invalidToken ∧
  (replacementFinal.transactions 0).error = none ∧
  replacementFinal.instanceOwner = none ∧ replacementFinal.deploymentOwner = none ∧
  replacementFinal.commits =
    [(0, [.upsertDeployment metadata "first-deployment-stable", .registration (.deleteRows [30]),
      .registration (.dropLogin 40), .registration (.createLogin 20 "candidate-role"),
      .registration (.insert candidate)])] ∧
  replacementFinal.db.registrations 10 = some candidate ∧
  replacementFinal.db.logins 20 = some "candidate-role" ∧ replacementFinal.db.logins 40 = none

theorem successful_registration_wrong_complete_response : WrongRegistrationResponse := by
  refine ⟨?_, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩
  apply Database.ext <;> try rfl
  · funext key
    by_cases newKey : key = 10 <;> by_cases oldKey : key = 30 <;>
      simp [replacementFinal, finishRegister, insertedRegister, beforeReplacement, createdRegister,
        creatingRegister, droppedOne, capturedRows, upsertedRegister, preparedRegister,
        selectedRegister, waitingRegister, cached, acknowledged, committing, committedState,
        replacementStart, initial, requests, replacementSeed, priorRow, candidate, proposal,
        applyWrites, applyWrite, update, put, expectedReplacement, newKey, oldKey]
  · funext login
    by_cases newLogin : login = 20 <;> by_cases oldLogin : login = 40 <;> by_cases unrelatedLogin : login = 99 <;>
      simp [replacementFinal, finishRegister, insertedRegister, beforeReplacement, createdRegister,
        creatingRegister, droppedOne, capturedRows, upsertedRegister, preparedRegister,
        selectedRegister, waitingRegister, cached, acknowledged, committing, committedState,
        replacementStart, initial, requests, replacementSeed, priorRow, candidate, proposal,
        applyWrites, applyWrite, update, put, expectedReplacement, unrelatedRoles,
        newLogin, oldLogin, unrelatedLogin]

#print axioms successful_registration_wrong_complete_response

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``WrongRegistrationResponse, ``successful_registration_wrong_complete_response] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected registration-response diagnostic axiom in {declaration}: {unexpected}"

end RegistrationLifetime.Witnesses
