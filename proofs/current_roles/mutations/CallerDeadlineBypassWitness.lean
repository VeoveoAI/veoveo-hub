-- Unqualified draft: the independent witness check has not passed. The recorded
-- run failed on module qualification/type errors; subsequent edits were not run.
-- See evidence/sdk-issuer-caller-deadline-control.json for the checkpoint boundary.
import HubCurrentRoles
import NativeSetProof
import SdkInternalAuthCallerExamples
import InternalAuthIssueProof

-- Intentionally imports neither CallerProof nor CallerWitnesses: the unchanged
-- caller theorem is expected to fail in this actual-Rust mutant graph.
open Aeneas Aeneas.Std RequestCompositionSdk
open SdkInternalAuthBridge SdkInternalAuthCallerExamples
open InternalAuthSpecification InternalAuthContextExamples InternalAuthIdentityExamples
open InternalAuthIdentityProof InternalAuthClaimsComponents InternalAuthIssueSpecification

namespace CallerDeadlineBypassWitness
noncomputable section

abbrev CallerError := veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpError
abbrev CallerResult := core.result.Result String CallerError

def expectedIdentity (issuer : internal_auth.HubInternalTokenIssuer) (jwtId : identity.JwtId) : Identity :=
  InternalAuthCorrectness.identityValue issuer (sdkRequest boundaryRequested)
    seedIssuedAt boundaryRequested jwtId

/-- Independent complete SDK reference for the caller's retained cap, followed
by its exact error/bearer projection. No desired result is a premise. -/
def reference (issuer : SdkInternalAuthBridge.HubIssuer) : Result CallerResult := do
  let result ← issueOutcome (issuerEquiv issuer) (sdkRequest boundaryCap)
  match result with
  | .Ok issued => .ok (.Ok issued.bearer_token)
  | .Err error => .ok (.Err (.InternalToken (errorToHub error)))

theorem independent_admission :
    requestContextAccepted (sdkRequest boundaryCap) ∧
    delegationError (sdkRequest boundaryCap) = none ∧
    effectiveDeadline (sdkRequest boundaryCap) = boundaryCap ∧
    Native.compareTimestamp boundaryCap seedIssuedAt = .eq ∧
    Native.compareTimestamp boundaryRequested seedIssuedAt = .gt ∧
    (sdkRequest boundaryCap).acting_server.isSome ∧
    (sdkRequest boundaryCap).delegation_context.isSome := by
  have admitted := request_constraints boundaryCap deadline_comparisons.2.2.2.1
  exact ⟨admitted.2.2.1, admitted.2.1, admitted.2.2.2,
    deadline_comparisons.2.2.2.2.2.2.1, deadline_comparisons.2.2.2.2.2.2.2, rfl, rfl⟩

private theorem option_clone {T : Type} (clone : core.clone.Clone T)
    (law : ∀ value, clone.clone value = .ok value) (value : Option T) :
    core.option.Option.Insts.CoreCloneClone.clone clone value = .ok value := by
  cases value <;> simp only [core.option.Option.Insts.CoreCloneClone.clone, law, bind_ok]

private def bearerResult : HubRustResult → CallerResult
  | .Ok token => .Ok token.bearer_token
  | .Err error => .Err (.InternalToken error)

private theorem minimum_self (value : HubTime) :
    core.cmp.Ord.min.trait_default
      (veoveo_hub.chrono.datetime.DateTime.Insts.CoreCmpOrd
        veoveo_hub.chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc) value value = .ok value := by
  simp only [core.cmp.Ord.min.trait_default, core.cmp.Ord.min.default,
    core.cmp.Ord.min_body, core.cmp.PartialOrd.lt_body,
    _root_.chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp]
  simp [SessionNative.timestampCompare]

