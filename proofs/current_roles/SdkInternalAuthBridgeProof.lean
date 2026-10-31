import SdkInternalAuthBridgeFields
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std

namespace SdkInternalAuthBridge
noncomputable section

/-- Independent physical domain: no context admission, policy or issuer success. -/
def PrincipalPhysical (value : veoveo_hub.veoveo_hub_sdk.identity.Principal) : Prop :=
  ∀ time ∈ value.authenticated_at, TimestampPhysical time

def AccessTokenPhysical (value : veoveo_hub.veoveo_hub_sdk.identity.AccessTokenSubject) : Prop :=
  TimestampPhysical value.issued_at ∧ (∀ time ∈ value.not_before, TimestampPhysical time) ∧
    TimestampPhysical value.expires_at

def ContextPhysical (value : veoveo_hub.veoveo_hub_sdk.internal_auth.request.HubRequestContext) : Prop :=
  AccessTokenPhysical value.access_token ∧ PrincipalPhysical value.principal

def DelegationPhysical (value : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegationContext) : Prop :=
  TimestampPhysical value.expires_at

def RequestPhysical (value : veoveo_hub.veoveo_hub_sdk.internal_auth.ForwardedRequest) : Prop :=
  PrincipalPhysical value.actor ∧
    (∀ context ∈ value.request_context, ContextPhysical context) ∧
    (∀ delegation ∈ value.delegation_context, DelegationPhysical delegation) ∧
    TimestampPhysical value.expires_at

def IdentityPhysical (value : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalIdentity) : Prop :=
  PrincipalPhysical value.actor ∧
    (∀ context ∈ value.request_context, ContextPhysical context) ∧
    (∀ delegation ∈ value.delegation_context, DelegationPhysical delegation) ∧
    TimestampPhysical value.issued_at ∧ TimestampPhysical value.not_before ∧
    TimestampPhysical value.expires_at

/-- Native traversal correspondence remains separate from these model laws.
Canonicalization is deterministic and proves order/uniqueness for each set. -/
def SetCanonical {S : Type} [LinearOrder S] (value : RequestCompositionSdk.OrderedSet S) : Prop :=
  value.values.Nodup ∧ value.values.Pairwise (· ≤ ·)

structure PrincipalCanonical (value : RequestCompositionSdk.identity.Principal) : Prop where
  groups : SetCanonical value.groups
  group_roles : SetCanonical value.group_roles
  roles : SetCanonical value.roles
  scopes : SetCanonical value.scopes
  data_labels : SetCanonical value.data_labels
  assurances : SetCanonical value.assurances

theorem id_fields_iff {hubTag sdkTag : Nat} (hub : CurrentRolesId hubTag)
    (sdk : RequestCompositionSdk.NativeId sdkTag) :
    IdFields hub sdk ↔ idEquiv hubTag sdkTag hub = sdk := by
  cases hub; cases sdk
  simp [IdFields, idEquiv]

theorem group_role_fields (value : veoveo_hub.veoveo_hub_sdk.identity.GroupRole) :
    GroupRoleFields value (groupRoleEquiv value) := by
  cases value <;> trivial

theorem group_membership_fields_iff
    (hub : veoveo_hub.veoveo_hub_sdk.identity.GroupMembership)
    (sdk : RequestCompositionSdk.identity.GroupMembership) :
    GroupMembershipFields hub sdk ↔ groupMembershipEquiv hub = sdk := by
  have fields : GroupMembershipFields hub sdk ↔
      IdFields hub.group sdk.group ∧ GroupRoleFields hub.role sdk.role :=
    ⟨fun relation => ⟨relation.group, relation.role⟩, fun relation => ⟨relation.1, relation.2⟩⟩
  rw [fields]
  cases hub with
  | mk hubGroup hubRole =>
    cases sdk with
    | mk sdkGroup sdkRole =>
      cases sdkGroup
      cases hubRole <;> cases sdkRole <;>
        simp [GroupRoleFields, groupMembershipEquiv, groupRoleEquiv,
          IdFields, idEquiv]

theorem assurance_fields_iff (hub : veoveo_hub.veoveo_hub_sdk.identity.PrincipalAssurance)
    (sdk : RequestCompositionSdk.identity.PrincipalAssurance) :
    AssuranceFields hub sdk ↔ assuranceEquiv hub = sdk := by
  cases hub; cases sdk; simp [AssuranceFields, assuranceEquiv]

private theorem ordered_fields {H S : Type} (codec : H ≃ S) [LinearOrder S]
    (fields : H → S → Prop) (same : ∀ hub sdk, fields hub sdk ↔ codec hub = sdk) (values : Finset H) :
    SetFields fields values (orderedSet codec values) := by
  intro entry
  simp only [orderedSet, Finset.mem_sort, Finset.mem_map, Equiv.toEmbedding_apply]
  constructor
  · rintro ⟨hub, member, equal⟩
    exact ⟨hub, member, (same hub entry).mpr equal⟩
  · rintro ⟨hub, member, relation⟩
    exact ⟨hub, member, (same hub entry).mp relation⟩

