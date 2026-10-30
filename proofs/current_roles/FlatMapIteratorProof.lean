import RegistrationIteratorProof

open Aeneas Aeneas.Std veoveo_hub IteratorProof

namespace FlatMapIteratorProof
noncomputable section

/- This is the existing finite raw enumeration framework. The per-item mapper
and IntoIterator premises describe actual calls; neither assumes the final
flattened/filtered permission result. Concrete closures instantiate them later. -/
def enumeration {I U F Outer Item Inner : Type}
    (outer : Enumeration I Outer) (inner : Enumeration Inner Item)
    (valid : F → Prop) (evaluate : Outer → U) (items : U → List Item) :
    Enumeration (core.iter.adapters.flatten.FlatMap I U F Item Inner) Item where
  remaining state := if state.exhausted then [] else
    (match state.inner with | none => [] | some cursor => inner.remaining cursor) ++
      (outer.remaining state.iter).flatMap (fun value => items (evaluate value))
  valid state := outer.valid state.iter ∧ valid state.mapper ∧
    (∀ cursor, state.inner = some cursor → inner.valid cursor) ∧
    (state.exhausted = true → state.inner = none ∧ outer.remaining state.iter = [])

section Traversal
variable {I U F Outer Item Inner : Type}
variable (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I Outer)
variable (outer : Enumeration I Outer) (outerLaw : CursorLaw iterator outer)
variable (into : Aeneas.Std.core.iter.traits.collect.IntoIterator U Item Inner)
variable (inner : Enumeration Inner Item) (innerLaw : CursorLaw into.iteratorInst inner)
variable (items : U → List Item)
variable (intoLaw : ∀ value, into.into_iter value ⦃ cursor =>
  inner.valid cursor ∧ inner.remaining cursor = items value ⦄)
variable (mapper : Aeneas.Std.core.ops.function.FnMut F Outer U)
variable (valid : F → Prop) (evaluate : Outer → U)
variable (mapperLaw : MapperLaw mapper valid evaluate)

