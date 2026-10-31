module
public import Aeneas
@[expose] public section
open Aeneas Aeneas.Std

namespace CollisionLeft

@[discriminant isize]
inductive LocalResult (T : Type) where
  | Single : T → LocalResult T
  | Ambiguous : T → T → LocalResult T
  | None : LocalResult T

end CollisionLeft
