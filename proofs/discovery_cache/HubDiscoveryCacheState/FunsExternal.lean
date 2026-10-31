-- Handwritten external primitive models based on generated interfaces.
-- Native representation/refinement boundaries are documented with each model.
module
public import Aeneas
public import HubDiscoveryCacheState.Types
public import Mathlib.Data.String.Basic
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
open HubDiscoveryCacheModel

namespace HubDiscoveryCacheModel

/- Third-party finite-map model. Native refinement requires coherent Borrow,
unique keys, lawful total comparison, and strict sorted order for pop_first.
These functions preserve comparator/Borrow fail/div; comparison trace counts,
allocation and destructor effects are not claimed. No own transition is opaque. -/
namespace NativeMap

def getBy {K : Type _} {V : Type _} {Q : Type _} (borrow : K → Result Q)
    (compare : Q → Q → Result Ordering) : List (K × V) → Q → Result (Option V)
  | [], _ => .ok none
  | (key, value) :: rest, query => do
    let borrowed ← borrow key
    let order ← compare borrowed query
    if order == .eq then .ok (some value) else getBy borrow compare rest query

def removeBy {K : Type _} {V : Type _} {Q : Type _} (borrow : K → Result Q)
    (compare : Q → Q → Result Ordering) :
    List (K × V) → Q → Result (Option V × List (K × V))
  | [], _ => .ok (none, [])
  | (key, value) :: rest, query => do
    let borrowed ← borrow key
    let order ← compare borrowed query
    if order == .eq then .ok (some value, rest)
    else
      let (old, remaining) ← removeBy borrow compare rest query
      .ok (old, (key, value) :: remaining)

def insertBy {K : Type _} {V : Type _} (compare : K → K → Result Ordering) :
    List (K × V) → K → V → Result (Option V × List (K × V))
  | [], key, value => .ok (none, [(key, value)])
  | (oldKey, oldValue) :: rest, key, value => do
    let order ← compare key oldKey
    match order with
    | .lt => .ok (none, (key, value) :: (oldKey, oldValue) :: rest)
    | .eq => .ok (some oldValue, (oldKey, value) :: rest)
    | .gt =>
      let (old, remaining) ← insertBy compare rest key value
      .ok (old, (oldKey, oldValue) :: remaining)

def lexCompare {T : Type _} (compare : T → T → Result Ordering) :
    List T → List T → Result Ordering
  | [], [] => .ok .eq
  | [], _ :: _ => .ok .lt
  | _ :: _, [] => .ok .gt
  | left :: rest, right :: tail => do
    let order ← compare left right
    match order with
    | .eq => lexCompare compare rest tail
    | other => .ok other

end NativeMap

/-- [core::array::{impl core::cmp::Ord for [T; N]}::cmp]:
    Source: '/rustc/library/core/src/array/mod.rs', lines 455:4-455:45
    Name pattern: [core::array::{core::cmp::Ord<[@T; @N]>}::cmp]
    Visibility: public -/
@[rust_fun "core::array::{core::cmp::Ord<[@T; @N]>}::cmp"]
def Array.Insts.CoreCmpOrd.cmp
  {T : Type _} {N : Std.Usize} (cmpOrdInst : core.cmp.Ord T) :
  Array T N → Array T N → Result Ordering
  := fun left right => NativeMap.lexCompare cmpOrdInst.cmp left.val right.val

/-- [core::borrow::{impl core::borrow::Borrow<T> for T}::borrow]:
    Source: '/rustc/library/core/src/borrow.rs', lines 214:4-214:26
    Name pattern: [core::borrow::{core::borrow::Borrow<@T, @T>}::borrow]
    Visibility: public -/
@[rust_fun "core::borrow::{core::borrow::Borrow<@T, @T>}::borrow"]
def core.borrow.Borrow.Blanket.borrow {T : Type _} : T → Result T
  := fun value => .ok value

/-- [core::option::{core::option::Option<T>}::is_none_or]:
    Source: '/rustc/library/core/src/option.rs', lines 708:4-708:95
    Name pattern: [core::option::{core::option::Option<@T>}::is_none_or]
    Visibility: public -/
