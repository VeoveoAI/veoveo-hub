import MembershipPrimitiveProof
import CollectionProof

open Aeneas Aeneas.Std veoveo_hub NativeSetProof IteratorProof
  MembershipPrimitiveProof

namespace WorkContextMembershipProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Principal := veoveo_hub_sdk.identity.Principal
abbrev Client := veoveo_types.platform_names.OAuthClientId
abbrev Rule := control_plane.work_context.WorkContextMembershipRule
abbrev Context := control_plane.work_context.WorkContextDefinition
abbrev Level := veoveo_types.authority.WorkContextMembershipLevel

/-- Independent OR of the four raw selector relations. Group roles, tenant,
scopes and other Principal fields do not occur in this calculation. The outer
catalog/browser admission checks those distinct domains separately. -/
def Matches (rule : Rule) (principal : Principal) (client : Client) : Prop :=
  principal.id ∈ rule.principals ∨
    ¬Disjoint rule.groups principal.groups ∨
    ¬Disjoint rule.roles principal.roles ∨ client ∈ rule.oauth_clients

def rank : Level → Nat
  | .Viewer => 0
  | .Contributor => 1
  | .Custodian => 2
  | .Owner => 3

/-- Declarative result over the ORIGINAL raw rules. Some must identify an
actually matching rule at the returned level AND dominate every matching rule.
No requested maximum or selected-list correctness is a theorem premise. -/
def ResultSpec (context : Context) (principal : Principal) (client : Client) :
    Option Level → Prop
  | none => ∀ rule ∈ context.memberships.val, ¬Matches rule principal client
  | some level =>
    (∃ rule ∈ context.memberships.val, Matches rule principal client ∧ rule.level = level) ∧
      ∀ rule ∈ context.memberships.val,
        Matches rule principal client → rank rule.level ≤ rank level

theorem rank_injective : Function.Injective rank := by
  intro left right same
  cases left <;> cases right <;> simp_all [rank]

theorem native_level_compare (left right : Level) :
    veoveo_types.authority.WorkContextMembershipLevel.Insts.CoreCmpOrd.cmp left right =
      .ok (compare (rank left) (rank right)) := by
  cases left <;> cases right <;>
    simp [veoveo_types.authority.WorkContextMembershipLevel.Insts.CoreCmpOrd.cmp, rank]
  all_goals
    simp only [Aeneas.Std.Discriminant.read_discriminant,
      veoveo_types.authority.WorkContextMembershipLevel.read_discriminant,
      IScalar.bv_toInt_eq, IScalar.ofInt_val_eq]
  all_goals decide +kernel

theorem principal_client_comparisons
    (left right : veoveo_types.identity.PrincipalId) (client other : Client) :
    veoveo_types.identity.PrincipalId.Insts.CoreCmpOrd.cmp left right =
      .ok ((taggedComparison 6).compare left right) ∧
    veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpOrd.cmp client other =
      .ok ((taggedComparison 32).compare client other) := by
  exact ⟨rfl, rfl⟩

theorem matches_exact (rule : Rule) (principal : Principal) (client : Client) :
    control_plane.work_context.WorkContextMembershipRule.matches rule principal client =
      .ok (decide (Matches rule principal client)) := by
  have principalLaw : LawfulComparison
      veoveo_types.identity.PrincipalId.Insts.CoreCmpOrd.cmp (taggedComparison 6) := by
    intro left right
    rfl
  have clientLaw : LawfulComparison
      veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpOrd.cmp (taggedComparison 32) := by
    intro left right
    rfl
  have groupLaw : LawfulComparison
      veoveo_types.identity.GroupId.Insts.CoreCmpOrd.cmp (taggedComparison 0) := by
    intro left right
    exact (group_operations_exact left right).1
  have roleLaw : LawfulComparison
      veoveo_types.identity.RoleId.Insts.CoreCmpOrd.cmp (taggedComparison 4) := by
    intro left right
    exact (role_operations_exact left right).1
  unfold control_plane.work_context.WorkContextMembershipRule.matches
  rw [set_contains_exact _ _ id (by intro value; rfl) _ _
    (taggedComparison 6) principalLaw]
  simp only [bind_ok]
  by_cases direct : principal.id ∈ rule.principals
  · simp [direct, Matches]
  · rw [set_disjoint_exact _ _ (taggedComparison 0) groupLaw]
    simp only [bind_ok]
    by_cases group : Disjoint rule.groups principal.groups
    · simp only [group, decide_true, ↓reduceIte]
      rw [set_disjoint_exact _ _ (taggedComparison 4) roleLaw]
      simp only [bind_ok]
      by_cases role : Disjoint rule.roles principal.roles
      · simp only [role, decide_true, ↓reduceIte]
        rw [set_contains_exact _ _ id (by intro value; rfl) _ _
          (taggedComparison 32) clientLaw]
        simp [Matches, direct, group, role]
      · simp [Matches, direct, group, role]
    · simp [Matches, direct, group]