/-- Actual mutated source body: all fields still forward, but expires_at uses
the later supplied deadline rather than the admitted caller's retained cap. -/
private theorem mutant_assembly (issuer : SdkInternalAuthBridge.HubIssuer) :
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
      (client issuer boundaryCap) (hubTimestamp boundaryRequested) =
    Aeneas.Std.bind (SdkInternalAuthBridge.issue issuer (requestToHub (sdkRequest boundaryRequested)))
      (fun result => .ok (bearerResult result)) := by
  have permissions := NativeSetProof.set_clone_exact
    veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone
    (fun _ => rfl) veoveo_hub.alloc.alloc.Global.Insts.CoreAllocAllocatorClone (client issuer boundaryCap).permissions
  have actingClone := option_clone
    veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone (fun _ => rfl)
    (client issuer boundaryCap).acting_server
  have taskClone := option_clone
    veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone (fun _ => rfl)
    (client issuer boundaryCap).acting_task
  have delegationClone := option_clone
    veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone (fun _ => rfl)
    (client issuer boundaryCap).delegation_context
  unfold veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
  simp only [veoveo_hub_sdk.identity.HubProfileId.Insts.CoreCloneClone.clone,
    veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.registration.declaration.ResourceIndicator.Insts.CoreCloneClone.clone,
    veoveo_types.platform_names.OAuthClientId.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone,
    veoveo_types.authority.InvocationAuthority.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.internal_auth.request.HubRequestContext.Insts.CoreCloneClone.clone,
    permissions, actingClone, taskClone, delegationClone, bind_ok]
  simp only [client, core.option.Option.map_or,
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until.closure.Insts.CoreOpsFunctionFnOnceTupleDateTimeUtcUtcDateTimeUtcUtc.call_once,
    minimum_self, bind_ok, bind_tc_eq, veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer.issue]
  with_unfolding_all
    change Aeneas.Std.bind
      (SdkInternalAuthBridge.issue issuer (requestToHub (sdkRequest boundaryRequested))) _ = _
  apply congrArg (fun continuation : HubRustResult → Result CallerResult =>
    Aeneas.Std.bind (SdkInternalAuthBridge.issue issuer (requestToHub (sdkRequest boundaryRequested))) continuation)
  funext result
  cases result <;> rfl

/-- This theorem computes the actual SDK implementation; its complete claims
argument carries the wrongly longer full timestamp identity. Equal NumericDate
seconds do not turn this into a verifier-usability or JWT-lifetime claim. -/
private theorem sdk_issued (issuer : internal_auth.HubInternalTokenIssuer)
    (observation : InternalAuthCorrectness.IdObservation) (jwtId : identity.JwtId)
    (parsed : observation.parsed = .Ok jwtId)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt)
    (bearer : String)
    (signed : internal_auth.HubInternalTokenIssuer.sign_forwarded issuer
      (claimsValue (expectedIdentity issuer jwtId)) = .ok (.Ok bearer)) :
    internal_auth.HubInternalTokenIssuer.issue issuer (sdkRequest boundaryRequested) =
      .ok (.Ok {bearer_token := bearer, identity := expectedIdentity issuer jwtId}) := by
  obtain ⟨owned, delegated, context, expires⟩ :=
    request_constraints boundaryRequested deadline_comparisons.2.2.2.2.1
  rw [InternalAuthIssueProof.issue_exact issuer _ (request_domain boundaryRequested)]
  unfold issueOutcome
  rw [owned]
  unfold identityOutcome
  rw [provider]
  simp only [bind_ok, bind_tc_ok, delegated]
  rw [clock]
  simp only [bind_ok, bind_tc_ok, context, ite_true, expires,
    deadline_comparisons.2.2.2.2.2.2.2]
  rw [observation.uuid_observed]
  simp only [bind_ok, bind_tc_ok]
  rw [observation.text_observed]
  simp only [bind_ok, bind_tc_ok]
  rw [observation.parse_observed]
  simp only [bind_ok, bind_tc_ok, parsed]
  change Aeneas.Std.bind
    (internal_auth.HubInternalTokenIssuer.sign_forwarded issuer
      (claimsValue (expectedIdentity issuer jwtId))) _ = _
  rw [signed]
  simp only [bind_ok, bind_tc_ok, expectedIdentity]

