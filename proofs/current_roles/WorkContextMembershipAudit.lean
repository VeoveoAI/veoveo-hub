import WorkContextMembershipProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

/- Actual membership calculation plus ordinary external operational definitions.
Only standard kernel axioms are allowed; native stdlib/ID representation and
current catalog/authority/browser correspondence remain explicit boundaries. -/

#print axioms MembershipPrimitiveProof.disjointBy_exact
#print axioms MembershipPrimitiveProof.maxLoop_correct
#print axioms MembershipPrimitiveProof.maximum_unique
#print axioms WorkContextMembershipProof.native_level_compare
#print axioms WorkContextMembershipProof.matches_exact
#print axioms WorkContextMembershipProof.membership_for_complete
#print axioms WorkContextMembershipProof.some_iff
#print axioms WorkContextMembershipProof.none_iff
#print axioms WorkContextMembershipProof.permutation_invariant
#print axioms WorkContextMembershipProof.duplicate_rule_invariant
#print axioms WorkContextMembershipProof.two_rule_maximum
#print axioms WorkContextMembershipProof.empty_selector_absent

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[
      ``CurrentRolesNative.disjointBy,
      ``CurrentRolesNative.maxLoop,
      ``alloc.collections.btree.set.BTreeSet.is_disjoint,
      ``core.iter.traits.iterator.Iterator.max.default,
      ``alloc.collections.btree.map.Iter.Insts.CoreIterTraitsIteratorIteratorPairSharedAKSharedAV.max,
      ``alloc.collections.btree.set.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT.max,
      ``veoveo_types.identity.PrincipalId.Insts.CoreCmpOrd.cmp,
      ``veoveo_types.identity.PrincipalId.Insts.CoreCmpPartialOrdPrincipalId.partial_cmp,
      ``veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpOrd.cmp,
      ``veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpPartialOrdOAuthClientId.partial_cmp,
      ``control_plane.work_context.WorkContextMembershipRule.matches,
      ``control_plane.work_context.WorkContextDefinition.membership_for,
      ``veoveo_types.authority.WorkContextMembershipLevel.Insts.CoreCmpOrd.cmp,
      ``MembershipPrimitiveProof.disjointBy_exact,
      ``MembershipPrimitiveProof.set_disjoint_exact,
      ``MembershipPrimitiveProof.maximum_singleton_seed,
      ``MembershipPrimitiveProof.maximum_add_dominated,
      ``MembershipPrimitiveProof.maxLoop_correct,
      ``MembershipPrimitiveProof.iterator_max_correct,
      ``MembershipPrimitiveProof.maximum_unique,
      ``MembershipPrimitiveProof.maximum_membership_invariant,
      ``WorkContextMembershipProof.rank_injective,
      ``WorkContextMembershipProof.native_level_compare,
      ``WorkContextMembershipProof.principal_client_comparisons,
      ``WorkContextMembershipProof.matches_exact,
      ``WorkContextMembershipProof.selector_closure_exact,
      ``WorkContextMembershipProof.level_closure_exact,
      ``WorkContextMembershipProof.selector_law,
      ``WorkContextMembershipProof.level_law,
      ``WorkContextMembershipProof.initial_valid,
      ``WorkContextMembershipProof.level_cursor_law,
      ``WorkContextMembershipProof.result_spec_iff_maximum,
      ``WorkContextMembershipProof.membership_for_correct,
      ``WorkContextMembershipProof.result_spec_unique,
      ``WorkContextMembershipProof.membership_for_complete,
      ``WorkContextMembershipProof.some_iff,
      ``WorkContextMembershipProof.none_iff,
      ``WorkContextMembershipProof.raw_rule_membership_invariant,
      ``WorkContextMembershipProof.permutation_invariant,
      ``WorkContextMembershipProof.client_selector_accepts,
      ``WorkContextMembershipProof.empty_rules_absent,
      ``WorkContextMembershipProof.duplicate_rule_invariant,
      ``WorkContextMembershipProof.empty_selector_absent,
      ``WorkContextMembershipProof.two_rule_maximum,
      ``MembershipPrimitiveProof.Maximum,
      ``WorkContextMembershipProof.Matches,
      ``WorkContextMembershipProof.rank,
      ``WorkContextMembershipProof.ResultSpec] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed Work Context membership axiom in {declaration}: {found}"
