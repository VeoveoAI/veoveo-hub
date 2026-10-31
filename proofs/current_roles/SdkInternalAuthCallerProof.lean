import HubCurrentRoles
import NativeSetProof
import SdkInternalAuthBridgeOperationsProof

open Aeneas Aeneas.Std veoveo_hub

namespace SdkInternalAuthCallerProof
noncomputable section
open SdkInternalAuthBridge

abbrev Client := mcp.upstream_authorized_http.HubAuthorizedHttpClient
abbrev CallerError := mcp.upstream_authorized_http.HubAuthorizedHttpError
abbrev CallerResult := core.result.Result String CallerError

/-- Independent full-resolution UTC ordering; no whole-second truncation. -/
def compareTime (left right : HubTime) : Ordering :=
  match compare left.1 right.1 with
  | .eq => compare left.2 right.2
  | ordering => ordering

def earlier (left right : HubTime) : HubTime :=
  if compareTime right left = .lt then right else left

theorem timestamp_compare (left right : HubTime)
    (leftPhysical : TimestampPhysical left) (rightPhysical : TimestampPhysical right) :
    RequestCompositionSdk.Native.compareTimestamp
      (timestamp left leftPhysical) (timestamp right rightPhysical) = compareTime left right := by
  simp [RequestCompositionSdk.Native.compareTimestamp, compareTime, timestamp,
    IScalar.ofInt, UScalar.ofNat]
  with_unfolding_all rfl

/-- The modeled comparison relation itself, independently of native Chrono
refinement (including validity of leap representations). -/
theorem hub_comparison (left right : HubTime) :
    SessionNative.timestampCompare left right = compareTime left right := rfl

theorem earlier_physical (left right : HubTime)
    (leftPhysical : TimestampPhysical left) (rightPhysical : TimestampPhysical right) :
    TimestampPhysical (earlier left right) := by
  unfold earlier
  split <;> assumption

theorem hub_minimum (left right : HubTime) :
    core.cmp.Ord.min.trait_default
      (chrono.datetime.DateTime.Insts.CoreCmpOrd chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc)
      left right = .ok (earlier left right) := by
  simp only [core.cmp.Ord.min.trait_default, core.cmp.Ord.min.default,
    core.cmp.Ord.min_body, core.cmp.PartialOrd.lt_body,
    chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp,
    bind_ok, hub_comparison]
  cases comparison : compareTime right left <;> simp [earlier, comparison]

/-- The SDK scalar conversion preserves the actual minimum, including nanos. -/
theorem sdk_minimum (left right : HubTime)
    (leftPhysical : TimestampPhysical left) (rightPhysical : TimestampPhysical right) :
    (do
      let value ← core.cmp.Ord.min.trait_default
        (RequestCompositionSdk.chrono.datetime.DateTime.Insts.CoreCmpOrd
          RequestCompositionSdk.chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc)
        (timestamp left leftPhysical) (timestamp right rightPhysical)
      Result.ok (hubTimestamp value)) = Result.ok (earlier left right) := by
  simp only [core.cmp.Ord.min.trait_default, core.cmp.Ord.min.default,
    core.cmp.Ord.min_body, core.cmp.PartialOrd.lt_body,
    RequestCompositionSdk.chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp,
    bind_ok, timestamp_compare]
  cases comparison : compareTime right left <;>
    simp [earlier, comparison, timestamp_roundtrip, bind_ok]

def callerDeadline (self : Client) (requested : HubTime) : HubTime :=
  match self.delegation_expires_at with
  | none => requested
  | some cap => earlier cap requested

/-- Every field of the actual caller's request, named independently of its do
expression. Its optional delegation cap precedes the SDK's own deadline rules. -/
def callerRequest (self : Client) (requested : HubTime) : HubRequest := {
  profile := self.profile
  server := self.server
  deployment := self.deployment
  audience := self.audience
  client := self.client
  actor := self.actor
  authority := self.authority
  permissions := self.permissions
  request_context := some self.request_context
  acting_server := self.acting_server
  acting_task := self.acting_task
  delegation_context := self.delegation_context
  expires_at := callerDeadline self requested }

structure CallerPhysical (self : Client) (requested : HubTime) : Prop where
  actor : PrincipalPhysical self.actor
  context : ContextPhysical self.request_context
  delegation : ∀ inherited ∈ self.delegation_context, DelegationPhysical inherited
  requested : TimestampPhysical requested
  cap : ∀ value ∈ self.delegation_expires_at, TimestampPhysical value

theorem caller_physical (self : Client) (requested : HubTime)
    (physical : CallerPhysical self requested) : RequestPhysical (callerRequest self requested) := by
  rcases physical with ⟨actorPhysical, contextPhysical, delegationPhysical, requestedPhysical, capPhysical⟩
  refine ⟨actorPhysical, ?_, delegationPhysical, ?_⟩
  · intro context member
    have same : self.request_context = context := by simpa [callerRequest] using member
    simpa only [same] using contextPhysical
  · change TimestampPhysical (callerDeadline self requested)
    unfold callerDeadline
    cases cap : self.delegation_expires_at with
    | none => exact requestedPhysical
    | some value => exact earlier_physical value requested (capPhysical value (by simp [cap])) requestedPhysical

def bearerResult : HubRustResult → CallerResult
  | .Ok issued => .Ok issued.bearer_token
  | .Err error => .Err (.InternalToken error)

def callerOutcome (issuer : HubIssuer) (request : RequestCompositionSdk.internal_auth.ForwardedRequest) :
    Result CallerResult :=
  Aeneas.Std.bind
    (InternalAuthIssueSpecification.issueOutcome (issuerEquiv issuer) request)
    (fun result => .ok (bearerResult (rustResultToHub result)))