abbrev selectorOps :=
  control_plane.work_context.WorkContextDefinition.membership_for.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3WorkContextMembershipRuleBool
abbrev levelOps :=
  control_plane.work_context.WorkContextDefinition.membership_for.closure_1.Insts.CoreOpsFunctionFnMutTupleShared0WorkContextMembershipRuleWorkContextMembershipLevel

theorem selector_closure_exact (principal : Principal) (client : Client) (rule : Rule) :
    selectorOps.call_mut (principal, client) rule =
      .ok (decide (Matches rule principal client), (principal, client)) := by
  simp [selectorOps,
    control_plane.work_context.WorkContextDefinition.membership_for.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3WorkContextMembershipRuleBool.call_mut,
    matches_exact]

theorem level_closure_exact (state : Unit) (rule : Rule) :
    levelOps.call_mut state rule = .ok (rule.level, ()) := by
  cases state
  rfl

theorem selector_law (principal : Principal) (client : Client) :
    PredicateLaw selectorOps (fun state => state = (principal, client))
      (fun rule => decide (Matches rule principal client)) := by
  intro state same rule
  subst state
  rw [selector_closure_exact]
  simp only [WP.spec_ok]
  exact ⟨trivial, rfl⟩

theorem level_law : MapperLaw levelOps (fun _ => True) (fun rule => rule.level) := by
  intro state _ rule
  rw [level_closure_exact]
  simp

abbrev rawIterator := Aeneas.Std.core.iter.traits.iterator.IteratorSliceIter Rule
abbrev selectedIterator :=
  core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator rawIterator selectorOps
abbrev levelIterator :=
  core.iter.adapters.map.Map.Insts.CoreIterTraitsIteratorIterator selectedIterator levelOps

def selectedEnumeration (principal : Principal) (client : Client) :=
  filterEnumeration (sliceEnumeration Rule) (fun state => state = (principal, client))
    (fun rule => decide (Matches rule principal client))

def levelEnumeration (principal : Principal) (client : Client) :=
  mapEnumeration (selectedEnumeration principal client) (fun (_ : Unit) => True)
    (fun rule => rule.level)

def initialState (context : Context) (principal : Principal) (client : Client) :
    Aeneas.Std.core.iter.adapters.map.Map
      (core.iter.adapters.filter.Filter
        (Aeneas.Std.core.slice.iter.Iter Rule) (Principal × Client)) Unit :=
  { iter := { iter := { slice := Aeneas.Std.alloc.vec.Vec.deref context.memberships, i := 0 },
              predicate := (principal, client) }, f := () }

theorem initial_valid (context : Context) (principal : Principal) (client : Client) :
    (levelEnumeration principal client).valid (initialState context principal client) := by
  exact ⟨⟨trivial, rfl⟩, trivial⟩

theorem level_cursor_law (principal : Principal) (client : Client) :
    CursorLaw levelIterator (levelEnumeration principal client) := by
  have selected := filter_cursor_law rawIterator (sliceEnumeration Rule)
    (slice_cursor_law Rule) selectorOps (fun state => state = (principal, client))
    _ (selector_law principal client)
  exact map_cursor_law selectedIterator (selectedEnumeration principal client)
    selected levelOps (fun _ => True) _ level_law

private def matchingLevels (context : Context) (principal : Principal) (client : Client) :=
  (context.memberships.val.filter (fun rule => decide (Matches rule principal client))).map
    (fun rule => rule.level)

theorem result_spec_iff_maximum (context : Context) (principal : Principal) (client : Client)
    (result : Option Level) :
    ResultSpec context principal client result ↔
      Maximum rank (matchingLevels context principal client) result := by
  cases result <;>
    simp [ResultSpec, Maximum, matchingLevels, List.mem_map, List.mem_filter,
      List.eq_nil_iff_forall_not_mem] <;> aesop

