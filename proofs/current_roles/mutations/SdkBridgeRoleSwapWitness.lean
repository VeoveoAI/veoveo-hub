import SdkInternalAuthBridgeFields
import Lean.Util.CollectAxioms

open SdkInternalAuthBridge

namespace SdkBridgeRoleSwapWitness

/-- Intentional proof-adapter fault only: even this swapped adapter is an inverse. -/
theorem inverse_still_holds :
    groupRoleEquiv.symm (groupRoleEquiv veoveo_hub.veoveo_hub_sdk.identity.GroupRole.Read) =
      veoveo_hub.veoveo_hub_sdk.identity.GroupRole.Read :=
  groupRoleEquiv.symm_apply_apply _

theorem read_becomes_write :
    groupRoleEquiv veoveo_hub.veoveo_hub_sdk.identity.GroupRole.Read =
      RequestCompositionSdk.identity.GroupRole.Write := rfl

/-- Independent same-constructor semantics rejects what roundtrip alone admits. -/
theorem independent_field_rejects :
    ¬ GroupRoleFields veoveo_hub.veoveo_hub_sdk.identity.GroupRole.Read
      (groupRoleEquiv veoveo_hub.veoveo_hub_sdk.identity.GroupRole.Read) := by
  simp [GroupRoleFields, groupRoleEquiv]

#print axioms inverse_still_holds
#print axioms read_becomes_write
#print axioms independent_field_rejects
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SdkBridgeRoleSwapWitness.inverse_still_holds,
      ``SdkBridgeRoleSwapWitness.read_becomes_write, ``SdkBridgeRoleSwapWitness.independent_field_rejects] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed adapter-swap witness dependency in {declaration}: {found}"

end SdkBridgeRoleSwapWitness
