import RequestCompositionHub.Funs
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std RequestCompositionHub

-- Independent specification definitions copied byte-for-byte from committed
-- SelectedRegistrationProof SHA01751908...fee209. Do not import that failed
-- theorem module or any artifact of it into this mutant witness.
namespace SelectedRegistrationProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p
abbrev Server := RequestCompositionHub.registration.snapshot.RegisteredServer
abbrev Snapshot := RequestCompositionHub.registration.snapshot.RegistrationSnapshot
abbrev Slug := veoveo_types.platform_names.ServerSlug
abbrev Deployment := veoveo_types.execution.DeploymentId

def lookupBy : List (Deployment × Server) → Deployment → Option Server
  | [], _ => none
  | (key, server) :: rest, query =>
    if key = query then some server else lookupBy rest query

def chooseOne (snapshot : Snapshot) (entry : Slug × veoveo_hub_sdk.execution.ResolvedComponent) :
    Option (Deployment × Server) :=
  match entry.2 with
  | .Unavailable => none
  | .Deployment deployment =>
    match lookupBy snapshot.servers deployment with
    | none => none
    | some server => if server.slug = entry.1 then some (deployment, server) else none

end
end SelectedRegistrationProof

namespace SelectedEntrySlugWitness
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p
open SelectedRegistrationProof
abbrev Namespaces := alloc.collections.btree.map.BTreeMap
  Slug RequestCompositionHub.registration.snapshot.Namespace alloc.alloc.Global

def oneRow (row : Server) (namespaces : Namespaces) : Snapshot := {
  servers := [(row.deployment, row)], namespaces }

abbrev selectOps := RequestCompositionHub.registration.snapshot.RegistrationSnapshot.selected.closure.Insts.CoreOpsFunctionFnMutTuplePairShared1ServerSlugShared2ResolvedComponentOptionPairDeploymentIdArcRegisteredServer
abbrev actualLookup : List (Deployment × Server) → Deployment → Result (Option Server) :=
  RequestCompositionHub.alloc.collections.btree.map.BTreeMap.get
    RequestCompositionHub.alloc.alloc.Global.Insts.CoreAllocAllocatorClone
    (RequestCompositionHub.core.borrow.Borrow.Blanket Deployment)
    RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd
    RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd

/-- The exact source-mutant body, with its pair-destructuring/clone callback
exposed under a tuple type. Kernel conversion must check this equality against
actual re-extracted code before any semantic conclusion is accepted. -/
private theorem mutant_entry_body (snapshot : Snapshot) (slug : Slug)
    (deployment : Deployment) :
    selectOps.call_mut snapshot (slug, .Deployment deployment) = (do
      let candidate ← actualLookup snapshot.servers deployment
      let branch ← RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitTry.branch candidate
      match branch with
      | .Continue server =>
        let picked ← Aeneas.Std.core.bool.Bool.then
          ({ call_once := fun pair (_ : Unit) => do
              let (key, value) := pair
              let cloned ← RequestCompositionHub.alloc.sync.Arc.Insts.CoreCloneClone.clone
                RequestCompositionHub.alloc.alloc.Global.Insts.CoreAllocAllocatorClone value
              .ok (key, cloned) } :
            Aeneas.Std.core.ops.function.FnOnce (Deployment × Server) Unit (Deployment × Server))
          true (deployment, server)
        .ok (picked, snapshot)
      | .Break residual =>
        let picked ← RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual
          (Deployment × Server) residual
        .ok (picked, snapshot)) := by
  with_unfolding_all rfl

/-- A complete arbitrary row keyed by its own deployment is a valid finite
native-map shape; no fabricated or duplicate-key route result is supplied. -/
theorem valid_row_shape (row : Server) (namespaces : Namespaces) :
    ((oneRow row namespaces).servers.map Prod.fst).Nodup ∧
      ∀ key stored, (key, stored) ∈ (oneRow row namespaces).servers → key = stored.deployment := by
  constructor
  · simp [oneRow]
  · intro key stored member
    simp only [oneRow, List.mem_singleton, Prod.mk.injEq] at member
    rcases member with ⟨rfl, rfl⟩
    rfl

/-- Actual outer Result success, exact full keyed row and unchanged closure
state, for any requested slug. Namespaces and all other row fields are arbitrary. -/
theorem mutant_accepts_row (row : Server) (slug : Slug) (namespaces : Namespaces) :
    selectOps.call_mut (oneRow row namespaces) (slug, .Deployment row.deployment) =
      .ok (some (row.deployment, row), oneRow row namespaces) := by
  rw [mutant_entry_body]
  simp [oneRow, actualLookup,
    RequestCompositionHub.alloc.collections.btree.map.BTreeMap.get,
    RequestCompositionHub.CurrentRolesSDK.mapGetBy,
    RequestCompositionHub.core.borrow.Borrow.Blanket,
    RequestCompositionHub.core.borrow.Borrow.Blanket.borrow,
    RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd,
    RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd.cmp,
    RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitTry.branch,
    Aeneas.Std.core.bool.Bool.then,
    RequestCompositionHub.alloc.sync.Arc.Insts.CoreCloneClone.clone,
    bind_ok, bind_tc_ok]

theorem independent_spec_refuses (row : Server) (slug : Slug) (namespaces : Namespaces)
    (wrong : row.slug ≠ slug) :
    chooseOne (oneRow row namespaces) (slug, .Deployment row.deployment) = none := by
  simp [chooseOne, lookupBy, oneRow, wrong]

/-- A same-key, wrong-slug source bypass violates the unchanged independent
entry specification while preserving the complete row/state and outer success.
Native BTreeMap/Arc refinement remains the existing explicit model boundary;
there is no collection-order or namespace-helper-completion premise. -/
theorem wrong_slug_violates_entry_contract
    (row : Server) (slug : Slug) (namespaces : Namespaces) (wrong : row.slug ≠ slug) :
    ((oneRow row namespaces).servers.map Prod.fst).Nodup ∧
    (∀ key stored, (key, stored) ∈ (oneRow row namespaces).servers → key = stored.deployment) ∧
    selectOps.call_mut (oneRow row namespaces) (slug, .Deployment row.deployment) =
      .ok (some (row.deployment, row), oneRow row namespaces) ∧
    chooseOne (oneRow row namespaces) (slug, .Deployment row.deployment) = none ∧
    selectOps.call_mut (oneRow row namespaces) (slug, .Deployment row.deployment) ≠
      .ok (chooseOne (oneRow row namespaces) (slug, .Deployment row.deployment),
        oneRow row namespaces) := by
  rcases valid_row_shape row namespaces with ⟨unique, coherent⟩
  have accepted := mutant_accepts_row row slug namespaces
  have refused := independent_spec_refuses row slug namespaces wrong
  refine ⟨unique, coherent, accepted, refused, ?_⟩
  rw [accepted, refused]
  simp

end
end SelectedEntrySlugWitness

#print axioms SelectedEntrySlugWitness.valid_row_shape
#print axioms SelectedEntrySlugWitness.mutant_accepts_row
#print axioms SelectedEntrySlugWitness.independent_spec_refuses
#print axioms SelectedEntrySlugWitness.wrong_slug_violates_entry_contract
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SelectedEntrySlugWitness.valid_row_shape,
      ``SelectedEntrySlugWitness.mutant_accepts_row,
      ``SelectedEntrySlugWitness.independent_spec_refuses,
      ``SelectedEntrySlugWitness.wrong_slug_violates_entry_contract] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unexpected selected-entry witness axiom in {declaration}: {found}"
