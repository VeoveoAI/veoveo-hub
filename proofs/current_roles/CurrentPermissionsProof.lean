import FlatMapIteratorProof
import AuthorityComposition

open Aeneas Aeneas.Std veoveo_hub IteratorProof CollectionProof NativeSetProof PolicyPrimitives
open RegistrationIteratorProof

namespace CurrentPermissionsProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Held := delegation.CurrentPermissions
abbrev Permission := RegistrationIteratorProof.PermissionName
abbrev Provider := RegistrationIteratorProof.Server
abbrev MapperState := delegation.HeldCurrentPermissionsBTreeSetPermissionNameGlobal.held.closure
abbrev FlatState := core.iter.adapters.flatten.FlatMap
  (Aeneas.Std.core.iter.adapters.map.Map (List Provider) Unit)
  (Finset Permission) MapperState Permission (List Permission)
abbrev actualHeld := delegation.CurrentPermissions.Insts.Veoveo_hubDelegationAuthorityHeldBTreeSetPermissionNameGlobal.held
abbrev flatIterator := core.iter.adapters.flatten.FlatMap.Insts.CoreIterTraitsIteratorIterator
  registrationIterator ownedPermissions grantOps
abbrev finalIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator
  flatIterator requestedOps

/- The extraction-only field label denotes Rust's original control_plane field.
Raw registrations are exactly the supplied deployment map values, not get(slug),
namespaces(), newest-only selection or an already authorized permission oracle. -/
def installation (held : Held) := held.catalog.control_plane_value.authorization
def registrations (held : Held) := entries held.catalog.registrations
def provider (held : Held) := providerPermissions (installation held) held.caller

def Current (held : Held) (permission : Permission) : Prop :=
  ∃ server ∈ registrations held,
    GrantedPermissionsProof.Grants (installation held)
      held.caller.request_context.principal server.authorization permission

def emitted (held : Held) : List Permission :=
  (registrations held).flatMap (fun server => (provider held server).toList)

def expected (held : Held) (requested : Finset Permission) : Finset Permission :=
  requested.filter (Current held)

theorem emitted_membership (held : Held) (permission : Permission) :
    permission ∈ emitted held ↔ Current held permission := by
  simp [emitted, Current, provider, provider_membership]

theorem expected_membership (held : Held) (requested : Finset Permission) (permission : Permission) :
    permission ∈ expected held requested ↔ permission ∈ requested ∧ Current held permission := by
  simp [expected]

def flatModel (held : Held) :=
  FlatMapIteratorProof.enumeration registrationEnumeration (listEnumeration Permission)
    (fun state => state = (installation held, held.caller))
    (provider held) (fun permissions => permissions.toList)

def flatState (held : Held) : FlatState :=
  { iter := registrationState held.catalog.registrations,
    mapper := (installation held, held.caller), inner := none, exhausted := false }

def finalModel (held : Held) (requested : Finset Permission) :=
  filterEnumeration (flatModel held) (fun state => state = requested)
    (fun permission => decide (permission ∈ requested))

def finalState (held : Held) (requested : Finset Permission) :
    core.iter.adapters.filter.Filter FlatState (Finset Permission) :=
  { iter := flatState held, predicate := requested }

theorem flat_cursor_law (held : Held) : CursorLaw flatIterator (flatModel held) := by
  have intoLaw : ∀ permissions : Finset Permission,
      ownedPermissions.into_iter permissions ⦃ cursor =>
        (listEnumeration Permission).valid cursor ∧
          (listEnumeration Permission).remaining cursor = permissions.toList ⦄ := by
    intro permissions
    rw [owned_permissions_into_exact]
    simp [listEnumeration]
  exact FlatMapIteratorProof.flat_map_cursor_law
    registrationIterator registrationEnumeration registration_cursor_law
    ownedPermissions (listEnumeration Permission) owned_permissions_cursor_law
    (fun permissions => permissions.toList) intoLaw
    grantOps (fun state => state = (installation held, held.caller)) (provider held)
    (grant_mapper_law (installation held) held.caller)