private theorem option_clone {T : Type} (clone : core.clone.Clone T)
    (law : ∀ value, clone.clone value = .ok value) (value : Option T) :
    core.option.Option.Insts.CoreCloneClone.clone clone value = .ok value := by
  cases value <;> simp only [core.option.Option.Insts.CoreCloneClone.clone, law, bind_ok]

private theorem caller_deadline_exact (self : Client) (requested : HubTime) :
    core.option.Option.map_or
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until.closure.Insts.CoreOpsFunctionFnOnceTupleDateTimeUtcUtcDateTimeUtcUtc
      self.delegation_expires_at requested requested = .ok (callerDeadline self requested) := by
  cases cap : self.delegation_expires_at <;>
    simp only [core.option.Option.map_or,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until.closure.Insts.CoreOpsFunctionFnOnceTupleDateTimeUtcUtcDateTimeUtcUtc.call_once,
      hub_minimum, callerDeadline, cap]

/-- Actual caller body, without assuming an issuer or any validator succeeds. -/
theorem caller_assembly (self : Client) (requested : HubTime) :
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until self requested =
      Aeneas.Std.bind (SdkInternalAuthBridge.issue self.issuer (callerRequest self requested))
        (fun result => .ok (bearerResult result)) := by
  have permissions := NativeSetProof.set_clone_exact
    veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone
    (fun _ => rfl) alloc.alloc.Global.Insts.CoreAllocAllocatorClone self.permissions
  have actingClone := option_clone
    veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone (fun _ => rfl) self.acting_server
  have taskClone := option_clone
    veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone (fun _ => rfl) self.acting_task
  have delegationClone := option_clone
    veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone
    (fun _ => rfl) self.delegation_context
  unfold mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
  simp only [veoveo_hub_sdk.identity.HubProfileId.Insts.CoreCloneClone.clone,
    veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.registration.declaration.ResourceIndicator.Insts.CoreCloneClone.clone,
    veoveo_types.platform_names.OAuthClientId.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone,
    veoveo_types.authority.InvocationAuthority.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.internal_auth.request.HubRequestContext.Insts.CoreCloneClone.clone,
    permissions, actingClone, taskClone, delegationClone, caller_deadline_exact,
    bind_ok, bind_tc_eq, veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer.issue]
  apply congrArg (fun continuation : HubRustResult → Result CallerResult =>
    Aeneas.Std.bind (SdkInternalAuthBridge.issue self.issuer (callerRequest self requested)) continuation)
  funext result
  cases result <;> rfl

/-- Complete prepared-stack outcome for the actual extracted Hub caller. All
refusals retain the SDK's exact priority; successful issuance passes the complete
computed claims to the signer before the caller selects the bearer. Native
failures/divergence remain in the binds. No native SDK239 equivalence is claimed. -/
theorem caller_exact (self : Client) (requested : HubTime)
    (physical : CallerPhysical self requested)
    (text : HubIssueTextDomain (callerRequest self requested)) :
    ∃ sdk, requestToSdk (callerRequest self requested) = some sdk ∧
      RequestFields (callerRequest self requested) sdk ∧ RequestCanonical sdk ∧
      requestToHub sdk = callerRequest self requested ∧
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until self requested =
        callerOutcome self.issuer sdk := by
  obtain ⟨sdk, converted, fields, canonical, roundtrip, outcome⟩ :=
    SdkInternalAuthBridge.issue_exact self.issuer (callerRequest self requested)
      (caller_physical self requested physical) text
  refine ⟨sdk, converted, fields, canonical, roundtrip, ?_⟩
  rw [caller_assembly, outcome]
  simp only [nativeResultToHub, Aeneas.Std.bind_assoc, bind_ok, callerOutcome]

theorem caller_outcome_fail (error : Error) :
    Aeneas.Std.bind (Result.fail error : Result SdkRustResult)
      (fun result => Result.ok (bearerResult (rustResultToHub result))) = Result.fail error := by simp

theorem caller_outcome_div :
    Aeneas.Std.bind (Result.div : Result SdkRustResult)
      (fun result => Result.ok (bearerResult (rustResultToHub result))) = Result.div := by simp

#print axioms timestamp_compare
#print axioms hub_minimum
#print axioms sdk_minimum
#print axioms caller_assembly
#print axioms caller_exact
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[``RequestCompositionSdk.internal_auth.ensure_jwt_crypto_provider,
    ``RequestCompositionSdk.chrono.offset.utc.Utc.now,
    ``RequestCompositionSdk.uuid.v7.Uuid.now_v7,
    ``RequestCompositionSdk.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionSdk.identity.JwtId.parse,
    ``RequestCompositionSdk.internal_auth.HubInternalTokenIssuer.sign_forwarded]
  for declaration in #[``SdkInternalAuthCallerProof.timestamp_compare,
      ``SdkInternalAuthCallerProof.hub_comparison, ``SdkInternalAuthCallerProof.earlier_physical,
      ``SdkInternalAuthCallerProof.hub_minimum, ``SdkInternalAuthCallerProof.sdk_minimum,
      ``SdkInternalAuthCallerProof.caller_physical, ``SdkInternalAuthCallerProof.caller_assembly,
      ``SdkInternalAuthCallerProof.caller_exact, ``SdkInternalAuthCallerProof.caller_outcome_fail,
      ``SdkInternalAuthCallerProof.caller_outcome_div] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !(standard ++ native).contains axiomName)).isEmpty do
      throwError "unreviewed prepared SDK caller connection in {declaration}: {found}"
end
end SdkInternalAuthCallerProof
