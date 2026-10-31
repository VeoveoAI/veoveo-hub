import DirectPolicyClientProof
import RegistrationIteratorProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub IteratorProof NativeSetProof PolicyPrimitives

namespace DelegatedPolicyClientProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Permission := DirectPolicyClientProof.Permission
abbrev Grants := Finset Permission
abbrev Installation := DirectPolicyClientProof.Installation
abbrev Server := DirectPolicyClientProof.Server
abbrev Subject := DirectPolicyClientProof.Subject
abbrev Context := DirectPolicyClientProof.Context
abbrev Delegation := delegation.SubjectDelegation
abbrev nativeNarrow :=
  alloc.collections.btree.set.BTreeSetPermissionNameGlobal.Insts.Veoveo_hubDelegationAuthorityPermissionsPermissionName.narrow
abbrev narrowOps :=
  delegation.PermissionsBTreeSetPermissionNameGlobalPermissionName.narrow.closure.Insts.CoreOpsFunctionFnMutTupleShared1PermissionNameBool
abbrev ownedPermissions := RegistrationIteratorProof.ownedPermissions
abbrev narrowIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator
  ownedPermissions.iteratorInst narrowOps

private theorem narrow_predicate_exact (carried : Grants) (permission : Permission) :
    narrowOps.call_mut carried permission = .ok (decide (permission ∈ carried), carried) := by
  have orderLaw : LawfulComparison
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp (taggedComparison 11) :=
    fun left right => (permission_operations_exact left right).1
  have member := set_contains_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
    (core.borrow.Borrow.Blanket Permission) id (by intro value; rfl)
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    (taggedComparison 11) orderLaw carried permission
  simp [narrowOps,
    delegation.PermissionsBTreeSetPermissionNameGlobalPermissionName.narrow.closure.Insts.CoreOpsFunctionFnMutTupleShared1PermissionNameBool.call_mut,
    member, bind_ok, bind_tc_ok]

private theorem narrow_predicate_law (carried : Grants) :
    PredicateLaw narrowOps (fun state => state = carried)
      (fun permission => decide (permission ∈ carried)) := by
  intro state same permission
  subst state
  rw [narrow_predicate_exact]
  simp

private def narrowModel (carried : Grants) :=
  filterEnumeration (listEnumeration Permission) (fun state : Grants => state = carried)
    (fun permission => decide (permission ∈ carried))

private def narrowState (granted carried : Grants) :
    core.iter.adapters.filter.Filter (List Permission) Grants :=
  { iter := granted.toList, predicate := carried }

/-- Total exact semantics of the actual native-set adapter, using its real owned
cursor, source predicate and collector. No replacement recursion or native-set
membership oracle is introduced. Native representation/refinement is unchanged. -/
theorem native_narrow_exact (granted carried : Grants) :
    nativeNarrow granted carried = .ok (granted ∩ carried) := by
  have cursor := filter_cursor_law ownedPermissions.iteratorInst (listEnumeration Permission)
    RegistrationIteratorProof.owned_permissions_cursor_law narrowOps
    (fun state => state = carried) (fun permission => decide (permission ∈ carried))
    (narrow_predicate_law carried)
  have orderLaw : LawfulComparison
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp (taggedComparison 11) :=
    fun left right => (permission_operations_exact left right).1
  have collected := set_collect_exact narrowIterator (narrowModel carried) cursor
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    (taggedComparison 11) orderLaw (narrowState granted carried) ⟨trivial, rfl⟩
  unfold nativeNarrow
    alloc.collections.btree.set.BTreeSetPermissionNameGlobal.Insts.Veoveo_hubDelegationAuthorityPermissionsPermissionName.narrow
  simp only [alloc.collections.btree.set.BTreeSet.Insts.CoreIterTraitsCollectIntoIteratorTIntoIter.into_iter,
    core.iter.traits.iterator.Iterator.filter.default, bind_ok, bind_tc_ok]
  change Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default narrowIterator
    (alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd)
    (narrowState granted carried) = .ok (granted ∩ carried)
  rw [collected]
  apply congrArg Result.ok
  ext permission
  simp [narrowModel, narrowState, filterEnumeration, listEnumeration]