theorem mutant_accepts (issuer : SdkInternalAuthBridge.HubIssuer)
    (observation : InternalAuthCorrectness.IdObservation) (jwtId : identity.JwtId)
    (parsed : observation.parsed = .Ok jwtId)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt)
    (bearer : String)
    (signed : internal_auth.HubInternalTokenIssuer.sign_forwarded (issuerEquiv issuer)
      (claimsValue (expectedIdentity (issuerEquiv issuer) jwtId)) = .ok (.Ok bearer)) :
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
      (client issuer boundaryCap) (hubTimestamp boundaryRequested) = .ok (.Ok bearer) := by
  rw [mutant_assembly]
  simp only [SdkInternalAuthBridge.issue, converted_request, bind_tc_eq]
  rw [sdk_issued _ observation jwtId parsed provider clock bearer signed]
  simp only [bind_ok, bearerResult, issuedToHub]

theorem reference_refuses (issuer : SdkInternalAuthBridge.HubIssuer)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt) :
    reference issuer = .ok (.Err (.InternalToken .ExpiredDelegation)) := by
  obtain ⟨owned, delegated, context, expires⟩ := request_constraints boundaryCap deadline_comparisons.2.2.2.1
  unfold reference issueOutcome
  rw [owned]
  unfold identityOutcome
  rw [provider]
  simp only [bind_ok, bind_tc_ok, delegated]
  rw [clock]
  simp only [bind_ok, bind_tc_ok, context, ite_true, expires,
    deadline_comparisons.2.2.2.2.2.2.1, reduceCtorEq, ite_false, errorToHub]

/-- Actual caller acceptance contradicts the independently computed complete
reference under the same typed native observations; no own success is assumed. -/
theorem wrong_acceptance (issuer : SdkInternalAuthBridge.HubIssuer)
    (observation : InternalAuthCorrectness.IdObservation) (jwtId : identity.JwtId)
    (parsed : observation.parsed = .Ok jwtId)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt)
    (bearer : String)
    (signed : internal_auth.HubInternalTokenIssuer.sign_forwarded (issuerEquiv issuer)
      (claimsValue (expectedIdentity (issuerEquiv issuer) jwtId)) = .ok (.Ok bearer)) :
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
      (client issuer boundaryCap) (hubTimestamp boundaryRequested) ≠ reference issuer := by
  rw [mutant_accepts issuer observation jwtId parsed provider clock bearer signed,
    reference_refuses issuer provider clock]
  intro same
  rw [Result.ok.injEq] at same
  cases same

#print axioms independent_admission
#print axioms mutant_accepts
#print axioms reference_refuses
#print axioms wrong_acceptance
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[``RequestCompositionSdk.internal_auth.ensure_jwt_crypto_provider,
    ``RequestCompositionSdk.chrono.offset.utc.Utc.now,
    ``RequestCompositionSdk.uuid.v7.Uuid.now_v7,
    ``RequestCompositionSdk.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionSdk.identity.JwtId.parse,
    ``RequestCompositionSdk.internal_auth.HubInternalTokenIssuer.sign_forwarded]
  let admission ← Lean.collectAxioms ``CallerDeadlineBypassWitness.independent_admission
  unless (admission.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
    throwError "unreviewed independent caller admission: {admission}"
  for declaration in #[``CallerDeadlineBypassWitness.mutant_accepts,
      ``CallerDeadlineBypassWitness.reference_refuses, ``CallerDeadlineBypassWitness.wrong_acceptance] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !(standard ++ native).contains axiomName)).isEmpty do
      throwError "unreviewed actual caller deadline fault witness in {declaration}: {found}"
end
end CallerDeadlineBypassWitness
