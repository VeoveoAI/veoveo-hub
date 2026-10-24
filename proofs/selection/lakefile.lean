import Lake
open Lake DSL

require aeneas from git
  "https://github.com/AeneasVerif/aeneas.git" @
  "fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf" / "backends/lean"

package hubSelectionPilot

@[default_target]
lean_lib SelectionProof

@[default_target]
lean_lib LookupProof

lean_lib HubSelectionProof
