import HeldRoleProof
import EffectiveRiskProof

open Aeneas Aeneas.Std veoveo_hub IteratorProof CollectionProof NativeSetProof PolicyPrimitives

namespace GrantedPermissionsProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Installation := control_plane.authorization_config.InstallationAuthorization
abbrev Role := control_plane.authorization_config.InstallationRole
abbrev Permission := veoveo_hub_sdk.registration.authorization.PermissionDeclaration
abbrev Declaration := veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration
abbrev Principal := veoveo_hub_sdk.identity.Principal
abbrev Risk := veoveo_hub_sdk.registration.access.Risk
abbrev RoleId := veoveo_types.identity.RoleId
abbrev PermissionName := veoveo_types.permission_names.PermissionName

/- One raw role must supply access AND all risks. Duplicate names do not combine roles. -/
def Allows (role : Role) (permission : Permission) (risks : Finset Risk) : Prop :=
  permission.access ∈ role.access ∧ risks ⊆ role.risks

def Selected (installation : Installation) (roles : List Role) (permission : Permission) : Prop :=
  ¬ EffectiveRiskProof.Denied permission.permission installation.overrides.val ∧
  ∃ role ∈ roles, Allows role permission
    (permission.risks ∪ EffectiveRiskProof.additions permission.permission installation.overrides.val)

def Grants (installation : Installation) (principal : Principal) (declaration : Declaration)
    (permissionName : PermissionName) : Prop :=
  ∃ permission ∈ declaration.permissions.val, permission.permission = permissionName ∧
    ¬ EffectiveRiskProof.Denied permission.permission installation.overrides.val ∧
    ∃ role ∈ installation.roles.val,
      HeldRoleProof.Holds principal installation.grants.val role.name ∧
      Allows role permission
        (permission.risks ∪ EffectiveRiskProof.additions permission.permission installation.overrides.val)

abbrev selectOps := policy.grants.granted_permissions.closure.Insts.CoreOpsFunctionFnMutTupleShared1Shared2InstallationRoleBool
abbrev allowsOps := policy.grants.granted_permissions.closure_1.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3InstallationRoleBool
abbrev permissionOps := policy.grants.granted_permissions.closure_1.Insts.CoreOpsFunctionFnMutTupleShared3Shared4PermissionDeclarationBool
abbrev permissionNameOps := policy.grants.granted_permissions.closure_2.Insts.CoreOpsFunctionFnMutTupleShared0PermissionDeclarationPermissionName

theorem selection_closure_exact (held : Finset RoleId) (role : Role) :
    selectOps.call_mut held role = .ok (decide (role.name ∈ held), held) := by
  have orderLaw : LawfulComparison veoveo_types.identity.RoleId.Insts.CoreCmpOrd.cmp (taggedComparison 4) :=
    fun left right => (role_operations_exact left right).1
  have member := set_contains_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
    (core.borrow.Borrow.Blanket RoleId) id (by intro value; rfl)
    veoveo_types.identity.RoleId.Insts.CoreCmpOrd veoveo_types.identity.RoleId.Insts.CoreCmpOrd
    (taggedComparison 4) orderLaw held role.name
  simp [selectOps,
    policy.grants.granted_permissions.closure.Insts.CoreOpsFunctionFnMutTupleShared1Shared2InstallationRoleBool.call_mut,
    member]

theorem selection_predicate_law (held : Finset RoleId) :
    PredicateLaw selectOps (fun state => state = held) (fun role => decide (role.name ∈ held)) := by
  intro state same role
  subst state
  rw [selection_closure_exact]
  simp

theorem allows_closure_exact (permission : Permission) (risks : Finset Risk) (role : Role) :
    allowsOps.call_mut (permission, risks) role =
      .ok (decide (Allows role permission risks), (permission, risks)) := by
  by_cases access : permission.access ∈ role.access <;> by_cases acceptedRisks : risks ⊆ role.risks <;>
    simp [allowsOps,
      policy.grants.granted_permissions.closure_1.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3InstallationRoleBool.call_mut,
      role_allows_exact, Allows, access, acceptedRisks]

