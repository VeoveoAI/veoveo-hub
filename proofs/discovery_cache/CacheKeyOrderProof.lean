import CacheKeyFrameProof
import Mathlib.Order.Prod.Lex.Basic
import Mathlib.Data.List.Lex

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof

namespace CacheKeyOrderProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Full-field, lossless lexical code in the same order as the actual generated
Ord implementation. This is lexicographic Product order, not pointwise order. -/
abbrev KeyCode := Nat ×ₗ _root_.String ×ₗ List Nat ×ₗ _root_.String

def code (key : Key) : KeyCode :=
  toLex (key.catalog_generation.val,
    toLex (key.principal.value,
      toLex (key.authorization_fingerprint.val.map (fun byte => byte.val), key.server.value)))

theorem code_fields (key : Key) :
    (ofLex (code key)).1 = key.catalog_generation.val ∧
    (ofLex (ofLex (code key)).2).1 = key.principal.value ∧
    (ofLex (ofLex (ofLex (code key)).2).2).1 =
      key.authorization_fingerprint.val.map (fun byte => byte.val) ∧
    (ofLex (ofLex (ofLex (code key)).2).2).2 = key.server.value :=
  ⟨rfl, rfl, rfl, rfl⟩

private theorem then_eq_then (first next : Ordering) :
    thenOrder first next = first.then next := by
  cases first <;> rfl

private theorem bytes_compare_code (left right : List U8) :
    lexOrder (fun a b : U8 => compare a.val b.val) left right =
      compare (left.map (fun byte => byte.val)) (right.map (fun byte => byte.val)) := by
  induction left generalizing right with
  | nil => cases right <;> rfl
  | cons head rest recursive =>
    cases right with
    | nil => rfl
    | cons other tail =>
      simp only [lexOrder, List.map_cons, List.compare_cons_cons, recursive]
      exact then_eq_then _ _

/-- Comparison agrees on every field/byte with the library lexical order. No
finite hash encoding is assumed injective and no field is dropped or truncated. -/
theorem keyOrder_code (left right : Key) :
    keyOrder left right = compare (code left) (code right) := by
  change keyOrder left right =
    (compare left.catalog_generation.val right.catalog_generation.val).then
      ((compare left.principal.value right.principal.value).then
        ((compare (left.authorization_fingerprint.val.map (fun byte => byte.val))
          (right.authorization_fingerprint.val.map (fun byte => byte.val))).then
          (compare left.server.value right.server.value)))
  rw [← bytes_compare_code]
  simp only [keyOrder, then_eq_then]

/-- Losslessness follows from the checked full-field comparison/equality
correspondence; it is not merely an asserted roundtrip for a chosen enumeration. -/
theorem code_injective : Function.Injective code := by
  intro left right same
  apply (keyOrder_eq left right).mp
  rw [keyOrder_code, same, ← keyOrder_code]
  exact (keyOrder_eq right right).mpr rfl

theorem actual_comparison_code (left right : Key) :
    keyCmp left right = .ok (compare (code left) (code right)) := by
  rw [key_comparison_exact, keyOrder_code]

abbrev Less (left right : Key) : Prop := keyOrder left right = .lt

/-- Use the exact lex comparator's Std law instances, without changing to the
competing LinearOrder.toOrd comparison instance. -/
theorem less_iff_code (left right : Key) :
    Less left right ↔ compare (code left) (code right) = .lt := by
  change keyOrder left right = .lt ↔ compare (code left) (code right) = .lt
  rw [keyOrder_code]

theorem less_irrefl (key : Key) : ¬Less key key := by
  have same := (keyOrder_eq key key).mpr rfl
  simp [Less, same]

theorem less_trans {first second third : Key} (left : Less first second)
    (right : Less second third) : Less first third := by
  have before := (less_iff_code first second).mp left
  have after := (less_iff_code second third).mp right
  apply (less_iff_code first third).mpr
  exact Std.TransCmp.lt_of_lt_of_le
    (cmp := fun a b : KeyCode => compare a b) before (by simp [after])

theorem less_asymm {first second : Key} (before : Less first second) : ¬Less second first := by
  intro reverse
  exact less_irrefl first (less_trans before reverse)

theorem less_ne {first second : Key} (before : Less first second) : first ≠ second := by
  intro same
  subst second
  exact less_irrefl first before

theorem greater_iff_reverse (first second : Key) :
    keyOrder first second = .gt ↔ Less second first := by
  rw [keyOrder_code, less_iff_code]
  exact Std.OrientedCmp.gt_iff_lt (cmp := fun a b : KeyCode => compare a b)

theorem key_trichotomy (first second : Key) :
    Less first second ∨ first = second ∨ Less second first := by
  cases compared : keyOrder first second with
  | lt => exact Or.inl compared
  | eq => exact Or.inr (Or.inl ((keyOrder_eq first second).mp compared))
  | gt => exact Or.inr (Or.inr ((greater_iff_reverse first second).mp compared))

end
end CacheKeyOrderProof

#print axioms CacheKeyOrderProof.code_injective
#print axioms CacheKeyOrderProof.actual_comparison_code
#print axioms CacheKeyOrderProof.less_trans
#print axioms CacheKeyOrderProof.key_trichotomy

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheKeyOrderProof.code_fields, ``CacheKeyOrderProof.keyOrder_code,
      ``CacheKeyOrderProof.code_injective, ``CacheKeyOrderProof.actual_comparison_code,
      ``CacheKeyOrderProof.less_iff_code, ``CacheKeyOrderProof.less_irrefl,
      ``CacheKeyOrderProof.less_trans, ``CacheKeyOrderProof.less_asymm,
      ``CacheKeyOrderProof.less_ne, ``CacheKeyOrderProof.greater_iff_reverse,
      ``CacheKeyOrderProof.key_trichotomy] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected actual cache-key order axiom in {declaration}: {dependencies}"
