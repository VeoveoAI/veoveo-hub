module
public import HubCurrentRoles.Types
public import RequestCompositionSdk.Types
public import Mathlib.Data.String.Basic
public import Mathlib.Data.Finset.Sort
public import Mathlib.Order.Basic
public import Lean.Util.CollectAxioms
@[expose] public section

open Aeneas Aeneas.Std

namespace SdkInternalAuthBridge
noncomputable section

/-- Representation equivalences preserve identifier bytes; the tags are local
proof-model domains, never new runtime IDs. Concrete records fix both tags. -/
def idEquiv (hubTag sdkTag : Nat) : CurrentRolesId hubTag ≃ RequestCompositionSdk.NativeId sdkTag where
  toFun value := ⟨value.value⟩
  invFun value := ⟨value.value⟩
  left_inv value := by cases value; rfl
  right_inv value := by cases value; rfl

def stringEquiv (hubTag : Nat) : CurrentRolesId hubTag ≃ String where
  toFun value := value.value
  invFun value := ⟨value⟩
  left_inv value := by cases value; rfl
  right_inv _ := rfl

/-- Signing-key and opaque native error payload identities are carried String
records in both models. This proves no key encoding or cryptographic property. -/
def opaqueEquiv (hubTag sdkTag : Nat) : RequestCompositionOpaque hubTag ≃
    RequestCompositionSdk.NativeId sdkTag where
  toFun value := ⟨value.value⟩
  invFun value := ⟨value.value⟩
  left_inv value := by cases value; rfl
  right_inv value := by cases value; rfl

instance sdkIdOrder (tag : Nat) : LinearOrder (RequestCompositionSdk.NativeId tag) :=
  LinearOrder.lift' (fun value => value.value) (by
    intro left right same
    cases left; cases right; cases same; rfl)

/-- The order used for tagged values is exactly their String-value order,
matching the existing Hub taggedComparison model; native byte/order refinement
is not inferred from the model-tag equivalence. -/
theorem sdkId_le_iff {tag : Nat} (left right : RequestCompositionSdk.NativeId tag) :
    left ≤ right ↔ left.value ≤ right.value := Iff.rfl

theorem id_order_correspondence {hubTag sdkTag : Nat} (left right : CurrentRolesId hubTag) :
    idEquiv hubTag sdkTag left ≤ idEquiv hubTag sdkTag right ↔ left.value ≤ right.value := Iff.rfl

/-- Sort by the supplied concrete value order. Finset.toList is never treated as
native traversal. Native comparator/refinement remains a separate correspondence. -/
def orderedSet {H S : Type} (codec : H ≃ S) [LinearOrder S] (values : Finset H) :
    RequestCompositionSdk.OrderedSet S :=
  ⟨(values.map codec.toEmbedding).sort (· ≤ ·)⟩

def unorderedSet {H S : Type} (codec : H ≃ S) (values : RequestCompositionSdk.OrderedSet S) :
    Finset H := by
  classical
  exact (values.values.map codec.symm).toFinset

theorem orderedSet_membership {H S : Type} (codec : H ≃ S) [LinearOrder S]
    (values : Finset H) (value : H) :
    codec value ∈ (orderedSet codec values).values ↔ value ∈ values := by
  simp [orderedSet]

theorem orderedSet_nodup {H S : Type} (codec : H ≃ S) [LinearOrder S] (values : Finset H) :
    (orderedSet codec values).values.Nodup := by
  exact Finset.sort_nodup _ _

theorem orderedSet_sorted {H S : Type} (codec : H ≃ S) [LinearOrder S] (values : Finset H) :
    (orderedSet codec values).values.Pairwise (· ≤ ·) := by
  exact Finset.pairwise_sort _ _

theorem set_roundtrip {H S : Type} (codec : H ≃ S) [LinearOrder S] (values : Finset H) :
    unorderedSet codec (orderedSet codec values) = values := by
  classical
  ext value
  simp [unorderedSet, orderedSet]
  constructor
  · rintro ⟨entry, member, same⟩
    simpa only [same] using member
  · intro member
    exact ⟨codec value, by simpa using member, codec.symm_apply_apply value⟩