theorem allows_predicate_law (permission : Permission) (risks : Finset Risk) :
    PredicateLaw allowsOps (fun state => state = (permission, risks))
      (fun role => decide (Allows role permission risks)) := by
  intro state same role
  subst state
  rw [allows_closure_exact]
  simp <;> rfl

/- This is the actual any implementation invoked by the extracted parent closure. -/
theorem roles_any_correct (permission : Permission) (risks : Finset Risk) (roles : alloc.vec.Vec Role) :
    core.slice.iter.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.any allowsOps
      { slice := Aeneas.Std.alloc.vec.Vec.deref roles, i := 0 } (permission, risks)
      ⦃ result => result.1 = decide (∃ role ∈ roles.val, Allows role permission risks) ⦄ := by
  unfold core.slice.iter.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.any
  apply WP.spec_mono (any_correct _ (sliceEnumeration Role) (slice_cursor_law Role)
    allowsOps (fun state => state = (permission, risks)) _ (allows_predicate_law permission risks)
    { slice := Aeneas.Std.alloc.vec.Vec.deref roles, i := 0 } (permission, risks) trivial rfl)
  intro result specification
  rw [specification.1]
  simp only [sliceEnumeration, Aeneas.Std.alloc.vec.Vec.deref, Aeneas.Std.Slice.from_val, List.drop_zero]
  have truth : roles.val.any (fun role => decide (Allows role permission risks)) = true ↔
      ∃ role ∈ roles.val, Allows role permission risks := by simp
  cases outcome : roles.val.any (fun role => decide (Allows role permission risks)) with
  | false =>
    have absent : ¬ ∃ role ∈ roles.val, Allows role permission risks := by
      intro existsRole
      have impossible := truth.mpr existsRole
      simp [outcome] at impossible
    simp [absent]
  | true => simp [truth.mp outcome]

theorem permission_closure_exact (installation : Installation) (roles : alloc.vec.Vec Role)
    (permission : Permission) :
    permissionOps.call_mut (installation, roles) permission =
      .ok (decide (Selected installation roles.val permission), (installation, roles)) := by
  apply exact_of_total
  /- Definitional normalization of the ACTUAL call_mut body as one typed term.
  Keeping the nested captured tuple typed by its generated alias avoids a mixed
  unfolded-alias/opaque-instance expression during subsequent rewriting. -/
  change (do
    let effective ← policy.grants.effective_risks installation permission
    match effective with
    | none => .ok (false, (installation, roles))
    | some risks =>
      let cursor ← Aeneas.Std.core.slice.Slice.iter (Aeneas.Std.alloc.vec.Vec.deref roles)
      let (answer, _) ← core.slice.iter.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.any
        allowsOps cursor ((permission, risks) : policy.grants.granted_permissions.closure_1.closure)
      .ok (answer, (installation, roles))) ⦃ result =>
        result = (decide (Selected installation roles.val permission), (installation, roles)) ⦄
  erw [EffectiveRiskProof.effective_risks_exact]
  by_cases denied : EffectiveRiskProof.Denied permission.permission installation.overrides.val
  · simp [EffectiveRiskProof.expected, denied, Selected]
  · simp only [EffectiveRiskProof.expected, denied, if_false, bind_ok, Aeneas.Std.core.slice.Slice.iter]
    apply WP.spec_bind (roles_any_correct permission
      (permission.risks ∪ EffectiveRiskProof.additions permission.permission installation.overrides.val) roles)
    rintro ⟨answer, next⟩ answerEq
    dsimp only at answerEq ⊢
    simp [WP.spec_ok, answerEq, Selected, denied] <;> rfl

theorem permission_name_closure_exact (state : Unit) (permission : Permission) :
    permissionNameOps.call_mut state permission = .ok (permission.permission, state) := by
  simp [permissionNameOps,
    policy.grants.granted_permissions.closure_2.Insts.CoreOpsFunctionFnMutTupleShared0PermissionDeclarationPermissionName.call_mut,
    veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone.clone]


