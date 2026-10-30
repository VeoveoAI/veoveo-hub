import IteratorProof

open Aeneas Aeneas.Std veoveo_hub IteratorProof NativeSetProof

namespace CollectionProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

/- Total Result, not dspec: a unique returned value gives an exact .ok equation. -/
theorem exact_of_total {T : Type} (operation : Result T) (expected : T)
    (total : operation ⦃ result => result = expected ⦄) : operation = .ok expected := by
  cases operation using Result.cases with
  | ret value => exact congrArg Result.ok ((WP.spec_ok value).mp total)
  | vis effect continuation => exact False.elim ((WP.spec_vis effect continuation).mp total)
  | div => exact False.elim (WP.spec_div.mp total)

section Set
variable {I T : Type}
variable (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
variable (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)
variable (comparison : T → T → Result Ordering) (order : Comparison T)
variable (comparisonLaw : LawfulComparison comparison order)

include cursorLaw comparisonLaw in
theorem collect_set_correct (state : I) (initial : Finset T) (valid : model.valid state) :
    CurrentRolesNative.collectSet iterator comparison state initial ⦃ result =>
      result = initial ∪ (model.remaining state).toFinset ⦄ := by
  unfold CurrentRolesNative.collectSet
  step with (cursorLaw state valid) as ⟨entry, next, takenSpec⟩
  rcases takenSpec with ⟨nextValid, entryEq, suffix⟩
  cases remaining : model.remaining state with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simp [remaining]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    dsimp only
    rw [insertBy_exact comparison order comparisonLaw]
    simp only [bind_ok]
    have recursive := collect_set_correct next (insert head initial) nextValid
    apply WP.spec_mono recursive
    intro result specification
    rw [specification, suffix]
    ext value
    simp [or_assoc, or_comm, or_left_comm]
termination_by (model.remaining state).length
decreasing_by simp_all

include cursorLaw comparisonLaw in
theorem collect_set_exact (state : I) (initial : Finset T) (valid : model.valid state) :
    CurrentRolesNative.collectSet iterator comparison state initial =
      .ok (initial ∪ (model.remaining state).toFinset) := by
  exact exact_of_total _ _ (collect_set_correct iterator model cursorLaw comparison order
    comparisonLaw state initial valid)

include cursorLaw comparisonLaw in
theorem collect_set_membership (state : I) (initial : Finset T) (valid : model.valid state) :
    CurrentRolesNative.collectSet iterator comparison state initial ⦃ result =>
      ∀ value, value ∈ result ↔ value ∈ initial ∨ value ∈ model.remaining state ⦄ := by
  apply WP.spec_mono (collect_set_correct iterator model cursorLaw comparison order comparisonLaw
    state initial valid)
  intro result specification value
  simp [specification]
end Set

section Vector
variable {I T : Type}
variable (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
variable (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)

include cursorLaw in
/- This is the EXISTING pinned Aeneas collector, not a copied accumulation loop. -/
theorem iter_to_list_correct (state : I) (accumulator : List T) (valid : model.valid state) :
    Aeneas.Std.alloc.vec.FromIteratorVec.iterToList iterator state accumulator ⦃ result =>
      result = accumulator.reverse ++ model.remaining state ⦄ := by
  unfold Aeneas.Std.alloc.vec.FromIteratorVec.iterToList
  step with (cursorLaw state valid) as ⟨entry, next, takenSpec⟩
  rcases takenSpec with ⟨nextValid, entryEq, suffix⟩
  cases remaining : model.remaining state with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simp [remaining]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    have recursive := iter_to_list_correct next (head :: accumulator) nextValid
    apply WP.spec_mono recursive
    intro result specification
    simpa [remaining, suffix, List.reverse_cons, List.append_assoc] using specification
termination_by (model.remaining state).length
decreasing_by simp_all

include cursorLaw in
theorem iter_to_list_exact (state : I) (valid : model.valid state) :
    Aeneas.Std.alloc.vec.FromIteratorVec.iterToList iterator state [] =
      .ok (model.remaining state) := by
  apply exact_of_total
  simpa using iter_to_list_correct iterator model cursorLaw state [] valid

include cursorLaw in
theorem vec_from_iterator_exact (state : I) (valid : model.valid state)
    (bound : (model.remaining state).length ≤ Usize.max) :
    Aeneas.Std.alloc.vec.FromIteratorVec.from_iter
      (Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket iterator) state =
      .ok (Aeneas.Std.alloc.vec.Vec.from (model.remaining state) bound) := by
  unfold Aeneas.Std.alloc.vec.FromIteratorVec.from_iter
  simp only [Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket.into_iter, bind_ok]
  rw [iter_to_list_exact iterator model cursorLaw state valid]
  simp [bound]

include cursorLaw in
theorem vec_collect_exact (state : I) (valid : model.valid state)
    (bound : (model.remaining state).length ≤ Usize.max) :
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default iterator
      (Aeneas.Std.core.iter.traits.collect.FromIteratorVec T) state =
      .ok (Aeneas.Std.alloc.vec.Vec.from (model.remaining state) bound) := by
  simpa only [Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default,
    Aeneas.Std.core.iter.traits.collect.FromIteratorVec] using
    vec_from_iterator_exact iterator model cursorLaw state valid bound
end Vector

/- Length facts are DERIVED from actual filter/map semantics, never supplied as a
promise that an arbitrary selected list fits the machine bound. -/
theorem filter_length_bound {I P T : Type} (model : Enumeration I T)
    (valid : P → Prop) (evaluate : T → Bool) (state : core.iter.adapters.filter.Filter I P) :
    ((filterEnumeration model valid evaluate).remaining state).length ≤
      (model.remaining state.iter).length := by
  exact List.length_filter_le _ _

theorem map_length_preserved {I P T B : Type} (model : Enumeration I T)
    (valid : P → Prop) (evaluate : T → B) (state : Aeneas.Std.core.iter.adapters.map.Map I P) :
    ((mapEnumeration model valid evaluate).remaining state).length =
      (model.remaining state.iter).length := by
  simp [mapEnumeration]

def filteredState {P T : Type} (source : Aeneas.Std.alloc.vec.Vec T) (predicate : P) :
    core.iter.adapters.filter.Filter (Aeneas.Std.core.slice.iter.Iter T) P :=
  { iter := { slice := Aeneas.Std.alloc.vec.Vec.deref source, i := 0 }, predicate := predicate }

theorem filtered_initial_valid {P T : Type} (source : Aeneas.Std.alloc.vec.Vec T)
    (valid : P → Prop) (evaluate : T → Bool) (predicate : P) (admitted : valid predicate) :
    (filterEnumeration (sliceEnumeration T) valid evaluate).valid (filteredState source predicate) := by
  exact ⟨trivial, admitted⟩

theorem filtered_machine_bound {P T : Type} (source : Aeneas.Std.alloc.vec.Vec T)
    (valid : P → Prop) (evaluate : T → Bool) (predicate : P) :
    ((filterEnumeration (sliceEnumeration T) valid evaluate).remaining
      (filteredState source predicate)).length ≤ Usize.max := by
  apply Nat.le_trans (filter_length_bound (sliceEnumeration T) valid evaluate (filteredState source predicate))
  simpa [sliceEnumeration, filteredState, Aeneas.Std.alloc.vec.Vec.deref] using source.slice.property

/- This is the actual generic Vec collect call made on a filtered raw Vec's
iterator. Its existing from_iter guard succeeds from the ORIGINAL Vec's bound. -/
theorem filtered_vec_collect_exact {P T : Type} (source : Aeneas.Std.alloc.vec.Vec T)
    (predicate : Aeneas.Std.core.ops.function.FnMut P T Bool) (valid : P → Prop)
    (evaluate : T → Bool) (predicateLaw : PredicateLaw predicate valid evaluate)
    (initial : P) (admitted : valid initial) :
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default
      (core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator
        (Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter T) predicate)
      (Aeneas.Std.core.iter.traits.collect.FromIteratorVec T) (filteredState source initial) =
      .ok (Aeneas.Std.alloc.vec.Vec.from
        ((filterEnumeration (sliceEnumeration T) valid evaluate).remaining (filteredState source initial))
        (filtered_machine_bound source valid evaluate initial)) := by
  exact vec_collect_exact _ _
    (filter_cursor_law _ _ (slice_cursor_law T) predicate valid evaluate predicateLaw)
    (filteredState source initial) (filtered_initial_valid source valid evaluate initial admitted)
    (filtered_machine_bound source valid evaluate initial)

#print axioms exact_of_total
#print axioms collect_set_correct
#print axioms collect_set_exact
#print axioms collect_set_membership
#print axioms iter_to_list_correct
#print axioms iter_to_list_exact
#print axioms vec_from_iterator_exact
#print axioms vec_collect_exact
#print axioms filter_length_bound
#print axioms map_length_preserved
#print axioms filtered_initial_valid
#print axioms filtered_machine_bound
#print axioms filtered_vec_collect_exact

end
end CollectionProof
