import HubCurrentRoles
import Mathlib.Data.String.Basic

open Aeneas Aeneas.Std veoveo_hub

namespace NativeSetProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

/- This is the equality-observing part needed by the extensional set model.
The concrete orders below also retain their entire actual Ordering result.
Native tree-order/borrow/clone refinement remains a separate stated boundary. -/
structure Comparison (T : Type) where
  compare : T → T → Ordering
  equal_iff : ∀ left right, compare left right = .eq ↔ left = right

def LawfulComparison {T : Type} (operation : T → T → Result Ordering)
    (model : Comparison T) : Prop :=
  ∀ left right, operation left right = .ok (model.compare left right)

def taggedComparison (tag : Nat) : Comparison (CurrentRolesId tag) where
  compare left right := compare left.value right.value
  equal_iff := by
    intro left right
    simp only [_root_.compare_eq_iff_eq]
    cases left
    cases right
    simp

theorem access_rank_injective : Function.Injective CurrentRolesNative.accessRank := by
  intro left right same
  cases left <;> cases right <;> simp_all [CurrentRolesNative.accessRank]

theorem risk_rank_injective : Function.Injective CurrentRolesNative.riskRank := by
  intro left right same
  cases left <;> cases right <;> simp_all [CurrentRolesNative.riskRank]

def accessComparison : Comparison veoveo_hub_sdk.registration.access.Access where
  compare left right := compare (CurrentRolesNative.accessRank left) (CurrentRolesNative.accessRank right)
  equal_iff := by
    intro left right
    rw [_root_.compare_eq_iff_eq]
    exact access_rank_injective.eq_iff

def riskComparison : Comparison veoveo_hub_sdk.registration.access.Risk where
  compare left right := compare (CurrentRolesNative.riskRank left) (CurrentRolesNative.riskRank right)
  equal_iff := by
    intro left right
    rw [_root_.compare_eq_iff_eq]
    exact risk_rank_injective.eq_iff

/- Exact complete comparison/partial-comparison/equality coherence, not an
assumed comparison result. String and Nat order are the ordinary Lean orders. -/
theorem access_operations_exact (left right : veoveo_hub_sdk.registration.access.Access) :
    veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpOrd.cmp left right =
      .ok (accessComparison.compare left right) ∧
    veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpPartialOrdAccess.partial_cmp left right =
      .ok (some (accessComparison.compare left right)) ∧
    veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpPartialEqAccess.eq left right =
      .ok (decide (left = right)) := by
  constructor
  · rfl
  constructor
  · rfl
  · simp only [veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpPartialEqAccess.eq,
      access_rank_injective.eq_iff]

theorem risk_operations_exact (left right : veoveo_hub_sdk.registration.access.Risk) :
    veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpOrd.cmp left right =
      .ok (riskComparison.compare left right) ∧
    veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpPartialOrdRisk.partial_cmp left right =
      .ok (some (riskComparison.compare left right)) ∧
    veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpPartialEqRisk.eq left right =
      .ok (decide (left = right)) := by
  constructor
  · rfl
  constructor
  · rfl
  · simp only [veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpPartialEqRisk.eq,
      risk_rank_injective.eq_iff]

theorem role_operations_exact (left right : veoveo_types.identity.RoleId) :
    veoveo_types.identity.RoleId.Insts.CoreCmpOrd.cmp left right =
      .ok ((taggedComparison 4).compare left right) ∧
    veoveo_types.identity.RoleId.Insts.CoreCmpPartialOrdRoleId.partial_cmp left right =
      .ok (some ((taggedComparison 4).compare left right)) ∧
    veoveo_types.identity.RoleId.Insts.CoreCmpPartialEqRoleId.eq left right =
      .ok (decide (left = right)) := by exact ⟨rfl, rfl, rfl⟩

theorem group_operations_exact (left right : veoveo_types.identity.GroupId) :
    veoveo_types.identity.GroupId.Insts.CoreCmpOrd.cmp left right =
      .ok ((taggedComparison 0).compare left right) ∧
    veoveo_types.identity.GroupId.Insts.CoreCmpPartialOrdGroupId.partial_cmp left right =
      .ok (some ((taggedComparison 0).compare left right)) ∧
    veoveo_types.identity.GroupId.Insts.CoreCmpPartialEqGroupId.eq left right =
      .ok (decide (left = right)) := by exact ⟨rfl, rfl, rfl⟩

