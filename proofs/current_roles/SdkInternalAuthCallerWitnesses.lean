import SdkInternalAuthCallerExamples
import SdkInternalAuthCallerProof
import InternalAuthIssueProof

open Aeneas Aeneas.Std RequestCompositionSdk
open InternalAuthSpecification InternalAuthContextExamples InternalAuthIdentityExamples
open InternalAuthIdentityProof InternalAuthClaimsComponents InternalAuthIssueSpecification
open InternalAuthClaimsExamples InternalAuthIssueExamples
open SdkInternalAuthBridge SdkInternalAuthCallerExamples

namespace SdkInternalAuthCallerWitnesses
noncomputable section

def expectedIdentity (issuer : internal_auth.HubInternalTokenIssuer)
    (deadline : NativeTimestamp) (jwtId : identity.JwtId) : Identity :=
  InternalAuthCorrectness.identityValue issuer (sdkRequest deadline) seedIssuedAt deadline jwtId

private theorem caller_request (issuer : SdkInternalAuthBridge.HubIssuer)
    (cap requested : NativeTimestamp)
    (afterCap : Native.compareTimestamp requested cap = .gt) :
    SdkInternalAuthCallerProof.callerRequest (client issuer cap) (hubTimestamp requested) =
      requestToHub (sdkRequest cap) := by
  have comparison := SdkInternalAuthCallerProof.timestamp_compare
    (hubTimestamp requested) (hubTimestamp cap)
    (hubTimestamp_physical requested) (hubTimestamp_physical cap)
  simp only [sdkTimestamp_roundtrip, afterCap] at comparison
  have clipped : SdkInternalAuthCallerProof.earlier (hubTimestamp cap) (hubTimestamp requested) =
      hubTimestamp cap := by
    unfold SdkInternalAuthCallerProof.earlier
    rw [← comparison]
    rfl
  simp only [SdkInternalAuthCallerProof.callerRequest, client,
    SdkInternalAuthCallerProof.callerDeadline, clipped]
  rfl

private theorem sdk_issued (issuer : internal_auth.HubInternalTokenIssuer)
    (deadline : NativeTimestamp)
    (beforeInherited : Native.compareTimestamp deadline inheritedDeadline = .lt)
    (future : Native.compareTimestamp deadline seedIssuedAt = .gt)
    (observation : InternalAuthCorrectness.IdObservation) (jwtId : identity.JwtId)
    (parsed : observation.parsed = .Ok jwtId)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt) (bearer : String)
    (signed : internal_auth.HubInternalTokenIssuer.sign_forwarded issuer
      (claimsValue (expectedIdentity issuer deadline jwtId)) = .ok (.Ok bearer)) :
    internal_auth.HubInternalTokenIssuer.issue issuer (sdkRequest deadline) =
      .ok (.Ok {bearer_token := bearer, identity := expectedIdentity issuer deadline jwtId}) := by
  obtain ⟨owned, delegated, context, expires⟩ := request_constraints deadline beforeInherited
  rw [InternalAuthIssueProof.issue_exact issuer _ (request_domain deadline)]
  unfold issueOutcome
  rw [owned]
  unfold identityOutcome
  rw [provider]
  simp only [bind_ok, bind_tc_ok, delegated]
  rw [clock]
  simp only [bind_ok, bind_tc_ok, context, ite_true, expires, future]
  rw [observation.uuid_observed]
  simp only [bind_ok, bind_tc_ok]
  rw [observation.text_observed]
  simp only [bind_ok, bind_tc_ok]
  rw [observation.parse_observed]
  simp only [bind_ok, bind_tc_ok, parsed]
  change Aeneas.Std.bind
    (internal_auth.HubInternalTokenIssuer.sign_forwarded issuer
      (claimsValue (expectedIdentity issuer deadline jwtId))) _ = _
  rw [signed]
  simp only [bind_ok, bind_tc_ok, expectedIdentity]

/-- Actual complete Hub caller acceptance for populated acting/context/Task
inputs, two owned permissions and two scopes. Only typed external primitive
observations are premises; no own conversion/admission/issuance result is assumed.
The signer observation names the complete independently computed claims. -/
theorem positive_issued (issuer : SdkInternalAuthBridge.HubIssuer)
    (observation : InternalAuthCorrectness.IdObservation) (jwtId : identity.JwtId)
    (parsed : observation.parsed = .Ok jwtId)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt) (bearer : String)
    (signed : internal_auth.HubInternalTokenIssuer.sign_forwarded (issuerEquiv issuer)
      (claimsValue (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)) = .ok (.Ok bearer)) :
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
      (client issuer positiveCap) (hubTimestamp positiveRequested) = .ok (.Ok bearer) := by
  rw [SdkInternalAuthCallerProof.caller_assembly,
    caller_request issuer positiveCap positiveRequested deadline_comparisons.2.1]
  simp only [SdkInternalAuthBridge.issue, converted_request, client, bind_tc_eq]
  rw [sdk_issued (issuerEquiv issuer) positiveCap deadline_comparisons.1
    deadline_comparisons.2.2.1 observation jwtId parsed provider clock bearer signed]
  simp only [bind_ok, SdkInternalAuthCallerProof.bearerResult, issuedToHub]