/- Intermediate Vec is produced by the ACTUAL filter collector. The bound is
proved from the original raw roles Vec, rather than assumed of a chosen result. -/
abbrev rawRoleIterator := Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter Role
abbrev selectionIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator rawRoleIterator selectOps

def selectionEnumeration (held : Finset RoleId) :=
  filterEnumeration (sliceEnumeration Role) (fun state => state = held)
    (fun role => decide (role.name ∈ held))

def selectedRoles (installation : Installation) (held : Finset RoleId) : alloc.vec.Vec Role :=
  Aeneas.Std.alloc.vec.Vec.from
    ((selectionEnumeration held).remaining (filteredState installation.roles held))
    (filtered_machine_bound installation.roles (fun state => state = held)
      (fun role => decide (role.name ∈ held)) held)

theorem selected_roles_collect (installation : Installation) (held : Finset RoleId) :
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default selectionIterator
      (Aeneas.Std.core.iter.traits.collect.FromIteratorVec Role) (filteredState installation.roles held) =
      .ok (selectedRoles installation held) := by
  exact filtered_vec_collect_exact installation.roles selectOps (fun state => state = held)
    (fun role => decide (role.name ∈ held)) (selection_predicate_law held) held rfl

theorem selected_roles_values (installation : Installation) (held : Finset RoleId) :
    (selectedRoles installation held).val =
      installation.roles.val.filter (fun role => decide (role.name ∈ held)) := by
  simp [selectedRoles, selectionEnumeration, filterEnumeration, sliceEnumeration, filteredState,
    Aeneas.Std.alloc.vec.Vec.deref]

theorem permission_predicate_law (installation : Installation) (roles : alloc.vec.Vec Role) :
    PredicateLaw permissionOps (fun state => state = (installation, roles))
      (fun permission => decide (Selected installation roles.val permission)) := by
  intro state same permission
  subst state
  rw [permission_closure_exact]
  simp <;> rfl

theorem permission_name_mapper_law :
    MapperLaw permissionNameOps (fun (_ : Unit) => True) (fun permission => permission.permission) := by
  intro state _ permission
  rw [permission_name_closure_exact]
  simp

abbrev rawPermissionIterator := Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter Permission
abbrev permissionIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator rawPermissionIterator permissionOps
abbrev outputIterator := core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator permissionIterator permissionNameOps

def permissionEnumeration (installation : Installation) (roles : alloc.vec.Vec Role) :=
  filterEnumeration (sliceEnumeration Permission)
    (fun (state : policy.grants.granted_permissions.closure_1) => state = (installation, roles))
    (fun permission => decide (Selected installation roles.val permission))

def outputEnumeration (installation : Installation) (roles : alloc.vec.Vec Role) :=
  mapEnumeration (permissionEnumeration installation roles) (fun (_ : Unit) => True)
    (fun permission => permission.permission)

def outputState (installation : Installation) (roles : alloc.vec.Vec Role) (declaration : Declaration) :
    Aeneas.Std.core.iter.adapters.map.Map
      (core.iter.adapters.filter.Filter (Aeneas.Std.core.slice.iter.Iter Permission)
        policy.grants.granted_permissions.closure_1) Unit :=
  { iter := filteredState declaration.permissions
      ((installation, roles) : policy.grants.granted_permissions.closure_1), f := () }

theorem output_initial_valid (installation : Installation) (roles : alloc.vec.Vec Role) (declaration : Declaration) :
    (outputEnumeration installation roles).valid (outputState installation roles declaration) := by
  exact ⟨⟨trivial, rfl⟩, trivial⟩

