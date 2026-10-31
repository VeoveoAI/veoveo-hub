import CollisionLeft
import CollisionRight
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std

#synth Discriminant (CollisionLeft.LocalResult Nat) Isize
#synth Discriminant (CollisionRight.LocalResult Nat) Isize

-- Default and explicit source discriminants must both remain exact. Distinct
-- values also catch accidental use of the other namespace's instance.
theorem left_single : read_discriminant (CollisionLeft.LocalResult.Single 17) = 0#isize := rfl
theorem left_ambiguous : read_discriminant (CollisionLeft.LocalResult.Ambiguous 17 29) = 1#isize := rfl
theorem left_none : read_discriminant (CollisionLeft.LocalResult.None (T := Nat)) = 2#isize := rfl
theorem right_single : read_discriminant (CollisionRight.LocalResult.Single 17) = 3#isize := rfl
theorem right_ambiguous : read_discriminant (CollisionRight.LocalResult.Ambiguous 17 29) = 8#isize := rfl
theorem right_none : read_discriminant (CollisionRight.LocalResult.None (T := Nat)) = 13#isize := rfl

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``left_single, ``left_ambiguous, ``left_none,
      ``right_single, ``right_ambiguous, ``right_none] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unexpected discriminant regression axiom in {declaration}: {found}"