private theorem ordered_canonical {H S : Type} (codec : H ≃ S) [LinearOrder S] (values : Finset H) :
    SetCanonical (orderedSet codec values) :=
  ⟨orderedSet_nodup codec values, orderedSet_sorted codec values⟩

private theorem optional_equiv_fields {H S : Type} (codec : H ≃ S)
    (fields : H → S → Prop) (law : ∀ hub, fields hub (codec hub)) (value : Option H) :
    OptionalFields fields value (value.map codec) := by
  cases value with
  | none => trivial
  | some value => exact law value

private theorem timestamp_fields (value : HubTime) (physical : TimestampPhysical value) :
    TimestampFields value (timestamp value physical) := by
  have exactValue := timestamp_roundtrip value physical
  exact ⟨(congrArg Prod.fst exactValue).symm, (congrArg Prod.snd exactValue).symm⟩

private theorem optional_timestamp (value : Option HubTime)
    (physical : ∀ time ∈ value, TimestampPhysical time) :
    ∃ converted, optional timestamp? value = some converted ∧
      OptionalFields TimestampFields value converted ∧ converted.map hubTimestamp = value := by
  cases value with
  | none => exact ⟨none, rfl, trivial, rfl⟩
  | some time =>
    have bound := physical time rfl
    refine ⟨some (timestamp time bound), ?_, timestamp_fields time bound, ?_⟩
    · simp [optional, (timestamp_checked time).1 bound]
    · exact congrArg some (timestamp_roundtrip time bound)

private theorem subject_fields (value : veoveo_hub.veoveo_types.identity.AccessSubject) :
    SubjectFields value (accessSubjectEquiv value) := by cases value <;> rfl

private theorem level_fields (value : veoveo_hub.veoveo_types.authority.AccessLevel) :
    AccessLevelFields value (accessLevelEquiv value) := by cases value <;> trivial

private theorem grant_fields (value : veoveo_hub.veoveo_types.authority.WorkContextGrant) :
    GrantFields value (grantEquiv value) :=
  ⟨subject_fields value.subject, level_fields value.level⟩

private theorem vector_fields {H S : Type} (codec : H ≃ S) (fields : H → S → Prop)
    (law : ∀ hub, fields hub (codec hub)) (value : alloc.vec.Vec H) :
    List.Forall₂ fields value.val (vectorEquiv codec value).val := by
  change List.Forall₂ fields value.val
    (alloc.vec.Vec.from (value.val.map codec) (by simpa using value.property)).val
  rw [alloc.vec.Vec.from_val]
  induction value.val with
  | nil => exact .nil
  | cons head tail induction => exact .cons (law head) induction

/-- All output fields including ordered initial grants and owner constructor. -/
theorem output_policy_fields (value : veoveo_hub.veoveo_types.authority.WorkContextOutputPolicy) :
    OutputPolicyFields value (outputPolicyToSdk value) := by
  refine ⟨subject_fields value.owner, vector_fields grantEquiv GrantFields grant_fields _, ?_, ?_⟩
  · apply optional_equiv_fields (idEquiv _ _) IdFields
    intro original
    rfl
  · exact ordered_fields (idEquiv _ _) IdFields id_fields_iff value.data_labels

theorem authority_fields (value : veoveo_hub.veoveo_types.authority.InvocationAuthority) :
    AuthorityFields value (authorityToSdk value) := by
  refine ⟨rfl, rfl, ?_, rfl, output_policy_fields value.output_policy, ?_⟩
  · change MembershipFields value.membership (membershipEquiv value.membership)
    cases value.membership <;> simp [MembershipFields, membershipEquiv]
  · change ProvenanceFields value.provenance (provenanceEquiv value.provenance)
    cases value.provenance <;> simp [ProvenanceFields, provenanceEquiv, IdFields, idEquiv]

/-- Full inverse on the Hub side is a supplement to the named field predicates. -/
theorem authority_roundtrip (value : veoveo_hub.veoveo_types.authority.InvocationAuthority) :
    authorityToHub (authorityToSdk value) = value := by
  cases value
  simp [authorityToSdk, authorityToHub, outputPolicyToSdk, outputPolicyToHub, set_roundtrip]

