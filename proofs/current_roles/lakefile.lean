import Lake
open Lake DSL
require aeneas from git
  "https://github.com/AeneasVerif/aeneas.git" @
  "fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf" / "backends/lean"
package hubCurrentRolesProof
@[default_target]
lean_lib HubCurrentRoles

@[default_target]
lean_lib NativeSetProof

@[default_target]
lean_lib IteratorProof

@[default_target]
lean_lib CollectionProof

@[default_target]
lean_lib PolicyPrimitives

@[default_target]
lean_lib HeldRoleProof

@[default_target]
lean_lib EffectiveRiskProof

@[default_target]
lean_lib GrantedPermissionsProof
