import SdkInternalAuthBridgeProof
import InternalAuthIssueExamples

open Aeneas Aeneas.Std RequestCompositionSdk
open InternalAuthSpecification InternalAuthContextExamples InternalAuthIdentityExamples
open InternalAuthContextProof InternalAuthIdentityProof
open InternalAuthClaimsExamples InternalAuthIssueExamples

namespace SdkInternalAuthCallerExamples
noncomputable section
open SdkInternalAuthBridge

private theorem timestamp_reencoded (value : NativeTimestamp) :
    timestamp? (hubTimestamp value) = some value := by
  simpa only [sdkTimestamp_roundtrip] using
    (timestamp_checked (hubTimestamp value)).1 (hubTimestamp_physical value)

private theorem optional_reencoded {H S : Type} (decode : S → H) (encode : H → Option S)
    (law : ∀ value, encode (decode value) = some value) (value : Option S) :
    SdkInternalAuthBridge.optional encode (value.map decode) = some value := by
  cases value <;> simp only [Option.map, SdkInternalAuthBridge.optional, law, Option.map_some]

private theorem principal_reencoded (value : identity.Principal) (canonical : PrincipalCanonical value) :
    principalToSdk (principalToHub value) = some value := by
  simp only [principalToSdk, principalToHub,
    optional_reencoded hubTimestamp timestamp? timestamp_reencoded,
    Option.bind_some, Equiv.apply_symm_apply, Option.map_map, Function.comp_def,
    Option.map_id_fun', ordered_roundtrip _ _ canonical.groups.1 canonical.groups.2,
    ordered_roundtrip _ _ canonical.group_roles.1 canonical.group_roles.2,
    ordered_roundtrip _ _ canonical.roles.1 canonical.roles.2,
    ordered_roundtrip _ _ canonical.scopes.1 canonical.scopes.2,
    ordered_roundtrip _ _ canonical.data_labels.1 canonical.data_labels.2,
    ordered_roundtrip _ _ canonical.assurances.1 canonical.assurances.2]
  with_unfolding_all rfl

private theorem authority_reencoded (value : veoveo_types.authority.InvocationAuthority)
    (canonical : SetCanonical value.output_policy.data_labels) :
    authorityToSdk (authorityToHub value) = value := by
  simp only [authorityToSdk, authorityToHub, outputPolicyToSdk, outputPolicyToHub,
    Equiv.apply_symm_apply, Option.map_map, Function.comp_def, Option.map_id_fun',
    ordered_roundtrip _ _ canonical.1 canonical.2]
  with_unfolding_all rfl

