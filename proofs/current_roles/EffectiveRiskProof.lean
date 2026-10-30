import PolicyPrimitives

open Aeneas Aeneas.Std veoveo_hub IteratorProof CollectionProof NativeSetProof PolicyPrimitives

namespace EffectiveRiskProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Risk := veoveo_hub_sdk.registration.access.Risk
abbrev Name := veoveo_types.permission_names.PermissionName
abbrev Override := control_plane.authorization_config.PermissionOverride

/- Declarative, occurrence-based override semantics: all additions, any denial. -/
def Denied (permissionName : Name) (overrides : List Override) : Prop := .Deny permissionName ∈ overrides

def riskUniverse : Finset Risk := {.Sensitive, .Destructive, .Billed, .Physical, .Administer}

theorem risk_universe_complete (risk : Risk) : risk ∈ riskUniverse := by
  cases risk <;> simp [riskUniverse]

def additions (permissionName : Name) (overrides : List Override) : Finset Risk :=
  riskUniverse.filter fun risk => ∃ added, .AddRisks permissionName added ∈ overrides ∧ risk ∈ added

theorem additions_membership (permissionName : Name) (overrides : List Override) (risk : Risk) :
    risk ∈ additions permissionName overrides ↔ ∃ added, .AddRisks permissionName added ∈ overrides ∧ risk ∈ added := by
  simp [additions, risk_universe_complete]

def expected (permissionName : Name) (overrides : List Override) (initial : Finset Risk) : Option (Finset Risk) :=
  if Denied permissionName overrides then none else some (initial ∪ additions permissionName overrides)

theorem expected_nil (permissionName : Name) (initial : Finset Risk) : expected permissionName [] initial = some initial := by
  simp [expected, Denied, additions]

theorem additions_add (permissionName other : Name) (added : Finset Risk) (tail : List Override) :
    additions permissionName (.AddRisks other added :: tail) =
      (if other = permissionName then added else ∅) ∪ additions permissionName tail := by
  ext risk
  by_cases same : other = permissionName
  · subst other
    simp only [if_pos rfl, Finset.mem_union, additions_membership, List.mem_cons,
      control_plane.authorization_config.PermissionOverride.AddRisks.injEq, true_and]
    aesop
  · simp [additions_membership, same, Ne.symm same]

theorem additions_deny (permissionName other : Name) (tail : List Override) :
    additions permissionName (.Deny other :: tail) = additions permissionName tail := by
  ext risk
  simp [additions_membership]

theorem expected_add (permissionName other : Name) (added initial : Finset Risk) (tail : List Override) :
    expected permissionName (.AddRisks other added :: tail) initial =
      if other = permissionName then expected permissionName tail (initial ∪ added) else expected permissionName tail initial := by
  by_cases same : other = permissionName <;>
    by_cases denied : control_plane.authorization_config.PermissionOverride.Deny permissionName ∈ tail <;>
    simp [expected, Denied, additions_add, same, denied, Finset.union_assoc]

theorem expected_deny (permissionName other : Name) (initial : Finset Risk) (tail : List Override) :
    expected permissionName (.Deny other :: tail) initial =
      if other = permissionName then none else expected permissionName tail initial := by
  by_cases same : other = permissionName
  · subst other
    simp [expected, Denied]
  · by_cases denied : control_plane.authorization_config.PermissionOverride.Deny permissionName ∈ tail <;>
      simp [expected, Denied, additions_deny, same, Ne.symm same, denied]

/- The generated loop is total on every finite raw suffix, even for configurations
with duplicate permission names or contradictory overrides. No validator premise. -/
theorem effective_loop_correct (cursor : Aeneas.Std.core.slice.iter.Iter Override)
    (permissionName : Name) (initial : Finset Risk) :
    policy.grants.effective_risks_loop cursor permissionName initial ⦃ result =>
      result = expected permissionName ((sliceEnumeration Override).remaining cursor) initial ⦄ := by
  unfold policy.grants.effective_risks_loop
  let target := expected permissionName ((sliceEnumeration Override).remaining cursor) initial
  refine loop.spec_decr_nat
    (fun state => ((sliceEnumeration Override).remaining state.1).length)
    (fun state => expected permissionName ((sliceEnumeration Override).remaining state.1) state.2 = target)
    (fun result => result = target) _ _ ?_ rfl
  rintro ⟨current, risks⟩ invariant
  dsimp only
  unfold policy.grants.effective_risks_loop.body
  step with (slice_cursor_law Override current trivial) as ⟨entry, next, nextSpec⟩
  rcases nextSpec with ⟨_, entryEq, suffix⟩
  cases remaining : (sliceEnumeration Override).remaining current with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    have done : some risks = target := by simpa [remaining, expected_nil] using invariant
    simpa using done
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    cases head with
    | AddRisks other added =>
      simp only [control_plane.authorization_config.PermissionOverride.permission, bind_ok,
        core.cmp.impls.PartialEqShared.ne, core.cmp.PartialEq.ne.trait_default,
        core.cmp.PartialEq.ne.default,
        veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialEqPermissionName,
        veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialEqPermissionName.eq]
      by_cases same : other = permissionName
      · simp [same]
        have orderLaw : LawfulComparison veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpOrd.cmp riskComparison :=
          fun left right => (risk_operations_exact left right).1
        rw [set_extend_exact veoveo_hub_sdk.registration.access.Risk.Insts.CoreCmpOrd
          veoveo_hub_sdk.registration.access.Risk.Insts.CoreMarkerCopy
          alloc.alloc.Global.Insts.CoreAllocAllocatorClone riskComparison orderLaw risks added]
        simp only [bind_ok, WP.spec_ok]
        constructor
        · have preserved : expected permissionName tail (risks ∪ added) = target := by
            simpa [remaining, expected_add, same] using invariant
          rw [suffix]
          convert preserved using 1
          all_goals first
            | rfl
            | exact Subsingleton.elim _ _
            | (congr 1; ext risk; simp)
        · simp [remaining, suffix]
      · simp [same]
        constructor
        · have preserved : expected permissionName tail risks = target := by
            simpa [remaining, expected_add, same] using invariant
          simpa only [suffix] using preserved
        · simp [remaining, suffix]
    | Deny other =>
      simp only [control_plane.authorization_config.PermissionOverride.permission, bind_ok,
        core.cmp.impls.PartialEqShared.ne, core.cmp.PartialEq.ne.trait_default,
        core.cmp.PartialEq.ne.default,
        veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialEqPermissionName,
        veoveo_types.permission_names.PermissionName.Insts.CoreCmpPartialEqPermissionName.eq]
      by_cases same : other = permissionName
      · have done : none = target := by simpa [remaining, expected_deny, same] using invariant
        simpa [same] using done
      · simp [same]
        constructor
        · have preserved : expected permissionName tail risks = target := by
            simpa [remaining, expected_deny, same] using invariant
          simpa only [suffix] using preserved
        · simp [remaining, suffix]