/-- Reverse roundtrip needs the native set representation conditions: no duplicate
entries and the same deterministic order. Arbitrary SDK lists are not conflated. -/
theorem ordered_roundtrip {H S : Type} (codec : H ≃ S) [LinearOrder S]
    (values : RequestCompositionSdk.OrderedSet S)
    (unique : values.values.Nodup) (sorted : values.values.Pairwise (· ≤ ·)) :
    orderedSet codec (unorderedSet codec values) = values := by
  classical
  have mapped : (unorderedSet codec values).map codec.toEmbedding = values.values.toFinset := by
    ext value
    simp [unorderedSet]
  cases values with
  | mk entries =>
    change RequestCompositionSdk.OrderedSet.mk
      (((unorderedSet codec ⟨entries⟩).map codec.toEmbedding).sort (· ≤ ·)) = ⟨entries⟩
    rw [mapped]
    apply congrArg RequestCompositionSdk.OrderedSet.mk
    exact (List.toFinset_sort (r := (· ≤ ·)) unique).mpr sorted

abbrev HubTime := Int × Nat

/-- Only physical scalar range; this includes representable leap-second nanos.
The narrower non-leap qualification is stated independently below. -/
structure TimestampPhysical (value : HubTime) : Prop where
  seconds : IScalar.cMin .I64 ≤ value.1 ∧ value.1 ≤ IScalar.cMax .I64
  nanoseconds : value.2 ≤ UScalar.cMax .U32

def timestamp (value : HubTime) (physical : TimestampPhysical value) :
    RequestCompositionSdk.NativeTimestamp := {
  seconds := IScalar.ofInt value.1 physical.seconds
  nanoseconds := UScalar.ofNat value.2 physical.nanoseconds }

def hubTimestamp (value : RequestCompositionSdk.NativeTimestamp) : HubTime :=
  (value.seconds.val, value.nanoseconds.val)

/-- No default or truncated representation for out-of-native-range model values. -/
def timestamp? (value : HubTime) : Option RequestCompositionSdk.NativeTimestamp := by
  classical
  exact if physical : TimestampPhysical value then some (timestamp value physical) else none

def TimestampNonLeap (value : HubTime) : Prop := value.2 < 1000000000

theorem timestamp_roundtrip (value : HubTime) (physical : TimestampPhysical value) :
    hubTimestamp (timestamp value physical) = value := by
  cases value
  simp [hubTimestamp, timestamp, IScalar.ofInt, UScalar.ofNat]

theorem hubTimestamp_physical (value : RequestCompositionSdk.NativeTimestamp) :
    TimestampPhysical (hubTimestamp value) := by
  constructor <;> simp only [hubTimestamp] <;> scalar_tac

theorem sdkTimestamp_roundtrip (value : RequestCompositionSdk.NativeTimestamp) :
    timestamp (hubTimestamp value) (hubTimestamp_physical value) = value := by
  cases value with
  | mk seconds nanos =>
    simp only [timestamp, hubTimestamp, IScalar.ofInt_val, UScalar.ofNat_self_val]

theorem timestamp_checked (value : HubTime) :
    (∀ physical : TimestampPhysical value, timestamp? value = some (timestamp value physical)) ∧
    (¬ TimestampPhysical value → timestamp? value = none) := by
  classical
  unfold timestamp?
  by_cases physical : TimestampPhysical value <;> simp [physical]

theorem nonleap_correspondence (value : HubTime) (physical : TimestampPhysical value) :
    TimestampNonLeap value ↔ (timestamp value physical).nanoseconds.val < 1000000000 := by
  simp [TimestampNonLeap, timestamp, UScalar.ofNat]

#print axioms orderedSet_membership
#print axioms orderedSet_nodup
#print axioms orderedSet_sorted
#print axioms id_order_correspondence
#print axioms nonleap_correspondence
#print axioms set_roundtrip
#print axioms ordered_roundtrip
#print axioms timestamp_roundtrip
#print axioms sdkTimestamp_roundtrip
#print axioms timestamp_checked
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SdkInternalAuthBridge.idEquiv, ``SdkInternalAuthBridge.stringEquiv,
      ``SdkInternalAuthBridge.opaqueEquiv, ``SdkInternalAuthBridge.id_order_correspondence,
      ``SdkInternalAuthBridge.orderedSet_membership, ``SdkInternalAuthBridge.orderedSet_nodup,
      ``SdkInternalAuthBridge.orderedSet_sorted, ``SdkInternalAuthBridge.nonleap_correspondence,
      ``SdkInternalAuthBridge.hubTimestamp_physical, ``SdkInternalAuthBridge.set_roundtrip,
      ``SdkInternalAuthBridge.ordered_roundtrip, ``SdkInternalAuthBridge.timestamp_roundtrip,
      ``SdkInternalAuthBridge.sdkTimestamp_roundtrip, ``SdkInternalAuthBridge.timestamp_checked] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed SDK representation bridge dependency in {declaration}: {found}"
end
end SdkInternalAuthBridge