/-- Physical sufficiency, complete independent field semantics, deterministic
set invariants and full Hub-side roundtrip. No conversion success is assumed. -/
theorem principal_exact (hub : veoveo_hub.veoveo_hub_sdk.identity.Principal)
    (physical : PrincipalPhysical hub) :
    ∃ sdk, principalToSdk hub = some sdk ∧ PrincipalFields hub sdk ∧
      PrincipalCanonical sdk ∧ principalToHub sdk = hub := by
  obtain ⟨authenticated, authenticatedEq, authenticatedFields, authenticatedRoundtrip⟩ :=
    optional_timestamp hub.authenticated_at physical
  have available : ∃ sdk, principalToSdk hub = some sdk := by
    simp [principalToSdk, authenticatedEq]
  obtain ⟨sdk, converted⟩ := available
  have shape := converted
  simp [principalToSdk, authenticatedEq] at shape
  subst sdk
  refine ⟨_, converted, ?_, ?_, ?_⟩
  · refine ⟨rfl, ?_, rfl, rfl, ?_, ?_, ?_, ?_, ?_, ?_, ?_, authenticatedFields⟩
    · cases hub.kind <;> trivial
    · apply optional_equiv_fields (idEquiv _ _) IdFields
      intro value; rfl
    · exact ordered_fields (idEquiv _ _) IdFields id_fields_iff _
    · exact ordered_fields groupMembershipEquiv GroupMembershipFields group_membership_fields_iff _
    · exact ordered_fields (idEquiv _ _) IdFields id_fields_iff _
    · exact ordered_fields (idEquiv _ _) IdFields id_fields_iff _
    · exact ordered_fields (idEquiv _ _) IdFields id_fields_iff _
    · exact ordered_fields assuranceEquiv AssuranceFields assurance_fields_iff _
  · exact ⟨ordered_canonical _ _, ordered_canonical _ _, ordered_canonical _ _,
      ordered_canonical _ _, ordered_canonical _ _, ordered_canonical _ _⟩
  · cases hub
    simp [principalToHub, set_roundtrip, authenticatedRoundtrip]

private theorem mode_fields (value : veoveo_hub.veoveo_types.provenance.InvocationMode) :
    ModeFields value (invocationModeEquiv value) := by cases value <;> trivial

private theorem ip_fields (value : veoveo_hub.core.net.ip_addr.IpAddr) :
    IpFields value (ipEquiv value) := by cases value <;> rfl

theorem audit_fields (value : veoveo_hub.veoveo_audit_contract.model.AuditRequest) :
    AuditFields value (auditEquiv value) := by
  refine ⟨rfl, rfl, rfl, optional_equiv_fields ipEquiv IpFields ip_fields _, ?_⟩
  apply optional_equiv_fields (idEquiv _ _) IdFields
  intro original; rfl

theorem access_token_exact (hub : veoveo_hub.veoveo_hub_sdk.identity.AccessTokenSubject)
    (physical : AccessTokenPhysical hub) :
    ∃ sdk, accessTokenToSdk hub = some sdk ∧ AccessTokenFields hub sdk ∧
      SetCanonical sdk.scopes ∧ accessTokenToHub sdk = hub := by
  obtain ⟨notBefore, notBeforeEq, notBeforeFields, notBeforeRoundtrip⟩ :=
    optional_timestamp hub.not_before physical.2.1
  have issuedEq := (timestamp_checked hub.issued_at).1 physical.1
  have expiresEq := (timestamp_checked hub.expires_at).1 physical.2.2
  have available : ∃ sdk, accessTokenToSdk hub = some sdk := by
    simp [accessTokenToSdk, issuedEq, expiresEq, notBeforeEq]
  obtain ⟨sdk, converted⟩ := available
  have shape := converted
  simp [accessTokenToSdk, issuedEq, expiresEq, notBeforeEq] at shape
  subst sdk
  refine ⟨_, converted, ?_, ordered_canonical _ _, ?_⟩
  · refine ⟨rfl, rfl, rfl, ?_, rfl, rfl, mode_fields hub.invocation_mode,
      ?_, ?_, ordered_fields (idEquiv _ _) IdFields id_fields_iff _, ?_,
      timestamp_fields _ physical.1, notBeforeFields, timestamp_fields _ physical.2.2⟩
    · apply optional_equiv_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
      intro original; rfl
    · apply optional_equiv_fields (idEquiv _ _) IdFields
      intro original; rfl
    · apply optional_equiv_fields (idEquiv _ _) IdFields
      intro original; rfl
    · apply optional_equiv_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
      intro original; rfl
  · cases hub
    simp [accessTokenToHub, set_roundtrip, notBeforeRoundtrip, timestamp_roundtrip]

structure ContextCanonical (value : RequestCompositionSdk.internal_auth.request.HubRequestContext) : Prop where
  token_scopes : SetCanonical value.access_token.scopes
  principal : PrincipalCanonical value.principal

theorem context_exact (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.request.HubRequestContext)
    (physical : ContextPhysical hub) :
    ∃ sdk, contextToSdk hub = some sdk ∧ ContextFields hub sdk ∧
      ContextCanonical sdk ∧ contextToHub sdk = hub := by
  obtain ⟨token, tokenEq, tokenFields, tokenCanonical, tokenRoundtrip⟩ :=
    access_token_exact hub.access_token physical.1
  obtain ⟨principal, principalEq, principalFields, principalCanonical, principalRoundtrip⟩ :=
    principal_exact hub.principal physical.2
  have available : ∃ sdk, contextToSdk hub = some sdk := by
    simp [contextToSdk, tokenEq, principalEq]
  obtain ⟨sdk, converted⟩ := available
  have shape := converted
  simp [contextToSdk, tokenEq, principalEq] at shape
  subst sdk
  refine ⟨_, converted, ⟨rfl, audit_fields _, tokenFields, principalFields⟩,
    ⟨tokenCanonical, principalCanonical⟩, ?_⟩
  cases hub
  simp [contextToHub, tokenRoundtrip, principalRoundtrip]

