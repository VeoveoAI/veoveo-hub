import GrantedPermissionsProof

open Aeneas Aeneas.Std veoveo_hub IteratorProof NativeSetProof

namespace RegistrationIteratorProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Server := registration.snapshot.RegisteredServer
abbrev Snapshot := registration.snapshot.RegistrationSnapshot
abbrev PermissionName := veoveo_types.permission_names.PermissionName
abbrev Installation := control_plane.authorization_config.InstallationAuthorization
abbrev Caller := veoveo_hub_sdk.internal_auth.exchange.DelegatedCaller

abbrev projectionOps := registration.snapshot.RegistrationSnapshot.iter.closure.Insts.CoreOpsFunctionFnMutTupleShared0ArcRegisteredServerSharedRegisteredServer
abbrev valuesIterator := alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV
  veoveo_types.execution.DeploymentId (alloc.sync.Arc Server)
abbrev registrationIterator := core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator
  valuesIterator projectionOps
abbrev grantOps := delegation.HeldCurrentPermissionsBTreeSetPermissionNameGlobal.held.closure.Insts.CoreOpsFunctionFnMutTupleShared3RegisteredServerBTreeSetPermissionNameGlobal
abbrev requestedOps := delegation.HeldCurrentPermissionsBTreeSetPermissionNameGlobal.held.closure_1.Insts.CoreOpsFunctionFnMutTupleShared2PermissionNameBool
abbrev ownedPermissions := alloc.collections.btree.set.BTreeSet.Insts.CoreIterTraitsCollectIntoIteratorTIntoIter
  PermissionName alloc.alloc.Global.Insts.CoreAllocAllocatorClone

/- Raw values, not selected providers or a precomputed permission result. Native
map/Arc representation and traversal refinement remain explicit boundaries. -/
def entries (snapshot : Snapshot) : List Server := snapshot.servers.map Prod.snd

def registrationEnumeration :=
  mapEnumeration (listEnumeration Server) (fun (_ : Unit) => True) (id : Server → Server)

def registrationState (snapshot : Snapshot) :
    Aeneas.Std.core.iter.adapters.map.Map (List Server) Unit :=
  { iter := entries snapshot, f := () }

theorem projection_exact (state : Unit) (server : Server) :
    projectionOps.call_mut state server = .ok (server, state) := by
  change registration.snapshot.RegistrationSnapshot.iter.closure.Insts.CoreOpsFunctionFnMutTupleShared0ArcRegisteredServerSharedRegisteredServer.call_mut
    state server = _
  simp only [registration.snapshot.RegistrationSnapshot.iter.closure.Insts.CoreOpsFunctionFnMutTupleShared0ArcRegisteredServerSharedRegisteredServer.call_mut,
    CurrentRolesNative.arcAsRef, bind_ok, bind_tc_ok]

theorem projection_mapper_law :
    MapperLaw projectionOps (fun (_ : Unit) => True) (id : Server → Server) := by
  intro state _ server
  rw [projection_exact]
  simp

theorem values_cursor_law : CursorLaw valuesIterator (listEnumeration Server) := by
  intro state _
  cases state <;> simp [valuesIterator,
    alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV.next,
    NextStep, listEnumeration, alloc.sync.Arc]

theorem registration_cursor_law : CursorLaw registrationIterator registrationEnumeration := by
  exact map_cursor_law valuesIterator (listEnumeration Server) values_cursor_law
    projectionOps (fun (_ : Unit) => True) id projection_mapper_law

theorem snapshot_iter_exact (snapshot : Snapshot) :
    registration.snapshot.RegistrationSnapshot.iter snapshot = .ok (registrationState snapshot) := by
  simp [registration.snapshot.RegistrationSnapshot.iter, alloc.collections.btree.map.BTreeMap.values,
    core.iter.traits.iterator.Iterator.map.default, registrationState, entries]

theorem registration_initial_valid (snapshot : Snapshot) :
    registrationEnumeration.valid (registrationState snapshot) := by
  exact ⟨trivial, trivial⟩

theorem registration_remaining (snapshot : Snapshot) :
    registrationEnumeration.remaining (registrationState snapshot) = entries snapshot := by
  simp [registrationEnumeration, mapEnumeration, listEnumeration, registrationState]

/- This concrete cursor law, not a generic assertion about arbitrary stateful
FusedIterator implementations, justifies default-Fuse value correspondence. -/
theorem registration_terminal_stable
    (state : Aeneas.Std.core.iter.adapters.map.Map (List Server) Unit)
    (empty : registrationEnumeration.remaining state = []) :
    registrationIterator.next state = .ok (none, state) := by
  have rawEmpty : state.iter = [] := by
    simpa [registrationEnumeration, mapEnumeration, listEnumeration] using empty
  cases state with
  | mk values mapper =>
    dsimp only at rawEmpty
    subst values
    simp [registrationIterator, valuesIterator,
      core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator.next,
      alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV.next]