/-- The actual SubjectDelegation wrapper uses only its carried permission set. -/
theorem subject_narrow_exact (info : Delegation) (granted : Grants) :
    delegation.SubjectDelegation.narrow info granted = .ok (granted ∩ info.permissions) := by
  unfold delegation.SubjectDelegation.narrow
  exact native_narrow_exact granted info.permissions

/-- Empty carried grants remain a successful empty result, not a new refusal. -/
theorem native_narrow_empty (granted : Grants) :
    nativeNarrow granted ∅ = .ok ∅ := by
  rw [native_narrow_exact]
  simp

/-- Independent mathematical meaning of the optional branch in connect_upstream. -/
def narrowed (subject : Subject) (granted : Grants) : Grants :=
  match subject.delegation with
  | none => granted
  | some info => granted ∩ info.permissions

/-- This is the literal source-reviewed optional branch, calling the extracted
wrapper in its Some arm. It adds no runtime reducer or authority rule. -/
theorem optional_narrow_exact (subject : Subject) (granted : Grants) :
    (match subject.delegation with
      | none => Result.ok granted
      | some info => delegation.SubjectDelegation.narrow info granted) =
      .ok (narrowed subject granted) := by
  cases delegated : subject.delegation with
  | none => simp only [narrowed, delegated]
  | some info => simpa only [narrowed, delegated] using subject_narrow_exact info granted

def expectedPermissions (installation : Installation) (server : Server) (subject : Subject) : Grants :=
  narrowed subject (DirectPolicyClientProof.expectedGrants installation server subject)

/-- Current policy always evaluates subject.principal. The original signed
context retained by delegation is not substituted for that current principal. -/
def Eligible (installation : Installation) (server : Server) (subject : Subject)
    (permission : Permission) : Prop :=
  GrantedPermissionsProof.Grants installation subject.principal server.authorization permission ∧
    match subject.delegation with
    | none => True
    | some info => permission ∈ info.permissions

theorem expected_permissions_membership (installation : Installation) (server : Server)
    (subject : Subject) :
    ∀ permission, permission ∈ expectedPermissions installation server subject ↔
      Eligible installation server subject permission := by
  have grants := GrantedPermissionsProof.granted_permissions_membership
    installation server.authorization subject.principal
  rw [GrantedPermissionsProof.granted_permissions_exact] at grants
  simp only [WP.spec_ok] at grants
  change ∀ permission,
    permission ∈ DirectPolicyClientProof.expectedGrants installation server subject ↔
      GrantedPermissionsProof.Grants installation subject.principal server.authorization permission at grants
  intro permission
  cases delegated : subject.delegation <;>
    simp [expectedPermissions, narrowed, Eligible, delegated, grants permission]

/-- Actual current grant computation, optional extracted narrowing, then actual
HTTP construction. Empty intersections are accepted whenever the existing context
check accepts; the complete expected value retains direct/delegated fields alike.
This is a source-reviewed subcall composition, not an async caller extraction. -/
theorem policy_client_exact
    (installation : Installation)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (subject : Subject) (context : Context)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context) :
    (do
      let granted ← policy.grants.granted_permissions
        installation server.authorization subject.principal
      let permissions ← match subject.delegation with
        | none => .ok granted
        | some info => delegation.SubjectDelegation.narrow info granted
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
        http issuer profile server subject permissions) =
      .ok (.Ok (SnapshotAssemblyProof.expectedClient http issuer profile server subject
        (expectedPermissions installation server subject) context)) ∧
    (∀ permission, permission ∈ expectedPermissions installation server subject ↔
      Eligible installation server subject permission) := by
  constructor
  · rw [GrantedPermissionsProof.granted_permissions_exact]
    simp only [bind_ok, bind_tc_ok]
    cases delegated : subject.delegation with
    | none =>
      simpa only [expectedPermissions, narrowed, delegated,
        DirectPolicyClientProof.expectedGrants] using
        SnapshotAssemblyProof.http_new_exact http issuer profile server subject
          (DirectPolicyClientProof.expectedGrants installation server subject) context accepted
    | some info =>
      -- Reduce the known Some match before rewriting its native call. The
      -- earlier bind equation has distributed the continuation into each arm.
      dsimp only
      rw [subject_narrow_exact]
      simpa only [bind_ok, bind_tc_ok, expectedPermissions, narrowed, delegated,
        DirectPolicyClientProof.expectedGrants] using
        SnapshotAssemblyProof.http_new_exact http issuer profile server subject
          (DirectPolicyClientProof.expectedGrants installation server subject ∩ info.permissions)
          context accepted
  · exact expected_permissions_membership installation server subject