private theorem acting_fields (value : veoveo_hub.veoveo_hub_sdk.internal_auth.ActingServer) :
    ActingFields value (actingEquiv value) := ⟨rfl, rfl, rfl⟩

private theorem task_fields (value : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegatingTask) :
    TaskFields value (taskEquiv value) := ⟨rfl, rfl, rfl⟩

theorem delegation_exact (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegationContext)
    (physical : DelegationPhysical hub) :
    ∃ sdk, delegationToSdk hub = some sdk ∧ DelegationFields hub sdk ∧ delegationToHub sdk = hub := by
  have expiresEq := (timestamp_checked hub.expires_at).1 physical
  have available : ∃ sdk, delegationToSdk hub = some sdk := by
    simp [delegationToSdk, expiresEq]
  obtain ⟨sdk, converted⟩ := available
  have shape := converted
  simp [delegationToSdk, expiresEq] at shape
  subst sdk
  refine ⟨_, converted, ⟨vector_fields actingEquiv ActingFields acting_fields _,
    optional_equiv_fields taskEquiv TaskFields task_fields _, timestamp_fields _ physical⟩, ?_⟩
  cases hub
  simp [delegationToHub]
  exact timestamp_roundtrip _ physical

private theorem optional_context_exact
    (hub : Option veoveo_hub.veoveo_hub_sdk.internal_auth.request.HubRequestContext)
    (physical : ∀ value ∈ hub, ContextPhysical value) :
    ∃ sdk, optional contextToSdk hub = some sdk ∧ OptionalFields ContextFields hub sdk ∧
      (∀ value ∈ sdk, ContextCanonical value) ∧ sdk.map contextToHub = hub := by
  cases hub with
  | none => exact ⟨none, rfl, trivial, by simp, rfl⟩
  | some hub =>
    obtain ⟨sdk, converted, fields, canonical, roundtrip⟩ := context_exact hub (physical hub rfl)
    refine ⟨some sdk, ?_, fields, ?_, congrArg some roundtrip⟩
    · simp [optional, converted]
    · intro value same
      cases same
      exact canonical

private theorem optional_delegation_exact
    (hub : Option veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegationContext)
    (physical : ∀ value ∈ hub, DelegationPhysical value) :
    ∃ sdk, optional delegationToSdk hub = some sdk ∧ OptionalFields DelegationFields hub sdk ∧
      sdk.map delegationToHub = hub := by
  cases hub with
  | none => exact ⟨none, rfl, trivial, rfl⟩
  | some hub =>
    obtain ⟨sdk, converted, fields, roundtrip⟩ := delegation_exact hub (physical hub rfl)
    exact ⟨some sdk, by simp [optional, converted], fields, congrArg some roundtrip⟩

structure RequestCanonical (value : RequestCompositionSdk.internal_auth.ForwardedRequest) : Prop where
  actor : PrincipalCanonical value.actor
  output_labels : SetCanonical value.authority.output_policy.data_labels
  permissions : SetCanonical value.permissions
  context : ∀ context ∈ value.request_context, ContextCanonical context

/-- Main caller-usable forward contract: independent physical bounds suffice for
an available conversion, with every named request field preserved. No own issuer,
context-admission or desired converted result is an input premise. -/
theorem request_exact (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.ForwardedRequest)
    (physical : RequestPhysical hub) :
    ∃ sdk, requestToSdk hub = some sdk ∧ RequestFields hub sdk ∧
      RequestCanonical sdk ∧ requestToHub sdk = hub := by
  obtain ⟨actor, actorEq, actorFields, actorCanonical, actorRoundtrip⟩ := principal_exact hub.actor physical.1
  obtain ⟨context, contextEq, contextFields, contextCanonical, contextRoundtrip⟩ :=
    optional_context_exact hub.request_context physical.2.1
  obtain ⟨delegation, delegationEq, delegationFields, delegationRoundtrip⟩ :=
    optional_delegation_exact hub.delegation_context physical.2.2.1
  have expiresEq := (timestamp_checked hub.expires_at).1 physical.2.2.2
  have available : ∃ sdk, requestToSdk hub = some sdk := by
    simp [requestToSdk, actorEq, contextEq, delegationEq, expiresEq]
  obtain ⟨sdk, converted⟩ := available
  have shape := converted
  simp [requestToSdk, actorEq, contextEq, delegationEq, expiresEq] at shape
  subst sdk
  refine ⟨_, converted, ?_, ?_, ?_⟩
  · refine ⟨rfl, rfl, rfl, rfl, rfl, actorFields, authority_fields _,
      ordered_fields (idEquiv _ _) IdFields id_fields_iff _, contextFields,
      optional_equiv_fields actingEquiv ActingFields acting_fields _, ?_, delegationFields,
      timestamp_fields _ physical.2.2.2⟩
    apply optional_equiv_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
    intro original; rfl
  · exact ⟨actorCanonical, ordered_canonical _ _, ordered_canonical _ _, contextCanonical⟩
  · cases hub
    simp [requestToHub, actorRoundtrip, authority_roundtrip, set_roundtrip,
      contextRoundtrip, delegationRoundtrip, timestamp_roundtrip]

