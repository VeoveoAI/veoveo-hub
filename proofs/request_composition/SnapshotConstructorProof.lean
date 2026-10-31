import SelectedRouteContextProof

open Aeneas Aeneas.Std RequestCompositionHub IteratorProof

namespace SnapshotConstructorProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Server := SelectedCollectionProof.Server
abbrev Snapshot := SelectedCollectionProof.Snapshot
abbrev Row := SelectedCollectionProof.Row
abbrev Deployment := SelectedCollectionProof.Deployment
abbrev comparison := SelectedMapInsertionProof.comparison

def CoherentRows (rows : List Row) : Prop :=
  ∀ entry ∈ rows, entry.1 = entry.2.deployment

/-- Any overwriting insertion preserves key/row coherence. No ordering or
same-key value compatibility is assumed: a repeated deployment may replace a
different complete row, exactly as the operational map permits. -/
theorem insert_coherent (key : Deployment) (row : Server) (before : List Row)
    (newCoherent : key = row.deployment) (coherent : CoherentRows before) :
    RequestCompositionHub.CurrentRolesNative.insertMap comparison key row before
      ⦃ after => CoherentRows after ⦄ := by
  revert coherent
  induction before with
  | nil =>
    intro _
    unfold RequestCompositionHub.CurrentRolesNative.insertMap
    simp only [WP.spec_ok]
    intro entry member
    have same : entry = (key, row) := by simpa using member
    subst entry
    exact newCoherent
  | cons head tail recursive =>
    intro coherent
    have tailCoherent : CoherentRows tail := by
      intro entry member
      exact coherent entry (List.mem_cons_of_mem head member)
    unfold RequestCompositionHub.CurrentRolesNative.insertMap
    have compareStep : comparison key head.1 = .ok (compare key.value head.1.value) := rfl
    rw [compareStep]
    simp only [bind_ok]
    cases compare key.value head.1.value with
    | lt =>
      simp only [WP.spec_ok]
      intro entry member
      rcases List.mem_cons.mp member with same | old
      · subst entry; exact newCoherent
      · exact coherent entry old
    | eq =>
      simp only [WP.spec_ok]
      intro entry member
      rcases List.mem_cons.mp member with same | old
      · subst entry; exact newCoherent
      · exact coherent entry (List.mem_cons_of_mem head old)
    | gt =>
      step with (recursive tailCoherent) as ⟨updated, updatedCoherent⟩
      try simp only [WP.spec_ok]
      intro entry member
      rcases List.mem_cons.mp member with same | old
      · subst entry; exact coherent head List.mem_cons_self
      · exact updatedCoherent entry old

/-- Finite cursor collection preserves only the invariant needed by routing.
This does not impose the false whole-row union premise on conflicting duplicate
constructor inputs, nor equate distinct partial_fixpoint definitions. -/
theorem collect_coherent {I : Type}
    (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I Row)
    (model : Enumeration I Row) (cursor : CursorLaw iterator model)
    (state : I) (before : List Row) (valid : model.valid state)
    (coherent : CoherentRows before) (pending : CoherentRows (model.remaining state)) :
    RequestCompositionHub.CurrentRolesNative.collectMap iterator comparison state before
      ⦃ after => CoherentRows after ⦄ := by
  unfold RequestCompositionHub.CurrentRolesNative.collectMap
  step with (cursor state valid) as ⟨entry, next, taken⟩
  rcases taken with ⟨nextValid, entryEq, suffix⟩
  change entry = (model.remaining state).head? at entryEq
  change model.remaining next = (model.remaining state).tail at suffix
  cases remaining : model.remaining state with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    subst entry
    simpa only [WP.spec_ok] using coherent
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    subst entry
    rcases head with ⟨key, row⟩
    have newCoherent : key = row.deployment := pending (key, row) (by rw [remaining]; simp)
    step with (insert_coherent key row before newCoherent coherent) as ⟨updated, updatedCoherent⟩
    have nextCoherent : CoherentRows (model.remaining next) := by
      intro entry member
      apply pending entry
      rw [remaining]
      exact List.mem_cons_of_mem _ (by simpa only [suffix] using member)
    exact collect_coherent iterator model cursor next updated nextValid updatedCoherent nextCoherent
termination_by (model.remaining state).length
decreasing_by simp_all

section Constructor
variable {S I : Type}
variable (ops : Aeneas.Std.core.iter.traits.collect.IntoIterator S Server I)

abbrev newMapper :=
  RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new.closure.Insts.CoreOpsFunctionFnMutTupleRegisteredServerPairDeploymentIdArcRegisteredServer ops

