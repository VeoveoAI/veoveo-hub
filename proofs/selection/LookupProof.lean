import SelectionProof

open Aeneas Aeneas.Std
open hub_selection_proof.kernel

namespace LookupProof

set_option maxRecDepth 2048

variable {B D : Type} [DecidableEq B]

/-- Every comparison terminates successfully and agrees with branch equality. -/
def CorrectComparator (eq : core.cmp.PartialEq B B) : Prop :=
  ∀ left right, eq.eq left right = .ok (decide (left = right))

/-- Mathematical matching entries, preserving multiplicity and their deployment. -/
def matching (branch : B) (entries : List (B × D)) : List D :=
  entries.filterMap (fun entry => if entry.1 = branch then some entry.2 else none)

@[simp] theorem matching_nil (branch : B) : matching branch ([] : List (B × D)) = [] := rfl

/-- Classification counts entries, not distinct deployment IDs. -/
def classification : List D → Matches D
  | [] => .Missing
  | [deployment] => .Unique deployment
  | _ :: _ :: _ => .Ambiguous

/-- Combine an already scanned summary with the remaining mathematical matches. -/
def finish (found : Matches D) (remaining : List D) : Matches D :=
  match found with
  | .Missing => classification remaining
  | .Unique deployment => match remaining with
    | [] => .Unique deployment
    | _ :: _ => .Ambiguous
  | .Ambiguous => .Ambiguous

@[simp] theorem finish_nil (found : Matches D) : finish found [] = found := by
  cases found <;> rfl

@[simp] theorem finish_missing_cons (deployment : D) (rest : List D) :
    finish .Missing (deployment :: rest) = finish (.Unique deployment) rest := by
  cases rest <;> rfl

@[simp] theorem matching_cons (branch : B) (entry : B × D) (rest : List (B × D)) :
    matching branch (entry :: rest) =
      if entry.1 = branch then entry.2 :: matching branch rest else matching branch rest := by
  by_cases same : entry.1 = branch <;> simp [matching, same]

theorem matching_drop (served : Slice (B × D)) (branch : B) (index : Nat)
    (bound : index < served.length) :
    matching branch (served.val.drop index) =
      if served.val[index].1 = branch then
        served.val[index].2 :: matching branch (served.val.drop (index + 1))
      else matching branch (served.val.drop (index + 1)) := by
  rw [List.drop_eq_getElem_cons bound, matching_cons]