@[rust_fun "core::option::{core::option::Option<@T>}::is_none_or"]
def core.option.Option.is_none_or
  {T : Type _} {T1 : Type _} (opsfunctionFnOnceT1TupleTBoolInst :
  core.ops.function.FnOnce T1 T Bool) :
  Option T → T1 → Result Bool
  := fun value predicate => match value with
  | none => .ok true
  | some item => opsfunctionFnOnceT1TupleTBoolInst.call_once predicate item

/-- [core::option::{impl core::cmp::PartialEq<core::option::Option<T>> for core::option::Option<T>}::eq]:
    Source: '/rustc/library/core/src/option.rs', lines 2441:4-2441:38
    Name pattern: [core::option::{core::cmp::PartialEq<core::option::Option<@T>, core::option::Option<@T>>}::eq]
    Visibility: public -/
@[rust_fun
  "core::option::{core::cmp::PartialEq<core::option::Option<@T>, core::option::Option<@T>>}::eq"]
def core.option.Option.Insts.CoreCmpPartialEqOption.eq
  {T : Type _} (cmpPartialEqInst : core.cmp.PartialEq T T) :
  Option T → Option T → Result Bool
  := fun left right => match left, right with
  | none, none => .ok true
  | some left, some right => cmpPartialEqInst.eq left right
  | _, _ => .ok false

/-- [core::option::{impl core::ops::try_trait::Try for core::option::Option<T>}::branch]:
    Source: '/rustc/library/core/src/option.rs', lines 2875:4-2875:64
    Name pattern: [core::option::{core::ops::try_trait::Try<core::option::Option<@T>>}::branch]
    Visibility: public -/
@[rust_fun
  "core::option::{core::ops::try_trait::Try<core::option::Option<@T>>}::branch"]
def core.option.Option.Insts.CoreOpsTry_traitTry.branch
  {T : Type _} :
  Option T → Result (core.ops.control_flow.ControlFlow (Option Never) T)
  := fun value => match value with
  | none => .ok (.Break none)
  | some item => .ok (.Continue item)

/-- [core::option::{impl core::ops::try_trait::FromResidual<core::option::Option<!>> for core::option::Option<T>}::from_residual]:
    Source: '/rustc/library/core/src/option.rs', lines 2889:4-2889:49
    Name pattern: [core::option::{core::ops::try_trait::FromResidual<core::option::Option<@T>, core::option::Option<!>>}::from_residual]
    Visibility: public -/
@[rust_fun
  "core::option::{core::ops::try_trait::FromResidual<core::option::Option<@T>, core::option::Option<!>>}::from_residual"]
def
  core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual
  (T : Type _) : Option Never → Result (Option T)
  := fun value => match value with
  | none => .ok none
  | some impossible => nomatch impossible

/-- [core::time::{core::time::Duration}::from_millis]:
    Source: '/rustc/library/core/src/time.rs', lines 244:4-244:53
    Name pattern: [core::time::{core::time::Duration}::from_millis]
    Visibility: public -/
