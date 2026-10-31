import HubDiscoveryCacheState.Funs

open Aeneas Aeneas.Std HubDiscoveryCacheModel

namespace CacheLinkOrderFaultWitness
noncomputable section

abbrev Key := mcp.discovery.DiscoveryCacheKey

/-- Concrete represented keys; native parsing/admission is tested separately. -/
def first : Key := {
  catalog_generation := 1#u64
  principal := "principal"
  authorization_fingerprint := Array.from (List.replicate 32 (0#u8)) (by simp)
  server := "linked-fixture"
  link := some "first"
}

def second : Key := { first with link := some "second" }

theorem keys_distinct : first ≠ second := by
  intro same
  have links := congrArg (fun key : Key => key.link) same
  simp [first, second] at links

private theorem bytes_reflexive (values : List U8) :
    NativeMap.lexCompare (liftFun2 core.cmp.impls.OrdU8.cmp) values values = .ok .eq := by
  induction values with
  | nil => rfl
  | cons head rest recursive =>
    have element : (liftFun2 core.cmp.impls.OrdU8.cmp head head : Result Ordering) =
        .ok .eq := by
      change (Result.ok (compare head.val head.val) : Result Ordering) = .ok .eq
      simp
    rw [NativeMap.lexCompare, element]
    simpa only [bind_tc_eq, bind_ok] using recursive

/-- This uses the actual mutated function without importing any failed proof.
All prefix values are equal, but the present links remain different. -/
theorem bad_comparison (key : Key) :
    mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd.cmp
      { key with link := some "first" } { key with link := some "second" } = .ok .eq := by
  have generation : (lift (core.cmp.impls.OrdU64.cmp key.catalog_generation
      key.catalog_generation) : Result Ordering) = .ok .eq := by
    change (Result.ok (compare key.catalog_generation.val key.catalog_generation.val) :
      Result Ordering) = .ok .eq
    simp
  have principal_order : compare key.principal key.principal = Ordering.eq :=
    (_root_.compare_eq_iff_eq).mpr rfl
  have server_order : compare key.server key.server = Ordering.eq :=
    (_root_.compare_eq_iff_eq).mpr rfl
  unfold mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd.cmp
  simp only [veoveo_types.identity.PrincipalId.Insts.CoreCmpOrd.cmp,
    veoveo_types.platform_names.ServerSlug.Insts.CoreCmpOrd.cmp,
    alloc.string.String.Insts.CoreCmpOrd.cmp, Array.Insts.CoreCmpOrd.cmp]
  rw [generation]
  simp only [bind_tc_eq, bind_ok, principal_order, server_order]
  rw [bytes_reflexive]
  simp only [bind_tc_eq, bind_ok, principal_order, server_order]
  simp only [core.option.Option.Insts.CoreCmpOrd.cmp,
    veoveo_hub_sdk.hub.tool_name.LinkName.Insts.CoreCmpOrd,
    veoveo_hub_sdk.hub.tool_name.LinkName.Insts.CoreCmpOrd.cmp,
    alloc.string.String.Insts.CoreCmpOrd.cmp]
  cases compared : compare ("first" : String) "second" <;>
    simp [compared, bind_tc_eq, bind_ok]

theorem concrete_alias : first ≠ second ∧
    mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd.cmp first second = .ok .eq := by
  refine ⟨keys_distinct, ?_⟩
  simpa only [first, second] using bad_comparison first

end
end CacheLinkOrderFaultWitness

#print axioms CacheLinkOrderFaultWitness.keys_distinct
#print axioms CacheLinkOrderFaultWitness.bad_comparison
#print axioms CacheLinkOrderFaultWitness.concrete_alias

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheLinkOrderFaultWitness.keys_distinct,
      ``CacheLinkOrderFaultWitness.bad_comparison,
      ``CacheLinkOrderFaultWitness.concrete_alias] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected independent bad-comparison witness axiom in {declaration}: {dependencies}"
