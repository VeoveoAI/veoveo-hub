import Lake
open Lake DSL

require aeneas from git
  "https://github.com/AeneasVerif/aeneas.git" @
  "fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf" / "backends/lean"

package controlPublicationProof

@[default_target]
lean_lib PublicationModel

@[default_target]
lean_lib PublicationProof

@[default_target]
lean_lib PublicationExamples

@[default_target]
lean_lib PublicationHistory

@[default_target]
lean_lib PublicationExecution

@[default_target]
lean_lib PublicationReplay

@[default_target]
lean_lib PublicationUncertainty

@[default_target]
lean_lib PublicationWitnesses
