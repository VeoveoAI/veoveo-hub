import IteratorProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub NativeSetProof IteratorProof

namespace MembershipPrimitiveProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

theorem disjointBy_exact {T : Type}
    (comparison : T → T → Result Ordering) (order : Comparison T)
    (law : LawfulComparison comparison order) (left right : List T) :
    CurrentRolesNative.disjointBy comparison left right =
      .ok (decide (∀ value ∈ left, value ∉ right)) := by
  induction left with
  | nil => simp [CurrentRolesNative.disjointBy]
  | cons head tail recursive =>
    rw [CurrentRolesNative.disjointBy,
      memberBy_exact (fun value => .ok value) id (by intro value; rfl)
        comparison order law right head]
    simp only [bind_ok]
    by_cases member : head ∈ right
    · simp [member, List.mem_cons, or_imp, forall_and]
    · simp [member, recursive, List.mem_cons, or_imp, forall_and]

theorem set_disjoint_exact {T A : Type}
    (allocator : core.alloc.AllocatorClone A) (native : core.cmp.Ord T)
    (order : Comparison T) (law : LawfulComparison native.cmp order)
    (left right : alloc.collections.btree.set.BTreeSet T A) :
    alloc.collections.btree.set.BTreeSet.is_disjoint allocator native left right =
      .ok (decide (Disjoint left right)) := by
  simpa [alloc.collections.btree.set.BTreeSet.is_disjoint,
    Finset.disjoint_left] using
    disjointBy_exact native.cmp order law left.toList right.toList

/-- Declarative maximum of the original raw values. The returned witness must
occur in the input; an arbitrary upper bound is insufficient. -/
def Maximum {T : Type} (rank : T → Nat) (values : List T) : Option T → Prop
  | none => values = []
  | some best => best ∈ values ∧ ∀ value ∈ values, rank value ≤ rank best

theorem maximum_singleton_seed {T : Type} (rank : T → Nat) (seed : Option T) :
    Maximum rank seed.toList seed := by
  cases seed <;> simp [Maximum]

theorem maximum_add_dominated {T : Type} (rank : T → Nat)
    (values : List T) (kept extra : T) (result : Option T)
    (member : kept ∈ values) (dominated : rank extra ≤ rank kept)
    (maximum : Maximum rank values result) :
    Maximum rank (extra :: values) result := by
  cases result with
  | none =>
    have empty : values = [] := maximum
    simp [empty] at member
  | some best =>
    rcases maximum with ⟨witness, bound⟩
    constructor
    · exact List.mem_cons_of_mem extra witness
    · intro value present
      rcases List.mem_cons.mp present with same | old
      · subst value
        exact le_trans dominated (bound kept member)
      · exact bound value old

section IteratorMaximum
variable {I T : Type}
variable (iterator : core.iter.traits.iterator.Iterator I T)
variable (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)
variable (comparison : T → T → Result Ordering) (rank : T → Nat)
variable (comparisonLaw : ∀ left right,
  comparison left right = .ok (compare (rank left) (rank right)))