theorem membership_for_correct (context : Context) (principal : Principal) (client : Client) :
    control_plane.work_context.WorkContextDefinition.membership_for context principal client
      ⦃ result => ResultSpec context principal client result ⦄ := by
  have maximum := iterator_max_correct levelIterator (levelEnumeration principal client)
    (level_cursor_law principal client)
    veoveo_types.authority.WorkContextMembershipLevel.Insts.CoreCmpOrd.cmp rank
    native_level_compare
    veoveo_types.authority.WorkContextMembershipLevel.Insts.CoreCmpOrd rfl
    (initialState context principal client) (initial_valid context principal client)
  unfold control_plane.work_context.WorkContextDefinition.membership_for
  simp only [Aeneas.Std.core.slice.Slice.iter,
    core.iter.traits.iterator.Iterator.filter.default,
    core.iter.traits.iterator.Iterator.map.default, bind_ok]
  change core.iter.traits.iterator.Iterator.max.default levelIterator _
    (initialState context principal client) ⦃ result => _ ⦄
  apply WP.spec_mono maximum
  intro result specification
  apply (result_spec_iff_maximum context principal client result).mpr
  simpa [levelEnumeration, selectedEnumeration, mapEnumeration, filterEnumeration,
    sliceEnumeration, initialState, matchingLevels, Aeneas.Std.alloc.vec.Vec.deref] using specification

theorem result_spec_unique (context : Context) (principal : Principal) (client : Client)
    {left right : Option Level}
    (leftSpec : ResultSpec context principal client left)
    (rightSpec : ResultSpec context principal client right) : left = right := by
  exact maximum_unique rank rank_injective (matchingLevels context principal client)
    ((result_spec_iff_maximum context principal client left).mp leftSpec)
    ((result_spec_iff_maximum context principal client right).mp rightSpec)

theorem membership_for_complete (context : Context) (principal : Principal) (client : Client) :
    ∃ result,
      control_plane.work_context.WorkContextDefinition.membership_for context principal client =
        .ok result ∧ ResultSpec context principal client result := by
  have total := membership_for_correct context principal client
  cases actual : control_plane.work_context.WorkContextDefinition.membership_for
      context principal client using Result.cases with
  | ret result =>
    rw [actual] at total
    exact ⟨result, rfl, (WP.spec_ok result).mp total⟩
  | vis effect continuation =>
    rw [actual] at total
    exact False.elim ((WP.spec_vis effect continuation).mp total)
  | div =>
    rw [actual] at total
    exact False.elim (WP.spec_div.mp total)

theorem some_iff (context : Context) (principal : Principal) (client : Client) (level : Level) :
    control_plane.work_context.WorkContextDefinition.membership_for context principal client =
      .ok (some level) ↔
      (∃ rule ∈ context.memberships.val, Matches rule principal client ∧ rule.level = level) ∧
        ∀ rule ∈ context.memberships.val,
          Matches rule principal client → rank rule.level ≤ rank level := by
  constructor
  · intro actual
    have total := membership_for_correct context principal client
    rw [actual] at total
    exact (WP.spec_ok (some level)).mp total
  · intro wanted
    obtain ⟨result, actual, specification⟩ := membership_for_complete context principal client
    have same : result = some level := result_spec_unique context principal client specification wanted
    simpa [same] using actual

theorem none_iff (context : Context) (principal : Principal) (client : Client) :
    control_plane.work_context.WorkContextDefinition.membership_for context principal client =
      .ok none ↔ ∀ rule ∈ context.memberships.val, ¬Matches rule principal client := by
  constructor
  · intro actual
    have total := membership_for_correct context principal client
    rw [actual] at total
    exact (WP.spec_ok (none : Option Level)).mp total
  · intro wanted
    obtain ⟨result, actual, specification⟩ := membership_for_complete context principal client
    have same : result = none := result_spec_unique context principal client specification wanted
    simpa [same] using actual

theorem raw_rule_membership_invariant (left right : Context)
    (principal : Principal) (client : Client)
    (sameRules : ∀ rule, rule ∈ left.memberships.val ↔ rule ∈ right.memberships.val) :
    control_plane.work_context.WorkContextDefinition.membership_for left principal client =
      control_plane.work_context.WorkContextDefinition.membership_for right principal client := by
  obtain ⟨leftResult, leftActual, leftSpec⟩ := membership_for_complete left principal client
  obtain ⟨rightResult, rightActual, rightSpec⟩ := membership_for_complete right principal client
  have transported : ResultSpec left principal client rightResult := by
    cases rightResult <;> simpa [ResultSpec, sameRules] using rightSpec
  have same := result_spec_unique left principal client leftSpec transported
  rw [leftActual, rightActual, same]

