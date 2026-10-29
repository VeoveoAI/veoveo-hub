import HubTaskProof

open Aeneas Aeneas.Std
open hub_task_proof

namespace TaskContinuityProof

set_option maxRecDepth 2048

/-- Rust permits unlawful or nonterminating comparisons. Both methods used by the
generated functions must terminate and agree with mathematical equality. -/
def LawfulEq {A : Type} [DecidableEq A] (eq : core.cmp.PartialEq A A) : Prop :=
  (∀ left right, eq.eq left right = .ok (decide (left = right))) ∧
  (∀ left right, eq.ne left right = .ok (decide (left ≠ right)))

/-- A set-containment specification: order and multiplicity do not matter. -/
def Contained {L : Type} (required held : List L) : Prop :=
  ∀ label ∈ required, label ∈ held

instance containedDecidable {L : Type} [DecidableEq L] (required held : List L) :
    Decidable (Contained required held) := by
  unfold Contained
  infer_instance

@[simp] theorem contained_nil {L : Type} (held : List L) : Contained [] held := by
  simp [Contained]

@[simp] theorem contained_cons {L : Type} (label : L) (rest held : List L) :
    Contained (label :: rest) held ↔ label ∈ held ∧ Contained rest held := by
  simp [Contained]

/-- Total correctness of the generated inner search from an arbitrary bounded
position. A returned end position means precisely that the remaining suffix has no
matching label. Both indexing and the usize increment are justified by the bounds. -/
theorem search_correct {L : Type} [DecidableEq L]
    (eq : core.cmp.PartialEq L L) (law : LawfulEq eq)
    (required held : Slice L) (i j : Usize)
    (requiredBound : i.val < required.length) (heldBound : j.val ≤ held.length) :
    decision.contains_all_loop0_loop0 eq required held i j ⦃ result =>
      result.val ≤ held.length ∧
      (result.val = held.length ↔ required.val[i.val] ∉ held.val.drop j.val) ⦄ := by
  unfold decision.contains_all_loop0_loop0
  rw [loop]
  unfold decision.contains_all_loop0_loop0.body
  dsimp only
  split
  next active =>
    have inBounds : j.val < held.length := by scalar_tac
    step as ⟨entry, entryEq⟩
    step as ⟨needle, needleEq⟩
    have suffix : held.val.drop j.val = entry :: held.val.drop (j.val + 1) := by
      simpa only [← entryEq] using List.drop_eq_getElem_cons inBounds
    by_cases same : entry = needle
    · simp [law.2, same]
      have notEnd : held.val.length ≠ j.val := Nat.ne_of_gt inBounds
      constructor
      · omega
      · simp [suffix, ← needleEq, same, eq_comm, notEnd]
    · simp [law.2, same]
      step as ⟨next, nextEq⟩
      have nextValue : next.val = j.val + 1 := by simpa using nextEq
      have nextBound : next.val ≤ held.length := by omega
      have recursive := search_correct eq law required held i next requiredBound nextBound
      have refined := WP.spec_mono recursive (fun result post => show
          result.val ≤ held.length ∧
          (result.val = held.length ↔ required.val[i.val] ∉ held.val.drop j.val) from by
        rcases post with ⟨bound, missing⟩
        refine ⟨bound, ?_⟩
        simpa [suffix, ← needleEq, same, eq_comm, nextValue] using missing)
      simpa [decision.contains_all_loop0_loop0,
        decision.contains_all_loop0_loop0.body, law.2, same, nextEq] using refined
  next inactive =>
    have atEnd : j.val = held.length := by scalar_tac
    simp [atEnd]
termination_by held.length - j.val
decreasing_by all_goals omega

/-- Total correctness of the generated outer loop, with no assumption about label
uniqueness, order, equal list lengths, or an already-decided containment result. -/
theorem containment_loop_correct {L : Type} [DecidableEq L]
    (eq : core.cmp.PartialEq L L) (law : LawfulEq eq)
    (required held : Slice L) (i : Usize) (bound : i.val ≤ required.length) :
    decision.contains_all_loop0 eq required held i ⦃ result =>
      result = true ↔ Contained (required.val.drop i.val) held.val ⦄ := by
  classical
  unfold decision.contains_all_loop0
  rw [loop]
  unfold decision.contains_all_loop0.body
  dsimp only
  split
  next active =>
    have inBounds : i.val < required.length := by scalar_tac
    have suffix := List.drop_eq_getElem_cons inBounds
    step with (search_correct eq law required held i 0#usize inBounds (by simp))
      as ⟨j, jBound, missing⟩
    split
    next absent =>
      have noLabel : required.val[i.val] ∉ held.val := missing.mp (by scalar_tac)
      rw [suffix, contained_cons]
      simp only [noLabel, false_and, bind_ok, WP.spec_ok, Bool.false_eq_true, iff_self]
    next present =>
      have hasLabel : required.val[i.val] ∈ held.val := by
        by_contra absent
        have atEnd := missing.mpr absent
        exact present (by scalar_tac)
      step as ⟨next, nextEq⟩
      have nextValue : next.val = i.val + 1 := by simpa using nextEq
      have nextBound : next.val ≤ required.length := by omega
      have recursive := containment_loop_correct eq law required held next nextBound
      have refined := WP.spec_mono recursive (fun result resultEq => show
          (result = true ↔ Contained (required.val.drop i.val) held.val) from by
        rw [suffix, contained_cons]
        rw [nextValue] at resultEq
        simpa only [hasLabel, true_and] using resultEq)
      simpa [decision.contains_all_loop0, decision.contains_all_loop0.body,
        nextEq] using refined
  next inactive =>
    have atEnd : i.val = required.length := by scalar_tac
    simp [atEnd]
termination_by required.length - i.val
decreasing_by all_goals omega

theorem containment_correct {L : Type} [DecidableEq L]
    (eq : core.cmp.PartialEq L L) (law : LawfulEq eq) (required held : Slice L) :
    decision.contains_all eq required held ⦃ result =>
      result = decide (Contained required.val held.val) ⦄ := by
  classical
  apply WP.spec_mono (containment_loop_correct eq law required held 0#usize (by simp))
  intro result resultIff
  cases result <;> simp_all

#print axioms search_correct
#print axioms containment_loop_correct
#print axioms containment_correct

end TaskContinuityProof
