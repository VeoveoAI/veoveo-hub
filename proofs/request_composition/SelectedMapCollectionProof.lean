import SelectedMapInsertionProof

open Aeneas Aeneas.Std RequestCompositionHub IteratorProof

namespace SelectedMapCollectionProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Snapshot := SelectedCollectionProof.Snapshot
abbrev Entry := SelectedCollectionProof.Entry
abbrev Row := SelectedCollectionProof.Row
abbrev Server := SelectedCollectionProof.Server
abbrev Deployment := SelectedCollectionProof.Deployment
abbrev FilterState := SelectedCollectionProof.FilterState
abbrev Composition := veoveo_hub_sdk.execution.ResolvedComposition
abbrev filteredIterator := SelectedCollectionProof.filteredIterator
abbrev filteredEnumeration := SelectedCollectionProof.filteredEnumeration
abbrev comparison := SelectedMapInsertionProof.comparison
abbrev Ordered := SelectedMapInsertionProof.Ordered
abbrev emitted := SelectedCollectionProof.emitted

/-- Independent complete collection postcondition. The output is ordered with
unique keys, and its whole rows are exactly the old accumulator and remaining
semantic emissions. The implementation's insertion loop does not define this
membership relation. -/
def Collected (before pending after : List Row) : Prop :=
  Ordered after ∧ (after.map Prod.fst).Nodup ∧
    ∀ candidate, candidate ∈ after ↔ candidate ∈ before ∨ candidate ∈ pending

/-- Complete raw route-map image; no composition-key uniqueness or desired-route
premise is needed at this layer. Per-slug getter uniqueness is separate. -/
def MapImage (snapshot : Snapshot) (components : List Entry) (rows : List Row) : Prop :=
  Ordered rows ∧ (rows.map Prod.fst).Nodup ∧
    ∀ key row, (key, row) ∈ rows ↔ SelectedCollectionProof.Eligible snapshot components key row

/-- This exact collectMap recurrence is total over the proved finite filtered
cursor. Every accumulator/remaining row belongs to one fixed emission image,
which derives equal-key compatibility for actual replacement insertion. -/
theorem collect_map_correct (snapshot : Snapshot) (components : List Entry)
    (state : FilterState) (before : List Row)
    (valid : (filteredEnumeration snapshot).valid state)
    (ordered : Ordered before)
    (beforeMembers : ∀ row ∈ before, row ∈ emitted snapshot components)
    (remainingMembers : ∀ row ∈ (filteredEnumeration snapshot).remaining state,
      row ∈ emitted snapshot components) :
    RequestCompositionHub.CurrentRolesNative.collectMap filteredIterator comparison state before
      ⦃ after => Collected before ((filteredEnumeration snapshot).remaining state) after ⦄ := by
  unfold RequestCompositionHub.CurrentRolesNative.collectMap
  step with (SelectedCollectionProof.filtered_cursor_law snapshot state valid)
    as ⟨entry, next, taken⟩
  rcases taken with ⟨nextValid, entryEq, suffix⟩
  change entry = ((filteredEnumeration snapshot).remaining state).head? at entryEq
  change (filteredEnumeration snapshot).remaining next =
    ((filteredEnumeration snapshot).remaining state).tail at suffix
  cases remaining : (filteredEnumeration snapshot).remaining state with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    subst entry
    simp only [WP.spec_ok]
    refine ⟨ordered, SelectedMapInsertionProof.ordered_keys_nodup _ ordered, ?_⟩
    intro candidate
    simp [remaining]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    subst entry
    rcases head with ⟨key, server⟩
    have newMember : (key, server) ∈ emitted snapshot components :=
      remainingMembers _ (by rw [remaining]; simp)
    step with (SelectedMapInsertionProof.insert_emitted_ordered snapshot components
      (key, server) before newMember beforeMembers ordered) as ⟨updated, inserted⟩
    rcases inserted with ⟨updatedOrdered, _, insertionMembership⟩
    have updatedMembers : ∀ row ∈ updated, row ∈ emitted snapshot components := by
      intro row member
      rcases (insertionMembership row).mp member with same | old
      · rw [same]; exact newMember
      · exact beforeMembers row old
    have nextMembers : ∀ row ∈ (filteredEnumeration snapshot).remaining next,
        row ∈ emitted snapshot components := by
      intro row member
      have inTail : row ∈ tail := by simpa only [suffix] using member
      apply remainingMembers row
      rw [remaining]
      exact List.mem_cons_of_mem _ inTail
    have recursive := collect_map_correct snapshot components next updated nextValid
      updatedOrdered updatedMembers nextMembers
    apply WP.spec_mono recursive
    intro result specification
    rcases specification with ⟨resultOrdered, resultUnique, membership⟩
    refine ⟨resultOrdered, resultUnique, ?_⟩
    intro candidate
    rw [membership candidate, insertionMembership candidate, suffix]
    simp only [List.mem_cons]
    tauto
termination_by ((filteredEnumeration snapshot).remaining state).length
decreasing_by simp_all

