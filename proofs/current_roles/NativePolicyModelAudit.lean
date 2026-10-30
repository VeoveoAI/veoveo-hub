import HubCurrentRoles
import CurrentPermissionsProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

-- Preliminary model/type compilation audit ONLY. These first two declarations
-- are intentional native-operation assumptions, not ordinary proved models.
-- No standard-three-only, native-clock or compiled-SDK route qualification claim.
#print axioms chrono.offset.utc.Utc.now
#print axioms veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access
#print axioms policy.evaluation.registered_target
#print axioms policy.evaluation.declared_route_access
#print axioms policy.evaluation.decision
#print axioms policy.evaluation.deny
#print axioms policy.evaluation.decide_server
#print axioms contract.actions.PolicyAction.name
#print axioms control_plane.policy.PolicyTarget.Insts.CoreCloneClone.clone

-- Enforced allowlists: old claims stay standard-only; the preliminary policy
-- model may additionally depend on exactly the TWO reviewed native symbols.
private def checkAxiomClosure (declarations allowed : Array Lean.Name) :
    Lean.Elab.Command.CommandElabM Unit := do
  for declaration in declarations do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun axiomName => !allowed.contains axiomName)
    unless unexpected.isEmpty do
      throwError "unreviewed axioms in {declaration}: {unexpected}"

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[``chrono.offset.utc.Utc.now,
    ``veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access]
  checkAxiomClosure #[``policy.evaluation.registered_target,
    ``contract.actions.PolicyAction.name,
    ``control_plane.policy.PolicyTarget.Insts.CoreCloneClone.clone] standard
  checkAxiomClosure #[``policy.evaluation.declared_route_access]
    (standard ++ #[``veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access])
  checkAxiomClosure #[``policy.evaluation.decision, ``policy.evaluation.deny]
    (standard ++ #[``chrono.offset.utc.Utc.now])
  checkAxiomClosure #[``policy.evaluation.decide_server] (standard ++ native)
  checkAxiomClosure #[``policy.grants.held_roles, ``policy.grants.effective_risks,
    ``policy.grants.granted_permissions,
    ``control_plane.authorization_config.InstallationRole.allows,
    ``delegation.CurrentPermissions.Insts.Veoveo_hubDelegationAuthorityHeldBTreeSetPermissionNameGlobal.held,
    ``CurrentPermissionsProof.held_exact, ``CurrentPermissionsProof.lawful_held,
    ``CurrentPermissionsProof.issued_membership,
    ``CurrentRolesNative.filterNext, ``CurrentRolesNative.any,
    ``CurrentRolesNative.collectSet] standard