abbrev mappedIterator :=
  RequestCompositionHub.core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator
    ops.iteratorInst (newMapper ops)

def constructorEnumeration (model : Enumeration I Server) :
    Enumeration (Aeneas.Std.core.iter.adapters.map.Map I Unit) Row :=
  mapEnumeration model (fun (_ : Unit) => True) (fun server => (server.deployment, server))

/-- Actual source field projection and Arc allocation model, not an assumed
mapping law for an arbitrary function or native iterator override. -/
theorem new_mapper_exact (state : Unit) (server : Server) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new.closure.Insts.CoreOpsFunctionFnMutTupleRegisteredServerPairDeploymentIdArcRegisteredServer.call_mut
      ops state server = .ok ((server.deployment, server), state) := by
  unfold RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new.closure.Insts.CoreOpsFunctionFnMutTupleRegisteredServerPairDeploymentIdArcRegisteredServer.call_mut
  simp only [RequestCompositionHub.alloc.sync.Arc.new, bind_ok]

/-- The actual map adapter over an explicitly finite input cursor. The native
IntoIterator/default-map/Arc refinement remains separate from this model law. -/
theorem constructor_cursor_law (model : Enumeration I Server)
    (cursor : CursorLaw ops.iteratorInst model) :
    CursorLaw (mappedIterator ops) (constructorEnumeration model) := by
  intro state valid
  change RequestCompositionHub.core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator.next
    ops.iteratorInst (newMapper ops) state
    ⦃ result => NextStep (constructorEnumeration model) state result ⦄
  unfold RequestCompositionHub.core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator.next
  step with (cursor state.iter valid.1) as ⟨entry, next, taken⟩
  rcases taken with ⟨nextValid, entryEq, suffix⟩
  change entry = (model.remaining state.iter).head? at entryEq
  change model.remaining next = (model.remaining state.iter).tail at suffix
  cases remaining : model.remaining state.iter with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    subst entry
    simp [NextStep, constructorEnumeration, mapEnumeration, remaining, suffix, nextValid]
  | cons server tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    subst entry
    simp only [bind_ok, bind_tc_ok]
    rw [new_mapper_exact]
    simp [NextStep, constructorEnumeration, mapEnumeration, remaining, suffix, nextValid]

theorem remaining_coherent (model : Enumeration I Server)
    (state : Aeneas.Std.core.iter.adapters.map.Map I Unit) :
    CoherentRows ((constructorEnumeration model).remaining state) := by
  intro entry member
  change entry ∈ (model.remaining state.iter).map (fun server => (server.deployment, server)) at member
  rcases List.mem_map.mp member with ⟨server, _, equal⟩
  subst entry
  rfl

private theorem result_exists_of_total {T : Type} (operation : Result T) (post : T → Prop)
    (total : operation ⦃ result => post result ⦄) :
    ∃ result, operation = .ok result ∧ post result := by
  cases operation using Result.cases with
  | ret value => exact ⟨value, rfl, (WP.spec_ok value).mp total⟩
  | vis effect continuation => exact False.elim ((WP.spec_vis effect continuation).mp total)
  | div => exact False.elim (WP.spec_div.mp total)

private theorem new_collection_body (source : S) (initial : I)
    (opened : ops.into_iter source = .ok initial) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new ops source = (do
      let rows ← RequestCompositionHub.CurrentRolesNative.collectMap (mappedIterator ops) comparison
        ({ iter := initial, f := () } : Aeneas.Std.core.iter.adapters.map.Map I Unit) []
      RequestCompositionHub.registration.snapshot.RegistrationSnapshot.of rows) := by
  unfold RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new
  rw [opened]
  simp only [Aeneas.Std.core.iter.traits.iterator.Iterator.map,
    RequestCompositionHub.core.iter.traits.iterator.Iterator.map.default,
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default,
    RequestCompositionHub.alloc.collections.btree.map.BTreeMapKVGlobal.Insts.CoreIterTraitsCollectFromIteratorPair,
    RequestCompositionHub.alloc.collections.btree.map.BTreeMapKVGlobal.Insts.CoreIterTraitsCollectFromIteratorPair.from_iter,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket.into_iter,
    RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd,
    mappedIterator, newMapper, comparison, bind_ok, bind_tc_ok]

