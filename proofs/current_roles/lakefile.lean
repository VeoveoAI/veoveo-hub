import Lake
open Lake DSL
require aeneas from git
  "https://github.com/AeneasVerif/aeneas.git" @
  "fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf" / "backends/lean"
require hubAuthorityProof from "../authority"
require sdkAuthorizationProof from git
  "https://github.com/VeoveoAI/veoveo-hub-sdk" @
  "239622d12c08b771cc5e813fe3e846385d8d04ad" / "proofs/authorization"

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

@[default_target]
lean_lib CurrentPermissionsModelAudit

@[default_target]
lean_lib RegistrationIteratorProof

@[default_target]
lean_lib FlatMapIteratorProof

@[default_target]
lean_lib CurrentPermissionsProof

@[default_target]
lean_lib NativePolicyModelAudit

@[default_target]
lean_lib RegisteredServerPolicyProof

@[default_target]
lean_lib RegisteredServerDecisionProof

@[default_target]
lean_lib RegisteredServerAdmissionProof

@[default_target]
lean_lib RegisteredServerDownstreamProof

@[default_target]
lean_lib SDKResolverAdapterProof

@[default_target]
lean_lib SDKAccessProof

@[default_target]
lean_lib SDKAccessCorollaries

@[default_target]
lean_lib SDKRegisteredPolicyProof

@[default_target]
lean_lib SessionModelAudit

@[default_target]
lean_lib SessionFamilyProof

@[default_target]
lean_lib RefreshModelAudit

@[default_target]
lean_lib RefreshPredicateProof
