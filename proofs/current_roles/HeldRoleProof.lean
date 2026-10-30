import PolicyPrimitives

open Aeneas Aeneas.Std veoveo_hub IteratorProof CollectionProof NativeSetProof PolicyPrimitives

namespace HeldRoleProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Principal := veoveo_hub_sdk.identity.Principal
abbrev Grant := control_plane.authorization_config.RoleGrant
abbrev Installation := control_plane.authorization_config.InstallationAuthorization
abbrev RoleId := veoveo_types.identity.RoleId

/- Declarative eligibility; no uniqueness of any raw grant/role name is assumed. -/
def TenantMatches (principal : Principal) (grant : Grant) : Prop :=
  grant.tenant = none ∨ ∃ tenant, grant.tenant = some tenant ∧ principal.tenant = some tenant

def SubjectMatches (principal : Principal) (grant : Grant) : Prop :=
  match grant.subject with
  | .Group group => group ∈ principal.groups
  | .Principal id => principal.id = id

def Holds (principal : Principal) (grants : List Grant) (role : RoleId) : Prop :=
  ∃ grant ∈ grants, grant.role = role ∧ TenantMatches principal grant ∧ SubjectMatches principal grant

def expectedHeld (principal : Principal) (grants : List Grant) : Finset RoleId :=
  (List.map (fun grant => grant.role)
    (grants.filter fun grant => decide (TenantMatches principal grant ∧ SubjectMatches principal grant))).toFinset

abbrev tenantOps := policy.grants.held_roles.closure.Insts.CoreOpsFunctionFnMutTupleShared1Shared2RoleGrantBool
abbrev subjectOps := policy.grants.held_roles.closure_1.Insts.CoreOpsFunctionFnMutTupleShared1Shared2RoleGrantBool
abbrev roleOps := policy.grants.held_roles.closure_2.Insts.CoreOpsFunctionFnMutTupleShared0RoleGrantRoleId

/- These are the actual emitted FnMut bodies, including unchanged captured state. -/
theorem tenant_closure_exact (principal : Principal) (grant : Grant) :
    tenantOps.call_mut principal grant = .ok (decide (TenantMatches principal grant), principal) := by
  cases grantTenant : grant.tenant <;> cases principalTenant : principal.tenant <;>
    simp [tenantOps,
      policy.grants.held_roles.closure.Insts.CoreOpsFunctionFnMutTupleShared1Shared2RoleGrantBool.call_mut,
      policy.grants.held_roles.closure.closure.Insts.CoreOpsFunctionFnOnceTupleShared1TenantIdBool,
      policy.grants.held_roles.closure.closure.Insts.CoreOpsFunctionFnOnceTupleShared1TenantIdBool.call_once,
      core.option.Option.as_ref, core.option.Option.is_none_or,
      core.option.Option.Insts.CoreCmpPartialEqOption.eq,
      core.cmp.PartialEqShared, core.cmp.impls.PartialEqShared.eq,
      veoveo_types.identity.TenantId.Insts.CoreCmpPartialEqTenantId.eq,
      TenantMatches, grantTenant, principalTenant]

theorem subject_closure_exact (principal : Principal) (grant : Grant) :
    subjectOps.call_mut principal grant = .ok (decide (SubjectMatches principal grant), principal) := by
  cases subject : grant.subject with
  | Group group =>
    have orderLaw : LawfulComparison veoveo_types.identity.GroupId.Insts.CoreCmpOrd.cmp (taggedComparison 0) := by
      intro left right
      exact (group_operations_exact left right).1
    have membership := set_contains_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      (core.borrow.Borrow.Blanket veoveo_types.identity.GroupId) id (by intro value; rfl)
      veoveo_types.identity.GroupId.Insts.CoreCmpOrd veoveo_types.identity.GroupId.Insts.CoreCmpOrd
      (taggedComparison 0) orderLaw principal.groups group
    simp [subjectOps,
      policy.grants.held_roles.closure_1.Insts.CoreOpsFunctionFnMutTupleShared1Shared2RoleGrantBool.call_mut,
      SubjectMatches, subject, membership]
  | Principal id =>
    simp [subjectOps,
      policy.grants.held_roles.closure_1.Insts.CoreOpsFunctionFnMutTupleShared1Shared2RoleGrantBool.call_mut,
      SubjectMatches, subject, veoveo_types.identity.PrincipalId.Insts.CoreCmpPartialEqPrincipalId.eq]

theorem role_closure_exact (state : Unit) (grant : Grant) :
    roleOps.call_mut state grant = .ok (grant.role, state) := by
  simp [roleOps,
    policy.grants.held_roles.closure_2.Insts.CoreOpsFunctionFnMutTupleShared0RoleGrantRoleId.call_mut,
    veoveo_types.identity.RoleId.Insts.CoreCloneClone.clone]