theorem permutation_invariant (left right : Context) (principal : Principal) (client : Client)
    (permuted : left.memberships.val.Perm right.memberships.val) :
    control_plane.work_context.WorkContextDefinition.membership_for left principal client =
      control_plane.work_context.WorkContextDefinition.membership_for right principal client := by
  exact raw_rule_membership_invariant left right principal client (fun rule => permuted.mem_iff)

/-- A real client-only rule is sufficient. The full Principal/context fields
remain actual inputs; this is not an assumed membership/admission premise. -/
def clientRule (level : Level) (client : Client) : Rule where
  level := level
  principals := ∅
  groups := ∅
  roles := ∅
  oauth_clients := {client}

theorem client_selector_accepts (context : Context) (principal : Principal)
    (client : Client) (level : Level) :
    control_plane.work_context.WorkContextDefinition.membership_for
      { context with
        memberships := (Aeneas.Std.alloc.vec.Vec.from [clientRule level client]
          (by change 1 ≤ Usize.max; scalar_tac)) }
      principal client = .ok (some level) := by
  apply (some_iff _ _ _ _).mpr
  change
    (∃ rule ∈ [clientRule level client], Matches rule principal client ∧ rule.level = level) ∧
      ∀ rule ∈ [clientRule level client],
        Matches rule principal client → rank rule.level ≤ rank level
  simp [Matches, clientRule]

theorem empty_rules_absent (context : Context) (principal : Principal) (client : Client) :
    control_plane.work_context.WorkContextDefinition.membership_for
      { context with memberships := Aeneas.Std.alloc.vec.Vec.from [] (by decide) }
      principal client = .ok none := by
  apply (none_iff _ _ _).mpr
  change ∀ rule ∈ ([] : List Rule), ¬Matches rule principal client
  simp

/-- Duplicating an already-present raw rule changes no selector/maximum result. -/
theorem duplicate_rule_invariant (before after : Context) (principal : Principal)
    (client : Client) (rule : Rule) (present : rule ∈ before.memberships.val)
    (added : after.memberships.val = rule :: before.memberships.val) :
    control_plane.work_context.WorkContextDefinition.membership_for before principal client =
      control_plane.work_context.WorkContextDefinition.membership_for after principal client := by
  apply raw_rule_membership_invariant
  intro candidate
  rw [added]
  constructor
  · exact List.mem_cons_of_mem rule
  · intro member
    rcases List.mem_cons.mp member with same | old
    · simpa [same] using present
    · exact old

/-- Even a nonempty raw rule list can legitimately yield None: empty selectors
are not an implicit default grant. -/
theorem empty_selector_absent (context : Context) (principal : Principal)
    (client : Client) (level : Level) :
    control_plane.work_context.WorkContextDefinition.membership_for
      { context with
        memberships := (Aeneas.Std.alloc.vec.Vec.from
          ([{ level := level, principals := ∅, groups := ∅, roles := ∅,
              oauth_clients := ∅ }] : List Rule)
          (by change 1 ≤ Usize.max; scalar_tac)) }
      principal client = .ok none := by
  apply (none_iff _ _ _).mpr
  change ∀ (rule : Rule), rule ∈
    [({ level := level, principals := ∅, groups := ∅, roles := ∅, oauth_clients := ∅ } : Rule)] →
      ¬Matches rule principal client
  simp [Matches]

/-- Two actual matching inputs require the higher exact level, not merely a
non-escalating level or blanket refusal. Full context/Principal remain inputs. -/
theorem two_rule_maximum (context : Context) (principal : Principal) (client : Client) :
    control_plane.work_context.WorkContextDefinition.membership_for
      { context with
        memberships := (Aeneas.Std.alloc.vec.Vec.from
          [clientRule .Viewer client, clientRule .Custodian client]
          (by change 2 ≤ Usize.max; scalar_tac)) }
      principal client = .ok (some .Custodian) := by
  apply (some_iff _ _ _ _).mpr
  change
    (∃ rule ∈ [clientRule .Viewer client, clientRule .Custodian client],
      Matches rule principal client ∧ rule.level = .Custodian) ∧
      ∀ rule ∈ [clientRule .Viewer client, clientRule .Custodian client],
        Matches rule principal client → rank rule.level ≤ rank .Custodian
  simp [Matches, clientRule, rank]

end
end WorkContextMembershipProof
