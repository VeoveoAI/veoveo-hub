import SelectedMapCollectionProof

open Aeneas Aeneas.Std RequestCompositionHub

namespace SelectedRouteContextProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Snapshot := SelectedCollectionProof.Snapshot
abbrev Server := SelectedCollectionProof.Server
abbrev Slug := SelectedCollectionProof.Slug
abbrev Deployment := SelectedCollectionProof.Deployment
abbrev Component := SelectedCollectionProof.Component
abbrev Entry := SelectedCollectionProof.Entry
abbrev Row := SelectedCollectionProof.Row
abbrev Composition := SelectedMapCollectionProof.Composition

/-- The native BTreeMap composition representation has unique slug keys. This
is an input refinement fact, not another authorization or rejection check. -/
def CompositionKeysUnique (composition : Composition) : Prop :=
  (composition.components.map Prod.fst).Nodup

/-- This is a separate constructor invariant, not a desired selected result.
The actual new() proof must derive it through arbitrary overwriting insertion. -/
def KeyRowCoherent (snapshot : Snapshot) : Prop :=
  ∀ key row, (key, row) ∈ snapshot.servers → key = row.deployment

/-- Independent lookup in the admitted capture, before consulting current rows. -/
def componentLookup : List Entry → Slug → Option Component
  | [], _ => none
  | (key, component) :: rest, slug =>
    if key = slug then some component else componentLookup rest slug

/-- Independent exact retained-component route. Current row lookup and slug
matching come from chooseOne; no chosen-output premise is supplied. -/
def resolveRoute (snapshot : Snapshot) (components : List Entry) (slug : Slug) : Option Row :=
  match componentLookup components slug with
  | none => none
  | some component => SelectedRegistrationProof.chooseOne snapshot (slug, component)

private theorem component_lookup_iff (components : List Entry) (slug : Slug)
    (component : Component) (unique : (components.map Prod.fst).Nodup) :
    componentLookup components slug = some component ↔ (slug, component) ∈ components := by
  revert unique
  induction components with
  | nil => intro _; simp [componentLookup]
  | cons entry tail recursive =>
    rcases entry with ⟨key, current⟩
    intro unique
    have parts : key ∉ tail.map Prod.fst ∧ (tail.map Prod.fst).Nodup := by
      simpa only [List.map_cons, List.nodup_cons] using unique
    by_cases same : key = slug
    · subst key
      constructor
      · intro found
        have equal : current = component := by simpa [componentLookup] using found
        subst component
        exact List.mem_cons_self
      · intro member
        rcases List.mem_cons.mp member with equal | inTail
        · have equalComponent : component = current := congrArg Prod.snd equal
          subst component
          simp [componentLookup]
        · exact False.elim (parts.1 (List.mem_map.mpr ⟨(slug, component), inTail, rfl⟩))
    · constructor
      · intro found
        have foundTail : componentLookup tail slug = some component := by
          simpa [componentLookup, same] using found
        exact List.mem_cons_of_mem _ ((recursive parts.2).mp foundTail)
      · intro member
        rcases List.mem_cons.mp member with equal | inTail
        · exact False.elim (same (congrArg Prod.fst equal).symm)
        · simpa [componentLookup, same] using (recursive parts.2).mpr inTail

/-- This is a specification equivalence derived from the already checked exact
entry image on one component, not another assumed source-result contract. -/
private theorem choose_one_some_iff (snapshot : Snapshot) (slug : Slug)
    (component : Component) (key : Deployment) (row : Server) :
    SelectedRegistrationProof.chooseOne snapshot (slug, component) = some (key, row) ↔
      component = .Deployment key ∧
      SelectedRegistrationProof.lookupBy snapshot.servers key = some row ∧ row.slug = slug := by
  have singleton := SelectedCollectionProof.emitted_membership snapshot [(slug, component)] key row
  simp only [SelectedCollectionProof.emitted, List.mem_filterMap] at singleton
  simpa [SelectedCollectionProof.Eligible, eq_comm, and_assoc, and_comm, and_left_comm] using singleton

