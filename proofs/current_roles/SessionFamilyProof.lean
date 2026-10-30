import NativeSetProof
import SessionModelAudit
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub NativeSetProof

namespace SessionFamilyProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Family := policy.session.SessionFamilyAuthority
abbrev Request := policy.session.SessionRequest
abbrev Timestamp := chrono.datetime.DateTime chrono.offset.utc.Utc chrono.offset.utc.Utc
abbrev Scope := veoveo_types.names.ScopeName
abbrev Issuer := veoveo_types.identity.TokenIssuer

/-- Full timestamp order, including subsecond precision. Native Chrono projection
and order preservation remain a separately stated refinement boundary. -/
def Before (left right : Timestamp) : Prop :=
  left.1 < right.1 ∨ left.1 = right.1 ∧ left.2 < right.2

/-- Independent field conditions of the actual pure method. Presence is the only
family-ID observation. The retained original issuer and generation are absent. -/
def Admitted (family : Family) (request : Request) : Prop :=
  request.token.session_family.isSome = true ∧
  family.revoked_at.isNone = true ∧
  Before request.now family.expires_at ∧
  family.principal_id = request.principal.id ∧
  family.tenant = request.principal.tenant ∧
  family.authorization_server = request.authorization_server ∧
  family.protected_resource = request.protected_resource ∧
  family.oauth_client_id = request.token.oauth_client_id ∧
  family.work_context = request.token.work_context ∧
  family.principal.principal.id = request.principal.id ∧
  family.principal.principal.kind = request.principal.kind ∧
  request.token.issuer = request.principal.issuer ∧
  request.token.subject = request.principal.subject ∧
  family.principal.principal.subject = request.principal.subject ∧
  family.principal.principal.tenant = request.principal.tenant ∧
  request.token.scopes ⊆ family.scopes

/-- Raw observations, not an admission Boolean or pre-authorized oracle. -/
def familyObserved (family : Family) :=
  (family.revoked_at.isNone, family.expires_at, family.principal_id, family.tenant,
   family.authorization_server, family.protected_resource, family.oauth_client_id,
   family.work_context, family.principal.principal.id, family.principal.principal.kind,
   family.principal.principal.subject, family.principal.principal.tenant, family.scopes)

def requestObserved (request : Request) :=
  (request.token.session_family.isSome, request.now, request.principal.id,
   request.principal.tenant, request.authorization_server, request.protected_resource,
   request.token.oauth_client_id, request.token.work_context, request.principal.kind,
   request.token.issuer, request.principal.issuer, request.token.subject,
   request.principal.subject, request.token.scopes)

abbrev utcOrder := chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime
  chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc
  chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc

theorem timestamp_compare_gt_iff (left right : Timestamp) :
    SessionNative.timestampCompare left right = .gt ↔ Before right left := by
  cases compared : compare left.1 right.1 with
  | lt =>
    have less := compare_lt_iff_lt.mp compared
    simp only [SessionNative.timestampCompare, compared]
    constructor
    · intro impossible
      cases impossible
    · intro before
      rcases before with before | ⟨same, _⟩ <;> omega
  | eq =>
    have same := _root_.compare_eq_iff_eq.mp compared
    simp [SessionNative.timestampCompare, compared, Before, same, compare_gt_iff_gt]
  | gt =>
    have greater := compare_gt_iff_gt.mp compared
    simp [SessionNative.timestampCompare, compared, Before, greater]

theorem utc_gt_exact (left right : Timestamp) :
    core.cmp.PartialOrd.gt.trait_default utcOrder left right =
      .ok (decide (Before right left)) := by
  rw [← propext (timestamp_compare_gt_iff left right)]
  simp only [core.cmp.PartialOrd.gt.trait_default, core.cmp.PartialOrd.gt.default,
    core.cmp.PartialOrd.gt_body, utcOrder,
    chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime,
    chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp]
  cases compared : SessionNative.timestampCompare left right <;>
    simp [compared, bind_ok, bind_tc_ok]

theorem tenant_equality_exact (left right : Option veoveo_types.identity.TenantId) :
    core.option.Option.Insts.CoreCmpPartialEqOption.eq
      veoveo_types.identity.TenantId.Insts.CoreCmpPartialEqTenantId left right =
      .ok (decide (left = right)) := by
  cases left <;> cases right <;>
    simp [core.option.Option.Insts.CoreCmpPartialEqOption.eq,
      veoveo_types.identity.TenantId.Insts.CoreCmpPartialEqTenantId.eq,
      bind_ok, bind_tc_ok]

