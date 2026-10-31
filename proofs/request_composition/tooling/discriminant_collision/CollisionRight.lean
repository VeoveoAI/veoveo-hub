module
public import Aeneas
@[expose] public section
open Aeneas Aeneas.Std

namespace CollisionRight

@[discriminant isize [3, 8, 13]]
inductive LocalResult (T : Type) where
  | Single : T → LocalResult T
  | Ambiguous : T → T → LocalResult T
  | None : LocalResult T

end CollisionRight