private theorem resolved_some_iff (snapshot : Snapshot) (components : List Entry)
    (slug : Slug) (key : Deployment) (row : Server)
    (unique : (components.map Prod.fst).Nodup) :
    resolveRoute snapshot components slug = some (key, row) ↔
      (slug, .Deployment key) ∈ components ∧
      SelectedRegistrationProof.lookupBy snapshot.servers key = some row ∧ row.slug = slug := by
  unfold resolveRoute
  cases found : componentLookup components slug with
  | none =>
    constructor
    · intro impossible; cases impossible
    · rintro ⟨member, _, _⟩
      have selected := (component_lookup_iff components slug (.Deployment key) unique).mpr member
      rw [found] at selected
      cases selected
  | some component =>
    rw [choose_one_some_iff]
    constructor
    · rintro ⟨rfl, current, same⟩
      exact ⟨(component_lookup_iff components slug (.Deployment key) unique).mp found, current, same⟩
    · rintro ⟨member, current, same⟩
      have selected := (component_lookup_iff components slug (.Deployment key) unique).mpr member
      have equal : component = .Deployment key := Option.some.inj (found.symm.trans selected)
      exact ⟨equal, current, same⟩

private theorem rows_nodup_of_keys (rows : List Row) (unique : (rows.map Prod.fst).Nodup) :
    rows.Nodup := by
  revert unique
  induction rows with
  | nil => intro _; simp
  | cons head tail recursive =>
    intro unique
    have parts : head.1 ∉ tail.map Prod.fst ∧ (tail.map Prod.fst).Nodup := by
      simpa only [List.map_cons, List.nodup_cons] using unique
    apply List.nodup_cons.mpr
    refine ⟨?_, recursive parts.2⟩
    intro member
    exact parts.1 (List.mem_map.mpr ⟨head, member, rfl⟩)

private theorem list_option_exact {T : Type} (values : List T) (choice : Option T)
    (unique : values.Nodup)
    (members : ∀ value, value ∈ values ↔ choice = some value) :
    values = choice.toList := by
  cases choice with
  | none =>
    cases values with
    | nil => rfl
    | cons head tail =>
      have impossible := (members head).mp (List.mem_cons_self)
      cases impossible
  | some chosen =>
    cases values with
    | nil =>
      have impossible := (members chosen).mpr rfl
      cases impossible
    | cons head tail =>
      have equal : chosen = head := Option.some.inj ((members head).mp List.mem_cons_self)
      subst head
      cases tail with
      | nil => rfl
      | cons second rest =>
        have equal : chosen = second := Option.some.inj ((members second).mp (by simp))
        subst second
        simp at unique

def matchingPairs (rows : List Row) (slug : Slug) : List Row :=
  rows.filter (fun row => decide (row.2.slug = slug))

private theorem matching_values_eq (rows : List Row) (slug : Slug) :
    (rows.map Prod.snd).filter (fun row => decide (row.slug = slug)) =
      (matchingPairs rows slug).map Prod.snd := by
  induction rows with
  | nil => rfl
  | cons head tail recursive =>
    by_cases same : head.2.slug = slug <;> simp [matchingPairs, same, recursive]

/-- Actual selected/get is determined by the retained capture and the current
supplied catalog generation. Its result is derived, not a theorem premise.
Unavailability, absent captures, missing rows and wrong slugs have no fallback. -/
theorem selected_get_exact (snapshot : Snapshot) (composition : Composition)
    (selected : Snapshot) (slug : Slug)
    (success : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected
      snapshot composition = .ok selected)
    (unique : CompositionKeysUnique composition) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get selected slug =
      .ok ((resolveRoute snapshot composition.components slug).map Prod.snd) := by
  have image := SelectedMapCollectionProof.selected_success_complete snapshot composition selected success
  rcases image with ⟨_, outputKeysUnique, outputMembers⟩
  have pairMembers : ∀ candidate, candidate ∈ matchingPairs selected.servers slug ↔
      resolveRoute snapshot composition.components slug = some candidate := by
    rintro ⟨key, row⟩
    simp only [matchingPairs, List.mem_filter, decide_eq_true_eq]
    rw [resolved_some_iff snapshot composition.components slug key row unique]
    constructor
    · rintro ⟨member, same⟩
      rcases (outputMembers key row).mp member with ⟨declared, component, current, matched⟩
      have sameSlug : declared = slug := matched.symm.trans same
      rw [sameSlug] at component
      exact ⟨component, current, same⟩
    · rintro ⟨component, current, same⟩
      exact ⟨(outputMembers key row).mpr ⟨slug, component, current, same⟩, same⟩
  have pairUnique := (rows_nodup_of_keys selected.servers outputKeysUnique).filter
    (fun row => decide (row.2.slug = slug))
  have matched : matchingPairs selected.servers slug =
      (resolveRoute snapshot composition.components slug).toList :=
    list_option_exact _ _ pairUnique pairMembers
  rw [SelectedRegistrationProof.get_exact]
  change (.ok (SelectedRegistrationProof.onlyMatch
    ((selected.servers.map Prod.snd).filter (fun row => decide (row.slug = slug)))) :
      Result (Option Server)) = _
  rw [matching_values_eq, matched]
  cases resolveRoute snapshot composition.components slug <;> rfl

