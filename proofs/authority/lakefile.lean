import Lake
open Lake DSL

require aeneas from git
  "https://github.com/AeneasVerif/aeneas.git" @
  "fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf" / "backends/lean"

package hubAuthorityProof
lean_lib HubAuthorityProof
@[default_target]
lean_lib AuthorityProof

@[default_target]
lean_lib AuthorityComposition

@[default_target]
lean_lib AuthorityInterpretation
