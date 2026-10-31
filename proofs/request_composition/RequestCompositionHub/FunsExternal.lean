-- Handwritten external operational models. Generated policy bodies are untouched.
-- These definitions model native primitives; their Rust refinement is not proved here.
module
public import Aeneas
public import RequestCompositionHub.Types
public import SdkAuthorizationProof
@[expose] public section
open Aeneas Aeneas.Std Result ControlFlow Error
set_option linter.dupNamespace false
set_option linter.hashCommand false
set_option linter.unusedVariables false
set_option linter.style.whitespace false
set_option linter.style.setOption false
set_option linter.style.longLine false
set_option linter.internalConstructors false

/- You can set the `maxHeartbeats` value with the `-max-heartbeats` CLI option -/
set_option maxHeartbeats 1000000

/- You can set the `maxRecDepth` value with the `-max-recdepth` CLI option -/
set_option maxRecDepth 2048
namespace RequestCompositionHub


noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

deriving instance DecidableEq for veoveo_hub_sdk.registration.access.Access
deriving instance DecidableEq for veoveo_hub_sdk.registration.access.Risk

universe u v w

namespace CurrentRolesNative
/- Each scan invokes the supplied actual operation and carries its returned state.
Arbitrary cyclic/failing operations may diverge/fail; finite/lawful inputs will be
separate theorem premises. No fuel bound truncates otherwise valid input. -/
def any {I : Type u} {P : Type v} {T : Type w} (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
    (predicate : Aeneas.Std.core.ops.function.FnMut P T Bool) (state : I) (test : P) :
    Result (Bool × I) := do
  let (entry, next) ← iterator.next state
  match entry with
  | none => .ok (false, next)
  | some value =>
    let (matched, test') ← predicate.call_mut test value
    if matched then .ok (true, next) else any iterator predicate next test'
partial_fixpoint

def filterNext {I : Type u} {P : Type v} {T : Type w} (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
    (predicate : Aeneas.Std.core.ops.function.FnMut P T Bool)
    (state : core.iter.adapters.filter.Filter I P) :
    Result (Option T × core.iter.adapters.filter.Filter I P) := do
  let (entry, next) ← iterator.next state.iter
  match entry with
  | none => .ok (none, { state with iter := next })
  | some value =>
    let (matched, test) ← predicate.call_mut state.predicate value
    let state' := { iter := next, predicate := test : core.iter.adapters.filter.Filter I P }
    if matched then .ok (some value, state') else filterNext iterator predicate state'
partial_fixpoint

def memberBy {T : Type u} {Q : Type v} (borrow : T → Result Q)
    (compare : Q → Q → Result Ordering) (values : List T) (query : Q) : Result Bool := do
  match values with
  | [] => .ok false
  | head :: tail =>
    let key ← borrow head
    let order ← compare key query
    if order == .eq then .ok true else memberBy borrow compare tail query

def subsetBy {T : Type u} (compare : T → T → Result Ordering)
    (left right : List T) : Result Bool := do
  match left with
  | [] => .ok true
  | head :: tail =>
    let member ← memberBy (fun key => .ok key) compare right head
    if member then subsetBy compare tail right else .ok false

def insertBy {T : Type u} (compare : T → T → Result Ordering)
    (set : Finset T) (value : T) : Result (Finset T) := do
  let member ← memberBy (fun key => .ok key) compare set.toList value
  if member then .ok set else .ok (insert value set)

def collectSet {I : Type u} {T : Type v} (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
    (compare : T → T → Result Ordering) (state : I) (set : Finset T) : Result (Finset T) := do
  let (entry, next) ← iterator.next state
  match entry with
  | none => .ok set
  | some value =>
    let set' ← insertBy compare set value
    collectSet iterator compare next set'
partial_fixpoint

def cloneList {T : Type u} (clone : T → Result T) (values : List T) : Result (List T) := do
  match values with
  | [] => .ok []
  | head :: tail =>
    let value ← clone head
    let rest ← cloneList clone tail
    .ok (value :: rest)

def accessRank : veoveo_hub_sdk.registration.access.Access → Nat
  | .Read => 0
  | .Write => 1

def riskRank : veoveo_hub_sdk.registration.access.Risk → Nat
  | .Sensitive => 0
  | .Destructive => 1
  | .Billed => 2
  | .Physical => 3
  | .Administer => 4
end CurrentRolesNative

/-- [core::borrow::{impl core::borrow::Borrow<T> for T}::borrow]:
    Source: '/rustc/library/core/src/borrow.rs', lines 214:4-214:26
    Name pattern: [core::borrow::{core::borrow::Borrow<@T, @T>}::borrow]
    Visibility: public -/
@[rust_fun "core::borrow::{core::borrow::Borrow<@T, @T>}::borrow"]
def core.borrow.Borrow.Blanket.borrow {T : Type _} : T → Result T
  := fun value => .ok value


/-- [core::iter::traits::iterator::Iterator::any]:
    Source: '/rustc/library/core/src/iter/traits/iterator.rs', lines 2897:4-2900:37
    Name pattern: [core::iter::traits::iterator::Iterator::any]
    Visibility: public -/
@[trait_default, rust_fun "core::iter::traits::iterator::Iterator::any"]
def core.iter.traits.iterator.Iterator.any.default
  {Self : Type _} {F : Type _} {Clause0_Item : Type _} (IteratorInst :
  core.iter.traits.iterator.Iterator Self Clause0_Item)
  (opsfunctionFnMutFTupleClause0_ItemBoolInst : core.ops.function.FnMut F
  Clause0_Item Bool) :
  Self → F → Result (Bool × Self)
  := CurrentRolesNative.any IteratorInst opsfunctionFnMutFTupleClause0_ItemBoolInst


/-- [core::iter::traits::iterator::Iterator::filter]:
    Source: '/rustc/library/core/src/iter/traits/iterator.rs', lines 966:4-969:38
    Name pattern: [core::iter::traits::iterator::Iterator::filter]
    Visibility: public -/
@[trait_default, rust_fun "core::iter::traits::iterator::Iterator::filter"]
def core.iter.traits.iterator.Iterator.filter.default
  {Self : Type _} {P : Type _} {Clause0_Item : Type _} (IteratorInst :
  core.iter.traits.iterator.Iterator Self Clause0_Item)
  (opsfunctionFnMutPTupleSharedClause0_ItemBoolInst : core.ops.function.FnMut P
  Clause0_Item Bool) :
  Self → P → Result (core.iter.adapters.filter.Filter Self P)
  := fun iterator predicate => .ok { iter := iterator, predicate := predicate }


/-- [core::iter::traits::iterator::Iterator::map]:
    Source: '/rustc/library/core/src/iter/traits/iterator.rs', lines 845:4-848:34
    Name pattern: [core::iter::traits::iterator::Iterator::map]
    Visibility: public -/
@[trait_default, rust_fun "core::iter::traits::iterator::Iterator::map"]
def core.iter.traits.iterator.Iterator.map.default
  {Self : Type _} {B : Type _} {F : Type _} {Clause0_Item : Type _}
  (IteratorInst : core.iter.traits.iterator.Iterator Self Clause0_Item)
  (opsfunctionFnMutFTupleClause0_ItemBInst : core.ops.function.FnMut F
  Clause0_Item B) :
  Self → F → Result (core.iter.adapters.map.Map Self F)
  := fun iterator mapper => .ok { iter := iterator, f := mapper }


/-- [core::iter::adapters::filter::{impl core::iter::traits::iterator::Iterator<Clause0_Item> for core::iter::adapters::filter::Filter<I, P>}::next]:
    Source: '/rustc/library/core/src/iter/adapters/filter.rs', lines 97:4-97:41
    Name pattern: [core::iter::adapters::filter::{core::iter::traits::iterator::Iterator<core::iter::adapters::filter::Filter<@I, @P>, @Clause0_Item>}::next]
    Visibility: public -/
@[rust_fun
  "core::iter::adapters::filter::{core::iter::traits::iterator::Iterator<core::iter::adapters::filter::Filter<@I, @P>, @Clause0_Item>}::next"]
def
  core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator.next
  {I : Type _} {P : Type _} {Clause0_Item : Type _} (traitsiteratorIteratorInst
  : core.iter.traits.iterator.Iterator I Clause0_Item)
  (opsfunctionFnMutPTupleShared0Clause0_ItemBoolInst : core.ops.function.FnMut
  P Clause0_Item Bool) :
  core.iter.adapters.filter.Filter I P → Result ((Option Clause0_Item) ×
    (core.iter.adapters.filter.Filter I P))
  := CurrentRolesNative.filterNext traitsiteratorIteratorInst opsfunctionFnMutPTupleShared0Clause0_ItemBoolInst


/-- [core::iter::adapters::map::{impl core::iter::traits::iterator::Iterator<B> for core::iter::adapters::map::Map<I, F>}::next]:
    Source: '/rustc/library/core/src/iter/adapters/map.rs', lines 106:4-106:35
    Name pattern: [core::iter::adapters::map::{core::iter::traits::iterator::Iterator<core::iter::adapters::map::Map<@I, @F>, @B>}::next]
    Visibility: public -/
@[rust_fun
  "core::iter::adapters::map::{core::iter::traits::iterator::Iterator<core::iter::adapters::map::Map<@I, @F>, @B>}::next"]
def core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator.next
  {B : Type _} {I : Type _} {F : Type _} {Clause0_Item : Type _}
  (traitsiteratorIteratorInst : core.iter.traits.iterator.Iterator I
  Clause0_Item) (opsfunctionFnMutFTupleClause0_ItemBInst :
  core.ops.function.FnMut F Clause0_Item B) :
  core.iter.adapters.map.Map I F → Result ((Option B) ×
    (core.iter.adapters.map.Map I F))
  := fun state => do
  let (entry, next) ← traitsiteratorIteratorInst.next state.iter
  match entry with
  | none => .ok (none, { state with iter := next })
  | some value =>
    let (mapped, mapper) ← opsfunctionFnMutFTupleClause0_ItemBInst.call_mut state.f value
    .ok (some mapped, { iter := next, f := mapper })


/-- [core::option::{core::option::Option<T>}::is_none_or]:
    Source: '/rustc/library/core/src/option.rs', lines 708:4-708:95
    Name pattern: [core::option::{core::option::Option<@T>}::is_none_or]
    Visibility: public -/
@[rust_fun "core::option::{core::option::Option<@T>}::is_none_or"]
def core.option.Option.is_none_or
  {T : Type _} {T1 : Type _} (opsfunctionFnOnceT1TupleTBoolInst :
  core.ops.function.FnOnce T1 T Bool) :
  Option T → T1 → Result Bool
  := fun option predicate => do
  match option with
  | none => .ok true
  | some value => opsfunctionFnOnceT1TupleTBoolInst.call_once predicate value


/-- [core::option::{core::option::Option<T>}::as_ref]:
    Source: '/rustc/library/core/src/option.rs', lines 743:4-743:44
    Name pattern: [core::option::{core::option::Option<@T>}::as_ref]
    Visibility: public -/
@[rust_fun "core::option::{core::option::Option<@T>}::as_ref"]
def core.option.Option.as_ref {T : Type _} : Option T → Result (Option T)
  := fun option => .ok option


/-- [core::option::{impl core::cmp::PartialEq<core::option::Option<T>> for core::option::Option<T>}::eq]:
    Source: '/rustc/library/core/src/option.rs', lines 2441:4-2441:38
    Name pattern: [core::option::{core::cmp::PartialEq<core::option::Option<@T>, core::option::Option<@T>>}::eq]
    Visibility: public -/
@[rust_fun
  "core::option::{core::cmp::PartialEq<core::option::Option<@T>, core::option::Option<@T>>}::eq"]
def core.option.Option.Insts.CoreCmpPartialEqOption.eq
  {T : Type _} (cmpPartialEqInst : core.cmp.PartialEq T T) :
  Option T → Option T → Result Bool
  := fun left right =>
  match left with
  | none => match right with
    | none => .ok true
    | some _ => .ok false
  | some left => match right with
    | none => .ok false
    | some right => cmpPartialEqInst.eq left right


/-- [core::slice::iter::{impl core::iter::traits::iterator::Iterator<&'a T> for core::slice::iter::Iter<'a, T>}::any]:
    Source: '/rustc/library/core/src/slice/iter/macros.rs', lines 321:12-324:45; this code is generated from a macro invocation at: '/rustc/library/core/src/slice/iter.rs', lines 140:0-148:2
    Name pattern: [core::slice::iter::{core::iter::traits::iterator::Iterator<core::slice::iter::Iter<'a, @T>, &'a @T>}::any]
    Visibility: public -/
@[rust_fun
  "core::slice::iter::{core::iter::traits::iterator::Iterator<core::slice::iter::Iter<'a, @T>, &'a @T>}::any"]
def core.slice.iter.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.any
  {T : Type _} {F : Type _} (opsfunctionFnMutFTupleSharedATBoolInst :
  core.ops.function.FnMut F T Bool) :
  core.slice.iter.Iter T → F → Result (Bool × (core.slice.iter.Iter T))
  := CurrentRolesNative.any (Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter T) opsfunctionFnMutFTupleSharedATBoolInst


/-- [alloc::collections::btree::set::{impl core::clone::Clone for alloc::collections::btree::set::BTreeSet<T, A>}::clone]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 118:4-118:27
    Name pattern: [alloc::collections::btree::set::{core::clone::Clone<alloc::collections::btree::set::BTreeSet<@T, @A>>}::clone]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::clone::Clone<alloc::collections::btree::set::BTreeSet<@T, @A>>}::clone"]
def alloc.collections.btree.set.BTreeSet.Insts.CoreCloneClone.clone
  {T : Type _} {A : Type _} (corecloneCloneInst : core.clone.Clone T)
  (coreallocAllocatorCloneInst : core.alloc.AllocatorClone A) :
  alloc.collections.btree.set.BTreeSet T A → Result
    (alloc.collections.btree.set.BTreeSet T A)
  := fun set => do
  let values ← CurrentRolesNative.cloneList corecloneCloneInst.clone set.toList
  .ok values.toFinset


/-- [alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<T, A>}::contains]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 606:4-609:15
    Name pattern: [alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, @A>}::contains]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, @A>}::contains"]
def alloc.collections.btree.set.BTreeSet.contains
  {T : Type _} {A : Type _} {Q : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) (coreborrowBorrowInst : core.borrow.Borrow T Q)
  (corecmpOrdInst : core.cmp.Ord T) (corecmpOrdInst1 : core.cmp.Ord Q) :
  alloc.collections.btree.set.BTreeSet T A → Q → Result Bool
  := fun set query => CurrentRolesNative.memberBy coreborrowBorrowInst.borrow corecmpOrdInst1.cmp set.toList query


/-- [alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<T, A>}::is_subset]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 684:4-686:15
    Name pattern: [alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, @A>}::is_subset]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, @A>}::is_subset"]