structure IdentityCanonical (value : RequestCompositionSdk.internal_auth.HubInternalIdentity) : Prop where
  actor : PrincipalCanonical value.actor
  output_labels : SetCanonical value.authority.output_policy.data_labels
  permissions : SetCanonical value.permissions
  context : ∀ context ∈ value.request_context, ContextCanonical context

/-- Full identity bytes and all three full timestamps survive the representation
bridge. NumericDate projection/crypto/authenticity remain separate obligations. -/
theorem identity_exact (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalIdentity)
    (physical : IdentityPhysical hub) :
    ∃ sdk, identityToSdk hub = some sdk ∧ IdentityFields hub sdk ∧
      IdentityCanonical sdk ∧ identityToHub sdk = hub := by
  obtain ⟨actor, actorEq, actorFields, actorCanonical, actorRoundtrip⟩ := principal_exact hub.actor physical.1
  obtain ⟨context, contextEq, contextFields, contextCanonical, contextRoundtrip⟩ :=
    optional_context_exact hub.request_context physical.2.1
  obtain ⟨delegation, delegationEq, delegationFields, delegationRoundtrip⟩ :=
    optional_delegation_exact hub.delegation_context physical.2.2.1
  have issuedEq := (timestamp_checked hub.issued_at).1 physical.2.2.2.1
  have notBeforeEq := (timestamp_checked hub.not_before).1 physical.2.2.2.2.1
  have expiresEq := (timestamp_checked hub.expires_at).1 physical.2.2.2.2.2
  have available : ∃ sdk, identityToSdk hub = some sdk := by
    simp [identityToSdk, actorEq, contextEq, delegationEq, issuedEq, notBeforeEq, expiresEq]
  obtain ⟨sdk, converted⟩ := available
  have shape := converted
  simp [identityToSdk, actorEq, contextEq, delegationEq, issuedEq, notBeforeEq, expiresEq] at shape
  subst sdk
  refine ⟨_, converted, ?_, ?_, ?_⟩
  · refine ⟨rfl, rfl, rfl, rfl, rfl, rfl, actorFields, authority_fields _,
      ordered_fields (idEquiv _ _) IdFields id_fields_iff _, contextFields,
      optional_equiv_fields actingEquiv ActingFields acting_fields _, ?_, delegationFields, rfl,
      timestamp_fields _ physical.2.2.2.1, timestamp_fields _ physical.2.2.2.2.1,
      timestamp_fields _ physical.2.2.2.2.2⟩
    apply optional_equiv_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
    intro original; rfl
  · exact ⟨actorCanonical, ordered_canonical _ _, ordered_canonical _ _, contextCanonical⟩
  · cases hub
    simp [identityToHub, actorRoundtrip, authority_roundtrip, set_roundtrip,
      contextRoundtrip, delegationRoundtrip, timestamp_roundtrip]

theorem issuer_fields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer) :
    IssuerFields hub (issuerEquiv hub) := ⟨rfl, rfl⟩

theorem issued_exact (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.IssuedHubInternalToken)
    (physical : IdentityPhysical hub.identity) :
    ∃ sdk, issuedToSdk hub = some sdk ∧ IssuedFields hub sdk ∧ issuedToHub sdk = hub := by
  obtain ⟨token, tokenEq, tokenFields, _, tokenRoundtrip⟩ := identity_exact hub.identity physical
  refine ⟨{bearer_token := hub.bearer_token, identity := token}, ?_, ⟨rfl, tokenFields⟩, ?_⟩
  · simp [issuedToSdk, tokenEq]
  · cases hub
    simp [issuedToHub, tokenRoundtrip]

/-- Every error constructor is retained, including constructor-specific payload
positions. A bijection alone would not establish these independent relations. -/
theorem error_fields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError) :
    ErrorFields hub (errorToSdk hub) := by
  cases hub <;> simp [ErrorFields, errorToSdk, IdFields, idEquiv, opaqueEquiv]
  rename_i algorithm
  cases algorithm <;> simp [AlgorithmFields, algorithmEquiv]

theorem error_roundtrip (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError) :
    errorToHub (errorToSdk hub) = hub := by
  cases hub <;> simp [errorToSdk, errorToHub]

theorem sdk_error_roundtrip (sdk : RequestCompositionSdk.internal_auth.InternalTokenError) :
    errorToSdk (errorToHub sdk) = sdk := by
  cases sdk <;> simp [errorToSdk, errorToHub]

