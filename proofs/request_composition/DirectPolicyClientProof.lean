import GrantedPermissionsProof
import SnapshotConstructorProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

namespace DirectPolicyClientProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Installation := GrantedPermissionsProof.Installation
abbrev Server := SnapshotAssemblyProof.Server
abbrev Subject := SnapshotAssemblyProof.Subject
abbrev Context := SnapshotAssemblyProof.Context
abbrev Permission := GrantedPermissionsProof.PermissionName

/-- Computed permission set of the existing independent role/override specification.
The installation and registration are from the supplied catalog generation; no
claim about a later publication or globally current database state is made. -/
def expectedGrants (installation : Installation) (server : Server) (subject : Subject) :=
  GrantedPermissionsProof.expectedPermissions installation
    (GrantedPermissionsProof.selectedRoles installation
      (HeldRoleProof.expectedHeld subject.principal installation.grants.val))
    server.authorization

/-- Source-reviewed direct-call composition of the actual grant computation and
actual HTTP constructor, using the same complete row and current subject. The
nondelegated premise selects the native caller branch that skips narrowing.
Context acceptance is the independently characterized existing constructor guard;
no desired permission set or successful policy result is assumed. Async catalog
capture/transport wiring and delegated narrowing are separate obligations. -/
theorem direct_policy_client_exact
    (installation : Installation)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (subject : Subject) (context : Context)
    (_direct : subject.delegation = none)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context) :
    (do
      let permissions ← policy.grants.granted_permissions
        installation server.authorization subject.principal
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
        http issuer profile server subject permissions) =
      .ok (.Ok (SnapshotAssemblyProof.expectedClient http issuer profile server subject
        (expectedGrants installation server subject) context)) ∧
    (∀ permission : Permission,
      permission ∈ expectedGrants installation server subject ↔
        GrantedPermissionsProof.Grants installation subject.principal
          server.authorization permission) := by
  constructor
  · rw [GrantedPermissionsProof.granted_permissions_exact]
    simp only [bind_ok, bind_tc_ok]
    exact SnapshotAssemblyProof.http_new_exact http issuer profile server subject
      (expectedGrants installation server subject) context accepted
  · have membership := GrantedPermissionsProof.granted_permissions_membership
      installation server.authorization subject.principal
    rw [GrantedPermissionsProof.granted_permissions_exact] at membership
    simpa only [expectedGrants, WP.spec_ok] using membership

#print axioms direct_policy_client_exact
end
end DirectPolicyClientProof

run_cmd do
  -- Actual new in the statement carries its refused-branch formatting callback.
  -- Neither formatting success/content nor issuer/helper behavior is assumed.
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound,
    ``alloc.string.ToString.Blanket.to_string,
    ``veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt]
  let found ← Lean.collectAxioms ``DirectPolicyClientProof.direct_policy_client_exact
  unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
    throwError "unreviewed direct policy/client axiom: {found}"