/-- Complete signer claims retain the full timestamp identity but project
NumericDate seconds explicitly. Source context/authority and the inherited Task
are typed preserved inputs, not authentication or liveness evidence. -/
theorem positive_complete_fields (issuer : SdkInternalAuthBridge.HubIssuer) (jwtId : identity.JwtId) :
    ClaimsExact (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)
      (claimsValue (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)) ∧
    IdentityFields (identityToHub (expectedIdentity (issuerEquiv issuer) positiveCap jwtId))
      (expectedIdentity (issuerEquiv issuer) positiveCap jwtId) ∧
    (expectedIdentity (issuerEquiv issuer) positiveCap jwtId).expires_at = positiveCap ∧
    (claimsValue (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)).standard.exp = positiveCap.seconds ∧
    (claimsValue (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)).standard.scope =
      some "operator:read operator:use" ∧
    (claimsValue (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)).standard.act.act =
      some (.mk currentActor.client (some (.mk firstActor.client none))) ∧
    (claimsValue (expectedIdentity (issuerEquiv issuer) positiveCap jwtId)).authorization_details.val =
      [{kind := .Permissions, actions := ordinaryRequest.permissions}] := by
  refine ⟨claims_value_exact _, identity_decoded_fields _, rfl, rfl, rfl, ?_, ?_⟩
  · rfl
  · simp [claimsValue, expectedIdentity, InternalAuthCorrectness.identityValue,
      sdkRequest, ordinaryRequest, InternalAuthClaimsExamples.richIdentity]

/-- Same admitted delegation with a cap exactly equal to the clock refuses.
Only provider/clock observations are needed: ID creation and signing are not
success assumptions and the full generic outcome remains fallible. -/
theorem boundary_refused (issuer : SdkInternalAuthBridge.HubIssuer)
    (provider : internal_auth.ensure_jwt_crypto_provider = .ok ())
    (clock : RequestCompositionSdk.chrono.offset.utc.Utc.now = .ok seedIssuedAt) :
    veoveo_hub.mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until
      (client issuer boundaryCap) (hubTimestamp boundaryRequested) =
      .ok (.Err (.InternalToken .ExpiredDelegation)) := by
  rw [SdkInternalAuthCallerProof.caller_assembly,
    caller_request issuer boundaryCap boundaryRequested deadline_comparisons.2.2.2.2.2.1]
  simp only [SdkInternalAuthBridge.issue, converted_request, client, bind_tc_eq]
  rw [InternalAuthIssueProof.issue_exact _ _ (request_domain boundaryCap)]
  obtain ⟨owned, delegated, context, expires⟩ := request_constraints boundaryCap deadline_comparisons.2.2.2.1
  unfold issueOutcome
  rw [owned]
  unfold identityOutcome
  rw [provider]
  simp only [bind_ok, bind_tc_ok, delegated]
  rw [clock]
  simp only [bind_ok, bind_tc_ok, context, ite_true, expires,
    deadline_comparisons.2.2.2.2.2.2.1, reduceCtorEq, ite_false,
    errorToHub, SdkInternalAuthCallerProof.bearerResult]

#print axioms positive_issued
#print axioms positive_complete_fields
#print axioms boundary_refused
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[``RequestCompositionSdk.internal_auth.ensure_jwt_crypto_provider,
    ``RequestCompositionSdk.chrono.offset.utc.Utc.now,
    ``RequestCompositionSdk.uuid.v7.Uuid.now_v7,
    ``RequestCompositionSdk.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionSdk.identity.JwtId.parse,
    ``RequestCompositionSdk.internal_auth.HubInternalTokenIssuer.sign_forwarded]
  for declaration in #[``SdkInternalAuthCallerWitnesses.positive_issued,
      ``SdkInternalAuthCallerWitnesses.positive_complete_fields,
      ``SdkInternalAuthCallerWitnesses.boundary_refused] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !(standard ++ native).contains axiomName)).isEmpty do
      throwError "unreviewed populated actual issuer caller in {declaration}: {found}"
end
end SdkInternalAuthCallerWitnesses