theorem permission_operations_exact (left right : veoveo_types.permission_names.PermissionName) :
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp left right =
      .ok ((taggedComparison 11).compare left right) ∧
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialOrdPermissionName.partial_cmp left right =
      .ok (some ((taggedComparison 11).compare left right)) ∧
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialEqPermissionName.eq left right =
      .ok (decide (left = right)) := by exact ⟨rfl, rfl, rfl⟩

theorem principal_tenant_equality_exact
    (left right : veoveo_types.identity.PrincipalId) (tenant other : veoveo_types.identity.TenantId) :
    veoveo_types.identity.PrincipalId.Insts.CoreCmpPartialEqPrincipalId.eq left right =
      .ok (decide (left = right)) ∧
    veoveo_types.identity.TenantId.Insts.CoreCmpPartialEqTenantId.eq tenant other =
      .ok (decide (tenant = other)) := by exact ⟨rfl, rfl⟩

theorem tagged_order_coherent (tag : Nat) (left right : CurrentRolesId tag) :
    ((taggedComparison tag).compare left right = .lt ↔ left.value < right.value) ∧
    ((taggedComparison tag).compare left right = .eq ↔ left = right) ∧
    ((taggedComparison tag).compare left right = .gt ↔ right.value < left.value) := by
  exact ⟨_root_.compare_lt_iff_lt, (taggedComparison tag).equal_iff left right,
    _root_.compare_gt_iff_gt⟩

theorem concrete_clones_exact (risk : veoveo_hub_sdk.registration.access.Risk)
    (role : veoveo_types.identity.RoleId) (permission : veoveo_types.permission_names.PermissionName) :
    veoveo_hub_sdk.registration.access.Risk.Insts.CoreCloneClone.clone risk = .ok risk ∧
    veoveo_types.identity.RoleId.Insts.CoreCloneClone.clone role = .ok role ∧
    veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone.clone permission = .ok permission := by
  exact ⟨rfl, rfl, rfl⟩

section Primitive
variable {T Q : Type}
variable (borrow : T → Result Q) (key : T → Q)
variable (borrowLaw : ∀ value, borrow value = .ok (key value))
variable (comparison : Q → Q → Result Ordering) (order : Comparison Q)
variable (comparisonLaw : LawfulComparison comparison order)

include borrowLaw comparisonLaw in
theorem memberBy_exact (values : List T) (query : Q) :
    CurrentRolesNative.memberBy borrow comparison values query =
      .ok (decide (∃ value ∈ values, key value = query)) := by
  unfold LawfulComparison at comparisonLaw
  induction values with
  | nil => simp [CurrentRolesNative.memberBy]
  | cons head tail recursive =>
    simp only [CurrentRolesNative.memberBy, borrowLaw, comparisonLaw, bind_ok]
    cases found : order.compare (key head) query with
    | eq =>
      have same : key head = query := (order.equal_iff _ _).mp found
      simp [found, same]
    | lt =>
      have different : key head ≠ query := by
        intro same
        have equal := (order.equal_iff _ _).mpr same
        rw [found] at equal
        cases equal
      simp [found, recursive, different]
    | gt =>
      have different : key head ≠ query := by
        intro same
        have equal := (order.equal_iff _ _).mpr same
        rw [found] at equal
        cases equal
      simp [found, recursive, different]
end Primitive

section SameType
variable {T : Type} (comparison : T → T → Result Ordering) (order : Comparison T)
variable (comparisonLaw : LawfulComparison comparison order)
include comparisonLaw in
theorem subsetBy_exact (left right : List T) :
    CurrentRolesNative.subsetBy comparison left right =
      .ok (decide (∀ value ∈ left, value ∈ right)) := by
  induction left with
  | nil => simp [CurrentRolesNative.subsetBy]
  | cons head tail recursive =>
    rw [CurrentRolesNative.subsetBy,
      memberBy_exact (fun value => .ok value) id (by intro value; rfl)
        comparison order comparisonLaw right head]
    simp only [bind_ok]
    by_cases member : head ∈ right
    · simp [member, recursive, List.mem_cons, or_imp, forall_and]
    · simp [member, List.mem_cons, or_imp, forall_and]

include comparisonLaw in
theorem insertBy_exact (set : Finset T) (value : T) :
    CurrentRolesNative.insertBy comparison set value = .ok (insert value set) := by
  rw [CurrentRolesNative.insertBy,
    memberBy_exact (fun value => .ok value) id (by intro value; rfl)
      comparison order comparisonLaw set.toList value]
  simp only [bind_ok]
  by_cases member : value ∈ set
  · simp [member, List.mem_cons, or_imp, forall_and]
  · simp [member, List.mem_cons, or_imp, forall_and]
