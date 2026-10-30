import NativeSetProof

open Aeneas Aeneas.Std veoveo_hub

namespace IteratorProof
noncomputable section

/- This is a finite RAW suffix plus an invariant on real iterator state. No
permission/role selection or filtered-result assumption belongs to this model. -/
structure Enumeration (I T : Type) where
  remaining : I → List T
  valid : I → Prop := fun _ => True

def NextStep {I T : Type} (model : Enumeration I T) (before : I)
    (result : Option T × I) : Prop :=
  model.valid result.2 ∧ result.1 = (model.remaining before).head? ∧
    model.remaining result.2 = (model.remaining before).tail

def CursorLaw {I T : Type} (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
    (model : Enumeration I T) : Prop :=
  ∀ state, model.valid state → iterator.next state ⦃ result => NextStep model state result ⦄

def PredicateLaw {P T : Type} (predicate : Aeneas.Std.core.ops.function.FnMut P T Bool)
    (valid : P → Prop) (evaluate : T → Bool) : Prop :=
  ∀ state, valid state → ∀ value,
    predicate.call_mut state value ⦃ result => result.1 = evaluate value ∧ valid result.2 ⦄

def MapperLaw {P T B : Type} (mapper : Aeneas.Std.core.ops.function.FnMut P T B)
    (valid : P → Prop) (evaluate : T → B) : Prop :=
  ∀ state, valid state → ∀ value,
    mapper.call_mut state value ⦃ result => result.1 = evaluate value ∧ valid result.2 ⦄

def listEnumeration (T : Type) : Enumeration (List T) T where
  remaining := id

def sliceEnumeration (T : Type) : Enumeration (Aeneas.Std.core.slice.iter.Iter T) T where
  remaining state := state.slice.val.drop state.i

theorem list_next_correct {T : Type} (state : List T) :
    alloc.collections.btree.set.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.next state
      ⦃ result => NextStep (listEnumeration T) state result ⦄ := by
  cases state <;> simp [alloc.collections.btree.set.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.next,
    NextStep, listEnumeration]

theorem slice_cursor_law (T : Type) :
    CursorLaw (Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter T) (sliceEnumeration T) := by
  intro state _
  change Aeneas.Std.core.slice.iter.IteratorSliceIter.next state ⦃ result => _ ⦄
  unfold Aeneas.Std.core.slice.iter.IteratorSliceIter.next
  split
  next active =>
    have bound : state.i < state.slice.val.length := by simpa using active
    have suffix := List.drop_eq_getElem_cons bound
    simp only [WP.spec_ok]
    change True ∧ some (state.slice.val[state.i]'bound) = (state.slice.val.drop state.i).head? ∧
      state.slice.val.drop (state.i + 1) = (state.slice.val.drop state.i).tail
    rw [suffix]
    exact ⟨trivial, rfl, rfl⟩
  next inactive =>
    have bound : state.slice.val.length ≤ state.i := by simpa using Nat.le_of_not_gt inactive
    have absent : state.slice.val.drop state.i = [] := List.drop_eq_nil_of_le bound
    simp only [WP.spec_ok]
    change True ∧ none = (state.slice.val.drop state.i).head? ∧
      state.slice.val.drop state.i = (state.slice.val.drop state.i).tail
    rw [absent]
    exact ⟨trivial, rfl, rfl⟩

def filterEnumeration {I P T : Type} (model : Enumeration I T)
    (valid : P → Prop) (evaluate : T → Bool) :
    Enumeration (core.iter.adapters.filter.Filter I P) T where
  remaining state := (model.remaining state.iter).filter evaluate
  valid state := model.valid state.iter ∧ valid state.predicate

def mapEnumeration {I P T B : Type} (model : Enumeration I T)
    (valid : P → Prop) (evaluate : T → B) :
    Enumeration (Aeneas.Std.core.iter.adapters.map.Map I P) B where
  remaining state := (model.remaining state.iter).map evaluate
  valid state := model.valid state.iter ∧ valid state.f

section Filter
variable {I P T : Type}
variable (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
variable (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)
variable (predicate : Aeneas.Std.core.ops.function.FnMut P T Bool)
variable (valid : P → Prop) (evaluate : T → Bool)
variable (predicateLaw : PredicateLaw predicate valid evaluate)

include cursorLaw predicateLaw in
theorem filter_next_correct (state : core.iter.adapters.filter.Filter I P)
    (admitted : (filterEnumeration model valid evaluate).valid state) :
    CurrentRolesNative.filterNext iterator predicate state
      ⦃ result => NextStep (filterEnumeration model valid evaluate) state result ⦄ := by
  unfold CurrentRolesNative.filterNext
  step with (cursorLaw state.iter admitted.1) as ⟨entry, next, takenSpec⟩
  rcases takenSpec with ⟨nextValid, entryEq, suffix⟩
  cases remaining : model.remaining state.iter with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simp [NextStep, filterEnumeration, remaining, suffix, nextValid, admitted.2]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    step with (predicateLaw state.predicate admitted.2 head) as ⟨hit, test, hitEq, testValid⟩
    subst hit
    cases found : evaluate head with
    | true =>
      simp [NextStep, filterEnumeration, remaining, suffix, found, nextValid, testValid]
    | false =>
      have recursive := filter_next_correct
        ({ iter := next, predicate := test } : core.iter.adapters.filter.Filter I P)
        (show (filterEnumeration model valid evaluate).valid _ from ⟨nextValid, testValid⟩)
      apply WP.spec_mono recursive
      intro result specification
      simpa [NextStep, filterEnumeration, remaining, suffix, found] using specification
termination_by (model.remaining state.iter).length
decreasing_by simp_all

include cursorLaw predicateLaw in
theorem filter_cursor_law : CursorLaw
    (core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator iterator predicate)
    (filterEnumeration model valid evaluate) := by
  intro state admitted
  simpa only [core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator.next] using
    filter_next_correct iterator model cursorLaw predicate valid evaluate predicateLaw state admitted
end Filter

section Map
variable {I P T B : Type}
variable (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
variable (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)
variable (mapper : Aeneas.Std.core.ops.function.FnMut P T B)
variable (valid : P → Prop) (evaluate : T → B)
variable (mapperLaw : MapperLaw mapper valid evaluate)

include cursorLaw mapperLaw in
theorem map_cursor_law : CursorLaw
    (core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator iterator mapper)
    (mapEnumeration model valid evaluate) := by
  intro state admitted
  change core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator.next iterator mapper state
    ⦃ result => NextStep (mapEnumeration model valid evaluate) state result ⦄
  unfold core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator.next
  step with (cursorLaw state.iter admitted.1) as ⟨entry, next, takenSpec⟩
  rcases takenSpec with ⟨nextValid, entryEq, suffix⟩
  cases remaining : model.remaining state.iter with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simp [NextStep, mapEnumeration, remaining, suffix, nextValid, admitted.2]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    step with (mapperLaw state.f admitted.2 head) as ⟨value, nextMapper, valueEq, mapperValid⟩
    subst value
    simp [NextStep, mapEnumeration, remaining, suffix, nextValid, mapperValid]
end Map

section Any
variable {I P T : Type}
variable (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
variable (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)
variable (predicate : Aeneas.Std.core.ops.function.FnMut P T Bool)
variable (valid : P → Prop) (evaluate : T → Bool)
variable (predicateLaw : PredicateLaw predicate valid evaluate)

include cursorLaw predicateLaw in
theorem any_correct (state : I) (test : P) (stateValid : model.valid state) (testValid : valid test) :
    CurrentRolesNative.any iterator predicate state test ⦃ result =>
      result.1 = (model.remaining state).any evaluate ∧ model.valid result.2 ∧
      (model.remaining result.2).length ≤ (model.remaining state).length ∧
      (result.1 = false → model.remaining result.2 = []) ⦄ := by
  unfold CurrentRolesNative.any
  step with (cursorLaw state stateValid) as ⟨entry, next, takenSpec⟩
  rcases takenSpec with ⟨nextValid, entryEq, suffix⟩
  cases remaining : model.remaining state with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simp [remaining, suffix, nextValid]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    step with (predicateLaw test testValid head) as ⟨hit, nextTest, hitEq, nextTestValid⟩
    subst hit
    cases found : evaluate head with
    | true => simp [remaining, suffix, nextValid, found]
    | false =>
      have recursive := any_correct next nextTest nextValid nextTestValid
      apply WP.spec_mono recursive
      intro result specification
      rcases specification with ⟨answer, kept, bound, exhausted⟩
      refine ⟨?_, kept, ?_, exhausted⟩
      · simpa [remaining, suffix, found] using answer
      · rw [suffix] at bound
        simpa [remaining] using Nat.le_trans bound (Nat.le_succ tail.length)
termination_by (model.remaining state).length
decreasing_by simp_all

/- This equation names the actual immediate returned cursor. No finite suffix
or behavior after it is assumed, so a later probe cannot run after a true match. -/
theorem any_stops_at_match (state next : I) (test updated : P) (value : T)
    (take : iterator.next state = .ok (some value, next))
    (hit : predicate.call_mut test value = .ok (true, updated)) :
    CurrentRolesNative.any iterator predicate state test = .ok (true, next) := by
  unfold CurrentRolesNative.any
  simp [take, hit]
end Any

#print axioms list_next_correct
#print axioms slice_cursor_law
#print axioms filter_next_correct
#print axioms filter_cursor_law
#print axioms map_cursor_law
#print axioms any_correct
#print axioms any_stops_at_match

end
end IteratorProof