/-- One computed retained route and one supplied catalog generation determine the
complete accepted client. Namespace-helper completion, native map/set refinement,
current snapshot acquisition, async transport wiring and SDK issuance remain
explicit boundaries. No desired row or grant-result premise is supplied. -/
theorem constructed_policy_route_exact {S I : Type}
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
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context) :
    registration.snapshot.RegistrationSnapshot.get selected slug =
      .ok ((SelectedRouteContextProof.resolveRoute generation.registrations
        composition.components slug).map Prod.snd) ∧
    auth.verified.AuthenticatedSubject.request_context subject =
      .ok (SnapshotAssemblyProof.expectedContext subject) ∧
    match SelectedRouteContextProof.resolveRoute generation.registrations
        composition.components slug with
    | none => registration.snapshot.RegistrationSnapshot.get selected slug = .ok none
    | some (key, row) =>
      row.slug = slug ∧ row.deployment = key ∧
      (do
        let granted ← policy.grants.granted_permissions
          generation.control_plane_value.authorization row.authorization subject.principal
        let permissions ← match subject.delegation with
          | none => .ok granted
          | some info => delegation.SubjectDelegation.narrow info granted
        mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
          http issuer profile row subject permissions) =
        .ok (.Ok { (SnapshotAssemblyProof.expectedClient http issuer profile row subject
          (expectedPermissions generation.control_plane_value.authorization row subject) context)
            with server := slug, deployment := key }) ∧
      (∀ permission,
        permission ∈ expectedPermissions generation.control_plane_value.authorization row subject ↔
          Eligible generation.control_plane_value.authorization row subject permission) := by
  have coherent := SnapshotConstructorProof.new_success_coherent ops source initial
    model cursor opened valid generation.registrations constructed
  have getter := SelectedRouteContextProof.selected_get_exact generation.registrations
    composition selected slug selectedSuccess unique
  refine ⟨getter, SnapshotAssemblyProof.request_context_exact subject, ?_⟩
  cases route : SelectedRouteContextProof.resolveRoute generation.registrations
      composition.components slug with
  | none => simpa only [route, Option.map_none] using getter
  | some pair =>
    rcases pair with ⟨key, row⟩
    rcases SelectedRouteContextProof.resolved_route_fields generation.registrations
      composition slug key row unique coherent route with ⟨sameSlug, sameDeployment⟩
    rcases policy_client_exact generation.control_plane_value.authorization
      http issuer profile row subject context accepted with ⟨client, membership⟩
    refine ⟨sameSlug, sameDeployment, ?_, membership⟩
    simpa [SnapshotAssemblyProof.expectedClient, ← sameSlug, ← sameDeployment] using client

#print axioms native_narrow_exact
#print axioms subject_narrow_exact
#print axioms native_narrow_empty
#print axioms optional_narrow_exact
#print axioms expected_permissions_membership
#print axioms policy_client_exact
#print axioms constructed_policy_route_exact
end
end DelegatedPolicyClientProof

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``DelegatedPolicyClientProof.native_narrow_exact,
      ``DelegatedPolicyClientProof.subject_narrow_exact,
      ``DelegatedPolicyClientProof.native_narrow_empty,
      ``DelegatedPolicyClientProof.optional_narrow_exact,
      ``DelegatedPolicyClientProof.expected_permissions_membership] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed native/optional narrowing axiom in {declaration}: {found}"
  let formatting := standard ++ #[``alloc.string.ToString.Blanket.to_string,
    ``veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt]
  let clientFound ← Lean.collectAxioms ``DelegatedPolicyClientProof.policy_client_exact
  unless (clientFound.filter (fun axiomName => !formatting.contains axiomName)).isEmpty do
    throwError "unreviewed narrowed policy/client axiom: {clientFound}"
  let routeAllowed := formatting.push ``registration.snapshot.RegistrationSnapshot.build_namespaces
  let routeFound ← Lean.collectAxioms ``DelegatedPolicyClientProof.constructed_policy_route_exact
  unless (routeFound.filter (fun axiomName => !routeAllowed.contains axiomName)).isEmpty do
    throwError "unreviewed constructed narrowed policy/route axiom: {routeFound}"
