import SessionFamilyProof
import RefreshModelAudit
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

namespace RefreshPredicateProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate
abbrev Family := store.refresh.RefreshFamily
abbrev Token := store.refresh.RefreshToken
abbrev Binding := store.refresh.RefreshBinding
abbrev Timestamp := SessionFamilyProof.Timestamp

/-- Independent binding observations; no expiry/revocation admission is inferred. -/
def Bound (family : Family) (binding : Binding) : Prop :=
  family.authorization_server = binding.authorization_server ∧
  family.protected_resource = binding.protected_resource ∧
  family.oauth_client_id = binding.oauth_client_id

/-- Exact checked scalar operation from the pinned Aeneas primitive library.
Its relation to unbounded generation arithmetic is proved separately below. -/
def NextGeneration (token successor : Token) : Prop :=
  some successor.generation = U64.checked_add token.generation 1#u64

/-- Full duplicate-delivery predicate, independent of store branch selection,
replacement pointer lookup, raw SQL projection, or the cipher's interpretation. -/
def Deliverable (family : Family) (token successor : Token) (now : Timestamp) : Prop :=
  token.consumed_at.isSome = true ∧
  token.replay_detected_at.isNone = true ∧
  family.revoked_at.isNone = true ∧
  SessionFamilyProof.Before now family.expires_at ∧
  successor.family = family.id ∧
  NextGeneration token successor ∧
  successor.generation = family.current_generation ∧
  SessionFamilyProof.Before now successor.expires_at ∧
  successor.consumed_at.isNone = true ∧
  successor.replay_detected_at.isNone = true ∧
  ∃ envelope, successor.delivery = some envelope ∧
    SessionFamilyProof.Before now envelope.expires_at

theorem bound_to_exact (family : Family) (binding : Binding) :
    store.refresh.RefreshFamily.bound_to family binding = .ok (decide (Bound family binding)) := by
  unfold store.refresh.RefreshFamily.bound_to
  simp only [contract.identity.AuthorizationServerId.Insts.CoreCmpPartialEqAuthorizationServerId.eq,
    veoveo_hub_sdk.identity.ProtectedResourceId.Insts.CoreCmpPartialEqProtectedResourceId.eq,
    veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpPartialEqOAuthClientId.eq,
    alloc.string.String.Insts.CoreCmpPartialEqString.eq, bind_ok, bind_tc_ok,
    SessionFamilyProof.guard_ok]
  apply congrArg Result.ok
  apply Bool.eq_iff_iff.mpr
  simp only [Bool.and_eq_true, decide_eq_true_eq]
  rfl

theorem u64_option_eq_exact (left right : Option U64) :
    core.option.Option.Insts.CoreCmpPartialEqOption.eq core.cmp.PartialEqU64 left right =
      .ok (decide (left = right)) := by
  cases left <;> cases right <;>
    simp [core.option.Option.Insts.CoreCmpPartialEqOption.eq,
      core.cmp.PartialEqU64, liftFun2, bind_ok, bind_tc_ok]

theorem envelope_window_exact (delivery : Option store.refresh.RefreshDeliveryEnvelope) (now : Timestamp) :
    (do
      let borrowed ← core.option.Option.as_ref delivery
      core.option.Option.is_some_and
        store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool
        borrowed now) =
      .ok (decide (∃ envelope, delivery = some envelope ∧
        SessionFamilyProof.Before now envelope.expires_at)) := by
  cases delivery <;>
    simp [core.option.Option.as_ref, core.option.Option.is_some_and,
      store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool,
      store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool.call_once,
      SessionFamilyProof.utc_gt_exact, bind_ok, bind_tc_ok]

theorem guard_prop_ok (condition : Prop) [Decidable condition] (result : Bool) :
    (if condition then Result.ok result else Result.ok false) =
      Result.ok (decide condition && result) := by
  by_cases holds : condition <;> simp [holds]

theorem deliverable_exact (family : Family) (token successor : Token) (now : Timestamp) :
    store.refresh.deliverable family token successor now =
      .ok (decide (Deliverable family token successor now)) := by
  unfold store.refresh.deliverable
  simp only [SessionFamilyProof.utc_gt_exact,
    veoveo_hub_sdk.identity.HubRefreshFamilyId.Insts.CoreCmpPartialEqHubRefreshFamilyId.eq,
    lift, bind_ok, u64_option_eq_exact]
  have presence (α : Type) (value : Option α) : core.option.Option.is_some value = value.isSome := by
    cases value <;> rfl
  have absence (α : Type) (value : Option α) : core.option.Option.is_none value = value.isNone := by
    cases value <;> rfl
  simp only [presence, absence]
  cases delivery : successor.delivery <;>
    simp only [delivery, core.option.Option.as_ref, bind_ok,
      core.option.Option.is_some_and,
      store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool,
      store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool.call_once,
      SessionFamilyProof.utc_gt_exact, guard_prop_ok]
  all_goals
    apply congrArg Result.ok
    apply Bool.eq_iff_iff.mpr
    simp [Bool.and_eq_true, decide_eq_true_eq, Deliverable, NextGeneration, delivery]

