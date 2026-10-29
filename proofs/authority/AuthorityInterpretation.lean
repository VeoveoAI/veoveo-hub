import AuthorityComposition

open Aeneas Aeneas.Std
open hub_authority_proof AuthorityProof

namespace AuthorityInterpretation

-- A finite mathematical interpretation witnesses consistency of the operational
-- premises. It is not an alternative Rust algorithm or a proof of BTreeSet.
def sets : SetModel (Finset Bool) Bool where
  members set := {permission | permission ∈ set}
  filter left right := left ∩ right
  filter_members := by
    intro left right
    ext permission
    simp

def permissions : authority.Permissions (Finset Bool) Bool where
  contains set permission := .ok (decide (permission ∈ set))
  is_empty set := .ok (decide (set = ∅))
  narrow left right := .ok (left ∩ right)

theorem permissions_law : LawfulPermissions permissions sets := by
  classical
  constructor
  · intro set permission
    by_cases member : permission ∈ set <;> simp [permissions, sets, member]
  constructor
  · intro set
    simp [permissions, sets]
  · intro left right
    rfl

def heldModel : HeldModel (Finset Bool) (Finset Bool) Bool sets where
  current set := {permission | permission ∈ set}
  result held requested := requested ∩ held
  result_members := by
    intro held requested
    ext permission
    simp [sets]

def held : authority.Held (Finset Bool) (Finset Bool) where
  held current requested := .ok (requested ∩ current)

theorem held_law : LawfulHeld held heldModel := by
  intro current requested
  rfl

def firstMissing (uses requested : Finset Bool) : Option Bool :=
  if false ∈ requested ∧ false ∉ uses then some false
  else if true ∈ requested ∧ true ∉ uses then some true else none

def usesModel : UsesModel (Finset Bool) (Finset Bool) Bool sets where
  declared set := {permission | permission ∈ set}
  firstMissing := firstMissing
  none_iff := by
    intro declared requested
    change firstMissing declared requested = none ↔
      ∀ permission, permission ∈ requested → permission ∈ declared
    constructor
    · intro absent permission member
      cases permission <;> by_contra missing <;>
        simp [firstMissing, member, missing] at absent
      split at absent <;> cases absent
    · intro subset
      have noFalse : ¬ (false ∈ requested ∧ false ∉ declared) := by
        rintro ⟨member, missing⟩
        exact missing (subset false member)
      have noTrue : ¬ (true ∈ requested ∧ true ∉ declared) := by
        rintro ⟨member, missing⟩
        exact missing (subset true member)
      simp [firstMissing, noFalse, noTrue]
  some_invalid := by
    intro declared requested permission found
    unfold firstMissing at found
    split at found
    · next invalid =>
      cases found
      exact invalid
    · split at found
      · next invalid =>
        cases found
        exact invalid
      · cases found

def uses : authority.Uses (Finset Bool) (Finset Bool) Bool where
  first_undeclared declared requested := .ok (firstMissing declared requested)

theorem uses_law : LawfulUses uses usesModel := by
  intro declared requested
  rfl

def resource : authority.ResourceKey Nat Nat where
  «matches» actual requested := .ok (decide (actual = requested))

theorem resource_law : LawfulResource resource id := by
  intro actual requested
  by_cases same : actual = requested <;> simp [resource, same]

-- A nonempty accepted exchange filters one requested permission away and retains
-- the concrete caller/actor/Task/deadline. This also rules out vacuous assumptions.
theorem positive_exchange :
    exchangeTrace uses held resource ({false, true} : Finset Bool) {false, true}
      (.Resource 19) (some 19) (Slice.new Unit) {true}
      ({ profile := 23, caller := 29, actor := 31, task := some 37, deadline := 41 } :
        authority.Context Nat Nat Nat (Option Nat) Nat) =
      .ok (.ok { permissions := {true}, resource := 19, context := { profile := 23, caller := 29, actor := 31, task := some 37, deadline := 41 } }) := by
  rw [exchange_exact sets usesModel uses uses_law heldModel held held_law id resource resource_law]
  simp [expectedExchange, usesModel, firstMissing, expectedAddress, heldModel, Slice.new, Slice.val, Data.ListN.ListN.toList]

-- Declared Authenticated access permits an empty narrowed permission payload.
theorem positive_authenticated_empty :
    authority.narrow permissions
      ({ allowed := true, reason := 7, granted := {false, true}, metadata := 29 } :
        authority.Decision (Finset Bool) Nat Nat) true
      (some (∅, .Authenticated)) 19 23 =
      .ok { allowed := true, reason := 7, granted := ∅, metadata := 29 } := by
  simp [authority.narrow, permissions]

#print axioms permissions_law
#print axioms held_law
#print axioms uses_law
#print axioms resource_law
#print axioms positive_exchange
#print axioms positive_authenticated_empty

end AuthorityInterpretation
