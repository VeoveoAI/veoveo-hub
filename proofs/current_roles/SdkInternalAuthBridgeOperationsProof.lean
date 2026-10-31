import SdkInternalAuthBridgeProof
import InternalAuthIssueProof
import SdkInternalAuthBridgeOperations

open Aeneas Aeneas.Std

namespace SdkInternalAuthBridge
noncomputable section

/-- Independent physical text bounds on the Hub input. The rendered-scope bound
holds for every duplicate-free enumeration of its finite scope set; it does not
declare an arbitrary enumeration to be the native BTreeSet traversal. Allocation,
UTF-8 and native traversal correspondence remain qualification boundaries. -/
structure HubIssueTextDomain (request : HubRequest) : Prop where
  server : InternalAuthContextProof.TextRepresentable request.server.value
  permissions : ∀ permission ∈ request.permissions,
    InternalAuthContextProof.TextRepresentable permission.value
  actor_id : InternalAuthContextProof.TextRepresentable request.actor.id.value
  actor_subject : InternalAuthContextProof.TextRepresentable request.actor.subject.value
  audience : InternalAuthContextProof.TextRepresentable request.audience.value
  scopes : ∀ scope ∈ request.actor.scopes,
    InternalAuthContextProof.TextRepresentable scope.value
  rendered_scopes : ∀ entries : List _root_.veoveo_types.names.ScopeName,
    entries.toFinset = request.actor.scopes → entries.Nodup →
      InternalAuthContextProof.TextRepresentable
        (String.intercalate " " (entries.map (·.value)))
  context : ∀ context ∈ request.request_context,
    InternalAuthContextProof.TextRepresentable context.principal.subject.value ∧
    InternalAuthContextProof.TextRepresentable context.access_token.issuer.value ∧
    InternalAuthContextProof.TextRepresentable context.access_token.oauth_client_id.value

private theorem enumeration {hubTag sdkTag : Nat}
    (original : Finset (CurrentRolesId hubTag))
    (entries : RequestCompositionSdk.OrderedSet (RequestCompositionSdk.NativeId sdkTag))
    (fields : SetFields IdFields original entries) (unique : entries.values.Nodup) :
    (entries.values.map (idEquiv hubTag sdkTag).symm).toFinset = original ∧
      (entries.values.map (idEquiv hubTag sdkTag).symm).Nodup := by
  classical
  constructor
  · ext value
    simp only [List.mem_toFinset, List.mem_map]
    constructor
    · rintro ⟨entry, member, same⟩
      obtain ⟨source, sourceMember, relation⟩ := (fields entry).mp member
      have equal := (id_fields_iff source entry).mp relation
      subst entry
      simp only [Equiv.symm_apply_apply] at same
      simpa only [same] using sourceMember
    · intro member
      refine ⟨idEquiv hubTag sdkTag value, ?_, (idEquiv hubTag sdkTag).symm_apply_apply value⟩
      exact (fields _).mpr ⟨value, member, (id_fields_iff _ _).mpr rfl⟩
  · exact unique.map (idEquiv hubTag sdkTag).symm.injective

private theorem text_fields {hubTag sdkTag : Nat} (hub : CurrentRolesId hubTag)
    (sdk : RequestCompositionSdk.NativeId sdkTag) (fields : IdFields hub sdk)
    (bound : InternalAuthContextProof.TextRepresentable hub.value) :
    InternalAuthContextProof.TextRepresentable sdk.value := by
  exact (show hub.value = sdk.value from fields) ▸ bound