end SameType

theorem cloneList_exact {T : Type} (clone : T → Result T)
    (cloneLaw : ∀ value, clone value = .ok value) (values : List T) :
    CurrentRolesNative.cloneList clone values = .ok values := by
  induction values with
  | nil => rfl
  | cons head tail recursive =>
    simp [CurrentRolesNative.cloneList, cloneLaw, recursive]

/- These statements concern ACTUAL external model operations called by the
extracted policy. Native BTreeSet representation/refinement is not proved by them. -/
theorem set_contains_exact {T Q A : Type}
    (allocator : core.alloc.AllocatorClone A) (borrow : core.borrow.Borrow T Q)
    (key : T → Q) (borrowLaw : ∀ value, borrow.borrow value = .ok (key value))
    (nativeOrder : core.cmp.Ord T) (queryOrder : core.cmp.Ord Q)
    (order : Comparison Q) (comparisonLaw : LawfulComparison queryOrder.cmp order)
    (set : alloc.collections.btree.set.BTreeSet T A) (query : Q) :
    alloc.collections.btree.set.BTreeSet.contains allocator borrow nativeOrder queryOrder set query =
      .ok (decide (∃ value ∈ set, key value = query)) := by
  simpa [alloc.collections.btree.set.BTreeSet.contains] using
    memberBy_exact borrow.borrow key borrowLaw queryOrder.cmp order comparisonLaw set.toList query

theorem set_subset_exact {T A : Type} (allocator : core.alloc.AllocatorClone A)
    (nativeOrder : core.cmp.Ord T) (order : Comparison T)
    (comparisonLaw : LawfulComparison nativeOrder.cmp order)
    (left right : alloc.collections.btree.set.BTreeSet T A) :
    alloc.collections.btree.set.BTreeSet.is_subset allocator nativeOrder left right =
      .ok (decide (left ⊆ right)) := by
  simpa [alloc.collections.btree.set.BTreeSet.is_subset, Finset.subset_iff] using
    subsetBy_exact nativeOrder.cmp order comparisonLaw left.toList right.toList

theorem set_clone_exact {T A : Type} (clone : core.clone.Clone T)
    (cloneLaw : ∀ value, clone.clone value = .ok value)
    (allocator : core.alloc.AllocatorClone A) (set : alloc.collections.btree.set.BTreeSet T A) :
    alloc.collections.btree.set.BTreeSet.Insts.CoreCloneClone.clone clone allocator set = .ok set := by
  simp [alloc.collections.btree.set.BTreeSet.Insts.CoreCloneClone.clone,
    cloneList_exact clone.clone cloneLaw]

theorem role_allows_exact (role : control_plane.authorization_config.InstallationRole)
    (access : veoveo_hub_sdk.registration.access.Access)
    (risks : Finset veoveo_hub_sdk.registration.access.Risk) :
    control_plane.authorization_config.InstallationRole.allows role access risks =
      .ok (decide (access ∈ role.access ∧ risks ⊆ role.risks)) := by
  have accessLaw : LawfulComparison
      veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpOrd.cmp accessComparison := by
    intro left right
    exact (access_operations_exact left right).1
  have riskLaw : LawfulComparison
      veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpOrd.cmp riskComparison := by
    intro left right
    exact (risk_operations_exact left right).1
  unfold control_plane.authorization_config.InstallationRole.allows
  rw [set_contains_exact _ _ id (by intro value; rfl) _ _ accessComparison accessLaw]
  simp only [bind_ok]
  by_cases member : access ∈ role.access
  · have subsetExact := set_subset_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpOrd riskComparison riskLaw risks role.risks
    convert subsetExact using 1 <;> simp [member]
  · simp [member]

#print axioms taggedComparison
#print axioms tagged_order_coherent
#print axioms concrete_clones_exact
#print axioms access_operations_exact
#print axioms risk_operations_exact
#print axioms role_operations_exact
#print axioms group_operations_exact
#print axioms permission_operations_exact
#print axioms principal_tenant_equality_exact
#print axioms memberBy_exact
#print axioms subsetBy_exact
#print axioms insertBy_exact
#print axioms cloneList_exact
#print axioms set_contains_exact
#print axioms set_subset_exact
#print axioms set_clone_exact
#print axioms role_allows_exact

end
end NativeSetProof
