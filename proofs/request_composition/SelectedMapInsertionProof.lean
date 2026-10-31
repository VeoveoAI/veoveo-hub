import SelectedCollectionProof

open Aeneas Aeneas.Std RequestCompositionHub

namespace SelectedMapInsertionProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Deployment := SelectedCollectionProof.Deployment
abbrev Row := SelectedCollectionProof.Row
abbrev Snapshot := SelectedCollectionProof.Snapshot
abbrev Entry := SelectedCollectionProof.Entry
abbrev comparison := RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd.cmp

/-- Strict ordering by the exact nominal string values compared by this
operational map model. Unique keys on an unsorted list would not suffice. -/
def Ordered (rows : List Row) : Prop :=
  rows.Pairwise (fun left right => left.1.value < right.1.value)

/-- Replacement at an existing key preserves the whole row only when all
such old rows equal the inserted row. The collection caller derives this from
shared eligible emissions; it is not a desired final-map assumption. -/
def Compatible (row : Row) (rows : List Row) : Prop :=
  ∀ old ∈ rows, old.1 = row.1 → old = row

/-- Independent insertion contract: strict order, unique output keys, and the
exact whole-row union. List ordering is not used to define membership. -/
def Inserted (row : Row) (before after : List Row) : Prop :=
  Ordered after ∧ (after.map Prod.fst).Nodup ∧
    ∀ candidate, candidate ∈ after ↔ candidate = row ∨ candidate ∈ before

private theorem key_eq_of_value_eq (left right : Deployment)
    (same : left.value = right.value) : left = right := by
  cases left
  cases right
  simp_all

/-- Output key uniqueness is a consequence of strict ordering, not an input
promise about what the collector will return. -/
theorem ordered_keys_nodup (rows : List Row) (ordered : Ordered rows) :
    (rows.map Prod.fst).Nodup := by
  revert ordered
  induction rows with
  | nil => intro _; simp
  | cons head tail recursive =>
    intro ordered
    change List.Pairwise _ (head :: tail) at ordered
    rcases List.pairwise_cons.mp ordered with ⟨headBefore, tailOrdered⟩
    simp only [List.map_cons, List.nodup_cons]
    refine ⟨?_, recursive tailOrdered⟩
    intro member
    rcases List.mem_map.mp member with ⟨other, inTail, same⟩
    have strict := headBefore other inTail
    rw [same] at strict
    exact lt_irrefl _ strict