include outerLaw innerLaw intoLaw mapperLaw in
theorem flat_map_next_correct
    (state : core.iter.adapters.flatten.FlatMap I U F Item Inner)
    (admitted : (enumeration outer inner valid evaluate items).valid state) :
    CurrentRolesNative.flatMapNext iterator into mapper state
      ⦃ result => NextStep (enumeration outer inner valid evaluate items) state result ⦄ := by
  rcases admitted with ⟨outerValid, mapperValid, innerValid, fused⟩
  unfold CurrentRolesNative.flatMapNext
  cases running : state.exhausted with
  | true =>
    simp only [running, ite_true, WP.spec_ok]
    refine ⟨⟨outerValid, mapperValid, innerValid, fused⟩, ?_, ?_⟩ <;>
      simp [enumeration, running]
  | false =>
    simp only [running, Bool.false_eq_true, ite_false]
    cases present : state.inner with
    | none =>
      simp only [present]
      step with (outerLaw state.iter outerValid) as ⟨entry, next, taken⟩
      rcases taken with ⟨nextValid, entryEq, suffix⟩
      cases remaining : outer.remaining state.iter with
      | nil =>
        simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
        cases entryEq
        simp [NextStep, enumeration, running, present, remaining, suffix,
          nextValid, mapperValid]
      | cons head tail =>
        simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
        cases entryEq
        step with (mapperLaw state.mapper mapperValid head) as
          ⟨mapped, updated, mappedEq, updatedValid⟩
        subst mapped
        step with (intoLaw (evaluate head)) as ⟨cursor, cursorValid, produced⟩
        let state' : core.iter.adapters.flatten.FlatMap I U F Item Inner :=
          { iter := next, mapper := updated, inner := some cursor, exhausted := false }
        have admitted' : (enumeration outer inner valid evaluate items).valid state' := by
          simp [state', enumeration, nextValid, updatedValid, cursorValid, running]
        have same : (enumeration outer inner valid evaluate items).remaining state' =
            (enumeration outer inner valid evaluate items).remaining state := by
          simp [state', enumeration, running, present, remaining, suffix, produced]
        have recursive := flat_map_next_correct state' admitted'
        have adjusted : CurrentRolesNative.flatMapNext iterator into mapper state'
            ⦃ result => NextStep (enumeration outer inner valid evaluate items) state result ⦄ := by
          apply Aeneas.Std.WP.spec_mono recursive
          intro result specification
          simpa only [NextStep, same] using specification
        simpa only [state'] using adjusted
    | some cursor =>
      simp only [present]
      step with (innerLaw cursor (innerValid cursor present)) as ⟨entry, next, taken⟩
      rcases taken with ⟨nextValid, entryEq, suffix⟩
      cases remaining : inner.remaining cursor with
      | nil =>
        simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
        cases entryEq
        dsimp only
        let state' : core.iter.adapters.flatten.FlatMap I U F Item Inner :=
          { iter := state.iter, mapper := state.mapper, inner := none, exhausted := false }
        have admitted' : (enumeration outer inner valid evaluate items).valid state' := by
          simp [state', enumeration, outerValid, mapperValid, running]
        have same : (enumeration outer inner valid evaluate items).remaining state' =
            (enumeration outer inner valid evaluate items).remaining state := by
          simp [state', enumeration, running, present, remaining]
        have recursive := flat_map_next_correct state' admitted'
        have adjusted : CurrentRolesNative.flatMapNext iterator into mapper state'
            ⦃ result => NextStep (enumeration outer inner valid evaluate items) state result ⦄ := by
          apply Aeneas.Std.WP.spec_mono recursive
          intro result specification
          simpa only [NextStep, same] using specification
        simpa only [state'] using adjusted
      | cons head tail =>
        simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
        cases entryEq
        simp [NextStep, enumeration, running, present, remaining, suffix,
          nextValid, outerValid, mapperValid]
termination_by 2 * (outer.remaining state.iter).length + (if state.inner.isSome then 1 else 0)
decreasing_by all_goals simp_all [state'] <;> omega

include outerLaw innerLaw intoLaw mapperLaw in
theorem flat_map_cursor_law : CursorLaw
    (core.iter.adapters.flatten.FlatMap.Insts.CoreIterTraitsIteratorIterator iterator into mapper)
    (enumeration outer inner valid evaluate items) := by
  intro state admitted
  simpa only [core.iter.adapters.flatten.FlatMap.Insts.CoreIterTraitsIteratorIterator.next] using
    flat_map_next_correct iterator outer outerLaw into inner innerLaw items intoLaw
      mapper valid evaluate mapperLaw state admitted

theorem constructor_exact (state : I) (mapping : F) :
    core.iter.traits.iterator.Iterator.flat_map.default iterator into mapper state mapping =
      .ok ({ iter := state, mapper := mapping, inner := none, exhausted := false } :
        core.iter.adapters.flatten.FlatMap I U F Item Inner) := by
  rfl

theorem initial_valid (state : I) (mapping : F)
    (rawValid : outer.valid state) (mappingValid : valid mapping) :
    (enumeration outer inner valid evaluate items).valid
      { iter := state, mapper := mapping, inner := none, exhausted := false } := by
  simp [enumeration, rawValid, mappingValid]

theorem initial_remaining (state : I) (mapping : F) :
    (enumeration outer inner valid evaluate items).remaining
      { iter := state, mapper := mapping, inner := none, exhausted := false } =
      (outer.remaining state).flatMap (fun value => items (evaluate value)) := by
  rfl

/- Immediate equations specify short circuit and carried state. No outer item
or mapper call occurs when a current inner emits a value or Fuse is exhausted. -/
theorem inner_match_stops
    (state : core.iter.adapters.flatten.FlatMap I U F Item Inner)
    (cursor next : Inner) (value : Item)
    (running : state.exhausted = false) (present : state.inner = some cursor)
    (take : into.iteratorInst.next cursor = .ok (some value, next)) :
    CurrentRolesNative.flatMapNext iterator into mapper state =
      .ok (some value, { state with inner := some next }) := by
  unfold CurrentRolesNative.flatMapNext
  simp only [running, Bool.false_eq_true, ite_false, present, take, bind_ok, bind_tc_ok]
  rfl

theorem exhausted_stops
    (state : core.iter.adapters.flatten.FlatMap I U F Item Inner)
    (finished : state.exhausted = true) :
    CurrentRolesNative.flatMapNext iterator into mapper state = .ok (none, state) := by
  unfold CurrentRolesNative.flatMapNext
  simp only [finished, ite_true]

end Traversal

#print axioms flat_map_next_correct
#print axioms flat_map_cursor_law
#print axioms constructor_exact
#print axioms initial_valid
#print axioms initial_remaining
#print axioms inner_match_stops
#print axioms exhausted_stops

end
end FlatMapIteratorProof