/-- Total correctness of the generated loop from any in-bounds index and summary.
The decreasing suffix length establishes termination; the bound discharges both
slice indexing and usize addition obligations. -/
theorem lookup_loop_correct (eq : core.cmp.PartialEq B B)
    (correct : CorrectComparator eq) (served : Slice (B × D)) (branch : B)
    (found : Matches D) (index : Usize) (bound : index.val ≤ served.length) :
    lookup_loop eq served branch found index ⦃ result =>
      result = finish found (matching branch (served.val.drop index.val)) ⦄ := by
  unfold lookup_loop
  rw [loop]
  dsimp only
  unfold lookup_loop.body
  dsimp only
  split
  next active =>
    have inBounds : index.val < served.length := by scalar_tac
    have suffix := matching_drop served branch index.val inBounds
    step as ⟨entry, entryEq⟩
    rcases entry with ⟨candidate, deployment⟩
    have suffix' : matching branch (served.val.drop index.val) =
        if candidate = branch then
          deployment :: matching branch (served.val.drop (index.val + 1))
        else matching branch (served.val.drop (index.val + 1)) := by
      simpa only [← entryEq] using suffix
    by_cases same : candidate = branch
    · simp [CorrectComparator] at correct
      simp [correct, same]
      cases found with
      | Missing =>
        step as ⟨next, nextEq⟩
        have nextValue : next.val = index.val + 1 := by simpa using nextEq
        have nextBound : next.val ≤ served.length := by omega
        have recursive := lookup_loop_correct eq correct served branch
          (.Unique deployment) next nextBound
        simpa [lookup_loop, lookup_loop.body, correct, suffix', same, nextEq] using recursive
      | Unique previous => simp [suffix', same, finish]
      | Ambiguous => simp [finish]
    · simp [CorrectComparator] at correct
      simp [correct, same]
      step as ⟨next, nextEq⟩
      have nextValue : next.val = index.val + 1 := by simpa using nextEq
      have nextBound : next.val ≤ served.length := by omega
      have recursive := lookup_loop_correct eq correct served branch found next nextBound
      simpa [lookup_loop, lookup_loop.body, correct, suffix', same, nextEq] using recursive
  next inactive =>
    have atEnd : index.val = served.length := by scalar_tac
    simp [atEnd]
termination_by served.length - index.val
decreasing_by all_goals omega

theorem lookup_correct (eq : core.cmp.PartialEq B B)
    (correct : CorrectComparator eq) (copyB : core.marker.Copy B)
    (copyD : core.marker.Copy D) (served : Slice (B × D)) (branch : B) :
    lookup eq copyB copyD served branch ⦃ result =>
      result = classification (matching branch served.val) ⦄ := by
  simpa [lookup, finish] using
    lookup_loop_correct eq correct served branch .Missing 0#usize (by simp)

theorem classification_missing_iff (hits : List D) :
    classification hits = .Missing ↔ hits.length = 0 := by
  cases hits with
  | nil => simp [classification]
  | cons first rest => cases rest <;> simp [classification]

theorem classification_unique_iff (hits : List D) (deployment : D) :
    classification hits = .Unique deployment ↔ hits = [deployment] := by
  cases hits with
  | nil => simp [classification]
  | cons first rest => cases rest <;> simp [classification]

theorem classification_ambiguous_iff (hits : List D) :
    classification hits = .Ambiguous ↔ 2 ≤ hits.length := by
  cases hits with
  | nil => simp [classification]
  | cons first rest => cases rest <;> simp [classification]

/-- A complete, terminating classification of arbitrary served slices. Unique
identifies the deployment of the sole matching entry, not merely some matching
deployment. Ambiguous counts entries even when their deployment values coincide. -/
theorem lookup_classifications (eq : core.cmp.PartialEq B B)
    (correct : CorrectComparator eq) (copyB : core.marker.Copy B)
    (copyD : core.marker.Copy D) (served : Slice (B × D)) (branch : B) :
    lookup eq copyB copyD served branch ⦃ result =>
      (result = .Missing ↔ (matching branch served.val).length = 0) ∧
      (∀ deployment, result = .Unique deployment ↔ matching branch served.val = [deployment]) ∧
      (result = .Ambiguous ↔ 2 ≤ (matching branch served.val).length) ⦄ := by
  apply WP.spec_mono (lookup_correct eq correct copyB copyD served branch)
  intro result resultEq
  subst result
  exact ⟨classification_missing_iff _, classification_unique_iff _, classification_ambiguous_iff _⟩

theorem matching_empty_iff (branch : B) (entries : List (B × D)) :
    matching branch entries = [] ↔ ∀ entry ∈ entries, entry.1 ≠ branch := by
  induction entries with
  | nil => simp
  | cons entry rest inductionHypothesis =>
    by_cases same : entry.1 = branch <;> simp [same, inductionHypothesis]

/-- Actual absence of an explicitly selected branch implies its typed unavailable
result, without assuming a lookup outcome. The original resolver noninterference
theorem removes the arbitrary base; total lookup refinement establishes Missing. -/
theorem absent_selection_is_unavailable (eq : core.cmp.PartialEq B B)
    (correct : CorrectComparator eq) (copyB : core.marker.Copy B)
    (copyD : core.marker.Copy D) (served : Slice (B × D)) (selected : B)
    (base : Option B) (absent : ∀ entry ∈ served.val, entry.1 ≠ selected) :
    resolve eq copyB copyD served (some selected) base ⦃ result =>
      result = .Err (.SelectedMissing selected) ⦄ := by
  rw [SelectionProof.explicit_selection_ignores_base eq copyB copyD served selected base none]
  have noMatches : matching selected served.val = [] :=
    (matching_empty_iff selected served.val).mpr absent
  unfold resolve
  apply WP.spec_bind (lookup_correct eq correct copyB copyD served selected)
  intro result resultEq
  simp only [noMatches, classification] at resultEq
  subst result
  simp

theorem same_deployment_duplicates_are_ambiguous (eq : core.cmp.PartialEq B B)
    (correct : CorrectComparator eq) (copyB : core.marker.Copy B)
    (copyD : core.marker.Copy D) (served : Slice (B × D)) (branch : B)
    (deployment : D) (entries : served.val = [(branch, deployment), (branch, deployment)]) :
    lookup eq copyB copyD served branch ⦃ result => result = .Ambiguous ⦄ := by
  apply WP.spec_mono (lookup_correct eq correct copyB copyD served branch)
  intro result resultEq
  simpa [entries, matching, classification] using resultEq

#print axioms lookup_loop_correct
#print axioms lookup_correct
#print axioms lookup_classifications
#print axioms absent_selection_is_unavailable
#print axioms same_deployment_duplicates_are_ambiguous

end LookupProof
