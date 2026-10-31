import SelectedRegistrationProof

open Aeneas Aeneas.Std RequestCompositionHub IteratorProof

namespace SelectedCollectionProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Server := SelectedRegistrationProof.Server
abbrev Snapshot := SelectedRegistrationProof.Snapshot
abbrev Slug := SelectedRegistrationProof.Slug
abbrev Deployment := SelectedRegistrationProof.Deployment
abbrev Component := veoveo_hub_sdk.execution.ResolvedComponent
abbrev Entry := Slug × Component
abbrev Row := Deployment × Server
abbrev selectOps := SelectedRegistrationProof.selectOps

/-- Independent complete route image. A raw component must request this exact
key; independent current keyed lookup supplies the whole row and its slug must
match. No selected-result or namespace-helper premise occurs here. -/
def Eligible (snapshot : Snapshot) (components : List Entry)
    (key : Deployment) (row : Server) : Prop :=
  ∃ slug, (slug, .Deployment key) ∈ components ∧
    SelectedRegistrationProof.lookupBy snapshot.servers key = some row ∧ row.slug = slug

/-- Semantic emission list before map collection; order is the finite raw input
order. Membership, not this order, specifies the final route-map image. -/
def emitted (snapshot : Snapshot) (components : List Entry) : List Row :=
  components.filterMap (SelectedRegistrationProof.chooseOne snapshot)

abbrev componentIterator :=
  RequestCompositionHub.alloc.collections.btree.map.Iter.Insts.CoreIterTraitsIteratorIteratorPairSharedAKSharedAV
    Slug Component

abbrev FilterState := core.iter.adapters.filter_map.FilterMap (List Entry) Snapshot

def filteredEnumeration (snapshot : Snapshot) : Enumeration FilterState Row where
  remaining state := emitted snapshot state.iter
  valid state := state.f = snapshot

abbrev filteredIterator :=
  RequestCompositionHub.core.iter.adapters.filter_map.FilterMap.Insts.CoreIterTraitsIteratorIterator
    componentIterator selectOps

/-- The actual component iterator consumes its finite raw suffix. -/
theorem component_cursor_law : CursorLaw componentIterator (listEnumeration Entry) := by
  intro state _
  cases state <;> simp [componentIterator,
    RequestCompositionHub.alloc.collections.btree.map.Iter.Insts.CoreIterTraitsIteratorIteratorPairSharedAKSharedAV.next,
    NextStep, listEnumeration]

/-- Total finite-suffix proof for THIS filterMapNext implementation. A skipped
entry decreases the raw suffix; an emitted entry preserves the full snapshot
closure. No equality of separate partial_fixpoint definitions is assumed. -/
theorem filter_map_next_correct (snapshot : Snapshot) (state : FilterState)
    (valid : (filteredEnumeration snapshot).valid state) :
    RequestCompositionHub.CurrentRolesNative.filterMapNext componentIterator selectOps state
      ⦃ result => NextStep (filteredEnumeration snapshot) state result ⦄ := by
  unfold RequestCompositionHub.CurrentRolesNative.filterMapNext
  step with (component_cursor_law state.iter trivial) as ⟨entry, next, taken⟩
  rcases taken with ⟨_, entryEq, suffix⟩
  change state.f = snapshot at valid
  change entry = state.iter.head? at entryEq
  change next = state.iter.tail at suffix
  cases remaining : state.iter with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    subst entry
    subst next
    simp [NextStep, filteredEnumeration, emitted, remaining, valid]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    subst entry
    subst next
    simp only [bind_ok, bind_tc_ok]
    rw [valid]
    have mapperStep :
        RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected.closure.Insts.CoreOpsFunctionFnMutTuplePairShared1ServerSlugShared2ResolvedComponentOptionPairDeploymentIdArcRegisteredServer.call_mut
          snapshot head =
          .ok (SelectedRegistrationProof.chooseOne snapshot head, snapshot) :=
      SelectedRegistrationProof.selected_entry_exact snapshot head
    rw [mapperStep]
    simp only [bind_ok, bind_tc_ok]
    cases chosen : SelectedRegistrationProof.chooseOne snapshot head with
    | some row =>
      simp [chosen, NextStep, filteredEnumeration, emitted, remaining]
    | none =>
      have recursive := filter_map_next_correct snapshot
        ({ iter := tail, f := snapshot } : FilterState) rfl
      apply WP.spec_mono recursive
      intro result specification
      simpa [NextStep, filteredEnumeration, emitted, remaining, chosen] using specification

