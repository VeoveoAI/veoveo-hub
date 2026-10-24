import HubSelectionProof

open Aeneas Aeneas.Std
open hub_selection_proof.kernel

namespace SelectionProof

/-- Structural noninterference: for every served slice and branch comparator,
an explicit selection's result is independent of the base, including modeled Rust
failure/divergence outcomes. No equality-law or successful-lookup premise is needed. -/
theorem explicit_selection_ignores_base
    {B D : Type} (eq : core.cmp.PartialEq B B)
    (copyB : core.marker.Copy B) (copyD : core.marker.Copy D)
    (served : Slice (B × D)) (selected : B) (base₁ base₂ : Option B) :
    resolve eq copyB copyD served (some selected) base₁ =
      resolve eq copyB copyD served (some selected) base₂ := by
  rfl

/-- Whenever the generated Rust scan successfully classifies the explicit branch
as missing, the generated resolver returns its typed selected-missing error. -/
theorem missing_selection_is_unavailable
    {B D : Type} (eq : core.cmp.PartialEq B B)
    (copyB : core.marker.Copy B) (copyD : core.marker.Copy D)
    (served : Slice (B × D)) (selected : B) (base : Option B)
    (missing : lookup eq copyB copyD served selected = .ok (.Missing)) :
    resolve eq copyB copyD served (some selected) base =
      .ok (.Err (.SelectedMissing selected)) := by
  simp [resolve, missing]

/-- In particular an available base does not repair a missing explicit branch.
Both classifications here are those of the generated Rust scan, not a copied scan. -/
theorem missing_selection_does_not_fall_back_to_available_base
    {B D : Type} (eq : core.cmp.PartialEq B B)
    (copyB : core.marker.Copy B) (copyD : core.marker.Copy D)
    (served : Slice (B × D)) (selected base : B) (deployment : D)
    (missing : lookup eq copyB copyD served selected = .ok (.Missing))
    (_available : lookup eq copyB copyD served base = .ok (.Unique deployment)) :
    resolve eq copyB copyD served (some selected) (some base) =
      .ok (.Err (.SelectedMissing selected)) := by
  exact missing_selection_is_unavailable eq copyB copyD served selected (some base) missing

#print axioms explicit_selection_ignores_base
#print axioms missing_selection_is_unavailable
#print axioms missing_selection_does_not_fall_back_to_available_base

end SelectionProof