private theorem lookup_row_member (rows : List Row) (key : Deployment) (row : Server)
    (found : SelectedRegistrationProof.lookupBy rows key = some row) : (key, row) ∈ rows := by
  induction rows with
  | nil => simp [SelectedRegistrationProof.lookupBy] at found
  | cons head tail recursive =>
    rcases head with ⟨headKey, headRow⟩
    by_cases same : headKey = key
    · have equal : headRow = row := by
        simpa [SelectedRegistrationProof.lookupBy, same] using found
      subst row
      subst headKey
      exact List.mem_cons_self
    · have inTail := recursive (by simpa [SelectedRegistrationProof.lookupBy, same] using found)
      exact List.mem_cons_of_mem _ inTail

/-- Row-field/captured-key equality comes from constructor coherence, separately
from the composition-key uniqueness used by the getter. -/
theorem resolved_route_fields (snapshot : Snapshot) (composition : Composition)
    (slug : Slug) (key : Deployment) (row : Server)
    (unique : CompositionKeysUnique composition) (coherent : KeyRowCoherent snapshot)
    (route : resolveRoute snapshot composition.components slug = some (key, row)) :
    row.slug = slug ∧ row.deployment = key := by
  rcases (resolved_some_iff snapshot composition.components slug key row unique).mp route with
    ⟨_, current, same⟩
  exact ⟨same, (coherent key row (lookup_row_member _ _ _ current)).symm⟩

/-- Connected accepted-route/context guarantee, using the complete current row.
The retained deployment is stable; endpoint/declaration may differ from an older
Task-creation generation. G is preserved as supplied, not proved current-policy
provenance here. Context refusal remains exactly the actual request_context Rust
Result; no success/content assumption is made for native error formatting. -/
theorem selected_route_context_exact
    (snapshot : Snapshot) (composition : Composition) (selected : Snapshot) (slug : Slug)
    (success : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected
      snapshot composition = .ok selected)
    (unique : CompositionKeysUnique composition) (coherent : KeyRowCoherent snapshot)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (subject : SnapshotAssemblyProof.Subject)
    (permissions : alloc.collections.btree.set.BTreeSet
      veoveo_types.permission_names.PermissionName alloc.alloc.Global) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get selected slug =
      .ok ((resolveRoute snapshot composition.components slug).map Prod.snd) ∧
    RequestCompositionHub.auth.verified.AuthenticatedSubject.request_context subject =
      .ok (SnapshotAssemblyProof.expectedContext subject) ∧
    ∀ key row context,
      resolveRoute snapshot composition.components slug = some (key, row) →
      SnapshotAssemblyProof.expectedContext subject = .Ok context →
      RequestCompositionHub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
        http issuer profile row subject permissions =
        .ok (.Ok { (SnapshotAssemblyProof.expectedClient
          http issuer profile row subject permissions context) with
            server := slug, deployment := key }) := by
  refine ⟨selected_get_exact snapshot composition selected slug success unique,
    SnapshotAssemblyProof.request_context_exact subject, ?_⟩
  intro key row context route accepted
  rcases resolved_route_fields snapshot composition slug key row unique coherent route with
    ⟨sameSlug, sameDeployment⟩
  have client := SnapshotAssemblyProof.http_new_exact http issuer profile row subject permissions context accepted
  simpa [SnapshotAssemblyProof.expectedClient, ← sameSlug, ← sameDeployment] using client

end
end SelectedRouteContextProof

#print axioms SelectedRouteContextProof.selected_get_exact
#print axioms SelectedRouteContextProof.resolved_route_fields
#print axioms SelectedRouteContextProof.selected_route_context_exact
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let withNamespaceBoundary := standard.push
    ``RequestCompositionHub.registration.snapshot.RegistrationSnapshot.build_namespaces
  let getter ← Lean.collectAxioms ``SelectedRouteContextProof.selected_get_exact
  unless (getter.filter (fun axiomName => !withNamespaceBoundary.contains axiomName)).isEmpty do
    throwError "unreviewed exact-route boundary: {getter}"
  let fields ← Lean.collectAxioms ``SelectedRouteContextProof.resolved_route_fields
  unless (fields.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
    throwError "unreviewed route-field axiom: {fields}"
  let withFormatting := withNamespaceBoundary ++ #[
    ``RequestCompositionHub.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionHub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt]
  let client ← Lean.collectAxioms ``SelectedRouteContextProof.selected_route_context_exact
  unless (client.filter (fun axiomName => !withFormatting.contains axiomName)).isEmpty do
    throwError "unreviewed route/context boundary: {client}"