/-- The total-WP contract supplies an actual outer .ok witness. No desired result
or helper-success axiom is used to eliminate errors or divergence. -/
private theorem result_exists_of_total {T : Type} (operation : Result T) (post : T → Prop)
    (total : operation ⦃ result => post result ⦄) :
    ∃ result, operation = .ok result ∧ post result := by
  cases operation using Result.cases with
  | ret value => exact ⟨value, rfl, (WP.spec_ok value).mp total⟩
  | vis effect continuation => exact False.elim ((WP.spec_vis effect continuation).mp total)
  | div => exact False.elim (WP.spec_div.mp total)

abbrev initial (snapshot : Snapshot) (components : List Entry) : FilterState :=
  { iter := components, f := snapshot }

/-- A name for the actual operational call, not a separate selection reducer. -/
abbrev collectEntries (snapshot : Snapshot) (components : List Entry) : Result (List Row) :=
  RequestCompositionHub.CurrentRolesNative.collectMap filteredIterator comparison
    (initial snapshot components) []

/-- Unconditional finite collection in this operational model: every and only
eligible whole row is present, and strict order/unique keys are derived. Native
BTreeMap/Arc/iterator correspondence is an explicit separate boundary. -/
theorem collect_complete (snapshot : Snapshot) (components : List Entry) :
    ∃ rows, collectEntries snapshot components = .ok rows ∧ MapImage snapshot components rows := by
  have total := collect_map_correct snapshot components (initial snapshot components) [] rfl
    (by simp [Ordered, SelectedMapInsertionProof.Ordered])
    (by intro row member; cases member)
    (by intro row member; exact member)
  rcases result_exists_of_total _ _ total with ⟨rows, success, ordered, unique, membership⟩
  refine ⟨rows, success, ordered, unique, ?_⟩
  intro key row
  have members : (key, row) ∈ rows ↔ (key, row) ∈ emitted snapshot components := by
    simpa only [List.not_mem_nil, false_or,
      initial, filteredEnumeration, SelectedCollectionProof.filteredEnumeration] using
      membership (key, row)
  exact members.trans (SelectedCollectionProof.emitted_membership snapshot components key row)

/-- Kernel-checked source factorization retains the exact extracted iterator,
filter_map, collect/from_iter, comparator and clone wrappers. The unresolved
namespace helper remains inside the actual of call, never a map-result oracle. -/
private theorem selected_collection_body (snapshot : Snapshot) (composition : Composition) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected snapshot composition = (do
      let rows ← collectEntries snapshot composition.components
      RequestCompositionHub.registration.snapshot.RegistrationSnapshot.of rows) := by
  unfold RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected
  simp only [
    RequestCompositionHub.veoveo_hub_sdk.execution.ResolvedComposition.impl.components,
    RequestCompositionHub.alloc.collections.btree.map.BTreeMap.iter,
    RequestCompositionHub.core.iter.traits.iterator.Iterator.filter_map.default,
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default,
    RequestCompositionHub.alloc.collections.btree.map.BTreeMapKVGlobal.Insts.CoreIterTraitsCollectFromIteratorPair,
    RequestCompositionHub.alloc.collections.btree.map.BTreeMapKVGlobal.Insts.CoreIterTraitsCollectFromIteratorPair.from_iter,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket.into_iter,
    RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd,
    collectEntries, initial, filteredIterator, comparison,
    bind_ok, bind_tc_ok]

/-- The collector's complete map is derived before the unresolved own-code
namespace helper runs. This does not assert total selected success. -/
theorem selected_factorization (snapshot : Snapshot) (composition : Composition) :
    ∃ rows, MapImage snapshot composition.components rows ∧
      RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected snapshot composition =
        RequestCompositionHub.registration.snapshot.RegistrationSnapshot.of rows := by
  rcases collect_complete snapshot composition.components with ⟨rows, collected, image⟩
  refine ⟨rows, image, ?_⟩
  rw [selected_collection_body, collected]
  simp only [bind_ok, bind_tc_ok]

/-- Conditional actual-caller success preserves the exact complete eligible map,
strict ordering and unique deployment keys. Namespace helper completion/order,
per-slug route uniqueness and request/issuer composition remain unproved. -/
theorem selected_success_complete (snapshot : Snapshot) (composition : Composition)
    (result : Snapshot)
    (success : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected
      snapshot composition = .ok result) :
    MapImage snapshot composition.components result.servers := by
  rcases selected_factorization snapshot composition with ⟨rows, image, factorization⟩
  have assembled : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.of rows = .ok result := by
    rw [← factorization]
    exact success
  have sameMap := (SnapshotAssemblyProof.extracted_of_success rows result assembled).1
  rw [sameMap]
  exact image

end
end SelectedMapCollectionProof

#print axioms SelectedMapCollectionProof.collect_map_correct
#print axioms SelectedMapCollectionProof.collect_complete
#print axioms SelectedMapCollectionProof.selected_factorization
#print axioms SelectedMapCollectionProof.selected_success_complete
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SelectedMapCollectionProof.collect_map_correct,
      ``SelectedMapCollectionProof.collect_complete] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed collector axiom in {declaration}: {found}"
  let withNamespaceBoundary := standard.push
    ``RequestCompositionHub.registration.snapshot.RegistrationSnapshot.build_namespaces
  for declaration in #[``SelectedMapCollectionProof.selected_factorization,
      ``SelectedMapCollectionProof.selected_success_complete] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !withNamespaceBoundary.contains axiomName)).isEmpty do
      throwError "unreviewed selected-caller boundary in {declaration}: {found}"
