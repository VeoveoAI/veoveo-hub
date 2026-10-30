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