theorem kind_equality_exact (left right : veoveo_hub_sdk.identity.PrincipalKind) :
    veoveo_hub_sdk.identity.PrincipalKind.Insts.CoreCmpPartialEqPrincipalKind.eq left right =
      .ok (decide (left = right)) := by
  cases left <;> cases right <;>
    simp [veoveo_hub_sdk.identity.PrincipalKind.Insts.CoreCmpPartialEqPrincipalKind.eq]

theorem scope_subset_exact (left right : Finset Scope) :
    alloc.collections.btree.set.BTreeSet.is_subset
      alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      veoveo_types.names.ScopeName.Insts.CoreCmpOrd left right =
      .ok (decide (left ⊆ right)) := by
  apply (set_subset_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
    veoveo_types.names.ScopeName.Insts.CoreCmpOrd (taggedComparison 1)
    (by intro left right; rfl) left right).trans
  apply congrArg Result.ok
  exact congrArg (@decide (left ⊆ right)) (Subsingleton.elim _ _)

/-- Normalize a short-circuit Boolean guard without expanding record fields. -/
theorem guard_ok (guard result : Bool) :
    (if guard then Result.ok result else Result.ok false) =
      Result.ok (guard && result) := by
  cases guard <;> rfl

theorem allows_exact (family : Family) (request : Request) :
    policy.session.SessionFamilyAuthority.allows family request =
      .ok (decide (Admitted family request)) := by
  unfold policy.session.SessionFamilyAuthority.allows
  simp only [utc_gt_exact, tenant_equality_exact, kind_equality_exact,
    scope_subset_exact,
    veoveo_types.identity.PrincipalId.Insts.CoreCmpPartialEqPrincipalId.eq,
    contract.identity.AuthorizationServerId.Insts.CoreCmpPartialEqAuthorizationServerId.eq,
    alloc.string.String.Insts.CoreCmpPartialEqString.eq,
    veoveo_hub_sdk.identity.ProtectedResourceId.Insts.CoreCmpPartialEqProtectedResourceId.eq,
    veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpPartialEqOAuthClientId.eq,
    veoveo_types.identity.WorkContextId.Insts.CoreCmpPartialEqWorkContextId.eq,
    veoveo_types.identity.TokenIssuer.Insts.CoreCmpPartialEqTokenIssuer.eq,
    veoveo_types.identity.TokenSubject.Insts.CoreCmpPartialEqTokenSubject.eq,
    bind_ok, bind_tc_ok]
  have familyPresence : core.option.Option.is_some request.token.session_family =
      request.token.session_family.isSome := by cases request.token.session_family <;> rfl
  have revocationPresence : core.option.Option.is_none family.revoked_at =
      family.revoked_at.isNone := by cases family.revoked_at <;> rfl
  rw [familyPresence, revocationPresence]
  simp only [guard_ok]
  apply congrArg Result.ok
  apply Bool.eq_iff_iff.mpr
  simp only [Bool.and_eq_true, decide_eq_true_eq]
  rfl

theorem admitted_iff (family : Family) (request : Request) :
    policy.session.SessionFamilyAuthority.allows family request = .ok true ↔
      Admitted family request := by
  rw [allows_exact]
  by_cases admitted : Admitted family request <;> simp [admitted]

theorem refused_iff (family : Family) (request : Request) :
    policy.session.SessionFamilyAuthority.allows family request = .ok false ↔
      ¬ Admitted family request := by
  rw [allows_exact]
  by_cases admitted : Admitted family request <;> simp [admitted]

theorem observed_admission_iff (family otherFamily : Family) (request otherRequest : Request)
    (familySame : familyObserved family = familyObserved otherFamily)
    (requestSame : requestObserved request = requestObserved otherRequest) :
    Admitted family request ↔ Admitted otherFamily otherRequest := by
  simp only [familyObserved, Prod.mk.injEq] at familySame
  simp only [requestObserved, Prod.mk.injEq] at requestSame
  simp_all [Admitted]