/-- Total structural proof of THIS handwritten insertMap used by the extracted
caller. Its actual comparator returns compare on key.value; both insertion and
replacement preserve the entire row. Native BTreeMap refinement is separate. -/
theorem insert_ordered_whole_row_union (row : Row) (before : List Row)
    (ordered : Ordered before) (compatible : Compatible row before) :
    RequestCompositionHub.CurrentRolesNative.insertMap comparison row.1 row.2 before
      ⦃ after => Inserted row before after ⦄ := by
  revert ordered compatible
  induction before with
  | nil =>
    intro _ _
    unfold RequestCompositionHub.CurrentRolesNative.insertMap
    simp only [WP.spec_ok]
    change Inserted row [] [row]
    refine ⟨?_, ?_, ?_⟩
    · simp [Ordered]
    · simp
    · intro candidate; simp
  | cons head tail recursive =>
    intro ordered compatible
    have orderedParts := List.pairwise_cons.mp ordered
    have headBefore := orderedParts.1
    have tailOrdered : Ordered tail := orderedParts.2
    have tailCompatible : Compatible row tail := by
      intro old member same
      exact compatible old (List.mem_cons_of_mem head member) same
    unfold RequestCompositionHub.CurrentRolesNative.insertMap
    have compareStep : comparison row.1 head.1 =
        .ok (compare row.1.value head.1.value) := rfl
    rw [compareStep]
    simp only [bind_ok]
    cases compared : compare row.1.value head.1.value with
    | lt =>
      have beforeHead : row.1.value < head.1.value := _root_.compare_lt_iff_lt.mp compared
      have outputOrdered : Ordered (row :: head :: tail) := by
        apply List.pairwise_cons.mpr
        refine ⟨?_, ordered⟩
        intro other member
        rcases List.mem_cons.mp member with equal | inTail
        · subst other; exact beforeHead
        · exact lt_trans beforeHead (headBefore other inTail)
      simp only [WP.spec_ok]
      change Inserted row (head :: tail) (row :: head :: tail)
      refine ⟨outputOrdered, ordered_keys_nodup _ outputOrdered, ?_⟩
      intro candidate; simp
    | eq =>
      have sameKey : row.1 = head.1 :=
        key_eq_of_value_eq _ _ (_root_.compare_eq_iff_eq.mp compared)
      have sameRow : head = row := compatible head (by simp) sameKey.symm
      subst head
      simp only [WP.spec_ok]
      change Inserted row (row :: tail) (row :: tail)
      refine ⟨ordered, ordered_keys_nodup _ ordered, ?_⟩
      intro candidate; simp
    | gt =>
      have afterHead : head.1.value < row.1.value := _root_.compare_gt_iff_gt.mp compared
      step with (recursive tailOrdered tailCompatible) as ⟨result, resultSpec⟩
      rcases resultSpec with ⟨resultOrdered, _, membership⟩
      have outputOrdered : Ordered (head :: result) := by
        apply List.pairwise_cons.mpr
        refine ⟨?_, resultOrdered⟩
        intro other member
        rcases (membership other).mp member with equal | inTail
        · subst other; exact afterHead
        · exact headBefore other inTail
      change Inserted row (head :: tail) (head :: result)
      refine ⟨outputOrdered, ordered_keys_nodup _ outputOrdered, ?_⟩
      intro candidate
      simp only [List.mem_cons, membership]
      tauto

/-- The exact-emission contract discharges compatibility of every accumulator
subset with every new emitted row, including repeated component selections. -/
theorem emitted_compatible (snapshot : Snapshot) (components : List Entry)
    (row : Row) (before : List Row)
    (newMember : row ∈ SelectedCollectionProof.emitted snapshot components)
    (oldMembers : ∀ old ∈ before, old ∈ SelectedCollectionProof.emitted snapshot components) :
    Compatible row before := by
  intro old member sameKey
  rcases row with ⟨key, server⟩
  rcases old with ⟨oldKey, oldServer⟩
  change oldKey = key at sameKey
  subst oldKey
  have sameServer := SelectedCollectionProof.emitted_key_functional
    snapshot components key oldServer server (oldMembers _ member) newMember
  exact congrArg (fun value => (key, value)) sameServer

/-- Complete ordered insertion specialized to the independently characterized
eligible stream. The only accumulator premises concern prior emitted rows and
ordering; all output properties are derived from the real operation. -/
theorem insert_emitted_ordered (snapshot : Snapshot) (components : List Entry)
    (row : Row) (before : List Row)
    (newMember : row ∈ SelectedCollectionProof.emitted snapshot components)
    (oldMembers : ∀ old ∈ before, old ∈ SelectedCollectionProof.emitted snapshot components)
    (ordered : Ordered before) :
    RequestCompositionHub.CurrentRolesNative.insertMap comparison row.1 row.2 before
      ⦃ after => Inserted row before after ⦄ := by
  exact insert_ordered_whole_row_union row before ordered
    (emitted_compatible snapshot components row before newMember oldMembers)

end
end SelectedMapInsertionProof

#print axioms SelectedMapInsertionProof.ordered_keys_nodup
#print axioms SelectedMapInsertionProof.insert_ordered_whole_row_union
#print axioms SelectedMapInsertionProof.emitted_compatible
#print axioms SelectedMapInsertionProof.insert_emitted_ordered
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SelectedMapInsertionProof.ordered_keys_nodup,
      ``SelectedMapInsertionProof.insert_ordered_whole_row_union,
      ``SelectedMapInsertionProof.emitted_compatible,
      ``SelectedMapInsertionProof.insert_emitted_ordered] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed ordered-insertion axiom in {declaration}: {found}"