private theorem token_reencoded (value : identity.AccessTokenSubject)
    (canonical : SetCanonical value.scopes) :
    accessTokenToSdk (accessTokenToHub value) = some value := by
  simp only [accessTokenToSdk, accessTokenToHub, timestamp_reencoded,
    optional_reencoded hubTimestamp timestamp? timestamp_reencoded,
    Option.bind_some, Equiv.apply_symm_apply, Option.map_map, Function.comp_def,
    Option.map_id_fun', ordered_roundtrip _ _ canonical.1 canonical.2]
  with_unfolding_all rfl

private theorem context_reencoded (value : internal_auth.request.HubRequestContext)
    (canonical : ContextCanonical value) : contextToSdk (contextToHub value) = some value := by
  simp only [contextToSdk, contextToHub, token_reencoded _ canonical.token_scopes,
    principal_reencoded _ canonical.principal, Option.bind_some, Equiv.apply_symm_apply]
  with_unfolding_all rfl

private theorem delegation_reencoded (value : internal_auth.exchange.DelegationContext) :
    delegationToSdk (delegationToHub value) = some value := by
  simp only [delegationToSdk, delegationToHub, timestamp_reencoded, Option.bind_some,
    Equiv.apply_symm_apply, Option.map_map, Function.comp_def, Option.map_id_fun']
  with_unfolding_all rfl

/-- Re-encoding used by the concrete caller fixtures, only for canonical typed
sets. Arbitrary SDK list order/duplicates are not silently discarded. -/
theorem request_reencoded (value : internal_auth.ForwardedRequest)
    (canonical : RequestCanonical value) : requestToSdk (requestToHub value) = some value := by
  have context : SdkInternalAuthBridge.optional contextToSdk (value.request_context.map contextToHub) = some value.request_context := by
    cases present : value.request_context with
    | none => rfl
    | some entry =>
      simp only [Option.map, SdkInternalAuthBridge.optional]
      rw [context_reencoded entry (canonical.context entry (by simp [present]))]
  simp only [requestToSdk, requestToHub, principal_reencoded _ canonical.actor, context,
    optional_reencoded delegationToHub delegationToSdk delegation_reencoded,
    timestamp_reencoded, Option.bind_some, Equiv.apply_symm_apply,
    authority_reencoded _ canonical.output_labels,
    ordered_roundtrip _ _ canonical.permissions.1 canonical.permissions.2,
    Option.map_map, Function.comp_def, Option.map_id_fun']
  with_unfolding_all rfl

def positiveCap : NativeTimestamp := {
  seconds := IScalar.ofInt 1788220810
  nanoseconds := UScalar.ofNat 123456789 }

def positiveRequested : NativeTimestamp := {
  seconds := IScalar.ofInt 1788220820
  nanoseconds := UScalar.ofNat 987654321 }

def boundaryCap : NativeTimestamp := seedIssuedAt
def boundaryRequested : NativeTimestamp := {
  seconds := IScalar.ofInt 1788220800
  nanoseconds := UScalar.ofNat 1 }

/-- Populated admitted delegation shape: current actor plus inherited alpha and
its matching originating Task; original source expiry is old, inherited expiry
is future. These typed fields assert no authentication or native Task liveness. -/
def sdkRequest (deadline : NativeTimestamp) : internal_auth.ForwardedRequest := {
  actingRequest ordinaryRequest ⟨"operator.inspect"⟩ firstActor currentActor
    "0199a000-0000-7000-8000-000000000006" with
  permissions := ordinaryRequest.permissions
  expires_at := deadline }

private theorem user_canonical : PrincipalCanonical (userPrincipal (some seedAuthenticatedAt)) := by
  constructor <;> constructor <;> decide +kernel

private theorem service_canonical : PrincipalCanonical (delegatedActor (some seedIssuedAt)) := by
  constructor <;> constructor <;> decide +kernel

theorem request_canonical (deadline : NativeTimestamp) : RequestCanonical (sdkRequest deadline) := by
  refine ⟨service_canonical, ?_, ?_, ?_⟩
  · constructor <;>
      (dsimp only [sdkRequest, actingRequest, ordinaryRequest, emptyDirectIdentity,
        richIdentity, delegatedAuthority, inputAuthority, seedAuthority]; decide +kernel)
  · constructor <;>
      (dsimp only [sdkRequest, ordinaryRequest, richIdentity]; decide +kernel)
  · intro context member
    have same : delegatedContext (sourceWithExpiry seedAuthenticatedAt) (some seedAuthenticatedAt) = context := by
      simpa only [sdkRequest, actingRequest, Option.mem_def, Option.some.injEq] using member
    subst context
    exact ⟨user_canonical.scopes, user_canonical⟩

/-- Synthetic transport payload is carried but never executed by this pure
issuer caller. The complete issuer/key is an arbitrary typed input. -/
def client (issuer : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer) (cap : NativeTimestamp) :
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient := {
  http := ⟨"synthetic transport fixture; no network execution"⟩
  issuer := issuer
  profile := (requestToHub (sdkRequest positiveRequested)).profile
  server := (requestToHub (sdkRequest positiveRequested)).server
  deployment := (requestToHub (sdkRequest positiveRequested)).deployment
  audience := (requestToHub (sdkRequest positiveRequested)).audience
  client := (requestToHub (sdkRequest positiveRequested)).client
  actor := (requestToHub (sdkRequest positiveRequested)).actor
  authority := (requestToHub (sdkRequest positiveRequested)).authority
  permissions := (requestToHub (sdkRequest positiveRequested)).permissions
  request_context := contextToHub
    (delegatedContext (sourceWithExpiry seedAuthenticatedAt) (some seedAuthenticatedAt))
  acting_server := (requestToHub (sdkRequest positiveRequested)).acting_server
  acting_task := none
  delegation_context := (requestToHub (sdkRequest positiveRequested)).delegation_context
  delegation_expires_at := some (hubTimestamp cap) }

theorem converted_request (deadline : NativeTimestamp) :
    requestToSdk (requestToHub (sdkRequest deadline)) = some (sdkRequest deadline) :=
  request_reencoded _ (request_canonical deadline)

private theorem short_text (text : String) (small : text.toByteArray.size ≤ 64) :
    TextRepresentable text := by
  unfold TextRepresentable
  rcases Usize.bounds_eq with width | width
  · rw [width, U32.max_eq]; omega
  · rw [width, U64.max_eq]; omega

theorem request_domain (deadline : NativeTimestamp) :
    InternalAuthIssueSpecification.IssueTextDomain (sdkRequest deadline) := by
  refine ⟨⟨ordinary_domain.permission_text.server, ordinary_domain.permission_text.permissions⟩,
    ?_, ?_, ordinary_domain.audience,
    ordinary_domain.scopes, ordinary_domain.rendered_scopes⟩
  · intro context present
    change some (delegatedContext (sourceWithExpiry seedAuthenticatedAt)
      (some seedAuthenticatedAt)) = some context at present
    cases Option.some.inj present
    constructor <;> apply short_text <;>
      dsimp only [sdkRequest, actingRequest, delegatedContext, sourceWithExpiry,
        inputContext, delegatedActor, userPrincipal, seedContext] <;> decide +kernel
  · apply short_text
    dsimp only [sdkRequest, actingRequest, delegatedActor]
    decide +kernel

/-- Independent ownership, complete context admission and inherited-chain/Task
checks. The expired original source is allowed on this actual acting path. -/
theorem request_constraints (deadline : NativeTimestamp)
    (before : Native.compareTimestamp deadline inheritedDeadline = .lt) :
    InternalAuthIssueSpecification.firstForeign (sdkRequest deadline).permissions.values
      (sdkRequest deadline).server = none ∧
    delegationError (sdkRequest deadline) = none ∧
    requestContextAccepted (sdkRequest deadline) ∧
    effectiveDeadline (sdkRequest deadline) = deadline := by
  refine ⟨ordinary_constraints.1, ?_, ?_, ?_⟩
  · simp [delegationError, sdkRequest, actingRequest, inheritedContext,
      alloc.vec.Vec.from_val, delegationDeadline, InternalAuthSpecification.earlier,
      before, Option.any]
  · change ContextAccepted (delegatedContext (sourceWithExpiry seedAuthenticatedAt)
      (some seedAuthenticatedAt)) (delegatedActor (some seedIssuedAt))
      (delegatedAuthority seedAuthority) ∧ _
    exact ⟨delegated_accepted _ _ _ _, rfl⟩
  · simp [effectiveDeadline, delegationDeadline, sdkRequest, actingRequest,
      inheritedContext, InternalAuthSpecification.earlier, before]

/-- The control crosses the strict expiry decision by one nanosecond while
keeping the NumericDate seconds equal. No verifier usability is inferred. -/
theorem deadline_comparisons :
    Native.compareTimestamp positiveCap inheritedDeadline = .lt ∧
    Native.compareTimestamp positiveRequested positiveCap = .gt ∧
    Native.compareTimestamp positiveCap seedIssuedAt = .gt ∧
    Native.compareTimestamp boundaryCap inheritedDeadline = .lt ∧
    Native.compareTimestamp boundaryRequested inheritedDeadline = .lt ∧
    Native.compareTimestamp boundaryRequested boundaryCap = .gt ∧
    Native.compareTimestamp boundaryCap seedIssuedAt = .eq ∧
    Native.compareTimestamp boundaryRequested seedIssuedAt = .gt := by
  decide +kernel

#print axioms request_reencoded
#print axioms request_canonical
#print axioms converted_request
#print axioms request_domain
#print axioms request_constraints
#print axioms deadline_comparisons
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SdkInternalAuthCallerExamples.request_reencoded,
      ``SdkInternalAuthCallerExamples.request_canonical,
      ``SdkInternalAuthCallerExamples.converted_request,
      ``SdkInternalAuthCallerExamples.request_domain,
      ``SdkInternalAuthCallerExamples.request_constraints,
      ``SdkInternalAuthCallerExamples.deadline_comparisons] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed independent caller fixture bridge in {declaration}: {found}"
end
end SdkInternalAuthCallerExamples