theorem flat_initial_valid (held : Held) : (flatModel held).valid (flatState held) := by
  exact FlatMapIteratorProof.initial_valid
    (outer := registrationEnumeration) (inner := listEnumeration Permission)
    (items := fun permissions : Finset Permission => permissions.toList)
    (valid := fun (state : MapperState) => state = (installation held, held.caller))
    (evaluate := provider held)
    (registrationState held.catalog.registrations) (installation held, held.caller)
    (registration_initial_valid held.catalog.registrations) rfl

theorem flat_initial_remaining (held : Held) :
    (flatModel held).remaining (flatState held) = emitted held := by
  simp [flatModel, flatState, FlatMapIteratorProof.enumeration,
    registration_remaining, emitted, registrations]

/- Derived concrete terminal stability is the native Fuse-specialization
correspondence obligation. No generic post-None call-count claim is made. -/
theorem actual_outer_terminal_stable
    (state : Aeneas.Std.core.iter.adapters.map.Map (List Provider) Unit)
    (empty : registrationEnumeration.remaining state = []) :
    registrationIterator.next state = .ok (none, state) :=
  registration_terminal_stable state empty

theorem final_cursor_law (held : Held) (requested : Finset Permission) :
    CursorLaw finalIterator (finalModel held requested) := by
  exact filter_cursor_law flatIterator (flatModel held) (flat_cursor_law held)
    requestedOps (fun state => state = requested) _ (requested_predicate_law requested)

theorem final_initial_valid (held : Held) (requested : Finset Permission) :
    (finalModel held requested).valid (finalState held requested) := by
  exact ⟨flat_initial_valid held, rfl⟩

theorem final_remaining (held : Held) (requested : Finset Permission) :
    (finalModel held requested).remaining (finalState held requested) =
      (emitted held).filter (fun permission => decide (permission ∈ requested)) := by
  simp only [finalModel, finalState, filterEnumeration]
  rw [flat_initial_remaining]

theorem final_values (held : Held) (requested : Finset Permission) :
    ((finalModel held requested).remaining (finalState held requested)).toFinset =
      expected held requested := by
  rw [final_remaining]
  ext permission
  simp [expected, emitted_membership, and_comm]

/- The actual production method, not a new wrapper with a precomputed result.
Every initial cursor/closure validity and every collection output is derived. -/
theorem held_exact (held : Held) (requested : Finset Permission) :
    actualHeld held requested = .ok (expected held requested) := by
  have orderLaw : LawfulComparison
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp (taggedComparison 11) :=
    fun left right => (permission_operations_exact left right).1
  have collected := set_collect_exact finalIterator (finalModel held requested)
    (final_cursor_law held requested)
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    (taggedComparison 11) orderLaw (finalState held requested) (final_initial_valid held requested)
  change delegation.CurrentPermissions.Insts.Veoveo_hubDelegationAuthorityHeldBTreeSetPermissionNameGlobal.held
    held requested = .ok (expected held requested)
  unfold delegation.CurrentPermissions.Insts.Veoveo_hubDelegationAuthorityHeldBTreeSetPermissionNameGlobal.held
  simp only [catalog.HubCatalog.control_plane, catalog.HubCatalog.impl.registrations,
    alloc.sync.Arc.Insts.CoreOpsDerefDeref.deref,
    registration.snapshot.RegistrationSnapshot.iter, alloc.collections.btree.map.BTreeMap.values,
    core.iter.traits.iterator.Iterator.map.default,
    core.iter.traits.iterator.Iterator.flat_map.default,
    core.iter.traits.iterator.Iterator.filter.default, bind_ok]
  change Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default finalIterator
    (alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd)
    (finalState held requested) = .ok (expected held requested)
  apply collected.trans
  apply congrArg Result.ok
  ext permission
  have agreement := (Finset.ext_iff.mp (final_values held requested)) permission
  simpa only [List.mem_toFinset] using agreement