theorem effective_risks_exact (installation : control_plane.authorization_config.InstallationAuthorization)
    (permission : veoveo_hub_sdk.registration.authorization.PermissionDeclaration) :
    policy.grants.effective_risks installation permission =
      .ok (expected permission.permission installation.overrides.val permission.risks) := by
  have cloned := set_clone_exact veoveo_hub_sdk.registration.access.Risk.Insts.CoreCloneClone
    (by intro risk; rfl) alloc.alloc.Global.Insts.CoreAllocAllocatorClone permission.risks
  unfold policy.grants.effective_risks
  rw [cloned]
  simp only [SharedAVec.Insts.CoreIterTraitsCollectIntoIteratorSharedATIter.into_iter,
    Aeneas.Std.core.slice.Slice.iter, bind_ok]
  apply exact_of_total
  simpa [sliceEnumeration, Aeneas.Std.alloc.vec.Vec.deref] using
    effective_loop_correct { slice := Aeneas.Std.alloc.vec.Vec.deref installation.overrides, i := 0 }
      permission.permission permission.risks

theorem effective_risks_denied_iff
    (installation : control_plane.authorization_config.InstallationAuthorization)
    (permission : veoveo_hub_sdk.registration.authorization.PermissionDeclaration) :
    policy.grants.effective_risks installation permission = .ok none ↔
      Denied permission.permission installation.overrides.val := by
  rw [effective_risks_exact]
  by_cases denied : Denied permission.permission installation.overrides.val <;>
    simp [expected, denied]

theorem effective_risks_present_iff
    (installation : control_plane.authorization_config.InstallationAuthorization)
    (permission : veoveo_hub_sdk.registration.authorization.PermissionDeclaration) (risks : Finset Risk) :
    policy.grants.effective_risks installation permission = .ok (some risks) ↔
      ¬ Denied permission.permission installation.overrides.val ∧
        risks = permission.risks ∪ additions permission.permission installation.overrides.val := by
  rw [effective_risks_exact]
  by_cases denied : Denied permission.permission installation.overrides.val <;>
    simp [expected, denied, eq_comm]

theorem effective_risks_membership
    (installation : control_plane.authorization_config.InstallationAuthorization)
    (permission : veoveo_hub_sdk.registration.authorization.PermissionDeclaration) :
    policy.grants.effective_risks installation permission ⦃ result =>
      (result = none ↔ Denied permission.permission installation.overrides.val) ∧
      ∀ risks, result = some risks ↔ ¬ Denied permission.permission installation.overrides.val ∧
        (∀ risk, risk ∈ risks ↔ risk ∈ permission.risks ∨
          ∃ added, .AddRisks permission.permission added ∈ installation.overrides.val ∧ risk ∈ added) ⦄ := by
  rw [effective_risks_exact]
  by_cases denied : Denied permission.permission installation.overrides.val
  · simp [expected, denied]
  · simp only [expected, denied, if_false, WP.spec_ok, reduceCtorEq, false_iff, not_false_eq_true,
      true_and]
    intro risks
    constructor
    · intro same
      cases Option.some.inj same
      intro risk
      simp [additions_membership]
    · intro membership
      congr 1
      ext risk
      simpa [additions_membership] using (membership risk).symm

#print axioms risk_universe_complete
#print axioms additions_membership
#print axioms expected_add
#print axioms expected_deny
#print axioms effective_loop_correct
#print axioms effective_risks_exact
#print axioms effective_risks_denied_iff
#print axioms effective_risks_present_iff
#print axioms effective_risks_membership
end
end EffectiveRiskProof
