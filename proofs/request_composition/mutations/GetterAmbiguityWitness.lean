import RequestCompositionHub.Funs
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std RequestCompositionHub

-- The following three independent specification definitions are copied exactly
-- from committed SelectedRegistrationProof SHA8dd2e35b...1542f128. The failed
-- theorem module and any old artifact of it are deliberately NOT imported.
namespace SelectedRegistrationProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p
abbrev Server := RequestCompositionHub.registration.snapshot.RegisteredServer
abbrev Snapshot := RequestCompositionHub.registration.snapshot.RegistrationSnapshot
abbrev Slug := veoveo_types.platform_names.ServerSlug
abbrev entries (snapshot : Snapshot) : List Server := snapshot.servers.map Prod.snd
def matching (snapshot : Snapshot) (slug : Slug) : List Server :=
  (entries snapshot).filter (fun server => decide (server.slug = slug))

def onlyMatch : List Server → Option Server
  | [server] => some server
  | _ => none

end
end SelectedRegistrationProof

namespace GetterAmbiguityWitness
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p
open SelectedRegistrationProof
abbrev Namespaces := alloc.collections.btree.map.BTreeMap
  veoveo_types.platform_names.ServerSlug RequestCompositionHub.registration.snapshot.Namespace alloc.alloc.Global

def twoRows (first second : Server) (namespaces : Namespaces) : Snapshot := {
  servers := [(first.deployment, first), (second.deployment, second)], namespaces }

/-- Two whole rows, distinct native map keys and exact key/deployment coherence;
no duplicate-key list or fabricated registration-result input is used. -/
theorem valid_row_shape (first second : Server) (namespaces : Namespaces)
    (distinct : first.deployment ≠ second.deployment) :
    ((twoRows first second namespaces).servers.map Prod.fst).Nodup ∧
      ∀ key row, (key, row) ∈ (twoRows first second namespaces).servers → key = row.deployment := by
  constructor
  · simp [twoRows, distinct]
  · intro key row member
    simp only [twoRows, List.mem_cons, List.mem_singleton, List.not_mem_nil, or_false, Prod.mk.injEq] at member
    rcases member with ⟨rfl, rfl⟩ | ⟨rfl, rfl⟩ <;> rfl

/-- Concrete source-mutant result in this model enumeration order. Every other
registration field and all namespace contents remain arbitrary complete values. -/
theorem mutant_returns_first (first second : Server) (namespaces : Namespaces) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
      (twoRows first second namespaces) first.slug = .ok (some first) := by
  unfold RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
  simp only [twoRows, RequestCompositionHub.alloc.collections.btree.map.BTreeMap.values,
    RequestCompositionHub.core.iter.traits.iterator.Iterator.filter.default,
    RequestCompositionHub.core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator.next,
    List.map_cons, List.map_nil, bind_ok, bind_tc_ok]
  -- One recurrence expansion only: do not place this recursive definition
  -- in simp's rule set, which would also expand the unreachable skip branch.
  unfold RequestCompositionHub.CurrentRolesNative.filterNext
  simp [
    RequestCompositionHub.alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV,
    RequestCompositionHub.alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV.next,
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3ArcRegisteredServerBool,
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3ArcRegisteredServerBool.call_mut,
    RequestCompositionHub.alloc.sync.Arc.Insts.CoreOpsDerefDeref.deref,
    RequestCompositionHub.veoveo_types.platform_names.ServerSlug.Insts.CoreCmpPartialEqServerSlug.eq,
    RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitTry.branch,
    RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
    RequestCompositionHub.alloc.sync.Arc.Insts.CoreConvertAsRef.as_ref,
    RequestCompositionHub.core.bool.Bool.then_some, bind_ok, bind_tc_ok]

theorem singleton_spec_refuses (first second : Server) (namespaces : Namespaces)
    (same : second.slug = first.slug) :
    onlyMatch (matching (twoRows first second namespaces) first.slug) = none := by
  simp [onlyMatch, matching, entries, twoRows, same]

/-- Either two-key enumeration order is a semantic counterexample. Native
BTreeMap/Arc iteration refinement remains the same explicit model boundary;
no assertion about a particular native ordering is added here. Outer Result
success is established, rather than assuming away failure or divergence. -/
theorem both_orders_violate_singleton_contract
    (first second : Server) (namespaces : Namespaces)
    (same : second.slug = first.slug) (distinct : first.deployment ≠ second.deployment) :
    ((twoRows first second namespaces).servers.map Prod.fst).Nodup ∧
    (∀ key row, (key, row) ∈ (twoRows first second namespaces).servers → key = row.deployment) ∧
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
      (twoRows first second namespaces) first.slug = .ok (some first) ∧
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
      (twoRows second first namespaces) second.slug = .ok (some second) ∧
    onlyMatch (matching (twoRows first second namespaces) first.slug) = none ∧
    onlyMatch (matching (twoRows second first namespaces) second.slug) = none ∧
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
      (twoRows first second namespaces) first.slug ≠
        .ok (onlyMatch (matching (twoRows first second namespaces) first.slug)) ∧
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
      (twoRows second first namespaces) second.slug ≠
        .ok (onlyMatch (matching (twoRows second first namespaces) second.slug)) := by
  have left := mutant_returns_first first second namespaces
  have right := mutant_returns_first second first namespaces
  have refused := singleton_spec_refuses first second namespaces same
  have reversed := singleton_spec_refuses second first namespaces same.symm
  rcases valid_row_shape first second namespaces distinct with ⟨unique, coherent⟩
  refine ⟨unique, coherent, left, right, refused, reversed, ?_, ?_⟩
  · rw [left, refused]
    simp
  · rw [right, reversed]
    simp

end
end GetterAmbiguityWitness

#print axioms GetterAmbiguityWitness.valid_row_shape
#print axioms GetterAmbiguityWitness.mutant_returns_first
#print axioms GetterAmbiguityWitness.singleton_spec_refuses
#print axioms GetterAmbiguityWitness.both_orders_violate_singleton_contract
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``GetterAmbiguityWitness.valid_row_shape,
      ``GetterAmbiguityWitness.mutant_returns_first,
      ``GetterAmbiguityWitness.singleton_spec_refuses,
      ``GetterAmbiguityWitness.both_orders_violate_singleton_contract] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unexpected ambiguity witness axiom in {declaration}: {found}"