@[rust_fun "core::time::{core::time::Duration}::from_millis"]
def core.time.Duration.from_millis : Std.U64 → Result core.time.Duration
  := fun value => .ok ⟨value.val * 1000000⟩

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::clear]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 672:4-672:27
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::clear]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::clear"]
def alloc.collections.btree.map.BTreeMap.clear
  {K : Type _} {V : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.map.BTreeMap K V A → Result
    (alloc.collections.btree.map.BTreeMap K V A)
  := fun _ => .ok []

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
  := NativeMap.getBy coreborrowBorrowInst.borrow corecmpOrdInst1.cmp

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::pop_first]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 874:4-876:15
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::pop_first]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::pop_first"]
def alloc.collections.btree.map.BTreeMap.pop_first
  {K : Type _} {V : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) (corecmpOrdInst : core.cmp.Ord K) :
  alloc.collections.btree.map.BTreeMap K V A → Result ((Option (K × V)) ×
    (alloc.collections.btree.map.BTreeMap K V A))
  := fun entries => match entries with
  | [] => .ok (none, [])
  | first :: rest => .ok (some first, rest)

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::contains_key]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 981:4-984:15
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::contains_key]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::contains_key"]
def alloc.collections.btree.map.BTreeMap.contains_key
  {K : Type _} {V : Type _} {A : Type _} {Q : Type _}
  (coreallocAllocatorCloneInst : core.alloc.AllocatorClone A)
  (coreborrowBorrowInst : core.borrow.Borrow K Q) (corecmpOrdInst :
  core.cmp.Ord K) (corecmpOrdInst1 : core.cmp.Ord Q) :
  alloc.collections.btree.map.BTreeMap K V A → Q → Result Bool
  := fun entries key => do
  let found ← NativeMap.getBy coreborrowBorrowInst.borrow corecmpOrdInst1.cmp entries key
  .ok found.isSome

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::insert]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 1047:4-1049:15
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::insert]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::insert"]
def alloc.collections.btree.map.BTreeMap.insert
  {K : Type _} {V : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) (corecmpOrdInst : core.cmp.Ord K) :
  alloc.collections.btree.map.BTreeMap K V A → K → V → Result ((Option V)
    × (alloc.collections.btree.map.BTreeMap K V A))
  := NativeMap.insertBy corecmpOrdInst.cmp

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::remove]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 1131:4-1134:15
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::remove]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::remove"]
def alloc.collections.btree.map.BTreeMap.remove
  {K : Type _} {V : Type _} {A : Type _} {Q : Type _}
  (coreallocAllocatorCloneInst : core.alloc.AllocatorClone A)
  (coreborrowBorrowInst : core.borrow.Borrow K Q) (corecmpOrdInst :
  core.cmp.Ord K) (corecmpOrdInst1 : core.cmp.Ord Q) :
  alloc.collections.btree.map.BTreeMap K V A → Q → Result ((Option V) ×
    (alloc.collections.btree.map.BTreeMap K V A))
  := NativeMap.removeBy coreborrowBorrowInst.borrow corecmpOrdInst1.cmp

/-- [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<K, V, A>}::len]:
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 2834:4-2834:36
    Name pattern: [alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::len]
    Visibility: public -/
@[rust_fun
  "alloc::collections::btree::map::{alloc::collections::btree::map::BTreeMap<@K, @V, @A>}::len"]
def alloc.collections.btree.map.BTreeMap.len
  {K : Type _} {V : Type _} {A : Type _} (coreallocAllocatorCloneInst :
  core.alloc.AllocatorClone A) :
  alloc.collections.btree.map.BTreeMap K V A → Result Std.Usize
  := fun entries =>
  if bound : entries.length ≤ Std.Usize.max then
    .ok (Std.Usize.ofNatCore entries.length (by scalar_tac))
  else .fail .maximumSizeExceeded

/-- [tokio::time::instant::{impl core::cmp::PartialEq<tokio::time::instant::Instant> for tokio::time::instant::Instant}::eq]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/time/instant.rs', lines 33:26-33:35
    Name pattern: [tokio::time::instant::{core::cmp::PartialEq<tokio::time::instant::Instant, tokio::time::instant::Instant>}::eq]
    Visibility: public -/
@[rust_fun
  "tokio::time::instant::{core::cmp::PartialEq<tokio::time::instant::Instant, tokio::time::instant::Instant>}::eq"]
def tokio.time.instant.Instant.Insts.CoreCmpPartialEqInstant.eq
  : tokio.time.instant.Instant → tokio.time.instant.Instant → Result Bool
  := fun left right => .ok (decide (left = right))

/-- [tokio::time::instant::{impl core::cmp::PartialOrd<tokio::time::instant::Instant> for tokio::time::instant::Instant}::partial_cmp]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/time/instant.rs', lines 33:37-33:47
    Name pattern: [tokio::time::instant::{core::cmp::PartialOrd<tokio::time::instant::Instant, tokio::time::instant::Instant>}::partial_cmp]
    Visibility: public -/
@[rust_fun
  "tokio::time::instant::{core::cmp::PartialOrd<tokio::time::instant::Instant, tokio::time::instant::Instant>}::partial_cmp"]