theorem output_cursor_law (installation : Installation) (roles : alloc.vec.Vec Role) :
    CursorLaw outputIterator (outputEnumeration installation roles) := by
  have permissionLaw := filter_cursor_law rawPermissionIterator (sliceEnumeration Permission)
    (slice_cursor_law Permission) permissionOps (fun state => state = (installation, roles)) _
    (permission_predicate_law installation roles)
  exact map_cursor_law permissionIterator (permissionEnumeration installation roles) permissionLaw
    permissionNameOps (fun (_ : Unit) => True) _ permission_name_mapper_law

def expectedPermissions (installation : Installation) (roles : alloc.vec.Vec Role) (declaration : Declaration) :
    Finset PermissionName :=
  (List.map (fun permission => permission.permission)
    (declaration.permissions.val.filter fun permission => decide (Selected installation roles.val permission))).toFinset

theorem granted_permissions_exact (installation : Installation) (declaration : Declaration) (principal : Principal) :
    policy.grants.granted_permissions installation declaration principal =
      .ok (expectedPermissions installation
        (selectedRoles installation (HeldRoleProof.expectedHeld principal installation.grants.val)) declaration) := by
  let held := HeldRoleProof.expectedHeld principal installation.grants.val
  let roles := selectedRoles installation held
  have orderLaw : LawfulComparison veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp
      (taggedComparison 11) := fun left right => (permission_operations_exact left right).1
  have collected := set_collect_exact outputIterator (outputEnumeration installation roles)
    (output_cursor_law installation roles) veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    (taggedComparison 11) orderLaw (outputState installation roles declaration)
    (output_initial_valid installation roles declaration)
  unfold policy.grants.granted_permissions
  simp only [Aeneas.Std.core.slice.Slice.iter, core.iter.traits.iterator.Iterator.filter.default,
    core.iter.traits.iterator.Iterator.map.default, bind_ok]
  /- After the existing pure-constructor/bind equations, normalize the actual
  two-collect term definitionally; preserve native calls and closure instances. -/
  change (do
    let actualHeld ← policy.grants.held_roles installation principal
    let actualRoles ← Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default selectionIterator
      (Aeneas.Std.core.iter.traits.collect.FromIteratorVec Role) (filteredState installation.roles actualHeld)
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default outputIterator
      (alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator
        veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd)
      (outputState installation actualRoles declaration)) = .ok (expectedPermissions installation roles declaration)
  rw [HeldRoleProof.held_roles_exact]
  simp only [bind_tc_ok]
  rw [selected_roles_collect installation held]
  simp only [bind_tc_ok]
  rw [collected]
  apply congrArg Result.ok
  ext permissionName
  simp [outputEnumeration, permissionEnumeration, mapEnumeration, filterEnumeration, sliceEnumeration,
    outputState, filteredState, Aeneas.Std.alloc.vec.Vec.deref, expectedPermissions]

theorem granted_permissions_membership (installation : Installation) (declaration : Declaration)
    (principal : Principal) :
    policy.grants.granted_permissions installation declaration principal ⦃ result =>
      ∀ permissionName, permissionName ∈ result ↔ Grants installation principal declaration permissionName ⦄ := by
  have holds := HeldRoleProof.held_roles_membership installation principal
  rw [HeldRoleProof.held_roles_exact] at holds
  simp only [WP.spec_ok] at holds
  rw [granted_permissions_exact]
  simp only [WP.spec_ok]
  intro permissionName
  simp [expectedPermissions, selected_roles_values, Selected, Grants, holds,
    and_assoc, and_comm, and_left_comm]

#print axioms selected_roles_collect
#print axioms selected_roles_values
#print axioms permission_predicate_law
#print axioms permission_name_mapper_law
#print axioms output_initial_valid
#print axioms output_cursor_law
#print axioms granted_permissions_exact
#print axioms granted_permissions_membership

#print axioms selection_closure_exact
#print axioms selection_predicate_law
#print axioms allows_closure_exact
#print axioms allows_predicate_law
#print axioms roles_any_correct
#print axioms permission_closure_exact
#print axioms permission_name_closure_exact
end
end GrantedPermissionsProof
