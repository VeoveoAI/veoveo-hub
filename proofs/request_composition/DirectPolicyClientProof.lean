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

/-- Retained-route/direct-policy composition for one supplied catalog generation.
The row is bound by the computed resolver, and the installation is projected from
that same generation. Constructor coherence supplies the deployment-field fact;
no desired row or grant set is assumed. An absent route has no getter fallback.
Actual constructor/selection success remains conditional on namespace-helper
completion. Snapshot acquisition, async transport wiring and delegated narrowing
remain outside this source-reviewed composition of extracted subcalls. -/
theorem constructed_direct_policy_route_exact {S I : Type}
    (ops : Aeneas.Std.core.iter.traits.collect.IntoIterator S Server I)
    (source : S) (initial : I)
    (model : IteratorProof.Enumeration I Server)
    (cursor : IteratorProof.CursorLaw ops.iteratorInst model)
    (opened : ops.into_iter source = .ok initial) (valid : model.valid initial)
    (generation : catalog.HubCatalog)
    (constructed : registration.snapshot.RegistrationSnapshot.new ops source =
      .ok generation.registrations)
    (composition : SelectedMapCollectionProof.Composition)
    (selected : SnapshotAssemblyProof.Snapshot) (slug : SelectedCollectionProof.Slug)
    (selectedSuccess : registration.snapshot.RegistrationSnapshot.selected
      generation.registrations composition = .ok selected)
    (unique : SelectedRouteContextProof.CompositionKeysUnique composition)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (subject : Subject) (context : Context)
    (direct : subject.delegation = none)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context) :
    registration.snapshot.RegistrationSnapshot.get selected slug =
      .ok ((SelectedRouteContextProof.resolveRoute generation.registrations
        composition.components slug).map Prod.snd) ∧
    match SelectedRouteContextProof.resolveRoute generation.registrations
        composition.components slug with
    | none => registration.snapshot.RegistrationSnapshot.get selected slug = .ok none
    | some (key, row) =>
      row.slug = slug ∧ row.deployment = key ∧
      (do
        let permissions ← policy.grants.granted_permissions
          generation.control_plane_value.authorization row.authorization subject.principal
        mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
          http issuer profile row subject permissions) =
        .ok (.Ok { (SnapshotAssemblyProof.expectedClient http issuer profile row subject
          (expectedGrants generation.control_plane_value.authorization row subject) context)
            with server := slug, deployment := key }) ∧
      (∀ permission : Permission,
        permission ∈ expectedGrants generation.control_plane_value.authorization row subject ↔
          GrantedPermissionsProof.Grants generation.control_plane_value.authorization
            subject.principal row.authorization permission) := by
  have coherent := SnapshotConstructorProof.new_success_coherent ops source initial
    model cursor opened valid generation.registrations constructed
  have getter := SelectedRouteContextProof.selected_get_exact generation.registrations
    composition selected slug selectedSuccess unique
  refine ⟨getter, ?_⟩
  cases route : SelectedRouteContextProof.resolveRoute generation.registrations
      composition.components slug with
  | none => simpa only [route, Option.map_none] using getter
  | some pair =>
    rcases pair with ⟨key, row⟩
    rcases SelectedRouteContextProof.resolved_route_fields generation.registrations
      composition slug key row unique coherent route with ⟨sameSlug, sameDeployment⟩
    rcases direct_policy_client_exact generation.control_plane_value.authorization
      http issuer profile row subject context direct accepted with ⟨client, membership⟩
    refine ⟨sameSlug, sameDeployment, ?_, membership⟩
    simpa [SnapshotAssemblyProof.expectedClient, ← sameSlug, ← sameDeployment] using client

#print axioms direct_policy_client_exact
#print axioms constructed_direct_policy_route_exact
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
  let routeAllowed := allowed.push
    ``registration.snapshot.RegistrationSnapshot.build_namespaces
  let routeFound ← Lean.collectAxioms ``DirectPolicyClientProof.constructed_direct_policy_route_exact
  unless (routeFound.filter (fun axiomName => !routeAllowed.contains axiomName)).isEmpty do
    throwError "unreviewed constructed direct policy/route axiom: {routeFound}"