def tokio.time.instant.Instant.Insts.CoreCmpPartialOrdInstant.partial_cmp
  :
  tokio.time.instant.Instant → tokio.time.instant.Instant → Result (Option
    Ordering)
  := fun left right => .ok (some (compare left.nanoseconds right.nanoseconds))

/- Explicit third-party normal-return/overflow boundary. Native Instant addition
can panic; no total native-addition assumption or millisecond truncation is made.
Operation contracts retain this Result bind; normal-return histories quantify its
observed result. This is not a cache transition or a clock/freshness axiom. -/
/-- [tokio::time::instant::{impl core::ops::arith::Add<core::time::Duration, tokio::time::instant::Instant> for tokio::time::instant::Instant}::add]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/time/instant.rs', lines 160:4-160:44
    Name pattern: [tokio::time::instant::{core::ops::arith::Add<tokio::time::instant::Instant, core::time::Duration, tokio::time::instant::Instant>}::add]
    Visibility: public -/
@[rust_fun
  "tokio::time::instant::{core::ops::arith::Add<tokio::time::instant::Instant, core::time::Duration, tokio::time::instant::Instant>}::add"]
axiom tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add
  :
  tokio.time.instant.Instant → core.time.Duration → Result
    tokio.time.instant.Instant

/-- [uuid::{impl core::cmp::PartialEq<uuid::Uuid> for uuid::Uuid}::eq]:
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/uuid-1.25.0/src/lib.rs', lines 446:31-446:40
    Name pattern: [uuid::{core::cmp::PartialEq<uuid::Uuid, uuid::Uuid>}::eq]
    Visibility: public -/
@[rust_fun "uuid::{core::cmp::PartialEq<uuid::Uuid, uuid::Uuid>}::eq"]
def uuid.Uuid.Insts.CoreCmpPartialEqUuid.eq
  : uuid.Uuid → uuid.Uuid → Result Bool
  := fun left right => .ok (decide (left = right))

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

/-- [veoveo_types::identity::{impl core::cmp::Ord for veoveo_types::identity::PrincipalId}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 9:0-9:69
    Name pattern: [veoveo_types::identity::{core::cmp::Ord<veoveo_types::identity::PrincipalId>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::identity::{core::cmp::Ord<veoveo_types::identity::PrincipalId>}::cmp"]
def veoveo_types.identity.PrincipalId.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.identity.PrincipalId → veoveo_types.identity.PrincipalId →
    Result Ordering
  := fun left right => .ok (compare left.value right.value)

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

/-- [veoveo_types::platform_names::{impl core::cmp::Ord for veoveo_types::platform_names::ServerSlug}::cmp]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 5:0-5:38
    Name pattern: [veoveo_types::platform_names::{core::cmp::Ord<veoveo_types::platform_names::ServerSlug>}::cmp]
    Visibility: public -/
@[rust_fun
  "veoveo_types::platform_names::{core::cmp::Ord<veoveo_types::platform_names::ServerSlug>}::cmp"]
def veoveo_types.platform_names.ServerSlug.Insts.CoreCmpOrd.cmp
  :
  veoveo_types.platform_names.ServerSlug →
    veoveo_types.platform_names.ServerSlug → Result Ordering
  := fun left right => .ok (compare left.value right.value)

/-- [veoveo_types::platform_names::{impl core::cmp::PartialEq<veoveo_types::platform_names::ServerSlug> for veoveo_types::platform_names::ServerSlug}::eq]:
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 5:0-5:38
    Name pattern: [veoveo_types::platform_names::{core::cmp::PartialEq<veoveo_types::platform_names::ServerSlug, veoveo_types::platform_names::ServerSlug>}::eq]
    Visibility: public -/
@[rust_fun
  "veoveo_types::platform_names::{core::cmp::PartialEq<veoveo_types::platform_names::ServerSlug, veoveo_types::platform_names::ServerSlug>}::eq"]
def
  veoveo_types.platform_names.ServerSlug.Insts.CoreCmpPartialEqServerSlug.eq
  :
  veoveo_types.platform_names.ServerSlug →
    veoveo_types.platform_names.ServerSlug → Result Bool
  := fun left right => .ok (decide (left = right))


end HubDiscoveryCacheModel