def alloc.collections.btree.set.BTreeSet.is_subset
  {T : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) (corecmpOrdInst : core.cmp.Ord T) :
  alloc.collections.btree.set.BTreeSet T A →
    alloc.collections.btree.set.BTreeSet T A → Result Bool
  := fun left right => CurrentRolesNative.subsetBy corecmpOrdInst.cmp left.toList right.toList


/-- [alloc::collections::btree::set::{impl core::iter::traits::collect::FromIterator<T> for alloc::collections::btree::set::BTreeSet<T, alloc::alloc::Global>}::from_iter]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1471:4-1471:67
    Name pattern: [alloc::collections::btree::set::{core::iter::traits::collect::FromIterator<alloc::collections::btree::set::BTreeSet<@T, alloc::alloc::Global>, @T>}::from_iter]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::iter::traits::collect::FromIterator<alloc::collections::btree::set::BTreeSet<@T, alloc::alloc::Global>, @T>}::from_iter"]
def
  alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator.from_iter
  {T : Type _} {I : Type _} {Clause1_IntoIter : Type _} (corecmpOrdInst :
  core.cmp.Ord T) (coreitertraitscollectIntoIteratorInst :
  core.iter.traits.collect.IntoIterator I T Clause1_IntoIter) :
  I → Result (alloc.collections.btree.set.BTreeSet T alloc.alloc.Global)
  := fun input => do
  let iterator ← coreitertraitscollectIntoIteratorInst.into_iter input
  CurrentRolesNative.collectSet coreitertraitscollectIntoIteratorInst.iteratorInst corecmpOrdInst.cmp iterator ∅


/-- [alloc::collections::btree::set::{impl core::iter::traits::iterator::Iterator<&'a T> for alloc::collections::btree::set::Iter<'a, T>}::next]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1765:4-1765:39
    Name pattern: [alloc::collections::btree::set::{core::iter::traits::iterator::Iterator<alloc::collections::btree::set::Iter<'a, @T>, &'a @T>}::next]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::iter::traits::iterator::Iterator<alloc::collections::btree::set::Iter<'a, @T>, &'a @T>}::next"]
def
  alloc.collections.btree.set.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.next
  {T : Type _} :
  alloc.collections.btree.set.Iter T → Result ((Option T) ×
    (alloc.collections.btree.set.Iter T))
  := fun values => .ok (match values with
  | [] => (none, [])
  | head :: tail => (some head, tail))