theorem owned_permissions_into_exact (permissions : Finset PermissionName) :
    ownedPermissions.into_iter permissions = .ok permissions.toList := by
  rfl

theorem owned_permissions_cursor_law :
    CursorLaw ownedPermissions.iteratorInst (listEnumeration PermissionName) := by
  intro state _
  cases state <;> simp [ownedPermissions,
    alloc.collections.btree.set.IntoIter.Insts.CoreIterTraitsIteratorIterator.next,
    NextStep, listEnumeration]

def providerPermissions (installation : Installation) (caller : Caller) (server : Server) :
    Finset PermissionName :=
  GrantedPermissionsProof.expectedPermissions installation
    (GrantedPermissionsProof.selectedRoles installation
      (HeldRoleProof.expectedHeld caller.request_context.principal installation.grants.val))
    server.authorization

theorem provider_membership (installation : Installation) (caller : Caller) (server : Server)
    (permission : PermissionName) :
    permission ∈ providerPermissions installation caller server ↔
      GrantedPermissionsProof.Grants installation caller.request_context.principal
        server.authorization permission := by
  have actual := GrantedPermissionsProof.granted_permissions_membership
    installation server.authorization caller.request_context.principal
  rw [GrantedPermissionsProof.granted_permissions_exact] at actual
  simp only [WP.spec_ok] at actual
  exact actual permission

/- The actual extracted closure calls actual granted_permissions with the ORIGINAL
caller's principal; it neither substitutes the actor nor assumes a grant result. -/
theorem grant_closure_exact (installation : Installation) (caller : Caller) (server : Server) :
    grantOps.call_mut (installation, caller) server =
      .ok (providerPermissions installation caller server, (installation, caller)) := by
  change delegation.HeldCurrentPermissionsBTreeSetPermissionNameGlobal.held.closure.Insts.CoreOpsFunctionFnMutTupleShared3RegisteredServerBTreeSetPermissionNameGlobal.call_mut
    (installation, caller) server = _
  unfold delegation.HeldCurrentPermissionsBTreeSetPermissionNameGlobal.held.closure.Insts.CoreOpsFunctionFnMutTupleShared3RegisteredServerBTreeSetPermissionNameGlobal.call_mut
  simp only [alloc.sync.Arc.Insts.CoreOpsDerefDeref.deref, bind_ok]
  change (do
    let permissions ← policy.grants.granted_permissions installation server.authorization
      caller.request_context.principal
    .ok (permissions, (installation, caller))) = _
  rw [GrantedPermissionsProof.granted_permissions_exact]
  simp only [bind_tc_ok]
  rfl

theorem grant_mapper_law (installation : Installation) (caller : Caller) :
    MapperLaw grantOps (fun state => state = (installation, caller))
      (providerPermissions installation caller) := by
  intro state same server
  subst state
  rw [grant_closure_exact]
  simp <;> rfl

theorem requested_closure_exact (requested : Finset PermissionName) (permission : PermissionName) :
    requestedOps.call_mut requested permission = .ok (decide (permission ∈ requested), requested) := by
  have orderLaw : LawfulComparison
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp (taggedComparison 11) :=
    fun left right => (permission_operations_exact left right).1
  have member := set_contains_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
    (core.borrow.Borrow.Blanket PermissionName) id (by intro value; rfl)
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    (taggedComparison 11) orderLaw requested permission
  simp [requestedOps,
    delegation.HeldCurrentPermissionsBTreeSetPermissionNameGlobal.held.closure_1.Insts.CoreOpsFunctionFnMutTupleShared2PermissionNameBool.call_mut,
    member]

theorem requested_predicate_law (requested : Finset PermissionName) :
    PredicateLaw requestedOps (fun state => state = requested)
      (fun permission => decide (permission ∈ requested)) := by
  intro state same permission
  subst state
  rw [requested_closure_exact]
  simp

#print axioms projection_exact
#print axioms values_cursor_law
#print axioms registration_cursor_law
#print axioms snapshot_iter_exact
#print axioms registration_initial_valid
#print axioms registration_terminal_stable
#print axioms owned_permissions_into_exact
#print axioms owned_permissions_cursor_law
#print axioms provider_membership
#print axioms grant_closure_exact
#print axioms grant_mapper_law
#print axioms requested_closure_exact
#print axioms requested_predicate_law

end
end RegistrationIteratorProof