theorem tenant_predicate_law (principal : Principal) :
    PredicateLaw tenantOps (fun state => state = principal)
      (fun grant => decide (TenantMatches principal grant)) := by
  intro state same grant
  subst state
  rw [tenant_closure_exact]
  simp

theorem subject_predicate_law (principal : Principal) :
    PredicateLaw subjectOps (fun state => state = principal)
      (fun grant => decide (SubjectMatches principal grant)) := by
  intro state same grant
  subst state
  rw [subject_closure_exact]
  simp

theorem role_mapper_law : MapperLaw roleOps (fun _ => True) (fun grant => grant.role) := by
  intro state _ grant
  rw [role_closure_exact]
  simp

abbrev rawIterator := Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter Grant
abbrev tenantIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator rawIterator tenantOps
abbrev subjectIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator tenantIterator subjectOps
abbrev roleIterator := core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator subjectIterator roleOps

def tenantEnumeration (principal : Principal) :=
  filterEnumeration (sliceEnumeration Grant) (fun state => state = principal)
    (fun grant => decide (TenantMatches principal grant))

def subjectEnumeration (principal : Principal) :=
  filterEnumeration (tenantEnumeration principal) (fun state => state = principal)
    (fun grant => decide (SubjectMatches principal grant))

def roleEnumeration (principal : Principal) :=
  mapEnumeration (subjectEnumeration principal) (fun (_ : Unit) => True) (fun grant => grant.role)

def initialState (installation : Installation) (principal : Principal) :
    Aeneas.Std.core.iter.adapters.map.Map
      (core.iter.adapters.filter.Filter
        (core.iter.adapters.filter.Filter (Aeneas.Std.core.slice.iter.Iter Grant) Principal) Principal) Unit :=
  let tenant := filteredState installation.grants principal
  { iter := { iter := tenant, predicate := principal }, f := () }

theorem initial_valid (installation : Installation) (principal : Principal) :
    (roleEnumeration principal).valid (initialState installation principal) := by
  exact ⟨⟨⟨trivial, rfl⟩, rfl⟩, trivial⟩

theorem role_cursor_law (principal : Principal) : CursorLaw roleIterator (roleEnumeration principal) := by
  have tenantLaw := filter_cursor_law rawIterator (sliceEnumeration Grant) (slice_cursor_law Grant)
    tenantOps (fun state => state = principal) _ (tenant_predicate_law principal)
  have subjectLaw := filter_cursor_law tenantIterator (tenantEnumeration principal) tenantLaw
    subjectOps (fun state => state = principal) _ (subject_predicate_law principal)
  exact map_cursor_law subjectIterator (subjectEnumeration principal) subjectLaw roleOps
    (fun _ => True) _ role_mapper_law

theorem held_roles_exact (installation : Installation) (principal : Principal) :
    policy.grants.held_roles installation principal = .ok (expectedHeld principal installation.grants.val) := by
  have orderLaw : LawfulComparison veoveo_types.identity.RoleId.Insts.CoreCmpOrd.cmp (taggedComparison 4) := by
    intro left right
    exact (role_operations_exact left right).1
  have collected := set_collect_exact roleIterator (roleEnumeration principal) (role_cursor_law principal)
    veoveo_types.identity.RoleId.Insts.CoreCmpOrd (taggedComparison 4) orderLaw
    (initialState installation principal) (initial_valid installation principal)
  unfold policy.grants.held_roles
  simp only [Aeneas.Std.core.slice.Slice.iter, core.iter.traits.iterator.Iterator.filter.default,
    core.iter.traits.iterator.Iterator.map.default, bind_ok]
  change Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default roleIterator _
    (initialState installation principal) = _
  rw [collected]
  apply congrArg Result.ok
  ext role
  simp [roleEnumeration, subjectEnumeration, tenantEnumeration, mapEnumeration, filterEnumeration,
    sliceEnumeration, initialState, filteredState, expectedHeld, Aeneas.Std.alloc.vec.Vec.deref,
    and_assoc, and_comm, and_left_comm]

theorem held_roles_membership (installation : Installation) (principal : Principal) :
    policy.grants.held_roles installation principal ⦃ result =>
      ∀ role, role ∈ result ↔ Holds principal installation.grants.val role ⦄ := by
  rw [held_roles_exact]
  simp [expectedHeld, Holds, and_assoc, and_comm, and_left_comm]

#print axioms tenant_closure_exact
#print axioms subject_closure_exact
#print axioms role_closure_exact
#print axioms tenant_predicate_law
#print axioms subject_predicate_law
#print axioms role_mapper_law
#print axioms initial_valid
#print axioms role_cursor_law
#print axioms held_roles_exact
#print axioms held_roles_membership

end
end HeldRoleProof
