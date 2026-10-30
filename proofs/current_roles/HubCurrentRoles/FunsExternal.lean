-- Handwritten external operational models. Generated policy bodies are untouched.
-- These definitions model native primitives; their Rust refinement is not proved here.
module
public import Aeneas
public import HubCurrentRoles.Types
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
open veoveo_hub


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