/-- Successful actual generic construction has coherent keys. Only a finite
operational cursor and the actual input conversion are assumed, not arbitrary
native IntoIterator totality or equal values for repeated deployment IDs.
Namespace-helper completion and native allocation remain explicit boundaries. -/
theorem new_success_coherent (source : S) (initial : I)
    (model : Enumeration I Server) (cursor : CursorLaw ops.iteratorInst model)
    (opened : ops.into_iter source = .ok initial) (valid : model.valid initial)
    (result : Snapshot)
    (success : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new ops source = .ok result) :
    SelectedRouteContextProof.KeyRowCoherent result := by
  let state : Aeneas.Std.core.iter.adapters.map.Map I Unit := { iter := initial, f := () }
  have total := collect_coherent (mappedIterator ops) (constructorEnumeration model)
    (constructor_cursor_law ops model cursor) state [] ⟨valid, trivial⟩
    (by intro entry member; cases member) (remaining_coherent model state)
  rcases result_exists_of_total _ _ total with ⟨rows, collected, coherent⟩
  have factorization := new_collection_body ops source initial opened
  rw [collected] at factorization
  simp only [bind_ok, bind_tc_ok] at factorization
  have assembled : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.of rows = .ok result := by
    rw [← factorization]
    exact success
  have sameMap := (SnapshotAssemblyProof.extracted_of_success rows result assembled).1
  intro key row member
  rw [sameMap] at member
  exact coherent (key, row) member

/-- The constructor discharges the row/key premise of the connected retained
route/context guarantee. Component uniqueness is still its separate native-map
boundary. Endpoint/declaration are from this supplied catalog generation. -/
theorem constructed_route_context_exact (source : S) (initial : I)
    (model : Enumeration I Server) (cursor : CursorLaw ops.iteratorInst model)
    (opened : ops.into_iter source = .ok initial) (valid : model.valid initial)
    (snapshot : Snapshot)
    (constructed : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.new ops source = .ok snapshot)
    (composition : SelectedMapCollectionProof.Composition) (selected : Snapshot)
    (slug : SelectedCollectionProof.Slug)
    (selectedSuccess : RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected
      snapshot composition = .ok selected)
    (unique : SelectedRouteContextProof.CompositionKeysUnique composition)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (subject : SnapshotAssemblyProof.Subject)
    (permissions : alloc.collections.btree.set.BTreeSet
      veoveo_types.permission_names.PermissionName alloc.alloc.Global) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get selected slug =
      .ok ((SelectedRouteContextProof.resolveRoute snapshot composition.components slug).map Prod.snd) ∧
    RequestCompositionHub.auth.verified.AuthenticatedSubject.request_context subject =
      .ok (SnapshotAssemblyProof.expectedContext subject) ∧
    ∀ key row context,
      SelectedRouteContextProof.resolveRoute snapshot composition.components slug = some (key, row) →
      SnapshotAssemblyProof.expectedContext subject = .Ok context →
      RequestCompositionHub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
        http issuer profile row subject permissions =
        .ok (.Ok { (SnapshotAssemblyProof.expectedClient
          http issuer profile row subject permissions context) with
            server := slug, deployment := key }) := by
  exact SelectedRouteContextProof.selected_route_context_exact snapshot composition selected slug
    selectedSuccess unique (new_success_coherent ops source initial model cursor opened valid snapshot constructed)
    http issuer profile subject permissions

end Constructor
end
end SnapshotConstructorProof

#print axioms SnapshotConstructorProof.insert_coherent
#print axioms SnapshotConstructorProof.collect_coherent
#print axioms SnapshotConstructorProof.new_mapper_exact
#print axioms SnapshotConstructorProof.constructor_cursor_law
#print axioms SnapshotConstructorProof.new_success_coherent
#print axioms SnapshotConstructorProof.constructed_route_context_exact
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SnapshotConstructorProof.insert_coherent,
      ``SnapshotConstructorProof.collect_coherent,
      ``SnapshotConstructorProof.new_mapper_exact,
      ``SnapshotConstructorProof.constructor_cursor_law] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed constructor primitive axiom in {declaration}: {found}"
  let withNamespaceBoundary := standard.push
    ``RequestCompositionHub.registration.snapshot.RegistrationSnapshot.build_namespaces
  let coherence ← Lean.collectAxioms ``SnapshotConstructorProof.new_success_coherent
  unless (coherence.filter (fun axiomName => !withNamespaceBoundary.contains axiomName)).isEmpty do
    throwError "unreviewed constructor boundary: {coherence}"
  let withFormatting := withNamespaceBoundary ++ #[
    ``RequestCompositionHub.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionHub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt]
  let connected ← Lean.collectAxioms ``SnapshotConstructorProof.constructed_route_context_exact
  unless (connected.filter (fun axiomName => !withFormatting.contains axiomName)).isEmpty do
    throwError "unreviewed constructed-route/context boundary: {connected}"