theorem deliverable_iff (family : Family) (token successor : Token) (now : Timestamp) :
    store.refresh.deliverable family token successor now = .ok true ↔
      Deliverable family token successor now := by
  rw [deliverable_exact]
  by_cases valid : Deliverable family token successor now <;> simp [valid]

theorem refuses_iff (family : Family) (token successor : Token) (now : Timestamp) :
    store.refresh.deliverable family token successor now = .ok false ↔
      ¬ Deliverable family token successor now := by
  rw [deliverable_exact]
  by_cases valid : Deliverable family token successor now <;> simp [valid]

/-- The arithmetic condition implies the unbounded successor equation; overflow
cannot masquerade as a successor. This uses the pinned checked-add theorem. -/
theorem next_generation_arithmetic (token successor : Token)
    (next : NextGeneration token successor) :
    successor.generation.val = token.generation.val + 1 := by
  have specification := U64.checked_add_bv_spec token.generation 1#u64
  have selected : U64.checked_add token.generation 1#u64 = some successor.generation := next.symm
  rw [selected] at specification
  simpa using specification.2.1

/-- Store-to-session mathematical projection. Actual from_stored/Serde/SQL/JWT
wiring remains a caller/native correspondence boundary, not an extraction here. -/
def sessionAuthority (family : Family) : policy.session.SessionFamilyAuthority where
  authorization_server := family.authorization_server
  protected_resource := family.protected_resource
  oauth_client_id := family.oauth_client_id
  work_context := family.work_context
  principal_id := family.principal.principal.id
  tenant := family.principal.principal.tenant
  scopes := family.scopes
  principal := { principal := family.principal.principal }
  current_generation := family.current_generation
  expires_at := family.expires_at
  revoked_at := family.revoked_at

theorem rotation_preserves_session_admission (family : Family)
    (request : SessionFamilyProof.Request) (generation : U64) :
    policy.session.SessionFamilyAuthority.allows
      (sessionAuthority { family with current_generation := generation }) request =
    policy.session.SessionFamilyAuthority.allows (sessionAuthority family) request := by
  exact SessionFamilyProof.generation_invariant (sessionAuthority family) request generation

/-- A compatible delivery window with arbitrary unused token IDs, digest,
principal/display metadata and envelope payload. No crypto meaning is assumed. -/
theorem coherent_delivery_accepts (family : Family) (token successor : Token)
    (envelope : store.refresh.RefreshDeliveryEnvelope) (now : Timestamp)
    (next : some successor.generation = U64.checked_add token.generation 1#u64)
    (current : successor.generation = family.current_generation)
    (notRevoked : family.revoked_at = none)
    (freshFamily : SessionFamilyProof.Before now family.expires_at)
    (freshDelivery : SessionFamilyProof.Before now envelope.expires_at) :
    store.refresh.deliverable family
      { token with consumed_at := some now, replay_detected_at := none }
      { successor with
        family := family.id
        expires_at := family.expires_at
        consumed_at := none
        replay_detected_at := none
        delivery := some envelope } now = .ok true := by
  apply (deliverable_iff _ _ _ _).mpr
  have nextCurrent : some family.current_generation = U64.checked_add token.generation 1#u64 := by
    simpa only [current] using next
  simp [Deliverable, NextGeneration, next, current, nextCurrent, notRevoked, freshFamily, freshDelivery]

theorem delivery_deadline_refuses (family : Family) (token successor : Token)
    (envelope : store.refresh.RefreshDeliveryEnvelope) (now : Timestamp)
    (stored : successor.delivery = some envelope) (deadline : envelope.expires_at = now) :
    store.refresh.deliverable family token successor now = .ok false := by
  apply (refuses_iff _ _ _ _).mpr
  simp [Deliverable, stored, deadline, SessionFamilyProof.Before]

end
end RefreshPredicateProof

#print axioms RefreshPredicateProof.bound_to_exact
#print axioms RefreshPredicateProof.deliverable_exact
#print axioms RefreshPredicateProof.deliverable_iff
#print axioms RefreshPredicateProof.refuses_iff

#print axioms RefreshPredicateProof.next_generation_arithmetic
#print axioms RefreshPredicateProof.rotation_preserves_session_admission
#print axioms RefreshPredicateProof.coherent_delivery_accepts
#print axioms RefreshPredicateProof.delivery_deadline_refuses

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshPredicateProof.bound_to_exact,
      ``RefreshPredicateProof.deliverable_exact, ``RefreshPredicateProof.deliverable_iff,
      ``RefreshPredicateProof.refuses_iff, ``RefreshPredicateProof.next_generation_arithmetic,
      ``RefreshPredicateProof.rotation_preserves_session_admission,
      ``RefreshPredicateProof.coherent_delivery_accepts, ``RefreshPredicateProof.delivery_deadline_refuses] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh predicate axiom in {declaration}: {found}"