include cursorLaw comparisonLaw in
theorem maxLoop_correct (state : I) (seed : Option T)
    (valid : model.valid state) :
    CurrentRolesNative.maxLoop iterator comparison state seed ⦃ result =>
      Maximum rank (seed.toList ++ model.remaining state) result ⦄ := by
  unfold CurrentRolesNative.maxLoop
  step with (cursorLaw state valid) as ⟨entry, next, taken⟩
  rcases taken with ⟨nextValid, entryEq, suffix⟩
  cases remaining : model.remaining state with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simpa [remaining, WP.spec_ok] using maximum_singleton_seed rank seed
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    cases seed with
    | none =>
      have recursive := maxLoop_correct next (some head) nextValid
      apply WP.spec_mono recursive
      intro result specification
      simpa [remaining, suffix] using specification
    | some previous =>
      simp only [comparisonLaw, bind_ok]
      cases ordered : compare (rank previous) (rank head) with
      | lt =>
        have recursive := maxLoop_correct next (some head) nextValid
        apply WP.spec_mono recursive
        intro result specification
        have normalized : Maximum rank (head :: tail) result := by
          simpa [suffix] using specification
        have bound : rank previous ≤ rank head :=
          Nat.le_of_lt (compare_lt_iff_lt.mp ordered)
        simpa [remaining] using maximum_add_dominated rank (head :: tail)
          head previous result (by simp) bound normalized
      | eq =>
        have recursive := maxLoop_correct next (some head) nextValid
        apply WP.spec_mono recursive
        intro result specification
        have normalized : Maximum rank (head :: tail) result := by
          simpa [suffix] using specification
        have bound : rank previous ≤ rank head :=
          Nat.le_of_eq (_root_.compare_eq_iff_eq.mp ordered)
        simpa [remaining] using maximum_add_dominated rank (head :: tail)
          head previous result (by simp) bound normalized
      | gt =>
        have recursive := maxLoop_correct next (some previous) nextValid
        apply WP.spec_mono recursive
        intro result specification
        have normalized : Maximum rank (previous :: tail) result := by
          simpa [suffix] using specification
        have bound : rank head ≤ rank previous :=
          Nat.le_of_lt (compare_gt_iff_gt.mp ordered)
        have enlarged := maximum_add_dominated rank (previous :: tail)
          previous head result (by simp) bound normalized
        cases result <;>
          simpa [Maximum, remaining, or_comm, or_left_comm, or_assoc] using enlarged
termination_by (model.remaining state).length
decreasing_by
  all_goals
    simp_wf
    dsimp only [Prod.snd] at suffix
    rw [suffix, remaining]
    simpa using Nat.lt_succ_self tail.length

include cursorLaw comparisonLaw in
theorem iterator_max_correct (native : core.cmp.Ord T)
    (nativeLaw : native.cmp = comparison) (state : I) (valid : model.valid state) :
    core.iter.traits.iterator.Iterator.max.default iterator native state ⦃ result =>
      Maximum rank (model.remaining state) result ⦄ := by
  simpa [core.iter.traits.iterator.Iterator.max.default, nativeLaw] using
    maxLoop_correct iterator model cursorLaw comparison rank comparisonLaw
      state none valid
end IteratorMaximum

theorem maximum_unique {T : Type} (rank : T → Nat)
    (injective : Function.Injective rank) (values : List T)
    {left right : Option T}
    (leftSpec : Maximum rank values left) (rightSpec : Maximum rank values right) :
    left = right := by
  cases left with
  | none =>
    cases right with
    | none => rfl
    | some value =>
      have empty : values = [] := leftSpec
      simpa [Maximum, empty] using rightSpec
  | some left =>
    cases right with
    | none =>
      have empty : values = [] := rightSpec
      simpa [Maximum, empty] using leftSpec
    | some right =>
      rcases leftSpec with ⟨leftMember, leftBound⟩
      rcases rightSpec with ⟨rightMember, rightBound⟩
      have same : left = right := injective
        (Nat.le_antisymm (rightBound left leftMember) (leftBound right rightMember))
      exact congrArg some same

theorem maximum_membership_invariant {T : Type} (rank : T → Nat)
    (left right : List T) (same : ∀ value, value ∈ left ↔ value ∈ right)
    (result : Option T) :
    Maximum rank left result ↔ Maximum rank right result := by
  cases result with
  | none =>
    simp only [Maximum]
    constructor <;> intro empty
    · apply List.eq_nil_iff_forall_not_mem.mpr
      intro value present
      have := (same value).mpr present
      simpa [empty] using this
    · apply List.eq_nil_iff_forall_not_mem.mpr
      intro value present
      have := (same value).mp present
      simpa [empty] using this
  | some best =>
    simp only [Maximum]
    constructor
    · rintro ⟨member, bound⟩
      exact ⟨(same best).mp member, fun value present => bound value ((same value).mpr present)⟩
    · rintro ⟨member, bound⟩
      exact ⟨(same best).mpr member, fun value present => bound value ((same value).mp present)⟩

end
end MembershipPrimitiveProof