private theorem unordered_fields {H S : Type} (codec : H ≃ S)
    (fields : H → S → Prop) (same : ∀ hub sdk, fields hub sdk ↔ codec hub = sdk)
    (values : RequestCompositionSdk.OrderedSet S) :
    SetFields fields (unorderedSet codec values) values := by
  classical
  intro entry
  constructor
  · intro member
    refine ⟨codec.symm entry, ?_, (same _ _).mpr (codec.apply_symm_apply entry)⟩
    simp [unorderedSet, member]
  · rintro ⟨hub, member, relation⟩
    simp only [unorderedSet, List.mem_toFinset, List.mem_map] at member
    obtain ⟨original, originalMember, equal⟩ := member
    have outputEqual : original = entry := by
      calc
        original = codec (codec.symm original) := (codec.apply_symm_apply original).symm
        _ = codec hub := congrArg codec equal
        _ = entry := (same _ _).mp relation
    exact outputEqual ▸ originalMember

private theorem optional_inverse_fields {H S : Type} (codec : H ≃ S)
    (fields : H → S → Prop) (law : ∀ sdk, fields (codec.symm sdk) sdk) (value : Option S) :
    OptionalFields fields (value.map codec.symm) value := by
  cases value with
  | none => trivial
  | some value => exact law value

private theorem inverse_vector_fields {H S : Type} (codec : H ≃ S)
    (fields : H → S → Prop) (law : ∀ sdk, fields (codec.symm sdk) sdk) (value : alloc.vec.Vec S) :
    List.Forall₂ fields ((vectorEquiv codec).symm value).val value.val := by
  change List.Forall₂ fields
    (alloc.vec.Vec.from (value.val.map codec.symm) (by simpa using value.property)).val value.val
  rw [alloc.vec.Vec.from_val]
  induction value.val with
  | nil => exact .nil
  | cons head tail induction => exact .cons (law head) induction

private theorem inverse_subject_fields (value : RequestCompositionSdk.veoveo_types.identity.AccessSubject) :
    SubjectFields (accessSubjectEquiv.symm value) value := by cases value <;> rfl

private theorem inverse_grant_fields (value : RequestCompositionSdk.veoveo_types.authority.WorkContextGrant) :
    GrantFields (grantEquiv.symm value) value := by
  refine ⟨inverse_subject_fields value.subject, ?_⟩
  change AccessLevelFields (accessLevelEquiv.symm value.level) value.level
  cases value.level <;> simp [AccessLevelFields, accessLevelEquiv]

theorem output_policy_decoded_fields (value : RequestCompositionSdk.veoveo_types.authority.WorkContextOutputPolicy) :
    OutputPolicyFields (outputPolicyToHub value) value := by
  refine ⟨inverse_subject_fields _, inverse_vector_fields grantEquiv GrantFields inverse_grant_fields _, ?_, ?_⟩
  · apply optional_inverse_fields (idEquiv _ _) IdFields
    intro sdk; rfl
  · exact unordered_fields (idEquiv _ _) IdFields id_fields_iff _

theorem authority_decoded_fields (value : RequestCompositionSdk.veoveo_types.authority.InvocationAuthority) :
    AuthorityFields (authorityToHub value) value := by
  refine ⟨rfl, rfl, ?_, rfl, output_policy_decoded_fields _, ?_⟩
  · change MembershipFields (membershipEquiv.symm value.membership) value.membership
    cases value.membership <;> simp [MembershipFields, membershipEquiv]
  · change ProvenanceFields (provenanceEquiv.symm value.provenance) value.provenance
    cases value.provenance <;> simp [ProvenanceFields, provenanceEquiv, IdFields, idEquiv]

private theorem optional_decoded_time_fields (value : Option RequestCompositionSdk.NativeTimestamp) :
    OptionalFields TimestampFields (value.map hubTimestamp) value := by
  cases value with
  | none => trivial
  | some _ => exact ⟨rfl, rfl⟩

/-- SDK output sets retain complete membership without inventing canonical order
for arbitrary SDK lists. Canonical input/output ordering is a separate invariant. -/
theorem principal_decoded_fields (value : RequestCompositionSdk.identity.Principal) :
    PrincipalFields (principalToHub value) value := by
  refine ⟨rfl, ?_, rfl, rfl, ?_, ?_, ?_, ?_, ?_, ?_, ?_, optional_decoded_time_fields _⟩
  · change KindFields (principalKindEquiv.symm value.kind) value.kind
    cases value.kind <;> simp [KindFields, principalKindEquiv]
  · apply optional_inverse_fields (idEquiv _ _) IdFields
    intro sdk; rfl
  · exact unordered_fields (idEquiv _ _) IdFields id_fields_iff _
  · exact unordered_fields groupMembershipEquiv GroupMembershipFields group_membership_fields_iff _
  · exact unordered_fields (idEquiv _ _) IdFields id_fields_iff _
  · exact unordered_fields (idEquiv _ _) IdFields id_fields_iff _
  · exact unordered_fields (idEquiv _ _) IdFields id_fields_iff _
  · exact unordered_fields assuranceEquiv AssuranceFields assurance_fields_iff _