theorem held_membership (held : Held) (requested : Finset Permission) :
    actualHeld held requested ⦃ result => ∀ permission,
      permission ∈ result ↔ permission ∈ requested ∧
        ∃ server ∈ registrations held,
          GrantedPermissionsProof.Grants (installation held)
            held.caller.request_context.principal server.authorization permission ⦄ := by
  rw [held_exact]
  simp only [WP.spec_ok]
  exact expected_membership held requested

theorem empty_request (held : Held) : actualHeld held ∅ = .ok ∅ := by
  rw [held_exact]
  simp [expected]

theorem empty_registrations (held : Held) (requested : Finset Permission)
    (empty : registrations held = []) : actualHeld held requested = .ok ∅ := by
  rw [held_exact]
  simp [expected, Current, empty]

/- Concrete connection to the already qualified generic authority namespace.
The operation field is the actual extracted method; LawfulHeld is proved below,
not supplied as a coarse current-result premise. No concrete types are cloned. -/
def permissionSets : AuthorityProof.SetModel (Finset Permission) Permission where
  members permissions := { permission | permission ∈ permissions }
  filter left right := left ∩ right
  filter_members := by intro left right; ext permission; simp

def heldModel : AuthorityProof.HeldModel Held (Finset Permission) Permission permissionSets where
  current held := { permission | Current held permission }
  result := expected
  result_members := by
    intro held requested
    ext permission
    simp [permissionSets, expected]

def heldOps : hub_authority_proof.authority.Held Held (Finset Permission) where
  held := actualHeld

theorem lawful_held : AuthorityProof.LawfulHeld heldOps heldModel := by
  intro held requested
  exact held_exact held requested

section Issuance
/- The generic supplied Context is preserved. This does not prove that the
async endpoint constructed Context.caller from held.caller, or catalog freshness;
those caller/transport/storage bindings remain separate evidence. -/
variable {U R K F C A T D N : Type}
variable (usesModel : AuthorityProof.UsesModel U (Finset Permission) Permission permissionSets)
variable (usesOps : hub_authority_proof.authority.Uses U (Finset Permission) Permission)
variable (usesLaw : AuthorityProof.LawfulUses usesOps usesModel)
variable (key : R → K) (resourceOps : hub_authority_proof.authority.ResourceKey R K)
variable (resourceLaw : AuthorityProof.LawfulResource resourceOps key)

include usesLaw resourceLaw in
theorem issued_membership (requested : Finset Permission) (uses : U)
    (destination : hub_authority_proof.authority.Destination R) (requestedResource : Option K)
    (missing : Slice N) (held : Held) (context : hub_authority_proof.authority.Context F C A T D) :
    AuthorityProof.exchangeTrace usesOps heldOps resourceOps requested uses destination
      requestedResource missing held context ⦃ result => ∀ output, result = .ok output →
        output.context = context ∧ ∀ permission, permission ∈ output.permissions ↔
          permission ∈ requested ∧ permission ∈ usesModel.declared uses ∧
          ∃ server ∈ registrations held,
            GrantedPermissionsProof.Grants (installation held)
              held.caller.request_context.principal server.authorization permission ⦄ := by
  simpa only [permissionSets, heldModel, Current, Set.mem_ofPred_eq] using
    AuthorityComposition.issued_membership_bound permissionSets usesModel usesOps usesLaw
      heldModel heldOps lawful_held key resourceOps resourceLaw requested uses destination
      requestedResource missing held context
end Issuance

#print axioms flat_cursor_law
#print axioms flat_initial_valid
#print axioms actual_outer_terminal_stable
#print axioms final_cursor_law
#print axioms final_values
#print axioms held_exact
#print axioms held_membership
#print axioms empty_request
#print axioms empty_registrations
#print axioms lawful_held
#print axioms issued_membership

end
end CurrentPermissionsProof
