import Lake
open Lake DSL

require aeneas from git
  "https://github.com/AeneasVerif/aeneas.git" @
  "fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf" / "backends/lean"

package registrationTransactionsProof

@[default_target]
lean_lib RegistrationModel

@[default_target]
lean_lib RegistrationSpecification

@[default_target]
lean_lib RegistrationProof

@[default_target]
lean_lib RegistrationLocks

@[default_target]
lean_lib RegistrationEffects

@[default_target]
lean_lib RegistrationObservations

@[default_target]
lean_lib RegistrationHistory

@[default_target]
lean_lib RegistrationWitnesses

@[default_target]
lean_lib RegistrationUpdateModel

@[default_target]
lean_lib RegistrationUpdateSpecification

@[default_target]
lean_lib RegistrationUpdateEffects

@[default_target]
lean_lib RegistrationUpdateProof

@[default_target]
lean_lib RegistrationUpdateLocks

@[default_target]
lean_lib RegistrationUpdateObservations

@[default_target]
lean_lib RegistrationUpdateHistory

@[default_target]
lean_lib RegistrationUpdateWitnesses

-- Additive manual instance-lifetime targets; existing default targets are unchanged.
lean_lib RegistrationLifetimeModel
lean_lib RegistrationLifetimeSpecification
lean_lib RegistrationLifetimeProof

lean_lib RegistrationLifetimeHistoryProof

lean_lib RegistrationLifetimeWitnesses
lean_lib RegistrationLifetimeWitnessProof
lean_lib RegistrationLifetimeWitnessConsequences

-- Additive manual audited STORE layer; existing/default targets are unchanged.
lean_lib RegistrationAuditedLifetimeModel
lean_lib RegistrationAuditedLifetimeSpecification
lean_lib RegistrationAuditedLifetimeProof

lean_lib RegistrationAuditedLifetimeWitnesses
lean_lib RegistrationAuditedLifetimeWitnessProof

lean_lib RegistrationAuditedLifetimeWitnessExecution

lean_lib RegistrationAuditedLifetimeRecoveryWitnesses
lean_lib RegistrationAuditedLifetimeRecoveryProof

lean_lib RegistrationPostcommitContinuation
lean_lib RegistrationPostcommitContinuationProof

-- Additive manual UPDATE audit adapter; existing/default targets are unchanged.
lean_lib RegistrationAuditedUpdateModel
lean_lib RegistrationAuditedUpdateSpecification
lean_lib RegistrationAuditedUpdateProof