private theorem inverse_ip_fields (value : RequestCompositionSdk.core.net.ip_addr.IpAddr) :
    IpFields (ipEquiv.symm value) value := by cases value <;> rfl

theorem audit_decoded_fields (value : RequestCompositionSdk.veoveo_audit_contract.model.AuditRequest) :
    AuditFields (auditEquiv.symm value) value := by
  refine ⟨rfl, rfl, rfl, optional_inverse_fields ipEquiv IpFields inverse_ip_fields _, ?_⟩
  apply optional_inverse_fields (idEquiv _ _) IdFields
  intro sdk; rfl

theorem access_token_decoded_fields (value : RequestCompositionSdk.identity.AccessTokenSubject) :
    AccessTokenFields (accessTokenToHub value) value := by
  refine ⟨rfl, rfl, rfl, ?_, rfl, rfl, ?_, ?_, ?_,
    unordered_fields (idEquiv _ _) IdFields id_fields_iff _, ?_, ⟨rfl, rfl⟩,
    optional_decoded_time_fields _, ⟨rfl, rfl⟩⟩
  · apply optional_inverse_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
    intro sdk; rfl
  · change ModeFields (invocationModeEquiv.symm value.invocation_mode) value.invocation_mode
    cases value.invocation_mode <;> simp [ModeFields, invocationModeEquiv]
  · apply optional_inverse_fields (idEquiv _ _) IdFields
    intro sdk; rfl
  · apply optional_inverse_fields (idEquiv _ _) IdFields
    intro sdk; rfl
  · apply optional_inverse_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
    intro sdk; rfl

theorem context_decoded_fields (value : RequestCompositionSdk.internal_auth.request.HubRequestContext) :
    ContextFields (contextToHub value) value :=
  ⟨rfl, audit_decoded_fields _, access_token_decoded_fields _, principal_decoded_fields _⟩

private theorem inverse_acting_fields (value : RequestCompositionSdk.internal_auth.ActingServer) :
    ActingFields (actingEquiv.symm value) value := ⟨rfl, rfl, rfl⟩

private theorem inverse_task_fields (value : RequestCompositionSdk.internal_auth.exchange.DelegatingTask) :
    TaskFields (taskEquiv.symm value) value := ⟨rfl, rfl, rfl⟩

theorem delegation_decoded_fields (value : RequestCompositionSdk.internal_auth.exchange.DelegationContext) :
    DelegationFields (delegationToHub value) value :=
  ⟨inverse_vector_fields actingEquiv ActingFields inverse_acting_fields _,
    optional_inverse_fields taskEquiv TaskFields inverse_task_fields _, ⟨rfl, rfl⟩⟩

private theorem optional_plain_fields {H S : Type} (convert : S → H) (fields : H → S → Prop)
    (law : ∀ value, fields (convert value) value) (value : Option S) :
    OptionalFields fields (value.map convert) value := by
  cases value with
  | none => trivial
  | some value => exact law value

theorem request_decoded_fields (value : RequestCompositionSdk.internal_auth.ForwardedRequest) :
    RequestFields (requestToHub value) value := by
  refine ⟨rfl, rfl, rfl, rfl, rfl, principal_decoded_fields _, authority_decoded_fields _,
    unordered_fields (idEquiv _ _) IdFields id_fields_iff _,
    optional_plain_fields contextToHub ContextFields context_decoded_fields _,
    optional_inverse_fields actingEquiv ActingFields inverse_acting_fields _, ?_,
    optional_plain_fields delegationToHub DelegationFields delegation_decoded_fields _, ⟨rfl, rfl⟩⟩
  apply optional_inverse_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
  intro sdk; rfl

/-- Complete successful-issuer-output direction, not just an inverse law. -/
theorem identity_decoded_fields (value : RequestCompositionSdk.internal_auth.HubInternalIdentity) :
    IdentityFields (identityToHub value) value := by
  refine ⟨rfl, rfl, rfl, rfl, rfl, rfl, principal_decoded_fields _, authority_decoded_fields _,
    unordered_fields (idEquiv _ _) IdFields id_fields_iff _,
    optional_plain_fields contextToHub ContextFields context_decoded_fields _,
    optional_inverse_fields actingEquiv ActingFields inverse_acting_fields _, ?_,
    optional_plain_fields delegationToHub DelegationFields delegation_decoded_fields _,
    rfl, ⟨rfl, rfl⟩, ⟨rfl, rfl⟩, ⟨rfl, rfl⟩⟩
  apply optional_inverse_fields (stringEquiv _) (fun hub sdk => hub.value = sdk)
  intro sdk; rfl

theorem issued_decoded_fields (value : RequestCompositionSdk.internal_auth.IssuedHubInternalToken) :
    IssuedFields (issuedToHub value) value := ⟨rfl, identity_decoded_fields _⟩

