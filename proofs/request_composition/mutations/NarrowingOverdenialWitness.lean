import HubCurrentRoles.Funs
import RegistrationIteratorProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub IteratorProof NativeSetProof PolicyPrimitives

-- This standalone witness imports only the raw model and unaffected generic
-- finite-operation proofs. Never import DelegatedPolicyClientProof, a rejected
-- theorem artifact, or the other source-control witness into this package.
-- Source baseline: Hub3bc2923 / SDK239622d / Core120306e, with the exact generic
-- delegation::authority::narrow exclusion from snapshot-narrow-exclude-plan1.
namespace NarrowingOverdenialWitness
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate
abbrev Permission := veoveo_types.permission_names.PermissionName
abbrev Grants := Finset Permission
abbrev Delegation := delegation.SubjectDelegation
abbrev nativeNarrow :=
  alloc.collections.btree.set.BTreeSetPermissionNameGlobal.Insts.Veoveo_hubDelegationAuthorityPermissionsPermissionName.narrow
abbrev narrowOps :=
  delegation.PermissionsBTreeSetPermissionNameGlobalPermissionName.narrow.closure.Insts.CoreOpsFunctionFnMutTupleShared1PermissionNameBool
abbrev ownedPermissions := RegistrationIteratorProof.ownedPermissions
abbrev narrowIterator := core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator
  ownedPermissions.iteratorInst narrowOps

/-- Independent mathematical specification; no actual source result is assumed. -/
def expectedIntersection (granted carried : Grants) : Grants := granted ∩ carried

private theorem narrow_predicate_exact (carried : Grants) (permission : Permission) :
    narrowOps.call_mut carried permission = .ok (false, carried) := by
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
      (fun (_ : Permission) => false) := by
  intro state same permission
  subst state
  rw [narrow_predicate_exact]
  simp

private def narrowModel (carried : Grants) :=
  filterEnumeration (listEnumeration Permission) (fun state : Grants => state = carried)
    (fun (_ : Permission) => false)

private def narrowState (granted carried : Grants) :
    core.iter.adapters.filter.Filter (List Permission) Grants :=
  { iter := granted.toList, predicate := carried }

/-- Total empty-result semantics of the actual mutated adapter, using its real
owned cursor, always-false source predicate and collector. This derives execution
from the emitted body rather than assuming an empty result or replacing recursion. -/
theorem mutant_native_empty (granted carried : Grants) :
    nativeNarrow granted carried = .ok (∅ : Grants) := by
  have cursor := filter_cursor_law ownedPermissions.iteratorInst (listEnumeration Permission)
    RegistrationIteratorProof.owned_permissions_cursor_law narrowOps
    (fun state => state = carried) (fun (_ : Permission) => false)
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
    (narrowState granted carried) = .ok (∅ : Grants)
  rw [collected]
  apply congrArg Result.ok
  ext permission
  simp [narrowModel, narrowState, filterEnumeration, listEnumeration]

/-- The unchanged wrapper propagates the concrete predicate fault; no own-code
function result is replaced by an assumed success. -/
theorem mutant_wrapper_empty (info : Delegation) (granted : Grants) :
    delegation.SubjectDelegation.narrow info granted = .ok (∅ : Grants) := by
  unfold delegation.SubjectDelegation.narrow
  exact mutant_native_empty granted info.permissions

/-- Both finite native-set inputs contain the same actual typed permission. The
mutant succeeds with no permission, losing a legitimate common member. -/
theorem singleton_permission_lost (permission : Permission) :
    nativeNarrow ({permission} : Grants) ({permission} : Grants) = .ok (∅ : Grants) ∧
      permission ∈ expectedIntersection ({permission} : Grants) ({permission} : Grants) := by
  refine ⟨mutant_native_empty _ _, ?_⟩
  simp [expectedIntersection]

/-- Successful overdenial contradicts the independent intersection even with
identical nonempty inputs; this is not merely frozen-proof syntax sensitivity. -/
theorem loss_violates_intersection (permission : Permission) :
    nativeNarrow ({permission} : Grants) ({permission} : Grants) ≠
      .ok (expectedIntersection ({permission} : Grants) ({permission} : Grants)) := by
  intro equal
  rw [mutant_native_empty, Result.ok.injEq] at equal
  have absent : permission ∉ (∅ : Grants) := by simp
  apply absent
  rw [equal]
  simp [expectedIntersection]

end
end NarrowingOverdenialWitness
#print axioms NarrowingOverdenialWitness.mutant_native_empty
#print axioms NarrowingOverdenialWitness.mutant_wrapper_empty
#print axioms NarrowingOverdenialWitness.singleton_permission_lost
#print axioms NarrowingOverdenialWitness.loss_violates_intersection
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``NarrowingOverdenialWitness.mutant_native_empty,
      ``NarrowingOverdenialWitness.mutant_wrapper_empty,
      ``NarrowingOverdenialWitness.singleton_permission_lost,
      ``NarrowingOverdenialWitness.loss_violates_intersection] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed narrowing source-control axiom in {declaration}: {found}"