termination_by state.iter.length
decreasing_by simp_all [listEnumeration]

/-- Actual iterator wrapper inherits the proved total step law. -/
theorem filtered_cursor_law (snapshot : Snapshot) :
    CursorLaw filteredIterator (filteredEnumeration snapshot) := by
  intro state valid
  simpa only [filteredIterator,
    RequestCompositionHub.core.iter.adapters.filter_map.FilterMap.Insts.CoreIterTraitsIteratorIterator.next] using
    filter_map_next_correct snapshot state valid

/-- Exact eligible contents in both directions, including complete row values.
Arbitrary repeated component entries are allowed at this raw-image boundary. -/
theorem emitted_membership (snapshot : Snapshot) (components : List Entry)
    (key : Deployment) (row : Server) :
    (key, row) ∈ emitted snapshot components ↔ Eligible snapshot components key row := by
  simp only [emitted, List.mem_filterMap]
  constructor
  · rintro ⟨⟨slug, component⟩, member, emission⟩
    cases component with
    | Unavailable =>
      simp [SelectedRegistrationProof.chooseOne] at emission
    | Deployment deployment =>
      cases found : SelectedRegistrationProof.lookupBy snapshot.servers deployment with
      | none => simp [SelectedRegistrationProof.chooseOne, found] at emission
      | some server =>
        by_cases same : server.slug = slug
        · simp only [SelectedRegistrationProof.chooseOne, found, if_pos same,
            Option.some.injEq, Prod.mk.injEq] at emission
          rcases emission with ⟨rfl, rfl⟩
          exact ⟨slug, member, found, same⟩
        · simp [SelectedRegistrationProof.chooseOne, found, same] at emission
  · rintro ⟨slug, member, found, same⟩
    exact ⟨(slug, .Deployment key), member, by
      simp [SelectedRegistrationProof.chooseOne, found, same]⟩

/-- Equal output keys necessarily carry the same whole looked-up row, so later
map deduplication cannot discard a distinct eligible route at that key. This
requires neither component-key uniqueness nor an assumed desired registration. -/
theorem emitted_key_functional (snapshot : Snapshot) (components : List Entry)
    (key : Deployment) (left right : Server)
    (first : (key, left) ∈ emitted snapshot components)
    (second : (key, right) ∈ emitted snapshot components) : left = right := by
  rcases (emitted_membership snapshot components key left).mp first with ⟨_, _, firstLookup, _⟩
  rcases (emitted_membership snapshot components key right).mp second with ⟨_, _, secondLookup, _⟩
  rw [firstLookup] at secondLookup
  exact Option.some.inj secondLookup

end
end SelectedCollectionProof

#print axioms SelectedCollectionProof.component_cursor_law
#print axioms SelectedCollectionProof.filter_map_next_correct
#print axioms SelectedCollectionProof.filtered_cursor_law
#print axioms SelectedCollectionProof.emitted_membership
#print axioms SelectedCollectionProof.emitted_key_functional
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SelectedCollectionProof.component_cursor_law,
      ``SelectedCollectionProof.filter_map_next_correct,
      ``SelectedCollectionProof.filtered_cursor_law,
      ``SelectedCollectionProof.emitted_membership,
      ``SelectedCollectionProof.emitted_key_functional] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed finite-selection axiom in {declaration}: {found}"