/-- Membership and complete field preservation derive the SDK's text domain.
Only scope uniqueness is needed here; no assumed converted issuer success. -/
theorem converted_issue_text (hub : HubRequest)
    (sdk : RequestCompositionSdk.internal_auth.ForwardedRequest)
    (fields : RequestFields hub sdk) (canonical : RequestCanonical sdk)
    (domain : HubIssueTextDomain hub) :
    InternalAuthIssueSpecification.IssueTextDomain sdk := by
  refine ⟨⟨?_, ?_⟩, ?_, ?_, ?_, ?_, ?_⟩
  · exact text_fields _ _ fields.server domain.server
  · intro entry member
    obtain ⟨source, sourceMember, equal⟩ := (fields.permissions entry).mp member
    exact text_fields _ _ equal (domain.permissions source sourceMember)
  · intro context present
    have relation := fields.request_context
    rw [present] at relation
    cases source : hub.request_context with
    | none => simp only [source, OptionalFields] at relation
    | some original =>
      have contextFields : ContextFields original context := by
        simpa only [source, OptionalFields] using relation
      obtain ⟨subject, issuer, client⟩ := domain.context original (by simp only [source, Option.mem_def])
      refine ⟨?_, ?_, ?_, ?_, ?_⟩
      · exact text_fields _ _ contextFields.principal.subject subject
      · exact text_fields _ _ fields.actor.id domain.actor_id
      · exact text_fields _ _ fields.actor.subject domain.actor_subject
      · exact text_fields _ _ contextFields.access_token.issuer issuer
      · exact text_fields _ _ contextFields.access_token.oauth_client_id client
  · exact text_fields _ _ fields.actor.id domain.actor_id
  · exact text_fields _ _ fields.audience domain.audience
  · intro entry member
    obtain ⟨source, sourceMember, equal⟩ := (fields.actor.scopes entry).mp member
    exact text_fields _ _ equal (domain.scopes source sourceMember)
  · obtain ⟨sameSet, unique⟩ := enumeration hub.actor.scopes sdk.actor.scopes
      fields.actor.scopes canonical.actor.scopes.1
    have bound := domain.rendered_scopes _ sameSet unique
    simp only [List.map_map] at bound
    with_unfolding_all exact bound

/-- Complete prepared-stack issuer outcome, with every native observation and
signer result left in its original bind. Conversion availability and the SDK text
domain follow from independent Hub bounds, not an own-result equality premise. -/
theorem issue_exact (issuer : HubIssuer) (request : HubRequest)
    (physical : RequestPhysical request) (text : HubIssueTextDomain request) :
    ∃ sdk, requestToSdk request = some sdk ∧ RequestFields request sdk ∧
      RequestCanonical sdk ∧ requestToHub sdk = request ∧
      issue issuer request = nativeResultToHub
        (InternalAuthIssueSpecification.issueOutcome (issuerEquiv issuer) sdk) := by
  obtain ⟨sdk, converted, fields, canonical, roundtrip⟩ := request_exact request physical
  refine ⟨sdk, converted, fields, canonical, roundtrip, ?_⟩
  simp only [issue, converted]
  rw [InternalAuthIssueProof.issue_exact _ _ (converted_issue_text request sdk fields canonical text)]
  simp only [nativeResultToHub, bind_tc_eq]
  apply congrArg (fun continuation : SdkRustResult → Result HubRustResult =>
    Aeneas.Std.bind (InternalAuthIssueSpecification.issueOutcome (issuerEquiv issuer) sdk) continuation)
  funext result
  cases result <;> rfl

#print axioms converted_issue_text
#print axioms issue_exact
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[``RequestCompositionSdk.internal_auth.ensure_jwt_crypto_provider,
    ``RequestCompositionSdk.chrono.offset.utc.Utc.now,
    ``RequestCompositionSdk.uuid.v7.Uuid.now_v7,
    ``RequestCompositionSdk.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionSdk.identity.JwtId.parse,
    ``RequestCompositionSdk.internal_auth.HubInternalTokenIssuer.sign_forwarded]
  for declaration in #[``SdkInternalAuthBridge.converted_issue_text, ``SdkInternalAuthBridge.issue_exact] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !(standard ++ native).contains axiomName)).isEmpty do
      throwError "unreviewed prepared SDK issue connection in {declaration}: {found}"
end
end SdkInternalAuthBridge
