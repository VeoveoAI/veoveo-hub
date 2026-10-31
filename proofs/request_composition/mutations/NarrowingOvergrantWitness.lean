import HubCurrentRoles.Funs
import RegistrationIteratorProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub IteratorProof NativeSetProof PolicyPrimitives

-- This standalone witness imports only the raw model and unaffected generic
-- finite-operation proofs. Never import DelegatedPolicyClientProof, a rejected
-- theorem artifact, or the other source-control witness into this package.
-- Source baseline: Hub3bc2923 / SDK239622d / Core120306e, with the exact generic
-- delegation::authority::narrow exclusion from snapshot-narrow-exclude-plan1.
namespace NarrowingOvergrantWitness
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
theorem native_intersection (granted carried : Grants) :
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

/-- Execute the actual source-mutant wrapper. Its existing clone operation and
unchanged native adapter are proved from their operational definitions/laws. -/
theorem mutant_wrapper_returns_granted (info : Delegation) (granted : Grants) :
    delegation.SubjectDelegation.narrow info granted = .ok granted := by
  have cloned := set_clone_exact veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone
    (by intro value; rfl) alloc.alloc.Global.Insts.CoreAllocAllocatorClone granted
  unfold delegation.SubjectDelegation.narrow
  rw [cloned]
  simp only [bind_ok, bind_tc_ok]
  change nativeNarrow granted granted = .ok granted
  simpa only [Finset.inter_self] using native_intersection granted granted

/-- Complete arbitrary delegation payload with only the carried grants changed.
No native authentication or crypto validity claim is made about its other fields. -/
def emptyCarried (base : Delegation) : Delegation := { base with permissions := ∅ }

/-- Singleton input is nonempty; the mutant accepts it although its permission
is absent from the carried set and from the independent intersection. -/
theorem singleton_overgrant (base : Delegation) (permission : Permission) :
    delegation.SubjectDelegation.narrow (emptyCarried base) ({permission} : Grants) =
        .ok ({permission} : Grants) ∧
      permission ∈ ({permission} : Grants) ∧
      permission ∉ expectedIntersection ({permission} : Grants) (emptyCarried base).permissions := by
  refine ⟨mutant_wrapper_returns_granted _ _, ?_, ?_⟩ <;>
    simp [expectedIntersection, emptyCarried]

/-- Actual outer success differs from the unchanged intended intersection. -/
theorem overgrant_violates_intersection (base : Delegation) (permission : Permission) :
    delegation.SubjectDelegation.narrow (emptyCarried base) ({permission} : Grants) ≠
      .ok (expectedIntersection ({permission} : Grants) (emptyCarried base).permissions) := by
  intro equal
  rw [mutant_wrapper_returns_granted, Result.ok.injEq] at equal
  have absent : permission ∉
      expectedIntersection ({permission} : Grants) (emptyCarried base).permissions := by
    simp [expectedIntersection, emptyCarried]
  apply absent
  rw [← equal]
  simp

end
end NarrowingOvergrantWitness
#print axioms NarrowingOvergrantWitness.native_intersection
#print axioms NarrowingOvergrantWitness.mutant_wrapper_returns_granted
#print axioms NarrowingOvergrantWitness.singleton_overgrant
#print axioms NarrowingOvergrantWitness.overgrant_violates_intersection
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``NarrowingOvergrantWitness.native_intersection,
      ``NarrowingOvergrantWitness.mutant_wrapper_returns_granted,
      ``NarrowingOvergrantWitness.singleton_overgrant,
      ``NarrowingOvergrantWitness.overgrant_violates_intersection] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed narrowing source-control axiom in {declaration}: {found}"