/-- [alloc::collections::btree::set::{impl core::iter::traits::collect::IntoIterator<&'a T, alloc::collections::btree::set::Iter<'a, T>> for &'a alloc::collections::btree::set::BTreeSet<T, A>}::into_iter]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1546:4-1546:37
    Name pattern: [alloc::collections::btree::set::{core::iter::traits::collect::IntoIterator<&'a alloc::collections::btree::set::BTreeSet<@T, @A>, &'a @T, alloc::collections::btree::set::Iter<'a, @T>>}::into_iter]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::iter::traits::collect::IntoIterator<&'a alloc::collections::btree::set::BTreeSet<@T, @A>, &'a @T, alloc::collections::btree::set::Iter<'a, @T>>}::into_iter"]
def
  SharedABTreeSet.Insts.CoreIterTraitsCollectIntoIteratorSharedATIter.into_iter
  {T : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.set.BTreeSet T A → Result
    (alloc.collections.btree.set.Iter T)
  := fun set => .ok set.toList


/-- [alloc::collections::btree::set::{impl core::iter::traits::collect::Extend<&'a T> for alloc::collections::btree::set::BTreeSet<T, A>}::extend]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1629:4-1629:64
    Name pattern: [alloc::collections::btree::set::{core::iter::traits::collect::Extend<alloc::collections::btree::set::BTreeSet<@T, @A>, &'a @T>}::extend]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::iter::traits::collect::Extend<alloc::collections::btree::set::BTreeSet<@T, @A>, &'a @T>}::extend"]
def
  alloc.collections.btree.set.BTreeSet.Insts.CoreIterTraitsCollectExtendSharedAT.extend
  {T : Type _} {A : Type _} {I : Type _} {Clause3_IntoIter : Type _}
  (corecmpOrdInst : core.cmp.Ord T) (coremarkerCopyInst : core.marker.Copy T)
  (coreallocAllocatorCloneInst : core.alloc.AllocatorClone A)
  (coreitertraitscollectIntoIteratorISharedATClause3_IntoIterInst :
  core.iter.traits.collect.IntoIterator I T Clause3_IntoIter) :
  alloc.collections.btree.set.BTreeSet T A → I → Result
    (alloc.collections.btree.set.BTreeSet T A)
  := fun set input => do
  let iterator ← coreitertraitscollectIntoIteratorISharedATClause3_IntoIterInst.into_iter input
  CurrentRolesNative.collectSet coreitertraitscollectIntoIteratorISharedATClause3_IntoIterInst.iteratorInst corecmpOrdInst.cmp iterator set


/-- [alloc::vec::{impl core::iter::traits::collect::IntoIterator<&'a T, core::slice::iter::Iter<'a, T>> for &'a alloc::vec::Vec<T>}::into_iter]:
    Source: '/rustc/library/alloc/src/vec/mod.rs', lines 4088:4-4088:40
    Name pattern: [alloc::vec::{core::iter::traits::collect::IntoIterator<&'a alloc::vec::Vec<@T>, &'a @T, core::slice::iter::Iter<'a, @T>>}::into_iter]
    Visibility: public -/
@[rust_fun
  "alloc::vec::{core::iter::traits::collect::IntoIterator<&'a alloc::vec::Vec<@T>, &'a @T, core::slice::iter::Iter<'a, @T>>}::into_iter"]
def SharedAVec.Insts.CoreIterTraitsCollectIntoIteratorSharedATIter.into_iter
  {T : Type _} (A : Type _) :
  alloc.vec.Vec T → Result (core.slice.iter.Iter T)
  := fun vector => Aeneas.Std.core.slice.Slice.iter (Aeneas.Std.alloc.vec.Vec.deref vector)


/-- [veoveo_hub_sdk::registration::access::{impl core::cmp::PartialEq<veoveo_hub_sdk::registration::access::Access> for veoveo_hub_sdk::registration::access::Access}::eq]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 4:29-4:38
    Name pattern: [veoveo_hub_sdk::registration::access::{core::cmp::PartialEq<veoveo_hub_sdk::registration::access::Access, veoveo_hub_sdk::registration::access::Access>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::cmp::PartialEq<veoveo_hub_sdk::registration::access::Access, veoveo_hub_sdk::registration::access::Access>}::eq"]
def veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpPartialEqAccess.eq
  :
  veoveo_hub_sdk.registration.access.Access →
    veoveo_hub_sdk.registration.access.Access → Result Bool
  := fun left right => .ok (decide (CurrentRolesNative.accessRank left = CurrentRolesNative.accessRank right))


/-- [veoveo_hub_sdk::registration::access::{impl core::cmp::PartialOrd<veoveo_hub_sdk::registration::access::Access> for veoveo_hub_sdk::registration::access::Access}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 4:44-4:54
    Name pattern: [veoveo_hub_sdk::registration::access::{core::cmp::PartialOrd<veoveo_hub_sdk::registration::access::Access, veoveo_hub_sdk::registration::access::Access>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::cmp::PartialOrd<veoveo_hub_sdk::registration::access::Access, veoveo_hub_sdk::registration::access::Access>}::partial_cmp"]
def
  veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpPartialOrdAccess.partial_cmp
  :
  veoveo_hub_sdk.registration.access.Access →
    veoveo_hub_sdk.registration.access.Access → Result (Option Ordering)
  := fun left right => .ok (some (compare (CurrentRolesNative.accessRank left) (CurrentRolesNative.accessRank right)))


/-- [veoveo_hub_sdk::registration::access::{impl core::cmp::Ord for veoveo_hub_sdk::registration::access::Access}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 4:56-4:59
    Name pattern: [veoveo_hub_sdk::registration::access::{core::cmp::Ord<veoveo_hub_sdk::registration::access::Access>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::cmp::Ord<veoveo_hub_sdk::registration::access::Access>}::cmp"]
def veoveo_hub_sdk.registration.access.Access.Insts.CoreCmpOrd.cmp
  :
  veoveo_hub_sdk.registration.access.Access →
    veoveo_hub_sdk.registration.access.Access → Result Ordering
  := fun left right => .ok (compare (CurrentRolesNative.accessRank left) (CurrentRolesNative.accessRank right))


/-- [veoveo_hub_sdk::registration::access::{impl core::clone::Clone for veoveo_hub_sdk::registration::access::Risk}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 12:16-12:21
    Name pattern: [veoveo_hub_sdk::registration::access::{core::clone::Clone<veoveo_hub_sdk::registration::access::Risk>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::clone::Clone<veoveo_hub_sdk::registration::access::Risk>}::clone"]
def veoveo_hub_sdk.registration.access.Risk.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.registration.access.Risk → Result
    veoveo_hub_sdk.registration.access.Risk
  := fun value => .ok value


/-- [veoveo_hub_sdk::registration::access::{impl core::cmp::PartialEq<veoveo_hub_sdk::registration::access::Risk> for veoveo_hub_sdk::registration::access::Risk}::eq]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 12:29-12:38
    Name pattern: [veoveo_hub_sdk::registration::access::{core::cmp::PartialEq<veoveo_hub_sdk::registration::access::Risk, veoveo_hub_sdk::registration::access::Risk>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::cmp::PartialEq<veoveo_hub_sdk::registration::access::Risk, veoveo_hub_sdk::registration::access::Risk>}::eq"]
def veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpPartialEqRisk.eq
  :
  veoveo_hub_sdk.registration.access.Risk →
    veoveo_hub_sdk.registration.access.Risk → Result Bool
  := fun left right => .ok (decide (CurrentRolesNative.riskRank left = CurrentRolesNative.riskRank right))


/-- [veoveo_hub_sdk::registration::access::{impl core::cmp::PartialOrd<veoveo_hub_sdk::registration::access::Risk> for veoveo_hub_sdk::registration::access::Risk}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 12:44-12:54
    Name pattern: [veoveo_hub_sdk::registration::access::{core::cmp::PartialOrd<veoveo_hub_sdk::registration::access::Risk, veoveo_hub_sdk::registration::access::Risk>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::cmp::PartialOrd<veoveo_hub_sdk::registration::access::Risk, veoveo_hub_sdk::registration::access::Risk>}::partial_cmp"]
def
  veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpPartialOrdRisk.partial_cmp
  :
  veoveo_hub_sdk.registration.access.Risk →
    veoveo_hub_sdk.registration.access.Risk → Result (Option Ordering)
  := fun left right => .ok (some (compare (CurrentRolesNative.riskRank left) (CurrentRolesNative.riskRank right)))


/-- [veoveo_hub_sdk::registration::access::{impl core::cmp::Ord for veoveo_hub_sdk::registration::access::Risk}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/access.rs', lines 12:56-12:59
    Name pattern: [veoveo_hub_sdk::registration::access::{core::cmp::Ord<veoveo_hub_sdk::registration::access::Risk>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::access::{core::cmp::Ord<veoveo_hub_sdk::registration::access::Risk>}::cmp"]
def veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpOrd.cmp
  :
  veoveo_hub_sdk.registration.access.Risk →
    veoveo_hub_sdk.registration.access.Risk → Result Ordering
  := fun left right => .ok (compare (CurrentRolesNative.riskRank left) (CurrentRolesNative.riskRank right))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::PrincipalId> for veoveo_types::identity::PrincipalId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 9:0-9:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::PrincipalId, veoveo_types::identity::PrincipalId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::PrincipalId, veoveo_types::identity::PrincipalId>}::eq"]
def veoveo_types.identity.PrincipalId.Insts.CoreCmpPartialEqPrincipalId.eq
  :
  veoveo_types.identity.PrincipalId → veoveo_types.identity.PrincipalId →
    Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::TenantId> for veoveo_types::identity::TenantId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 13:0-13:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::TenantId, veoveo_types::identity::TenantId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::TenantId, veoveo_types::identity::TenantId>}::eq"]
def veoveo_types.identity.TenantId.Insts.CoreCmpPartialEqTenantId.eq
  :
  veoveo_types.identity.TenantId → veoveo_types.identity.TenantId → Result
    Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::cmp::Ord for veoveo_types::identity::GroupId}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 25:0-25:69
    Name pattern: [veoveo_types::identity::{core::cmp::Ord<veoveo_types::identity::GroupId>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::Ord<veoveo_types::identity::GroupId>}::cmp"]
def veoveo_types.identity.GroupId.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.identity.GroupId → veoveo_types.identity.GroupId → Result
    Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_types::identity::{impl core::cmp::PartialOrd<veoveo_types::identity::GroupId> for veoveo_types::identity::GroupId}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 25:0-25:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialOrd<veoveo_types::identity::GroupId, veoveo_types::identity::GroupId>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialOrd<veoveo_types::identity::GroupId, veoveo_types::identity::GroupId>}::partial_cmp"]
def veoveo_types.identity.GroupId.Insts.CoreCmpPartialOrdGroupId.partial_cmp
  :
  veoveo_types.identity.GroupId → veoveo_types.identity.GroupId → Result
    (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::GroupId> for veoveo_types::identity::GroupId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 25:0-25:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::GroupId, veoveo_types::identity::GroupId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::GroupId, veoveo_types::identity::GroupId>}::eq"]
def veoveo_types.identity.GroupId.Insts.CoreCmpPartialEqGroupId.eq
  :
  veoveo_types.identity.GroupId → veoveo_types.identity.GroupId → Result
    Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::cmp::Ord for veoveo_types::identity::RoleId}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 29:0-29:69
    Name pattern: [veoveo_types::identity::{core::cmp::Ord<veoveo_types::identity::RoleId>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::Ord<veoveo_types::identity::RoleId>}::cmp"]
def veoveo_types.identity.RoleId.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.identity.RoleId → veoveo_types.identity.RoleId → Result
    Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_types::identity::{impl core::cmp::PartialOrd<veoveo_types::identity::RoleId> for veoveo_types::identity::RoleId}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 29:0-29:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialOrd<veoveo_types::identity::RoleId, veoveo_types::identity::RoleId>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialOrd<veoveo_types::identity::RoleId, veoveo_types::identity::RoleId>}::partial_cmp"]
def veoveo_types.identity.RoleId.Insts.CoreCmpPartialOrdRoleId.partial_cmp
  :
  veoveo_types.identity.RoleId → veoveo_types.identity.RoleId → Result
    (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::RoleId> for veoveo_types::identity::RoleId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 29:0-29:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::RoleId, veoveo_types::identity::RoleId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::RoleId, veoveo_types::identity::RoleId>}::eq"]
def veoveo_types.identity.RoleId.Insts.CoreCmpPartialEqRoleId.eq
  :
  veoveo_types.identity.RoleId → veoveo_types.identity.RoleId → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::clone::Clone for veoveo_types::identity::RoleId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 29:0-29:69
    Name pattern: [veoveo_types::identity::{core::clone::Clone<veoveo_types::identity::RoleId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::clone::Clone<veoveo_types::identity::RoleId>}::clone"]
def veoveo_types.identity.RoleId.Insts.CoreCloneClone.clone
  : veoveo_types.identity.RoleId → Result veoveo_types.identity.RoleId
  := fun value => .ok value


/-- [veoveo_types::permission_names::{impl core::cmp::PartialEq<veoveo_types::permission_names::PermissionName> for veoveo_types::permission_names::PermissionName}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/permission_names.rs', lines 6:0-6:42
    Name pattern: [veoveo_types::permission_names::{core::cmp::PartialEq<veoveo_types::permission_names::PermissionName, veoveo_types::permission_names::PermissionName>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::permission_names::{core::cmp::PartialEq<veoveo_types::permission_names::PermissionName, veoveo_types::permission_names::PermissionName>}::eq"]
def
  veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialEqPermissionName.eq
  :
  veoveo_types.permission_names.PermissionName →
    veoveo_types.permission_names.PermissionName → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::permission_names::{impl core::cmp::Ord for veoveo_types::permission_names::PermissionName}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/permission_names.rs', lines 6:0-6:42
    Name pattern: [veoveo_types::permission_names::{core::cmp::Ord<veoveo_types::permission_names::PermissionName>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::permission_names::{core::cmp::Ord<veoveo_types::permission_names::PermissionName>}::cmp"]
def veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.permission_names.PermissionName →
    veoveo_types.permission_names.PermissionName → Result Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_types::permission_names::{impl core::cmp::PartialOrd<veoveo_types::permission_names::PermissionName> for veoveo_types::permission_names::PermissionName}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/permission_names.rs', lines 6:0-6:42
    Name pattern: [veoveo_types::permission_names::{core::cmp::PartialOrd<veoveo_types::permission_names::PermissionName, veoveo_types::permission_names::PermissionName>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::permission_names::{core::cmp::PartialOrd<veoveo_types::permission_names::PermissionName, veoveo_types::permission_names::PermissionName>}::partial_cmp"]
def
  veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialOrdPermissionName.partial_cmp
  :
  veoveo_types.permission_names.PermissionName →
    veoveo_types.permission_names.PermissionName → Result (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_types::permission_names::{impl core::clone::Clone for veoveo_types::permission_names::PermissionName}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/permission_names.rs', lines 6:0-6:42
    Name pattern: [veoveo_types::permission_names::{core::clone::Clone<veoveo_types::permission_names::PermissionName>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::permission_names::{core::clone::Clone<veoveo_types::permission_names::PermissionName>}::clone"]
def veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone.clone
  :
  veoveo_types.permission_names.PermissionName → Result
    veoveo_types.permission_names.PermissionName
  := fun value => .ok value



/- Forward-only FlatMap operational state. Each step invokes actual next/FnMut/
IntoIterator methods and preserves their returned state. No arbitrary fuel or
precomputed flattened result is used; totality is a later finite-cursor theorem.
`exhausted` models the default native Fuse, preventing resurrection after outer
None. The native FusedIterator specialization can still call next after None;
correspondence for the concrete stable terminal map/list cursor must be derived,
not assumed for arbitrary stateful FusedIterator implementations. Post-terminal
native call-count traces and unused double-ended operations are not modeled. -/
universe uI uU uF uOuter uItem uInner
namespace CurrentRolesNative

def flatMapNext {I : Type uI} {U : Type uU} {F : Type uF}
    {Outer : Type uOuter} {Item : Type uItem} {Inner : Type uInner}
    (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I Outer)
    (into : Aeneas.Std.core.iter.traits.collect.IntoIterator U Item Inner)
    (mapper : Aeneas.Std.core.ops.function.FnMut F Outer U)
    (state : core.iter.adapters.flatten.FlatMap I U F Item Inner) :
    Result (Option Item × core.iter.adapters.flatten.FlatMap I U F Item Inner) := do
  if state.exhausted then .ok (none, state) else
  match state.inner with
  | some inner =>
    let (entry, next) ← into.iteratorInst.next inner
    match entry with
    | some value => .ok (some value, { state with inner := some next })
    | none => flatMapNext iterator into mapper { state with inner := none }
  | none =>
    let (entry, next) ← iterator.next state.iter
    match entry with
    | none => .ok (none, { state with iter := next, exhausted := true })
    | some value =>
      let (mapped, mapper') ← mapper.call_mut state.mapper value
      let inner ← into.into_iter mapped
      flatMapNext iterator into mapper { state with iter := next, mapper := mapper', inner := some inner }
partial_fixpoint

/- Native Arc::as_ref VALUE projection. Its Rust reference/alias/lifetime
refinement and the single forwarding closure are explicit trusted adapters. -/
def arcAsRef {T : Type u} (value : alloc.sync.Arc T) : Result T := .ok value

end CurrentRolesNative

/-- [core::iter::traits::iterator::Iterator::flat_map]:
    Source: '/rustc/library/core/src/iter/traits/iterator.rs', lines 1547:4-1551:34
    Name pattern: [core::iter::traits::iterator::Iterator::flat_map]
    Visibility: public -/
@[trait_default, rust_fun "core::iter::traits::iterator::Iterator::flat_map"]
def core.iter.traits.iterator.Iterator.flat_map.default
  {Self : Type _} {U : Type _} {F : Type _} {Clause0_Item : Type _}
  {Clause1_Item : Type _} {Clause1_IntoIter : Type _} (IteratorInst :
  core.iter.traits.iterator.Iterator Self Clause0_Item)
  (collectIntoIteratorInst : core.iter.traits.collect.IntoIterator U
  Clause1_Item Clause1_IntoIter) (opsfunctionFnMutFTupleClause0_ItemUInst :
  core.ops.function.FnMut F Clause0_Item U) :
  Self → F → Result (core.iter.adapters.flatten.FlatMap Self U F
    Clause1_Item Clause1_IntoIter)
  := fun state mapper => .ok { iter := state, mapper := mapper, inner := none, exhausted := false }

/-- [core::iter::adapters::flatten::{impl core::iter::traits::iterator::Iterator<Clause1_Item> for core::iter::adapters::flatten::FlatMap<I, U, F, Clause1_Item, Clause1_IntoIter>}::next]:
    Source: '/rustc/library/core/src/iter/adapters/flatten.rs', lines 63:4-63:41
    Name pattern: [core::iter::adapters::flatten::{core::iter::traits::iterator::Iterator<core::iter::adapters::flatten::FlatMap<@I, @U, @F, @Clause1_Item, @Clause1_IntoIter>, @Clause1_Item>}::next]
    Visibility: public -/
@[rust_fun
  "core::iter::adapters::flatten::{core::iter::traits::iterator::Iterator<core::iter::adapters::flatten::FlatMap<@I, @U, @F, @Clause1_Item, @Clause1_IntoIter>, @Clause1_Item>}::next"]
def
  core.iter.adapters.flatten.FlatMap.Insts.CoreIterTraitsIteratorIterator.next
  {I : Type _} {U : Type _} {F : Type _} {Clause0_Item : Type _} {Clause1_Item
  : Type _} {Clause1_IntoIter : Type _} (traitsiteratorIteratorInst :
  core.iter.traits.iterator.Iterator I Clause0_Item)
  (traitscollectIntoIteratorInst : core.iter.traits.collect.IntoIterator U
  Clause1_Item Clause1_IntoIter) (opsfunctionFnMutFTupleClause0_ItemUInst :
  core.ops.function.FnMut F Clause0_Item U) :
  core.iter.adapters.flatten.FlatMap I U F Clause1_Item Clause1_IntoIter →
    Result ((Option Clause1_Item) × (core.iter.adapters.flatten.FlatMap I U F
    Clause1_Item Clause1_IntoIter))
  := CurrentRolesNative.flatMapNext traitsiteratorIteratorInst
    traitscollectIntoIteratorInst opsfunctionFnMutFTupleClause0_ItemUInst

/-- [alloc::collections::btree::map::{impl core::iter::traits::iterator::Iterator<&'a V> for alloc::collections::btree::map::Values<'a, K, V>}::next]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 2080:4-2080:39
    Name pattern: [alloc::collections::btree::map::{core::iter::traits::iterator::Iterator<alloc::collections::btree::map::Values<'a, @K, @V>, &'a @V>}::next]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{core::iter::traits::iterator::Iterator<alloc::collections::btree::map::Values<'a, @K, @V>, &'a @V>}::next"]
def
  alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV.next
  {K : Type _} {V : Type _} :
  alloc.collections.btree.map.Values K V → Result ((Option V) ×
    (alloc.collections.btree.map.Values K V))
  := fun values => .ok (match values with
  | [] => (none, [])
  | head :: tail => (some head, tail))

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::values]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 2786:4-2786:44
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::values]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::values"]
def alloc.collections.btree.map.BTreeMap.values
  {K : Type _} {V : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.map.BTreeMap K V A → Result
    (alloc.collections.btree.map.Values K V)
  := fun entries => .ok (entries.map Prod.snd)

/-- [alloc::collections::btree::set::{impl core::iter::traits::iterator::Iterator<T> for alloc::collections::btree::set::IntoIter<T, A>}::next]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1816:4-1816:35
    Name pattern: [alloc::collections::btree::set::{core::iter::traits::iterator::Iterator<alloc::collections::btree::set::IntoIter<@T, @A>, @T>}::next]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::iter::traits::iterator::Iterator<alloc::collections::btree::set::IntoIter<@T, @A>, @T>}::next"]
def
  alloc.collections.btree.set.IntoIter.Insts.CoreIterTraitsIteratorIterator.next
  {T : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.set.IntoIter T A → Result ((Option T) ×
    (alloc.collections.btree.set.IntoIter T A))
  := fun values => .ok (match values with
  | [] => (none, [])
  | head :: tail => (some head, tail))

/-- [alloc::collections::btree::set::{impl core::iter::traits::collect::IntoIterator<T, alloc::collections::btree::set::IntoIter<T, A>> for alloc::collections::btree::set::BTreeSet<T, A>}::into_iter]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1536:4-1536:40
    Name pattern: [alloc::collections::btree::set::{core::iter::traits::collect::IntoIterator<alloc::collections::btree::set::BTreeSet<@T, @A>, @T, alloc::collections::btree::set::IntoIter<@T, @A>>}::into_iter]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{core::iter::traits::collect::IntoIterator<alloc::collections::btree::set::BTreeSet<@T, @A>, @T, alloc::collections::btree::set::IntoIter<@T, @A>>}::into_iter"]
def
  alloc.collections.btree.set.BTreeSet.Insts.CoreIterTraitsCollectIntoIteratorTIntoIter.into_iter
  {T : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.set.BTreeSet T A → Result
    (alloc.collections.btree.set.IntoIter T A)
  := fun set => .ok set.toList

/-- [alloc::sync::{impl core::ops::deref::Deref<T> for alloc::sync::Arc<T>}::deref]:
    Source: '/rustc/library/alloc/src/sync.rs', lines 2570:4-2570:25
    Name pattern: [alloc::sync::{core::ops::deref::Deref<alloc::sync::Arc<@T>, @T>}::deref]
    Visibility: public -/
@[rust_fun
  "alloc::sync::{core::ops::deref::Deref<alloc::sync::Arc<@T>, @T>}::deref"]
def alloc.sync.Arc.Insts.CoreOpsDerefDeref.deref
  {T : Type _} (A : Type _) : alloc.sync.Arc T → Result T
  := fun value => .ok value

/-- [veoveo_hub::registration::snapshot::{veoveo_hub::registration::snapshot::RegistrationSnapshot}::iter::{impl core::ops::function::FnMut<(&'_0 alloc::sync::Arc<veoveo_hub::registration::snapshot::RegisteredServer>,), &'_ veoveo_hub::registration::snapshot::RegisteredServer> for veoveo_hub::registration::snapshot::{veoveo_hub::registration::snapshot::RegistrationSnapshot}::iter::{closure}}::call_mut]:
    Source: 'crates/veoveo-hub/src/registration/snapshot.rs', lines 160:34-160:58 -/
def
  registration.snapshot.RegistrationSnapshot.iter.closure.Insts.CoreOpsFunctionFnMutTupleShared0ArcRegisteredServerSharedRegisteredServer.call_mut
  :
  registration.snapshot.RegistrationSnapshot.iter.closure → alloc.sync.Arc
    registration.snapshot.RegisteredServer → Result
    (registration.snapshot.RegisteredServer ×
    registration.snapshot.RegistrationSnapshot.iter.closure)
  := fun state value => do
    let server ← CurrentRolesNative.arcAsRef value
    .ok (server, state)

/-- Empty native set constructor, freshly reached by the isolated overdenial
control. This value model does not claim allocator or native representation behavior. -/
@[rust_fun
  "alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, alloc::alloc::Global>}::new"]
def alloc.collections.btree.set.BTreeSetTGlobal.new
    (T : Type _) : Result (alloc.collections.btree.set.BTreeSet T alloc.alloc.Global) :=
  .ok ∅

/- Conditional policy-decision extension. Existing primitive models above are
unchanged. The only new native operation assumptions are explicitly marked.
Debug formatting is inert; no formatting trace is claimed, and closed kernel
action spelling must prove parse success before expect. -/

/-- [core::convert::{impl core::convert::AsRef<U> for &'_0 T}::as_ref]:
    Source: '/rustc/library/core/src/convert/mod.rs', lines 719:4-719:26
    Name pattern: [core::convert::{core::convert::AsRef<&'0 @T, @U>}::as_ref]
    Visibility: public -/
@[rust_fun "core::convert::{core::convert::AsRef<&'0 @T, @U>}::as_ref"]
def Shared0T.Insts.CoreConvertAsRef.as_ref
  {T : Type _} {U : Type _} (AsRefInst : core.convert.AsRef T U) :
  T → Result U
  := fun value => AsRefInst.as_ref value

/-- [core::convert::{impl core::convert::AsRef<str> for str}::as_ref]:
    Source: '/rustc/library/core/src/convert/mod.rs', lines 852:4-852:28
    Name pattern: [core::convert::{core::convert::AsRef<str, str>}::as_ref]
    Visibility: public -/
@[rust_fun "core::convert::{core::convert::AsRef<str, str>}::as_ref"]
def Str.Insts.CoreConvertAsRefStr.as_ref : Str → Result Str
  := fun value => .ok value

/-- [core::option::{impl core::clone::Clone for core::option::Option<T>}::clone]:
    Source: '/rustc/library/core/src/option.rs', lines 2278:4-2278:27
    Name pattern: [core::option::{core::clone::Clone<core::option::Option<@T>>}::clone]
    Visibility: public -/
@[rust_fun
  "core::option::{core::clone::Clone<core::option::Option<@T>>}::clone"]
def core.option.Option.Insts.CoreCloneClone.clone
  {T : Type _} (cloneCloneInst : core.clone.Clone T) :
  Option T → Result (Option T)
  := fun value => do
    match value with
    | none => .ok none
    | some item =>
      let copied ← cloneCloneInst.clone item
      .ok (some copied)

/-- [core::option::{impl core::ops::try_trait::Try for core::option::Option<T>}::branch]:
    Source: '/rustc/library/core/src/option.rs', lines 2875:4-2875:64
    Name pattern: [core::option::{core::ops::try_trait::Try<core::option::Option<@T>>}::branch]
    Visibility: public -/
@[rust_fun
  "core::option::{core::ops::try_trait::Try<core::option::Option<@T>>}::branch"]
def core.option.Option.Insts.CoreOpsTry_traitTry.branch
  {T : Type _} :
  Option T → Result (core.ops.control_flow.ControlFlow (Option Never) T)
  := fun value => .ok (match value with
    | none => .Break none
    | some item => .Continue item)

/-- [core::option::{impl core::ops::try_trait::FromResidual<core::option::Option<!>> for core::option::Option<T>}::from_residual]:
    Source: '/rustc/library/core/src/option.rs', lines 2889:4-2889:49
    Name pattern: [core::option::{core::ops::try_trait::FromResidual<core::option::Option<@T>, core::option::Option<!>>}::from_residual]
    Visibility: public -/
@[rust_fun
  "core::option::{core::ops::try_trait::FromResidual<core::option::Option<@T>, core::option::Option<!>>}::from_residual"]
def
  core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual
  (T : Type _) : Option Never → Result (Option T)
  := fun residual => match residual with
    | none => .ok none
    | some impossible => nomatch impossible

/-- [alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<T, A>}::is_empty]:
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 1292:4-1292:40
    Name pattern: [alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, @A>}::is_empty]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::set::{alloc::collections::btree::set::BTreeSet<@T, @A>}::is_empty"]
def alloc.collections.btree.set.BTreeSet.is_empty
  {T : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.set.BTreeSet T A → Result Bool
  := fun values => .ok (decide (values = ∅))

/-- [alloc::string::{impl core::clone::Clone for alloc::string::String}::clone]:
    Source: '/rustc/library/alloc/src/string.rs', lines 2425:4-2425:27
    Name pattern: [alloc::string::{core::clone::Clone<alloc::string::String>}::clone]
    Visibility: public -/
@[rust_fun "alloc::string::{core::clone::Clone<alloc::string::String>}::clone"]
def alloc.string.String.Insts.CoreCloneClone.clone : String → Result String
  := fun value => .ok value

-- EXPLICIT NATIVE OPERATION SYMBOL: conditional observation, not an implementation.
-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [chrono::offset::utc::{chrono::offset::utc::Utc}::now]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 95:4-95:33
    Name pattern: [chrono::offset::utc::{chrono::offset::utc::Utc}::now]
    Visibility: public -/
@[rust_fun "chrono::offset::utc::{chrono::offset::utc::Utc}::now"]
axiom chrono.offset.utc.Utc.now
  :
  Result (chrono.datetime.DateTime chrono.offset.utc.Utc chrono.offset.utc.Utc)

/-- [serde_json::value::{impl core::clone::Clone for serde_json::value::Value}::clone]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.151/src/value/mod.rs', lines 115:9-115:14
    Name pattern: [serde_json::value::{core::clone::Clone<serde_json::value::Value>}::clone]
    Visibility: public -/
@[rust_fun
  "serde_json::value::{core::clone::Clone<serde_json::value::Value>}::clone"]
def serde_json.value.Value.Insts.CoreCloneClone.clone
  : serde_json.value.Value → Result serde_json.value.Value
  := fun value => .ok value

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::HubProfileId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 154:0-154:40
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::HubProfileId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::HubProfileId>}::clone"]
def veoveo_hub_sdk.identity.HubProfileId.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.HubProfileId → Result
    veoveo_hub_sdk.identity.HubProfileId
  := fun value => .ok value

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::LocalToolName}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 171:0-171:40
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::LocalToolName>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::LocalToolName>}::clone"]
def veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.LocalToolName → Result
    veoveo_hub_sdk.identity.LocalToolName
  := fun value => .ok value

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::PromptName}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 175:0-175:40
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::PromptName>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::PromptName>}::clone"]
def veoveo_hub_sdk.identity.PromptName.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.PromptName → Result
    veoveo_hub_sdk.identity.PromptName
  := fun value => .ok value

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::CanonicalTaskId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 187:0-187:39
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::CanonicalTaskId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::CanonicalTaskId>}::clone"]
def veoveo_hub_sdk.identity.CanonicalTaskId.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.CanonicalTaskId → Result
    veoveo_hub_sdk.identity.CanonicalTaskId
  := fun value => .ok value

/-- [veoveo_types::actions::{impl core::clone::Clone for veoveo_types::actions::ActionName}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/actions.rs', lines 6:0-6:38
    Name pattern: [veoveo_types::actions::{core::clone::Clone<veoveo_types::actions::ActionName>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::actions::{core::clone::Clone<veoveo_types::actions::ActionName>}::clone"]
def veoveo_types.actions.ActionName.Insts.CoreCloneClone.clone
  : veoveo_types.actions.ActionName → Result veoveo_types.actions.ActionName
  := fun value => .ok value

/-- [veoveo_types::actions::{veoveo_types::actions::ActionName}::parse]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/actions.rs', lines 6:0-6:38
    Name pattern: [veoveo_types::actions::{veoveo_types::actions::ActionName}::parse]
    Visibility: public -/
@[rust_fun "veoveo_types::actions::{veoveo_types::actions::ActionName}::parse"]
def veoveo_types.actions.ActionName.parse
  {T0 : Type _} (coreconvertAsRefT0StrInst : core.convert.AsRef T0 Str) :
  T0 → Result (core.result.Result veoveo_types.actions.ActionName
    veoveo_types.extensions.ExtensionError)
  := fun value => do
    let text ← coreconvertAsRefT0StrInst.as_ref value
    let bytes := text.val.map (fun byte => byte.val)
    let firstLower := match bytes with
      | [] => false
      | first :: _ => decide (97 ≤ first ∧ first ≤ 122)
    let allowed := bytes.all (fun byte => decide (
      (97 ≤ byte ∧ byte ≤ 122) ∨ (48 ≤ byte ∧ byte ≤ 57) ∨
      byte = 95 ∨ byte = 45 ∨ byte = 46))
    if firstLower && decide (bytes.length ≤ 128) && allowed then
      .ok (.Ok ⟨_root_.String.ofList (bytes.map Char.ofNat)⟩)
    else .ok (.Err ⟨"invalid action name"⟩)

/-- [veoveo_types::actions::{veoveo_types::actions::ActionHandle}::name]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/actions.rs', lines 25:4-25:37
    Name pattern: [veoveo_types::actions::{veoveo_types::actions::ActionHandle}::name]
    Visibility: public -/
@[rust_fun
  "veoveo_types::actions::{veoveo_types::actions::ActionHandle}::name"]
def veoveo_types.actions.ActionHandle.name
  :
  veoveo_types.actions.ActionHandle → Result veoveo_types.actions.ActionName
  := fun handle => .ok handle.actionName

/-- [veoveo_types::extensions::{impl core::clone::Clone for veoveo_types::extensions::ExtensionName}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/extensions.rs', lines 15:0-15:41
    Name pattern: [veoveo_types::extensions::{core::clone::Clone<veoveo_types::extensions::ExtensionName>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::extensions::{core::clone::Clone<veoveo_types::extensions::ExtensionName>}::clone"]
def veoveo_types.extensions.ExtensionName.Insts.CoreCloneClone.clone
  :
  veoveo_types.extensions.ExtensionName → Result
    veoveo_types.extensions.ExtensionName
  := fun value => .ok value

/-- [veoveo_types::extensions::{impl core::fmt::Debug for veoveo_types::extensions::ExtensionError}::fmt]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/extensions.rs', lines 29:9-29:14
    Name pattern: [veoveo_types::extensions::{core::fmt::Debug<veoveo_types::extensions::ExtensionError>}::fmt]
    Visibility: public -/
@[rust_fun
  "veoveo_types::extensions::{core::fmt::Debug<veoveo_types::extensions::ExtensionError>}::fmt"]
def veoveo_types.extensions.ExtensionError.Insts.CoreFmtDebug.fmt
  :
  veoveo_types.extensions.ExtensionError → core.fmt.Formatter → Result
    ((core.result.Result Unit core.fmt.Error) × core.fmt.Formatter)
  := fun _ formatter => .ok (.Ok (), formatter)

/-- [veoveo_types::extensions::{impl core::clone::Clone for veoveo_types::extensions::AdmittedExtensions}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/extensions.rs', lines 235:9-235:14
    Name pattern: [veoveo_types::extensions::{core::clone::Clone<veoveo_types::extensions::AdmittedExtensions>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::extensions::{core::clone::Clone<veoveo_types::extensions::AdmittedExtensions>}::clone"]
def veoveo_types.extensions.AdmittedExtensions.Insts.CoreCloneClone.clone
  :
  veoveo_types.extensions.AdmittedExtensions → Result
    veoveo_types.extensions.AdmittedExtensions
  := fun value => .ok value

/-- [veoveo_types::identity::{impl core::clone::Clone for veoveo_types::identity::PrincipalId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 9:0-9:69
    Name pattern: [veoveo_types::identity::{core::clone::Clone<veoveo_types::identity::PrincipalId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::clone::Clone<veoveo_types::identity::PrincipalId>}::clone"]
def veoveo_types.identity.PrincipalId.Insts.CoreCloneClone.clone
  :
  veoveo_types.identity.PrincipalId → Result
    veoveo_types.identity.PrincipalId
  := fun value => .ok value

/-- [veoveo_types::identity::{impl core::clone::Clone for veoveo_types::identity::TenantId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 13:0-13:69
    Name pattern: [veoveo_types::identity::{core::clone::Clone<veoveo_types::identity::TenantId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::clone::Clone<veoveo_types::identity::TenantId>}::clone"]
def veoveo_types.identity.TenantId.Insts.CoreCloneClone.clone
  : veoveo_types.identity.TenantId → Result veoveo_types.identity.TenantId
  := fun value => .ok value

/-- [veoveo_types::platform_names::{impl core::clone::Clone for veoveo_types::platform_names::ServerSlug}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 5:0-5:38
    Name pattern: [veoveo_types::platform_names::{core::clone::Clone<veoveo_types::platform_names::ServerSlug>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::platform_names::{core::clone::Clone<veoveo_types::platform_names::ServerSlug>}::clone"]
def veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone
  :
  veoveo_types.platform_names.ServerSlug → Result
    veoveo_types.platform_names.ServerSlug
  := fun value => .ok value

/-- [veoveo_types::resource::{impl core::clone::Clone for veoveo_types::resource::ResourceUri}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource.rs', lines 8:16-8:21
    Name pattern: [veoveo_types::resource::{core::clone::Clone<veoveo_types::resource::ResourceUri>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource::{core::clone::Clone<veoveo_types::resource::ResourceUri>}::clone"]
def veoveo_types.resource.ResourceUri.Insts.CoreCloneClone.clone
  :
  veoveo_types.resource.ResourceUri → Result
    veoveo_types.resource.ResourceUri
  := fun value => .ok value

/-- [veoveo_types::resource_template::{impl core::clone::Clone for veoveo_types::resource_template::ResourceTemplateUri}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource_template.rs', lines 21:16-21:21
    Name pattern: [veoveo_types::resource_template::{core::clone::Clone<veoveo_types::resource_template::ResourceTemplateUri>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource_template::{core::clone::Clone<veoveo_types::resource_template::ResourceTemplateUri>}::clone"]
def
  veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCloneClone.clone
  :
  veoveo_types.resource_template.ResourceTemplateUri → Result
    veoveo_types.resource_template.ResourceTemplateUri
  := fun value => .ok value

/-- [veoveo_types::task::{impl core::clone::Clone for veoveo_types::task::TaskId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/task.rs', lines 16:0-16:41
    Name pattern: [veoveo_types::task::{core::clone::Clone<veoveo_types::task::TaskId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::task::{core::clone::Clone<veoveo_types::task::TaskId>}::clone"]
def veoveo_types.task.TaskId.Insts.CoreCloneClone.clone
  : veoveo_types.task.TaskId → Result veoveo_types.task.TaskId
  := fun value => .ok value

/- These adapters preserve the actual supplied operations, including failure,
divergence and updated cursor state. The resolver model delegates to the pinned,
SDK-owned generated algorithm. Source-compilation/linkage to the foreign optimized
MIR remains explicit; definitional equality below does not prove that boundary. -/
namespace CurrentRolesSDK
universe u₁ u₂ u₃ u₄

def cursorToSDK {C : Type u₁} {I : Type u₂}
    (ops : veoveo_hub_sdk.registration.authorization_decision.Cursor C I) :
    sdk_authorization_proof.decision.Cursor C I := { next := ops.next }

def matcherToSDK {M : Type u₁} {T : Type u₂}
    (ops : veoveo_hub_sdk.registration.authorization_decision.Matcher M T) :
    sdk_authorization_proof.decision.Matcher M T := { «matches» := ops.«matches» }

/- Finite membership model; native sorted traversal/comparison traces are not
claimed. Native correspondence requires unique keys, lawful total ordering and
Borrow coherence. Supplied Borrow/compare failure is propagated. -/
def mapGetBy {K : Type u₁} {V : Type u₂} {Q : Type u₃}
    (borrow : K → Result Q) (compare : Q → Q → Result Ordering)
    (entries : List (K × V)) (query : Q) : Result (Option V) := do
  match entries with
  | [] => .ok none
  | (key, value) :: rest =>
    let borrowed ← borrow key
    let order ← compare borrowed query
    if order == .eq then .ok (some value) else mapGetBy borrow compare rest query
end CurrentRolesSDK

deriving instance DecidableEq for veoveo_hub_sdk.registration.authorization.RouteAccess
/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::get]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 719:4-722:15
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::get]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::get"]
def alloc.collections.btree.map.BTreeMap.get
  {K : Type _} {V : Type _} {A : Type _} {Q : Type _}
  (coreallocAllocatorCloneInst : core.alloc.AllocatorClone A)
  (coreborrowBorrowInst : core.borrow.Borrow K Q) (corecmpOrdInst :
  core.cmp.Ord K) (corecmpOrdInst1 : core.cmp.Ord Q) :
  alloc.collections.btree.map.BTreeMap K V A → Q → Result (Option V)
  := fun entries query => CurrentRolesSDK.mapGetBy coreborrowBorrowInst.borrow
    corecmpOrdInst1.cmp entries query


/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::iter]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 2711:4-2711:40
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::iter]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::iter"]
def alloc.collections.btree.map.BTreeMap.iter
  {K : Type _} {V : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.map.BTreeMap K V A → Result
    (alloc.collections.btree.map.Iter K V)
  := fun entries => .ok entries


/-- [veoveo_hub_sdk::identity::{impl core::cmp::Ord for veoveo_hub_sdk::identity::LocalToolName}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 171:0-171:40
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::Ord<veoveo_hub_sdk::identity::LocalToolName>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::Ord<veoveo_hub_sdk::identity::LocalToolName>}::cmp"]
def veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCmpOrd.cmp
  :
  veoveo_hub_sdk.identity.LocalToolName →
    veoveo_hub_sdk.identity.LocalToolName → Result Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_hub_sdk::identity::{impl core::cmp::PartialOrd<veoveo_hub_sdk::identity::LocalToolName> for veoveo_hub_sdk::identity::LocalToolName}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 171:0-171:40
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::PartialOrd<veoveo_hub_sdk::identity::LocalToolName, veoveo_hub_sdk::identity::LocalToolName>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialOrd<veoveo_hub_sdk::identity::LocalToolName, veoveo_hub_sdk::identity::LocalToolName>}::partial_cmp"]
def
  veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCmpPartialOrdLocalToolName.partial_cmp
  :
  veoveo_hub_sdk.identity.LocalToolName →
    veoveo_hub_sdk.identity.LocalToolName → Result (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_hub_sdk::identity::{impl core::cmp::PartialEq<veoveo_hub_sdk::identity::LocalToolName> for veoveo_hub_sdk::identity::LocalToolName}::eq]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 171:0-171:40
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::LocalToolName, veoveo_hub_sdk::identity::LocalToolName>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::LocalToolName, veoveo_hub_sdk::identity::LocalToolName>}::eq"]
def
  veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCmpPartialEqLocalToolName.eq
  :
  veoveo_hub_sdk.identity.LocalToolName →
    veoveo_hub_sdk.identity.LocalToolName → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_hub_sdk::identity::{impl core::cmp::Ord for veoveo_hub_sdk::identity::PromptName}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 175:0-175:40
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::Ord<veoveo_hub_sdk::identity::PromptName>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::Ord<veoveo_hub_sdk::identity::PromptName>}::cmp"]
def veoveo_hub_sdk.identity.PromptName.Insts.CoreCmpOrd.cmp
  :
  veoveo_hub_sdk.identity.PromptName → veoveo_hub_sdk.identity.PromptName →
    Result Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_hub_sdk::identity::{impl core::cmp::PartialOrd<veoveo_hub_sdk::identity::PromptName> for veoveo_hub_sdk::identity::PromptName}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 175:0-175:40
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::PartialOrd<veoveo_hub_sdk::identity::PromptName, veoveo_hub_sdk::identity::PromptName>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialOrd<veoveo_hub_sdk::identity::PromptName, veoveo_hub_sdk::identity::PromptName>}::partial_cmp"]
def
  veoveo_hub_sdk.identity.PromptName.Insts.CoreCmpPartialOrdPromptName.partial_cmp
  :
  veoveo_hub_sdk.identity.PromptName → veoveo_hub_sdk.identity.PromptName →
    Result (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_hub_sdk::identity::{impl core::cmp::PartialEq<veoveo_hub_sdk::identity::PromptName> for veoveo_hub_sdk::identity::PromptName}::eq]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 175:0-175:40
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::PromptName, veoveo_hub_sdk::identity::PromptName>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::PromptName, veoveo_hub_sdk::identity::PromptName>}::eq"]
def veoveo_hub_sdk.identity.PromptName.Insts.CoreCmpPartialEqPromptName.eq
  :
  veoveo_hub_sdk.identity.PromptName → veoveo_hub_sdk.identity.PromptName →
    Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_hub_sdk::registration::authorization::{impl core::cmp::PartialEq<veoveo_hub_sdk::registration::authorization::RouteAccess> for veoveo_hub_sdk::registration::authorization::RouteAccess}::eq]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/registration/authorization.rs', lines 30:23-30:32
    Name pattern: [veoveo_hub_sdk::registration::authorization::{core::cmp::PartialEq<veoveo_hub_sdk::registration::authorization::RouteAccess, veoveo_hub_sdk::registration::authorization::RouteAccess>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::authorization::{core::cmp::PartialEq<veoveo_hub_sdk::registration::authorization::RouteAccess, veoveo_hub_sdk::registration::authorization::RouteAccess>}::eq"]
def
  veoveo_hub_sdk.registration.authorization.RouteAccess.Insts.CoreCmpPartialEqRouteAccess.eq
  :
  veoveo_hub_sdk.registration.authorization.RouteAccess →
    veoveo_hub_sdk.registration.authorization.RouteAccess → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::resource_template::{impl core::cmp::Ord for veoveo_types::resource_template::ResourceTemplateUri}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource_template.rs', lines 21:50-21:53
    Name pattern: [veoveo_types::resource_template::{core::cmp::Ord<veoveo_types::resource_template::ResourceTemplateUri>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource_template::{core::cmp::Ord<veoveo_types::resource_template::ResourceTemplateUri>}::cmp"]
def veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.resource_template.ResourceTemplateUri →
    veoveo_types.resource_template.ResourceTemplateUri → Result Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_types::resource_template::{impl core::cmp::PartialOrd<veoveo_types::resource_template::ResourceTemplateUri> for veoveo_types::resource_template::ResourceTemplateUri}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource_template.rs', lines 21:38-21:48
    Name pattern: [veoveo_types::resource_template::{core::cmp::PartialOrd<veoveo_types::resource_template::ResourceTemplateUri, veoveo_types::resource_template::ResourceTemplateUri>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource_template::{core::cmp::PartialOrd<veoveo_types::resource_template::ResourceTemplateUri, veoveo_types::resource_template::ResourceTemplateUri>}::partial_cmp"]
def
  veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCmpPartialOrdResourceTemplateUri.partial_cmp
  :
  veoveo_types.resource_template.ResourceTemplateUri →
    veoveo_types.resource_template.ResourceTemplateUri → Result (Option
    Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_types::resource_template::{impl core::cmp::PartialEq<veoveo_types::resource_template::ResourceTemplateUri> for veoveo_types::resource_template::ResourceTemplateUri}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource_template.rs', lines 21:23-21:32
    Name pattern: [veoveo_types::resource_template::{core::cmp::PartialEq<veoveo_types::resource_template::ResourceTemplateUri, veoveo_types::resource_template::ResourceTemplateUri>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource_template::{core::cmp::PartialEq<veoveo_types::resource_template::ResourceTemplateUri, veoveo_types::resource_template::ResourceTemplateUri>}::eq"]
def
  veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCmpPartialEqResourceTemplateUri.eq
  :
  veoveo_types.resource_template.ResourceTemplateUri →
    veoveo_types.resource_template.ResourceTemplateUri → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::resource::{impl core::cmp::Ord for veoveo_types::resource::ResourceUri}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource.rs', lines 8:50-8:53
    Name pattern: [veoveo_types::resource::{core::cmp::Ord<veoveo_types::resource::ResourceUri>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource::{core::cmp::Ord<veoveo_types::resource::ResourceUri>}::cmp"]
def veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.resource.ResourceUri → veoveo_types.resource.ResourceUri →
    Result Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_types::resource::{impl core::cmp::PartialOrd<veoveo_types::resource::ResourceUri> for veoveo_types::resource::ResourceUri}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource.rs', lines 8:38-8:48
    Name pattern: [veoveo_types::resource::{core::cmp::PartialOrd<veoveo_types::resource::ResourceUri, veoveo_types::resource::ResourceUri>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource::{core::cmp::PartialOrd<veoveo_types::resource::ResourceUri, veoveo_types::resource::ResourceUri>}::partial_cmp"]
def
  veoveo_types.resource.ResourceUri.Insts.CoreCmpPartialOrdResourceUri.partial_cmp
  :
  veoveo_types.resource.ResourceUri → veoveo_types.resource.ResourceUri →
    Result (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_types::resource::{impl core::cmp::PartialEq<veoveo_types::resource::ResourceUri> for veoveo_types::resource::ResourceUri}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource.rs', lines 8:23-8:32
    Name pattern: [veoveo_types::resource::{core::cmp::PartialEq<veoveo_types::resource::ResourceUri, veoveo_types::resource::ResourceUri>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::resource::{core::cmp::PartialEq<veoveo_types::resource::ResourceUri, veoveo_types::resource::ResourceUri>}::eq"]
def veoveo_types.resource.ResourceUri.Insts.CoreCmpPartialEqResourceUri.eq
  :
  veoveo_types.resource.ResourceUri → veoveo_types.resource.ResourceUri →
    Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_hub_sdk::registration::authorization_decision::resolve]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/registration/authorization_decision.rs', lines 14:0-18:14
    Name pattern: [veoveo_hub_sdk::registration::authorization_decision::resolve] -/
@[rust_fun "veoveo_hub_sdk::registration::authorization_decision::resolve"]
def veoveo_hub_sdk.registration.authorization_decision.resolve
  {T : Type _} {A : Type _} {C : Type _} {M : Type _} (corecmpPartialEqInst :
  core.cmp.PartialEq A A) (coremarkerCopyInst : core.marker.Copy A)
  (CursorCPairInst : veoveo_hub_sdk.registration.authorization_decision.Cursor
  C (T × A)) (MatcherInst :
  veoveo_hub_sdk.registration.authorization_decision.Matcher M T) :
  Option A → C → M → Result ((Option A) × C)
  := fun exact candidates matcher =>
    sdk_authorization_proof.decision.resolve corecmpPartialEqInst coremarkerCopyInst
      (CurrentRolesSDK.cursorToSDK CursorCPairInst) (CurrentRolesSDK.matcherToSDK MatcherInst)
      exact candidates matcher


-- EXPLICIT NATIVE OPERATION SYMBOL: parser/decomposition trace boundary.
-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [veoveo_hub_sdk::registration::authorization_template::{veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate<'a>}::parse]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/registration/authorization_template.rs', lines 19:4-19:74
    Name pattern: [veoveo_hub_sdk::registration::authorization_template::{veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate<'a>}::parse] -/
@[rust_fun
  "veoveo_hub_sdk::registration::authorization_template::{veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate<'a>}::parse"]
axiom veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse
  :
  veoveo_types.resource_template.ResourceTemplateUri → Result (Option
    veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate)

-- EXPLICIT NATIVE OPERATION SYMBOL: parser/decomposition trace boundary.

-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [veoveo_hub_sdk::registration::authorization_template::{veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate<'a>}::matches]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/registration/authorization_template.rs', lines 107:4-107:59
    Name pattern: [veoveo_hub_sdk::registration::authorization_template::{veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate<'a>}::matches] -/
@[rust_fun
  "veoveo_hub_sdk::registration::authorization_template::{veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate<'a>}::matches"]
axiom veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches
  :
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate →
    veoveo_types.resource.ResourceUri → Result Bool

/-- [veoveo_hub_sdk::registration::authorization::{impl veoveo_hub_sdk::registration::authorization_decision::Cursor<(&'a veoveo_types::resource_template::ResourceTemplateUri, &'a veoveo_hub_sdk::registration::authorization::RouteAccess)> for alloc::collections::btree::map::Iter<'a, veoveo_types::resource_template::ResourceTemplateUri, veoveo_hub_sdk::registration::authorization::RouteAccess>}::next]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/registration/authorization.rs', lines 297:4-297:76
    Name pattern: [veoveo_hub_sdk::registration::authorization::{veoveo_hub_sdk::registration::authorization_decision::Cursor<alloc::collections::btree::map::Iter<'a, veoveo_types::resource_template::ResourceTemplateUri, veoveo_hub_sdk::registration::authorization::RouteAccess>, (&'a veoveo_types::resource_template::ResourceTemplateUri, &'a veoveo_hub_sdk::registration::authorization::RouteAccess)>}::next] -/
@[rust_fun
  "veoveo_hub_sdk::registration::authorization::{veoveo_hub_sdk::registration::authorization_decision::Cursor<alloc::collections::btree::map::Iter<'a, veoveo_types::resource_template::ResourceTemplateUri, veoveo_hub_sdk::registration::authorization::RouteAccess>, (&'a veoveo_types::resource_template::ResourceTemplateUri, &'a veoveo_hub_sdk::registration::authorization::RouteAccess)>}::next"]
def
  alloc.collections.btree.map.IterAResourceTemplateUriRouteAccess.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionCursorPairSharedAResourceTemplateUriSharedARouteAccess.next
  :
  alloc.collections.btree.map.Iter
    veoveo_types.resource_template.ResourceTemplateUri
    veoveo_hub_sdk.registration.authorization.RouteAccess → Result ((Option
    (veoveo_types.resource_template.ResourceTemplateUri ×
    veoveo_hub_sdk.registration.authorization.RouteAccess)) ×
    (alloc.collections.btree.map.Iter
    veoveo_types.resource_template.ResourceTemplateUri
    veoveo_hub_sdk.registration.authorization.RouteAccess))
  := fun entries => match entries with
    | [] => .ok (none, [])
    | entry :: rest => .ok (some entry, rest)

-- Session-family primitive models. DateTime already denotes an exact ordered
-- (seconds, subsecond) pair. Connecting it to native Chrono's UTC datetime and
-- preserving comparison (including leap-second validity) is a refinement boundary.
namespace SessionNative

def timestampCompare (left right : Int × Nat) : Ordering :=
  match compare left.1 right.1 with
  | .eq => compare left.2 right.2
  | ordering => ordering

end SessionNative

/-- [alloc::string::{impl core::cmp::PartialEq<alloc::string::String> for alloc::string::String}::eq]:
    Source: '/rustc/library/alloc/src/string.rs', lines 350:9-350:18
    Name pattern: [alloc::string::{core::cmp::PartialEq<alloc::string::String, alloc::string::String>}::eq]
    Visibility: public -/
@[rust_fun
  "alloc::string::{core::cmp::PartialEq<alloc::string::String, alloc::string::String>}::eq"]
def alloc.string.String.Insts.CoreCmpPartialEqString.eq
  : String → String → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [chrono::datetime::{impl core::cmp::PartialEq<chrono::datetime::DateTime<Tz2, Clause1_Offset>> for chrono::datetime::DateTime<Tz, Clause0_Offset>}::eq]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/datetime/mod.rs', lines 1473:4-1473:47
    Name pattern: [chrono::datetime::{core::cmp::PartialEq<chrono::datetime::DateTime<@Tz, @Clause0_Offset>, chrono::datetime::DateTime<@Tz2, @Clause1_Offset>>}::eq]
    Visibility: public -/
@[rust_fun
  "chrono::datetime::{core::cmp::PartialEq<chrono::datetime::DateTime<@Tz, @Clause0_Offset>, chrono::datetime::DateTime<@Tz2, @Clause1_Offset>>}::eq"]
def chrono.datetime.DateTime.Insts.CoreCmpPartialEqDateTime.eq
  {Tz : Type _} {Tz2 : Type _} {Clause0_Offset : Type _} {Clause1_Offset :
  Type _} (offsetTimeZoneInst : chrono.offset.TimeZone Tz Clause0_Offset)
  (offsetTimeZoneInst1 : chrono.offset.TimeZone Tz2 Clause1_Offset) :
  chrono.datetime.DateTime Tz Clause0_Offset → chrono.datetime.DateTime Tz2
    Clause1_Offset → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [chrono::datetime::{impl core::cmp::PartialOrd<chrono::datetime::DateTime<Tz2, Clause1_Offset>> for chrono::datetime::DateTime<Tz, Clause0_Offset>}::partial_cmp]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/datetime/mod.rs', lines 1502:4-1502:68
    Name pattern: [chrono::datetime::{core::cmp::PartialOrd<chrono::datetime::DateTime<@Tz, @Clause0_Offset>, chrono::datetime::DateTime<@Tz2, @Clause1_Offset>>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "chrono::datetime::{core::cmp::PartialOrd<chrono::datetime::DateTime<@Tz, @Clause0_Offset>, chrono::datetime::DateTime<@Tz2, @Clause1_Offset>>}::partial_cmp"]
def chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp
  {Tz : Type _} {Tz2 : Type _} {Clause0_Offset : Type _} {Clause1_Offset :
  Type _} (offsetTimeZoneInst : chrono.offset.TimeZone Tz Clause0_Offset)
  (offsetTimeZoneInst1 : chrono.offset.TimeZone Tz2 Clause1_Offset) :
  chrono.datetime.DateTime Tz Clause0_Offset → chrono.datetime.DateTime Tz2
    Clause1_Offset → Result (Option Ordering)
  := fun left right => .ok (some (SessionNative.timestampCompare left right))


/-- [chrono::offset::utc::{impl core::clone::Clone for chrono::offset::utc::Utc}::clone]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 43:15-43:20
    Name pattern: [chrono::offset::utc::{core::clone::Clone<chrono::offset::utc::Utc>}::clone]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{core::clone::Clone<chrono::offset::utc::Utc>}::clone"]
def chrono.offset.utc.Utc.Insts.CoreCloneClone.clone
  : chrono.offset.utc.Utc → Result chrono.offset.utc.Utc
  := fun value => .ok value


/-- [chrono::offset::utc::{impl core::fmt::Debug for chrono::offset::utc::Utc}::fmt]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 143:4-143:56
    Name pattern: [chrono::offset::utc::{core::fmt::Debug<chrono::offset::utc::Utc>}::fmt]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{core::fmt::Debug<chrono::offset::utc::Utc>}::fmt"]
def chrono.offset.utc.Utc.Insts.CoreFmtDebug.fmt
  :
  chrono.offset.utc.Utc → core.fmt.Formatter → Result ((core.result.Result
    Unit core.fmt.Error) × core.fmt.Formatter)
  := fun _ formatter => core.fmt.Formatter.write_str formatter (toStr "Z")


/-- [chrono::offset::utc::{impl chrono::offset::Offset for chrono::offset::utc::Utc}::fix]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 137:4-137:32
    Name pattern: [chrono::offset::utc::{chrono::offset::Offset<chrono::offset::utc::Utc>}::fix]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{chrono::offset::Offset<chrono::offset::utc::Utc>}::fix"]
def chrono.offset.utc.Utc.Insts.ChronoOffsetOffset.fix
  : chrono.offset.utc.Utc → Result chrono.offset.fixed.FixedOffset
  := fun _ => .ok { seconds := 0 }


/-- [chrono::offset::utc::{impl chrono::offset::TimeZone<chrono::offset::utc::Utc> for chrono::offset::utc::Utc}::offset_from_utc_datetime]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 131:4-131:67
    Name pattern: [chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_utc_datetime]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_utc_datetime"]
def
  chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_utc_datetime
  :
  chrono.offset.utc.Utc → chrono.naive.datetime.NaiveDateTime → Result
    chrono.offset.utc.Utc
  := fun _ _ => .ok ()


/-- [chrono::offset::utc::{impl chrono::offset::TimeZone<chrono::offset::utc::Utc> for chrono::offset::utc::Utc}::offset_from_utc_date]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 128:4-128:59
    Name pattern: [chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_utc_date]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_utc_date"]
def chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_utc_date
  :
  chrono.offset.utc.Utc → chrono.naive.date.NaiveDate → Result
    chrono.offset.utc.Utc
  := fun _ _ => .ok ()


/-- [chrono::offset::utc::{impl chrono::offset::TimeZone<chrono::offset::utc::Utc> for chrono::offset::utc::Utc}::offset_from_local_datetime]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 124:4-124:88
    Name pattern: [chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_local_datetime]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_local_datetime"]
def
  chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_local_datetime
  :
  chrono.offset.utc.Utc → chrono.naive.datetime.NaiveDateTime → Result
    (chrono.offset.LocalResult chrono.offset.utc.Utc)
  := fun _ _ => .ok (.Single ())


/-- [chrono::offset::utc::{impl chrono::offset::TimeZone<chrono::offset::utc::Utc> for chrono::offset::utc::Utc}::offset_from_local_date]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 121:4-121:80
    Name pattern: [chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_local_date]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::offset_from_local_date"]
def
  chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_local_date
  :
  chrono.offset.utc.Utc → chrono.naive.date.NaiveDate → Result
    (chrono.offset.LocalResult chrono.offset.utc.Utc)
  := fun _ _ => .ok (.Single ())


/-- [chrono::offset::utc::{impl chrono::offset::TimeZone<chrono::offset::utc::Utc> for chrono::offset::utc::Utc}::from_offset]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/offset/utc.rs', lines 117:4-117:39
    Name pattern: [chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::from_offset]
    Visibility: public -/
@[rust_fun
  "chrono::offset::utc::{chrono::offset::TimeZone<chrono::offset::utc::Utc, chrono::offset::utc::Utc>}::from_offset"]
def chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.from_offset
  : chrono.offset.utc.Utc → Result chrono.offset.utc.Utc
  := fun _ => .ok ()


/-- [veoveo_hub_sdk::identity::{impl core::cmp::PartialEq<veoveo_hub_sdk::identity::PrincipalKind> for veoveo_hub_sdk::identity::PrincipalKind}::eq]:
    Source: '/home/rozgo/.cache/lean-pilot/hub-session-family-20261030/session-probe-1/extraction/sdk-root/crates/veoveo-hub-sdk/src/identity.rs', lines 112:29-112:38
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::PrincipalKind, veoveo_hub_sdk::identity::PrincipalKind>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::PrincipalKind, veoveo_hub_sdk::identity::PrincipalKind>}::eq"]
def
  veoveo_hub_sdk.identity.PrincipalKind.Insts.CoreCmpPartialEqPrincipalKind.eq
  :
  veoveo_hub_sdk.identity.PrincipalKind →
    veoveo_hub_sdk.identity.PrincipalKind → Result Bool
  := fun left right => .ok (match left, right with
    | .User, .User => true
    | .Service, .Service => true
    | _, _ => false)


/-- [veoveo_hub_sdk::identity::{impl core::cmp::PartialEq<veoveo_hub_sdk::identity::ProtectedResourceId> for veoveo_hub_sdk::identity::ProtectedResourceId}::eq]:
    Source: '/home/rozgo/.cache/lean-pilot/hub-session-family-20261030/session-probe-1/extraction/sdk-root/crates/veoveo-hub-sdk/src/identity.rs', lines 163:0-163:43
    Name pattern: [veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::ProtectedResourceId, veoveo_hub_sdk::identity::ProtectedResourceId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::ProtectedResourceId, veoveo_hub_sdk::identity::ProtectedResourceId>}::eq"]
def
  veoveo_hub_sdk.identity.ProtectedResourceId.Insts.CoreCmpPartialEqProtectedResourceId.eq
  :
  veoveo_hub_sdk.identity.ProtectedResourceId →
    veoveo_hub_sdk.identity.ProtectedResourceId → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::WorkContextId> for veoveo_types::identity::WorkContextId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 17:0-17:66
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::WorkContextId, veoveo_types::identity::WorkContextId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::WorkContextId, veoveo_types::identity::WorkContextId>}::eq"]
def
  veoveo_types.identity.WorkContextId.Insts.CoreCmpPartialEqWorkContextId.eq
  :
  veoveo_types.identity.WorkContextId → veoveo_types.identity.WorkContextId
    → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::TokenIssuer> for veoveo_types::identity::TokenIssuer}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 114:0-114:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::TokenIssuer, veoveo_types::identity::TokenIssuer>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::TokenIssuer, veoveo_types::identity::TokenIssuer>}::eq"]
def veoveo_types.identity.TokenIssuer.Insts.CoreCmpPartialEqTokenIssuer.eq
  :
  veoveo_types.identity.TokenIssuer → veoveo_types.identity.TokenIssuer →
    Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::identity::{impl core::cmp::PartialEq<veoveo_types::identity::TokenSubject> for veoveo_types::identity::TokenSubject}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 117:0-117:69
    Name pattern: [veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::TokenSubject, veoveo_types::identity::TokenSubject>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::PartialEq<veoveo_types::identity::TokenSubject, veoveo_types::identity::TokenSubject>}::eq"]
def veoveo_types.identity.TokenSubject.Insts.CoreCmpPartialEqTokenSubject.eq
  :
  veoveo_types.identity.TokenSubject → veoveo_types.identity.TokenSubject →
    Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::names::{impl core::cmp::Ord for veoveo_types::names::ScopeName}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/names.rs', lines 4:0-4:37
    Name pattern: [veoveo_types::names::{core::cmp::Ord<veoveo_types::names::ScopeName>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::names::{core::cmp::Ord<veoveo_types::names::ScopeName>}::cmp"]
def veoveo_types.names.ScopeName.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.names.ScopeName → veoveo_types.names.ScopeName → Result
    Ordering
  := fun left right => .ok (compare left.value right.value)


/-- [veoveo_types::names::{impl core::cmp::PartialOrd<veoveo_types::names::ScopeName> for veoveo_types::names::ScopeName}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/names.rs', lines 4:0-4:37
    Name pattern: [veoveo_types::names::{core::cmp::PartialOrd<veoveo_types::names::ScopeName, veoveo_types::names::ScopeName>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::names::{core::cmp::PartialOrd<veoveo_types::names::ScopeName, veoveo_types::names::ScopeName>}::partial_cmp"]
def veoveo_types.names.ScopeName.Insts.CoreCmpPartialOrdScopeName.partial_cmp
  :
  veoveo_types.names.ScopeName → veoveo_types.names.ScopeName → Result
    (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))


/-- [veoveo_types::names::{impl core::cmp::PartialEq<veoveo_types::names::ScopeName> for veoveo_types::names::ScopeName}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/names.rs', lines 4:0-4:37
    Name pattern: [veoveo_types::names::{core::cmp::PartialEq<veoveo_types::names::ScopeName, veoveo_types::names::ScopeName>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::names::{core::cmp::PartialEq<veoveo_types::names::ScopeName, veoveo_types::names::ScopeName>}::eq"]
def veoveo_types.names.ScopeName.Insts.CoreCmpPartialEqScopeName.eq
  :
  veoveo_types.names.ScopeName → veoveo_types.names.ScopeName → Result Bool
  := fun left right => .ok (decide (left = right))


/-- [veoveo_types::platform_names::{impl core::cmp::PartialEq<veoveo_types::platform_names::OAuthClientId> for veoveo_types::platform_names::OAuthClientId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 39:0-39:36
    Name pattern: [veoveo_types::platform_names::{core::cmp::PartialEq<veoveo_types::platform_names::OAuthClientId, veoveo_types::platform_names::OAuthClientId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::platform_names::{core::cmp::PartialEq<veoveo_types::platform_names::OAuthClientId, veoveo_types::platform_names::OAuthClientId>}::eq"]
def
  veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpPartialEqOAuthClientId.eq
  :
  veoveo_types.platform_names.OAuthClientId →
    veoveo_types.platform_names.OAuthClientId → Result Bool
  := fun left right => .ok (decide (left = right))

/-- Ordinary nominal equality; native family-ID decoding/refinement is outside
this extracted predicate proof. -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::cmp::PartialEq<veoveo_hub_sdk::identity::HubRefreshFamilyId, veoveo_hub_sdk::identity::HubRefreshFamilyId>}::eq"]
def veoveo_hub_sdk.identity.HubRefreshFamilyId.Insts.CoreCmpPartialEqHubRefreshFamilyId.eq
    (left right : veoveo_hub_sdk.identity.HubRefreshFamilyId) : Result Bool :=
  .ok (decide (left = right))

namespace CurrentRolesNative
-- Comparator-driven map insertion retains last-write-wins and key ordering.
def insertMap {K V : Type _} (cmp : K → K → Result Ordering)
    (key : K) (value : V) (entries : List (K × V)) : Result (List (K × V)) := do
  match entries with
  | [] => .ok [(key, value)]
  | (k, v) :: rest =>
    let order ← cmp key k
    match order with
    | .lt => .ok ((key, value) :: (k, v) :: rest)
    | .eq => .ok ((key, value) :: rest)
    | .gt => do let tail ← insertMap cmp key value rest; .ok ((k, v) :: tail)

def collectMap {I K V : Type _}
    (iterator : core.iter.traits.iterator.Iterator I (K × V))
    (cmp : K → K → Result Ordering) (state : I) (entries : List (K × V)) :
    Result (List (K × V)) := do
  let (entry, next) ← iterator.next state
  match entry with
  | none => .ok entries
  | some (key, value) =>
    let entries' ← insertMap cmp key value entries
    collectMap iterator cmp next entries'
partial_fixpoint
end CurrentRolesNative

namespace CurrentRolesNative
def filterMapNext {I F A B : Type _}
    (iterator : core.iter.traits.iterator.Iterator I A)
    (mapper : core.ops.function.FnMut F A (Option B))
    (state : core.iter.adapters.filter_map.FilterMap I F) :
    Result (Option B × core.iter.adapters.filter_map.FilterMap I F) := do
  let (entry, next) ← iterator.next state.iter
  match entry with
  | none => .ok (none, { state with iter := next })
  | some value =>
    let (mapped, f) ← mapper.call_mut state.f value
    let state' := { iter := next, f : core.iter.adapters.filter_map.FilterMap I F }
    match mapped with
    | some value => .ok (some value, state')
    | none => filterMapNext iterator mapper state'
partial_fixpoint
end CurrentRolesNative

/-- [core::bool::{bool}::then_some]:
    Source: '/rustc/library/core/src/bool.rs', lines 36:4-36:72
    Name pattern: [core::bool::{bool}::then_some]
    Visibility: public -/
@[rust_fun "core::bool::{bool}::then_some"]
def core.bool.Bool.then_some {T : Type _} : Bool → T → Result (Option T)
  := fun condition value => .ok (if condition then some value else none)

/-- [core::iter::traits::iterator::Iterator::filter_map]:
    Source: '/rustc/library/core/src/iter/traits/iterator.rs', lines 1011:4-1014:42
    Name pattern: [core::iter::traits::iterator::Iterator::filter_map]
    Visibility: public -/
@[trait_default, rust_fun "core::iter::traits::iterator::Iterator::filter_map"]
def core.iter.traits.iterator.Iterator.filter_map.default
  {Self : Type _} {B : Type _} {F : Type _} {Clause0_Item : Type _}
  (IteratorInst : core.iter.traits.iterator.Iterator Self Clause0_Item)
  (opsfunctionFnMutFTupleClause0_ItemOptionInst : core.ops.function.FnMut F
  Clause0_Item (Option B)) :
  Self → F → Result (core.iter.adapters.filter_map.FilterMap Self F)
  := fun state f => .ok { iter := state, f }

/-- [core::iter::adapters::filter_map::{impl core::iter::traits::iterator::Iterator<B> for core::iter::adapters::filter_map::FilterMap<I, F>}::next]:
    Source: '/rustc/library/core/src/iter/adapters/filter_map.rs', lines 63:4-63:35
    Name pattern: [core::iter::adapters::filter_map::{core::iter::traits::iterator::Iterator<core::iter::adapters::filter_map::FilterMap<@I, @F>, @B>}::next]
    Visibility: public -/
@[rust_fun
  "core::iter::adapters::filter_map::{core::iter::traits::iterator::Iterator<core::iter::adapters::filter_map::FilterMap<@I, @F>, @B>}::next"]
def core.iter.adapters.filter_map.FilterMap.Insts.CoreIterTraitsIteratorIterator.next
  {B : Type _} {I : Type _} {F : Type _} {Clause0_Item : Type _}
  (traitsiteratorIteratorInst : core.iter.traits.iterator.Iterator I
  Clause0_Item) (opsfunctionFnMutFTupleClause0_ItemOptionInst :
  core.ops.function.FnMut F Clause0_Item (Option B)) :
  core.iter.adapters.filter_map.FilterMap I F → Result ((Option B) ×
    (core.iter.adapters.filter_map.FilterMap I F))
  := CurrentRolesNative.filterMapNext traitsiteratorIteratorInst opsfunctionFnMutFTupleClause0_ItemOptionInst

/-- [core::option::{core::option::Option<T>}::map_or]:
    Source: '/rustc/library/core/src/option.rs', lines 1224:4-1227:28
    Name pattern: [core::option::{core::option::Option<@T>}::map_or]
    Visibility: public -/
@[rust_fun "core::option::{core::option::Option<@T>}::map_or"]
def core.option.Option.map_or
  {T : Type _} {U : Type _} {F : Type _} (opsfunctionFnOnceFTupleTUInst :
  core.ops.function.FnOnce F T U) :
  Option T → U → F → Result U
  := fun option default f => match option with | none => .ok default | some value => opsfunctionFnOnceFTupleTUInst.call_once f value

/-- [core::option::{core::option::Option<T>}::and_then]:
    Source: '/rustc/library/core/src/option.rs', lines 1539:4-1541:61
    Name pattern: [core::option::{core::option::Option<@T>}::and_then]
    Visibility: public -/
@[rust_fun "core::option::{core::option::Option<@T>}::and_then"]
def core.option.Option.and_then
  {T : Type _} {U : Type _} {F : Type _} (opsfunctionFnOnceFTupleTOptionInst :
  core.ops.function.FnOnce F T (Option U)) :
  Option T → F → Result (Option U)
  := fun option f => match option with | none => .ok none | some value => opsfunctionFnOnceFTupleTOptionInst.call_once f value

/-- [core::option::{core::option::Option<&'_0 T>}::cloned]:
    Source: '/rustc/library/core/src/option.rs', lines 2162:4-2164:17
    Name pattern: [core::option::{core::option::Option<&'0 @T>}::cloned]
    Visibility: public -/
@[rust_fun "core::option::{core::option::Option<&'0 @T>}::cloned"]
def core.option.OptionShared0T.cloned
  {T : Type _} (cloneCloneInst : core.clone.Clone T) :
  Option T → Result (Option T)
  := fun option => match option with | none => .ok none | some value => do let cloned ← cloneCloneInst.clone value; .ok (some cloned)

/-- [alloc::collections::btree::map::{impl core::iter::traits::iterator::Iterator<(&'a K, &'a V)> for alloc::collections::btree::map::Iter<'a, K, V>}::next]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 1737:4-1737:48
    Name pattern: [alloc::collections::btree::map::{core::iter::traits::iterator::Iterator<alloc::collections::btree::map::Iter<'a, @K, @V>, (&'a @K, &'a @V)>}::next]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{core::iter::traits::iterator::Iterator<alloc::collections::btree::map::Iter<'a, @K, @V>, (&'a @K, &'a @V)>}::next"]
def alloc.collections.btree.map.Iter.Insts.CoreIterTraitsIteratorIteratorPairSharedAKSharedAV.next
  {K : Type _} {V : Type _} :
  alloc.collections.btree.map.Iter K V → Result ((Option (K × V)) ×
    (alloc.collections.btree.map.Iter K V))
  := fun entries => .ok (match entries with | [] => (none, []) | entry :: rest => (some entry, rest))

/-- [alloc::collections::btree::map::{impl core::iter::traits::collect::FromIterator<(K, V)> for alloc::collections::btree::map::BTreeMap<K, V, alloc::alloc::Global>}::from_iter]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 2554:4-2554:75
    Name pattern: [alloc::collections::btree::map::{core::iter::traits::collect::FromIterator<alloc::collections::btree::map::BTreeMap<@K, @V, alloc::alloc::Global>, (@K, @V)>}::from_iter]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{core::iter::traits::collect::FromIterator<alloc::collections::btree::map::BTreeMap<@K, @V, alloc::alloc::Global>, (@K, @V)>}::from_iter"]
def alloc.collections.btree.map.BTreeMapKVGlobal.Insts.CoreIterTraitsCollectFromIteratorPair.from_iter
  {K : Type _} {V : Type _} {I : Type _} {Clause1_IntoIter : Type _}
  (corecmpOrdInst : core.cmp.Ord K)
  (coreitertraitscollectIntoIteratorIPairClause1_IntoIterInst :
  core.iter.traits.collect.IntoIterator I (K × V) Clause1_IntoIter) :
  I → Result (alloc.collections.btree.map.BTreeMap K V alloc.alloc.Global)
  := fun source => do
    let iterator ← coreitertraitscollectIntoIteratorIPairClause1_IntoIterInst.into_iter source
    CurrentRolesNative.collectMap coreitertraitscollectIntoIteratorIPairClause1_IntoIterInst.iteratorInst corecmpOrdInst.cmp iterator []

-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [alloc::string::{impl alloc::string::ToString for T}::to_string]:
    Source: '/rustc/library/alloc/src/string.rs', lines 2965:4-2965:33
    Name pattern: [alloc::string::{alloc::string::ToString<@T>}::to_string]
    Visibility: public -/
@[rust_fun "alloc::string::{alloc::string::ToString<@T>}::to_string"]
axiom alloc.string.ToString.Blanket.to_string
  {T : Type _} (corefmtDisplayInst : core.fmt.Display T) : T → Result String

/-- [alloc::string::{impl core::convert::From<alloc::string::String> for alloc::borrow::Cow<'a, str, alloc::string::String>}::from]:
    Source: '/rustc/library/alloc/src/string.rs', lines 3316:4-3316:38
    Name pattern: [alloc::string::{core::convert::From<alloc::borrow::Cow<'a, str, alloc::string::String>, alloc::string::String>}::from]
    Visibility: public -/
@[rust_fun
  "alloc::string::{core::convert::From<alloc::borrow::Cow<'a, str, alloc::string::String>, alloc::string::String>}::from"]
def alloc.borrow.CowAStrString.Insts.CoreConvertFromString.from
  : String → Result (alloc.borrow.Cow Str String)
  := fun text => .ok (.Owned text)

/-- [alloc::sync::{alloc::sync::Arc<T>}::new]:
    Source: '/rustc/library/alloc/src/sync.rs', lines 444:4-444:33
    Name pattern: [alloc::sync::{alloc::sync::Arc<@T>}::new]
    Visibility: public -/
-- Existing Arc representation erases allocation/refcounts to the whole value.
-- Native allocation/failure/refinement remains outside this operational model.
@[rust_fun "alloc::sync::{alloc::sync::Arc<@T>}::new"]
def alloc.sync.Arc.new {T : Type _} : T → Result (alloc.sync.Arc T)
  := fun value => .ok value

/-- [alloc::sync::{impl core::clone::Clone for alloc::sync::Arc<T>}::clone]:
    Source: '/rustc/library/alloc/src/sync.rs', lines 2521:4-2521:32
    Name pattern: [alloc::sync::{core::clone::Clone<alloc::sync::Arc<@T>>}::clone]
    Visibility: public -/
@[rust_fun "alloc::sync::{core::clone::Clone<alloc::sync::Arc<@T>>}::clone"]
def alloc.sync.Arc.Insts.CoreCloneClone.clone
  {T : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.sync.Arc T → Result (alloc.sync.Arc T)
  := fun value => .ok value

/-- [alloc::sync::{impl core::convert::AsRef<T> for alloc::sync::Arc<T>}::as_ref]:
    Source: '/rustc/library/alloc/src/sync.rs', lines 4383:4-4383:26
    Name pattern: [alloc::sync::{core::convert::AsRef<alloc::sync::Arc<@T>, @T>}::as_ref]
    Visibility: public -/
@[rust_fun
  "alloc::sync::{core::convert::AsRef<alloc::sync::Arc<@T>, @T>}::as_ref"]
def alloc.sync.Arc.Insts.CoreConvertAsRef.as_ref
  {T : Type _} (A : Type _) : alloc.sync.Arc T → Result T
  := fun value => .ok value

/-- [chrono::datetime::{impl core::cmp::Ord for chrono::datetime::DateTime<Tz, Clause0_Offset>}::cmp]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/datetime/mod.rs', lines 1508:4-1508:51
    Name pattern: [chrono::datetime::{core::cmp::Ord<chrono::datetime::DateTime<@Tz, @Clause0_Offset>>}::cmp]
    Visibility: public -/
@[rust_fun
  "chrono::datetime::{core::cmp::Ord<chrono::datetime::DateTime<@Tz, @Clause0_Offset>>}::cmp"]
def chrono.datetime.DateTime.Insts.CoreCmpOrd.cmp
  {Tz : Type _} {Clause0_Offset : Type _} (offsetTimeZoneInst :
  chrono.offset.TimeZone Tz Clause0_Offset) :
  chrono.datetime.DateTime Tz Clause0_Offset → chrono.datetime.DateTime Tz
    Clause0_Offset → Result Ordering
  := fun left right => .ok (SessionNative.timestampCompare left right)

/-- [rmcp::model::{rmcp::model::ErrorData}::internal_error]:
    Source: '/cargo/git/checkouts/rust-sdk-ed0400b05853d1c3/faa4e9d/crates/rmcp/src/model.rs', lines 714:4-714:93
    Name pattern: [rmcp::model::{rmcp::model::ErrorData}::internal_error]
    Visibility: public -/
@[rust_fun "rmcp::model::{rmcp::model::ErrorData}::internal_error"]
def rmcp.model.ErrorData.internal_error
  {T0 : Type _} (coreconvertIntoT0CowStaticStrStringInst : core.convert.Into T0
  (alloc.borrow.Cow Str String)) :
  T0 → Option serde_json.value.Value → Result rmcp.model.ErrorData
  := fun message data => do let text ← coreconvertIntoT0CowStaticStrStringInst.into message; .ok { code := (-32603)#i32, message := text, data }

/-- [url::{impl core::clone::Clone for url::Url}::clone]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/url-2.5.8/src/lib.rs', lines 227:9-227:14
    Name pattern: [url::{core::clone::Clone<url::Url>}::clone]
    Visibility: public -/
@[rust_fun "url::{core::clone::Clone<url::Url>}::clone"]
def url.Url.Insts.CoreCloneClone.clone : url.Url → Result url.Url
  := fun value => .ok value

/-- [url::{url::Url}::as_str]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/url-2.5.8/src/lib.rs', lines 653:4-653:32
    Name pattern: [url::{url::Url}::as_str]
    Visibility: public -/
@[rust_fun "url::{url::Url}::as_str"]
-- Exact UTF-8 bytes in the pinned Str = Slice U8 representation.
-- Only native pointer-width representability is required. The oversized branch
-- is explicit model failure, not a new Rust URL check or a universal size axiom.
def url.Url.as_str : url.Url → Result Str
  := fun value =>
    let bytes : List U8 := value.value.toByteArray.toList.map (fun x =>
      ⟨x.toNat, by cases x; simp only [UInt8.toNat_ofBitVec,
        UScalarTy.U8_numBits_eq, Nat.reducePow]; omega⟩)
    if representable : value.value.toByteArray.size ≤ Usize.max
    then .ok (Slice.from bytes (by
      simpa only [bytes, List.length_map, ByteArray.length_toList] using representable))
    else .fail .maximumSizeExceeded

/-- [veoveo_audit_contract::model::{impl core::clone::Clone for veoveo_audit_contract::model::AuditRequest}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/audit/contract/src/model.rs', lines 60:16-60:21
    Name pattern: [veoveo_audit_contract::model::{core::clone::Clone<veoveo_audit_contract::model::AuditRequest>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_audit_contract::model::{core::clone::Clone<veoveo_audit_contract::model::AuditRequest>}::clone"]
def veoveo_audit_contract.model.AuditRequest.Insts.CoreCloneClone.clone
  :
  veoveo_audit_contract.model.AuditRequest → Result
    veoveo_audit_contract.model.AuditRequest
  := fun value => .ok value

/-- [veoveo_hub_sdk::execution::{impl core::clone::Clone for veoveo_hub_sdk::execution::ExecutionContext}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/execution.rs', lines 31:16-31:21
    Name pattern: [veoveo_hub_sdk::execution::{core::clone::Clone<veoveo_hub_sdk::execution::ExecutionContext>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::execution::{core::clone::Clone<veoveo_hub_sdk::execution::ExecutionContext>}::clone"]
def veoveo_hub_sdk.execution.ExecutionContext.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.execution.ExecutionContext → Result
    veoveo_hub_sdk.execution.ExecutionContext
  := fun value => .ok value

/-- [veoveo_hub_sdk::execution::{impl core::cmp::PartialEq<veoveo_hub_sdk::execution::ExecutionContext> for veoveo_hub_sdk::execution::ExecutionContext}::eq]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/execution.rs', lines 31:23-31:32
    Name pattern: [veoveo_hub_sdk::execution::{core::cmp::PartialEq<veoveo_hub_sdk::execution::ExecutionContext, veoveo_hub_sdk::execution::ExecutionContext>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::execution::{core::cmp::PartialEq<veoveo_hub_sdk::execution::ExecutionContext, veoveo_hub_sdk::execution::ExecutionContext>}::eq"]
def veoveo_hub_sdk.execution.ExecutionContext.Insts.CoreCmpPartialEqExecutionContext.eq
  :
  veoveo_hub_sdk.execution.ExecutionContext →
    veoveo_hub_sdk.execution.ExecutionContext → Result Bool
  := fun left right => .ok (decide (left = right))

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::Principal}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 61:16-61:21
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::Principal>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::Principal>}::clone"]
def veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.Principal → Result
    veoveo_hub_sdk.identity.Principal
  := fun value => .ok value

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::AccessTokenSubject}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 127:16-127:21
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::AccessTokenSubject>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::AccessTokenSubject>}::clone"]
def veoveo_hub_sdk.identity.AccessTokenSubject.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.AccessTokenSubject → Result
    veoveo_hub_sdk.identity.AccessTokenSubject
  := fun value => .ok value

/-- [veoveo_hub_sdk::identity::{impl core::clone::Clone for veoveo_hub_sdk::identity::OpaqueTaskId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/identity.rs', lines 183:0-183:43
    Name pattern: [veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::OpaqueTaskId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::identity::{core::clone::Clone<veoveo_hub_sdk::identity::OpaqueTaskId>}::clone"]
def veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.identity.OpaqueTaskId → Result
    veoveo_hub_sdk.identity.OpaqueTaskId
  := fun value => .ok value

/-- [veoveo_hub_sdk::internal_auth::exchange::{impl core::clone::Clone for veoveo_hub_sdk::internal_auth::exchange::DelegationContext}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/internal_auth/exchange.rs', lines 27:16-27:21
    Name pattern: [veoveo_hub_sdk::internal_auth::exchange::{core::clone::Clone<veoveo_hub_sdk::internal_auth::exchange::DelegationContext>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::internal_auth::exchange::{core::clone::Clone<veoveo_hub_sdk::internal_auth::exchange::DelegationContext>}::clone"]
def veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.internal_auth.exchange.DelegationContext → Result
    veoveo_hub_sdk.internal_auth.exchange.DelegationContext
  := fun value => .ok value

/-- [veoveo_hub_sdk::internal_auth::request::{impl core::clone::Clone for veoveo_hub_sdk::internal_auth::request::HubRequestContext}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/internal_auth/request.rs', lines 6:16-6:21
    Name pattern: [veoveo_hub_sdk::internal_auth::request::{core::clone::Clone<veoveo_hub_sdk::internal_auth::request::HubRequestContext>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::internal_auth::request::{core::clone::Clone<veoveo_hub_sdk::internal_auth::request::HubRequestContext>}::clone"]
def veoveo_hub_sdk.internal_auth.request.HubRequestContext.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.internal_auth.request.HubRequestContext → Result
    veoveo_hub_sdk.internal_auth.request.HubRequestContext
  := fun value => .ok value

/-- [veoveo_hub_sdk::internal_auth::{impl core::clone::Clone for veoveo_hub_sdk::internal_auth::ActingServer}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/internal_auth.rs', lines 290:16-290:21
    Name pattern: [veoveo_hub_sdk::internal_auth::{core::clone::Clone<veoveo_hub_sdk::internal_auth::ActingServer>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::internal_auth::{core::clone::Clone<veoveo_hub_sdk::internal_auth::ActingServer>}::clone"]
def veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.internal_auth.ActingServer → Result
    veoveo_hub_sdk.internal_auth.ActingServer
  := fun value => .ok value

-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [veoveo_hub_sdk::internal_auth::{veoveo_hub_sdk::internal_auth::HubInternalTokenIssuer}::issue]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/internal_auth.rs', lines 336:4-339:59
    Name pattern: [veoveo_hub_sdk::internal_auth::{veoveo_hub_sdk::internal_auth::HubInternalTokenIssuer}::issue]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::internal_auth::{veoveo_hub_sdk::internal_auth::HubInternalTokenIssuer}::issue"]
axiom veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer.issue
  :
  veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer →
    veoveo_hub_sdk.internal_auth.ForwardedRequest → Result
    (core.result.Result veoveo_hub_sdk.internal_auth.IssuedHubInternalToken
    veoveo_hub_sdk.internal_auth.InternalTokenError)

-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [veoveo_hub_sdk::internal_auth::{impl core::fmt::Display for veoveo_hub_sdk::internal_auth::InternalTokenError}::fmt]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/internal_auth.rs', lines 920:4-920:60
    Name pattern: [veoveo_hub_sdk::internal_auth::{core::fmt::Display<veoveo_hub_sdk::internal_auth::InternalTokenError>}::fmt]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::internal_auth::{core::fmt::Display<veoveo_hub_sdk::internal_auth::InternalTokenError>}::fmt"]
axiom veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt
  :
  veoveo_hub_sdk.internal_auth.InternalTokenError → core.fmt.Formatter →
    Result ((core.result.Result Unit core.fmt.Error) × core.fmt.Formatter)

/-- [veoveo_hub_sdk::registration::declaration::{impl core::clone::Clone for veoveo_hub_sdk::registration::declaration::ResourceIndicator}::clone]:
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/239622d/crates/veoveo-hub-sdk/src/registration/declaration.rs', lines 128:11-128:16
    Name pattern: [veoveo_hub_sdk::registration::declaration::{core::clone::Clone<veoveo_hub_sdk::registration::declaration::ResourceIndicator>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_hub_sdk::registration::declaration::{core::clone::Clone<veoveo_hub_sdk::registration::declaration::ResourceIndicator>}::clone"]
def veoveo_hub_sdk.registration.declaration.ResourceIndicator.Insts.CoreCloneClone.clone
  :
  veoveo_hub_sdk.registration.declaration.ResourceIndicator → Result
    veoveo_hub_sdk.registration.declaration.ResourceIndicator
  := fun value => .ok value

/-- [veoveo_types::authority::{impl core::clone::Clone for veoveo_types::authority::InvocationAuthority}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/authority.rs', lines 72:16-72:21
    Name pattern: [veoveo_types::authority::{core::clone::Clone<veoveo_types::authority::InvocationAuthority>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::authority::{core::clone::Clone<veoveo_types::authority::InvocationAuthority>}::clone"]
def veoveo_types.authority.InvocationAuthority.Insts.CoreCloneClone.clone
  :
  veoveo_types.authority.InvocationAuthority → Result
    veoveo_types.authority.InvocationAuthority
  := fun value => .ok value

/-- [veoveo_types::execution::{impl core::cmp::Ord for veoveo_types::execution::DeploymentId}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/execution.rs', lines 18:0-18:46
    Name pattern: [veoveo_types::execution::{core::cmp::Ord<veoveo_types::execution::DeploymentId>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::execution::{core::cmp::Ord<veoveo_types::execution::DeploymentId>}::cmp"]
def veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.execution.DeploymentId → veoveo_types.execution.DeploymentId
    → Result Ordering
  := fun left right => .ok (compare left.value right.value)

/-- [veoveo_types::execution::{impl core::cmp::PartialOrd<veoveo_types::execution::DeploymentId> for veoveo_types::execution::DeploymentId}::partial_cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/execution.rs', lines 18:0-18:46
    Name pattern: [veoveo_types::execution::{core::cmp::PartialOrd<veoveo_types::execution::DeploymentId, veoveo_types::execution::DeploymentId>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::execution::{core::cmp::PartialOrd<veoveo_types::execution::DeploymentId, veoveo_types::execution::DeploymentId>}::partial_cmp"]
def veoveo_types.execution.DeploymentId.Insts.CoreCmpPartialOrdDeploymentId.partial_cmp
  :
  veoveo_types.execution.DeploymentId → veoveo_types.execution.DeploymentId
    → Result (Option Ordering)
  := fun left right => .ok (some (compare left.value right.value))

/-- [veoveo_types::execution::{impl core::cmp::PartialEq<veoveo_types::execution::DeploymentId> for veoveo_types::execution::DeploymentId}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/execution.rs', lines 18:0-18:46
    Name pattern: [veoveo_types::execution::{core::cmp::PartialEq<veoveo_types::execution::DeploymentId, veoveo_types::execution::DeploymentId>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::execution::{core::cmp::PartialEq<veoveo_types::execution::DeploymentId, veoveo_types::execution::DeploymentId>}::eq"]
def veoveo_types.execution.DeploymentId.Insts.CoreCmpPartialEqDeploymentId.eq
  :
  veoveo_types.execution.DeploymentId → veoveo_types.execution.DeploymentId
    → Result Bool
  := fun left right => .ok (decide (left = right))

/-- [veoveo_types::platform_names::{impl core::cmp::PartialEq<veoveo_types::platform_names::ServerSlug> for veoveo_types::platform_names::ServerSlug}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 5:0-5:38
    Name pattern: [veoveo_types::platform_names::{core::cmp::PartialEq<veoveo_types::platform_names::ServerSlug, veoveo_types::platform_names::ServerSlug>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::platform_names::{core::cmp::PartialEq<veoveo_types::platform_names::ServerSlug, veoveo_types::platform_names::ServerSlug>}::eq"]
def veoveo_types.platform_names.ServerSlug.Insts.CoreCmpPartialEqServerSlug.eq
  :
  veoveo_types.platform_names.ServerSlug →
    veoveo_types.platform_names.ServerSlug → Result Bool
  := fun left right => .ok (decide (left = right))

/-- [veoveo_types::platform_names::{impl core::clone::Clone for veoveo_types::platform_names::OAuthClientId}::clone]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 39:0-39:36
    Name pattern: [veoveo_types::platform_names::{core::clone::Clone<veoveo_types::platform_names::OAuthClientId>}::clone]
    Visibility: public -/
@[rust_fun
  "veoveo_types::platform_names::{core::clone::Clone<veoveo_types::platform_names::OAuthClientId>}::clone"]
def veoveo_types.platform_names.OAuthClientId.Insts.CoreCloneClone.clone
  :
  veoveo_types.platform_names.OAuthClientId → Result
    veoveo_types.platform_names.OAuthClientId
  := fun value => .ok value

-- OPEN NATIVE OBSERVATION: no success/output contract is asserted here.
/-- [veoveo_hub::registration::snapshot::{veoveo_hub::registration::snapshot::RegistrationSnapshot}::build_namespaces]:
    Source: 'crates/veoveo-hub/src/registration/snapshot.rs', lines 98:4-128:5 -/
axiom registration.snapshot.RegistrationSnapshot.build_namespaces
  :
  alloc.collections.btree.map.BTreeMap veoveo_types.execution.DeploymentId
    (alloc.sync.Arc registration.snapshot.RegisteredServer) alloc.alloc.Global
    → Result (alloc.collections.btree.map.BTreeMap
    veoveo_types.platform_names.ServerSlug registration.snapshot.Namespace
    alloc.alloc.Global)



end
end RequestCompositionHub


namespace Aeneas.Std.core.iter.traits.iterator

/-- Method spelling emitted for generic iterator dictionary dispatch. Forward
verbatim to the existing operational default; no extra mapping law is assumed. -/
def Iterator.map
    {Self : Type _} {B : Type _} {F : Type _} {Clause0_Item : Type _}
    (iterator : Iterator Self Clause0_Item)
    (mapper : Aeneas.Std.core.ops.function.FnMut F Clause0_Item B) :=
  RequestCompositionHub.core.iter.traits.iterator.Iterator.map.default iterator mapper

end Aeneas.Std.core.iter.traits.iterator