theorem error_decoded_fields (value : RequestCompositionSdk.internal_auth.InternalTokenError) :
    ErrorFields (errorToHub value) value := by
  have fields := error_fields (errorToHub value)
  simpa only [sdk_error_roundtrip] using fields

abbrev HubRustResult := core.result.Result
  veoveo_hub.veoveo_hub_sdk.internal_auth.IssuedHubInternalToken
  veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError
abbrev SdkRustResult := core.result.Result RequestCompositionSdk.internal_auth.IssuedHubInternalToken
  RequestCompositionSdk.internal_auth.InternalTokenError

def rustResultToHub : SdkRustResult → HubRustResult
  | .Ok value => .Ok (issuedToHub value)
  | .Err error => .Err (errorToHub error)

def rustResultToSdk : HubRustResult → Option SdkRustResult
  | .Ok value => (issuedToSdk value).map core.result.Result.Ok
  | .Err error => some (.Err (errorToSdk error))

def RustResultFields : HubRustResult → SdkRustResult → Prop
  | .Ok hub, .Ok sdk => IssuedFields hub sdk
  | .Err hub, .Err sdk => ErrorFields hub sdk
  | _, _ => False

def ResultPhysical : HubRustResult → Prop
  | .Ok value => IdentityPhysical value.identity
  | .Err _ => True

theorem rust_result_decoded_fields (value : SdkRustResult) :
    RustResultFields (rustResultToHub value) value := by
  cases value with
  | Ok value => exact issued_decoded_fields value
  | Err error => exact error_decoded_fields error

theorem rust_result_exact (hub : HubRustResult) (physical : ResultPhysical hub) :
    ∃ sdk, rustResultToSdk hub = some sdk ∧ RustResultFields hub sdk ∧ rustResultToHub sdk = hub := by
  cases hub with
  | Err error => exact ⟨.Err (errorToSdk error), rfl, error_fields error, congrArg core.result.Result.Err (error_roundtrip error)⟩
  | Ok value =>
    obtain ⟨sdk, converted, fields, roundtrip⟩ := issued_exact value physical
    exact ⟨.Ok sdk, by simp [rustResultToSdk, converted], fields, congrArg core.result.Result.Ok roundtrip⟩

/-- Same Aeneas effects/failures/divergence; only returned Rust values are mapped. -/
def nativeResultToHub (value : Result SdkRustResult) : Result HubRustResult :=
  Aeneas.Std.bind value (fun result => .ok (rustResultToHub result))

theorem native_result_ok (value : SdkRustResult) :
    nativeResultToHub (.ok value) = .ok (rustResultToHub value) := by simp [nativeResultToHub]

theorem native_result_fail (error : Error) :
    nativeResultToHub (.fail error) = .fail error := by simp [nativeResultToHub]

theorem native_result_div : nativeResultToHub .div = .div := by simp [nativeResultToHub]

theorem native_result_vis (effect : RustEffect.Input) (continuation : RustEffect.Output effect → Result SdkRustResult) :
    nativeResultToHub (.vis effect continuation) = .vis effect (fun output => nativeResultToHub (continuation output)) := by
  simp [nativeResultToHub]

#print axioms request_exact
#print axioms identity_exact
#print axioms issued_decoded_fields
#print axioms error_fields
#print axioms error_decoded_fields
#print axioms rust_result_exact
#print axioms rust_result_decoded_fields
#print axioms native_result_fail
#print axioms native_result_div
#print axioms native_result_vis
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SdkInternalAuthBridge.authority_fields, ``SdkInternalAuthBridge.authority_roundtrip,
      ``SdkInternalAuthBridge.principal_exact, ``SdkInternalAuthBridge.access_token_exact,
      ``SdkInternalAuthBridge.context_exact, ``SdkInternalAuthBridge.delegation_exact,
      ``SdkInternalAuthBridge.request_exact, ``SdkInternalAuthBridge.identity_exact,
      ``SdkInternalAuthBridge.issuer_fields, ``SdkInternalAuthBridge.issued_exact,
      ``SdkInternalAuthBridge.request_decoded_fields, ``SdkInternalAuthBridge.identity_decoded_fields,
      ``SdkInternalAuthBridge.issued_decoded_fields, ``SdkInternalAuthBridge.error_fields,
      ``SdkInternalAuthBridge.error_roundtrip, ``SdkInternalAuthBridge.sdk_error_roundtrip,
      ``SdkInternalAuthBridge.error_decoded_fields, ``SdkInternalAuthBridge.rust_result_exact,
      ``SdkInternalAuthBridge.rust_result_decoded_fields, ``SdkInternalAuthBridge.native_result_ok,
      ``SdkInternalAuthBridge.native_result_fail, ``SdkInternalAuthBridge.native_result_div,
      ``SdkInternalAuthBridge.native_result_vis] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed full SDK value/field bridge in {declaration}: {found}"
end
end SdkInternalAuthBridge