theorem observed_equivalence (family otherFamily : Family) (request otherRequest : Request)
    (familySame : familyObserved family = familyObserved otherFamily)
    (requestSame : requestObserved request = requestObserved otherRequest) :
    policy.session.SessionFamilyAuthority.allows family request =
      policy.session.SessionFamilyAuthority.allows otherFamily otherRequest := by
  rw [allows_exact, allows_exact,
    propext (observed_admission_iff family otherFamily request otherRequest familySame requestSame)]

theorem generation_invariant (family : Family) (request : Request) (generation : U64) :
    policy.session.SessionFamilyAuthority.allows { family with current_generation := generation } request =
      policy.session.SessionFamilyAuthority.allows family request :=
  observed_equivalence _ _ _ _ rfl rfl

theorem retained_issuer_invariant (family : Family) (request : Request) (issuer : Issuer) :
    policy.session.SessionFamilyAuthority.allows
      { family with principal := { principal := { family.principal.principal with issuer := issuer } } }
      request = policy.session.SessionFamilyAuthority.allows family request :=
  observed_equivalence _ _ _ _ rfl rfl

/-- A raw coherent family for arbitrary valid token/current-principal binding.
The retained issuer and refresh generation remain arbitrary. -/
def coherentFamily (request : Request) (expiry : Timestamp) (scopes : Finset Scope)
    (retainedIssuer : Issuer) (generation : U64) : Family where
  authorization_server := request.authorization_server
  protected_resource := request.protected_resource
  oauth_client_id := request.token.oauth_client_id
  work_context := request.token.work_context
  principal_id := request.principal.id
  tenant := request.principal.tenant
  scopes := scopes
  principal := { principal := { request.principal with issuer := retainedIssuer } }
  current_generation := generation
  expires_at := expiry
  revoked_at := none

theorem coherent_accepts (request : Request) (expiry : Timestamp) (scopes : Finset Scope)
    (retainedIssuer : Issuer) (generation : U64)
    (hasFamily : request.token.session_family.isSome = true)
    (issuer : request.token.issuer = request.principal.issuer)
    (subject : request.token.subject = request.principal.subject)
    (fresh : Before request.now expiry) (scopeBound : request.token.scopes ⊆ scopes) :
    policy.session.SessionFamilyAuthority.allows
      (coherentFamily request expiry scopes retainedIssuer generation) request = .ok true := by
  apply (admitted_iff _ _).mpr
  simp [Admitted, coherentFamily, hasFamily, issuer, subject, fresh, scopeBound]

/-- Concrete same-second strictness at one nanosecond, with every other
admission binding supplied as raw input conditions. -/
theorem coherent_nanosecond_accepts (request : Request) (seconds : Int)
    (scopes : Finset Scope) (retainedIssuer : Issuer) (generation : U64)
    (hasFamily : request.token.session_family.isSome = true)
    (issuer : request.token.issuer = request.principal.issuer)
    (subject : request.token.subject = request.principal.subject)
    (nowAtStart : request.now = (seconds, 0))
    (scopeBound : request.token.scopes ⊆ scopes) :
    policy.session.SessionFamilyAuthority.allows
      (coherentFamily request (seconds, 1) scopes retainedIssuer generation) request = .ok true := by
  apply coherent_accepts request (seconds, 1) scopes retainedIssuer generation
    hasFamily issuer subject _ scopeBound
  rw [nowAtStart]
  exact Or.inr ⟨rfl, by omega⟩

theorem expiry_equality_refuses (family : Family) (request : Request)
    (same : family.expires_at = request.now) :
    policy.session.SessionFamilyAuthority.allows family request = .ok false := by
  apply (refused_iff _ _).mpr
  simp [Admitted, same, Before]

#print axioms allows_exact
#print axioms admitted_iff
#print axioms refused_iff
#print axioms observed_equivalence
#print axioms generation_invariant
#print axioms retained_issuer_invariant
#print axioms coherent_accepts
#print axioms coherent_nanosecond_accepts
#print axioms expiry_equality_refuses

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``allows_exact, ``admitted_iff, ``refused_iff,
      ``observed_equivalence, ``generation_invariant, ``retained_issuer_invariant,
      ``coherent_accepts, ``coherent_nanosecond_accepts, ``expiry_equality_refuses] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed session proof axiom in {declaration}: {found}"

end
end SessionFamilyProof
