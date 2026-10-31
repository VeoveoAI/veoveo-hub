import SdkInternalAuthBridgeValues
import Mathlib.Order.Prod.Lex.Basic

open Aeneas Aeneas.Std

namespace SdkInternalAuthBridge
noncomputable section

-- Concrete carried records. This module changes no operational issuer body.
-- Every tagged field is mapped by its native field domain, retaining its value.

def groupRoleEquiv : veoveo_hub.veoveo_hub_sdk.identity.GroupRole ≃
    RequestCompositionSdk.identity.GroupRole where
  toFun value := match value with
    | .Read => .Read
    | .Write => .Write
    | .Admin => .Admin
  invFun value := match value with
    | .Read => .Read
    | .Write => .Write
    | .Admin => .Admin
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def assuranceEquiv : veoveo_hub.veoveo_hub_sdk.identity.PrincipalAssurance ≃
    RequestCompositionSdk.identity.PrincipalAssurance where
  toFun value := match value with
    | .UsPerson => .UsPerson
  invFun value := match value with
    | .UsPerson => .UsPerson
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def principalKindEquiv : veoveo_hub.veoveo_hub_sdk.identity.PrincipalKind ≃
    RequestCompositionSdk.identity.PrincipalKind where
  toFun value := match value with
    | .User => .User
    | .Service => .Service
  invFun value := match value with
    | .User => .User
    | .Service => .Service
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def invocationModeEquiv : veoveo_hub.veoveo_types.provenance.InvocationMode ≃
    RequestCompositionSdk.veoveo_types.provenance.InvocationMode where
  toFun value := match value with
    | .Direct => .Direct
    | .Delegated => .Delegated
    | .Automated => .Automated
  invFun value := match value with
    | .Direct => .Direct
    | .Delegated => .Delegated
    | .Automated => .Automated
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def accessLevelEquiv : veoveo_hub.veoveo_types.authority.AccessLevel ≃
    RequestCompositionSdk.veoveo_types.authority.AccessLevel where
  toFun value := match value with
    | .Read => .Read
    | .Write => .Write
    | .Admin => .Admin
  invFun value := match value with
    | .Read => .Read
    | .Write => .Write
    | .Admin => .Admin
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def membershipEquiv : veoveo_hub.veoveo_types.authority.WorkContextMembershipLevel ≃
    RequestCompositionSdk.veoveo_types.authority.WorkContextMembershipLevel where
  toFun value := match value with
    | .Viewer => .Viewer
    | .Contributor => .Contributor
    | .Custodian => .Custodian
    | .Owner => .Owner
  invFun value := match value with
    | .Viewer => .Viewer
    | .Contributor => .Contributor
    | .Custodian => .Custodian
    | .Owner => .Owner
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def algorithmEquiv : veoveo_hub.jsonwebtoken.algorithms.Algorithm ≃
    RequestCompositionSdk.jsonwebtoken.algorithms.Algorithm where
  toFun value := match value with
    | .HS256 => .HS256
    | .HS384 => .HS384
    | .HS512 => .HS512
    | .ES256 => .ES256
    | .ES384 => .ES384
    | .RS256 => .RS256
    | .RS384 => .RS384
    | .RS512 => .RS512
    | .PS256 => .PS256
    | .PS384 => .PS384
    | .PS512 => .PS512
    | .EdDSA => .EdDSA
  invFun value := match value with
    | .HS256 => .HS256
    | .HS384 => .HS384
    | .HS512 => .HS512
    | .ES256 => .ES256
    | .ES384 => .ES384
    | .RS256 => .RS256
    | .RS384 => .RS384
    | .RS512 => .RS512
    | .PS256 => .PS256
    | .PS384 => .PS384
    | .PS512 => .PS512
    | .EdDSA => .EdDSA
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def groupMembershipEquiv : veoveo_hub.veoveo_hub_sdk.identity.GroupMembership ≃
    RequestCompositionSdk.identity.GroupMembership where
  toFun value := {group := idEquiv _ _ value.group, role := groupRoleEquiv value.role}
  invFun value := {group := (idEquiv _ _).symm value.group, role := groupRoleEquiv.symm value.role}
  left_inv value := by cases value; simp
  right_inv value := by cases value; simp

def groupRoleRank : RequestCompositionSdk.identity.GroupRole → Nat
  | .Read => 0 | .Write => 1 | .Admin => 2

private theorem groupRoleRank_injective : Function.Injective groupRoleRank := by
  intro left right same
  cases left <;> cases right <;> simp_all [groupRoleRank]

instance sdkGroupMembershipOrder : LinearOrder RequestCompositionSdk.identity.GroupMembership :=
  LinearOrder.lift' (fun value => toLex (value.group.value, groupRoleRank value.role)) (by
    intro left right same
    have fields := congrArg ofLex same
    have groups := congrArg Prod.fst fields
    have roles := groupRoleRank_injective (congrArg Prod.snd fields)
    cases left with
    | mk leftGroup leftRole =>
      cases right with
      | mk rightGroup rightRole =>
        cases leftGroup; cases rightGroup
        cases groups; cases roles; rfl)

instance sdkAssuranceOrder : LinearOrder RequestCompositionSdk.identity.PrincipalAssurance :=
  LinearOrder.lift' (fun _ => (0 : Nat)) (by intro left right _; cases left; cases right; rfl)

/-- Vectors preserve their complete order and length, including output initial
 grants and inherited actor chains. They are never converted through a set. -/
def vectorEquiv {H S : Type} (codec : H ≃ S) : alloc.vec.Vec H ≃ alloc.vec.Vec S where
  toFun value := alloc.vec.Vec.from (value.val.map codec) (by simpa using value.property)
  invFun value := alloc.vec.Vec.from (value.val.map codec.symm) (by simpa using value.property)
  left_inv value := by apply alloc.vec.Vec.ext; simp
  right_inv value := by apply alloc.vec.Vec.ext; simp

def accessSubjectEquiv : veoveo_hub.veoveo_types.identity.AccessSubject ≃
    RequestCompositionSdk.veoveo_types.identity.AccessSubject where
  toFun value := match value with
    | .Principal principal => .Principal (idEquiv _ _ principal)
    | .Group group => .Group (idEquiv _ _ group)
  invFun value := match value with
    | .Principal principal => .Principal ((idEquiv _ _).symm principal)
    | .Group group => .Group ((idEquiv _ _).symm group)
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def provenanceEquiv : veoveo_hub.veoveo_types.provenance.InvocationProvenance ≃
    RequestCompositionSdk.veoveo_types.provenance.InvocationProvenance where
  toFun value := match value with
    | .Direct initiator => .Direct (idEquiv _ _ initiator)
    | .Delegated initiator delegation => .Delegated (idEquiv _ _ initiator) (idEquiv _ _ delegation)
    | .Automated => .Automated
  invFun value := match value with
    | .Direct initiator => .Direct ((idEquiv _ _).symm initiator)
    | .Delegated initiator delegation => .Delegated ((idEquiv _ _).symm initiator) ((idEquiv _ _).symm delegation)
    | .Automated => .Automated
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def grantEquiv : veoveo_hub.veoveo_types.authority.WorkContextGrant ≃
    RequestCompositionSdk.veoveo_types.authority.WorkContextGrant where
  toFun value := {subject := accessSubjectEquiv value.subject, level := accessLevelEquiv value.level}
  invFun value := {subject := accessSubjectEquiv.symm value.subject, level := accessLevelEquiv.symm value.level}
  left_inv value := by cases value; simp
  right_inv value := by cases value; simp

def outputPolicyToSdk (value : veoveo_hub.veoveo_types.authority.WorkContextOutputPolicy) :
    RequestCompositionSdk.veoveo_types.authority.WorkContextOutputPolicy := {
  owner := accessSubjectEquiv value.owner
  initial_grants := vectorEquiv grantEquiv value.initial_grants
  classification := value.classification.map (idEquiv _ _)
  data_labels := orderedSet (idEquiv _ _) value.data_labels }

def outputPolicyToHub (value : RequestCompositionSdk.veoveo_types.authority.WorkContextOutputPolicy) :
    veoveo_hub.veoveo_types.authority.WorkContextOutputPolicy := {
  owner := accessSubjectEquiv.symm value.owner
  initial_grants := (vectorEquiv grantEquiv).symm value.initial_grants
  classification := value.classification.map (idEquiv _ _).symm
  data_labels := unorderedSet (idEquiv _ _) value.data_labels }

def authorityToSdk (value : veoveo_hub.veoveo_types.authority.InvocationAuthority) :
    RequestCompositionSdk.veoveo_types.authority.InvocationAuthority := {
  work_context := idEquiv _ _ value.work_context
  tenant := idEquiv _ _ value.tenant
  membership := membershipEquiv value.membership
  policy_revision := idEquiv _ _ value.policy_revision
  output_policy := outputPolicyToSdk value.output_policy
  provenance := provenanceEquiv value.provenance }

def authorityToHub (value : RequestCompositionSdk.veoveo_types.authority.InvocationAuthority) :
    veoveo_hub.veoveo_types.authority.InvocationAuthority := {
  work_context := (idEquiv _ _).symm value.work_context
  tenant := (idEquiv _ _).symm value.tenant
  membership := membershipEquiv.symm value.membership
  policy_revision := (idEquiv _ _).symm value.policy_revision
  output_policy := outputPolicyToHub value.output_policy
  provenance := provenanceEquiv.symm value.provenance }

/-- Optional carried values retain presence/absence exactly; only a physically
unrepresentable timestamp can make a record conversion unavailable. -/
def optional {H S : Type} (convert : H → Option S) : Option H → Option (Option S)
  | none => some none
  | some value => (convert value).map some

def principalToSdk (value : veoveo_hub.veoveo_hub_sdk.identity.Principal) :
    Option RequestCompositionSdk.identity.Principal := do
  let authenticated ← optional timestamp? value.authenticated_at
  pure {
    id := idEquiv _ _ value.id
    kind := principalKindEquiv value.kind
    issuer := idEquiv _ _ value.issuer
    subject := idEquiv _ _ value.subject
    tenant := value.tenant.map (idEquiv _ _)
    groups := orderedSet (idEquiv _ _) value.groups
    group_roles := orderedSet groupMembershipEquiv value.group_roles
    roles := orderedSet (idEquiv _ _) value.roles
    scopes := orderedSet (idEquiv _ _) value.scopes
    data_labels := orderedSet (idEquiv _ _) value.data_labels
    assurances := orderedSet assuranceEquiv value.assurances
    authenticated_at := authenticated }

def principalToHub (value : RequestCompositionSdk.identity.Principal) :
    veoveo_hub.veoveo_hub_sdk.identity.Principal := {
  id := (idEquiv _ _).symm value.id
  kind := principalKindEquiv.symm value.kind
  issuer := (idEquiv _ _).symm value.issuer
  subject := (idEquiv _ _).symm value.subject
  tenant := value.tenant.map (idEquiv _ _).symm
  groups := unorderedSet (idEquiv _ _) value.groups
  group_roles := unorderedSet groupMembershipEquiv value.group_roles
  roles := unorderedSet (idEquiv _ _) value.roles
  scopes := unorderedSet (idEquiv _ _) value.scopes
  data_labels := unorderedSet (idEquiv _ _) value.data_labels
  assurances := unorderedSet assuranceEquiv value.assurances
  authenticated_at := value.authenticated_at.map hubTimestamp }

def ipEquiv : veoveo_hub.core.net.ip_addr.IpAddr ≃ RequestCompositionSdk.core.net.ip_addr.IpAddr where
  toFun value := match value with
    | .V4 address => .V4 (idEquiv _ _ address)
    | .V6 address => .V6 (idEquiv _ _ address)
  invFun value := match value with
    | .V4 address => .V4 ((idEquiv _ _).symm address)
    | .V6 address => .V6 ((idEquiv _ _).symm address)
  left_inv value := by cases value <;> rfl
  right_inv value := by cases value <;> rfl

def executionEquiv : veoveo_hub.veoveo_hub_sdk.execution.ExecutionContext ≃
    RequestCompositionSdk.execution.ExecutionContext where
  toFun value := {composition := idEquiv _ _ value.composition}
  invFun value := {composition := (idEquiv _ _).symm value.composition}
  left_inv value := by cases value; rfl
  right_inv value := by cases value; rfl

def auditEquiv : veoveo_hub.veoveo_audit_contract.model.AuditRequest ≃
    RequestCompositionSdk.veoveo_audit_contract.model.AuditRequest where
  toFun value := {
    id := idEquiv _ _ value.id
    trace_id := idEquiv _ _ value.trace_id
    span_id := idEquiv _ _ value.span_id
    source_ip := value.source_ip.map ipEquiv
    task := value.task.map (idEquiv _ _) }
  invFun value := {
    id := (idEquiv _ _).symm value.id
    trace_id := (idEquiv _ _).symm value.trace_id
    span_id := (idEquiv _ _).symm value.span_id
    source_ip := value.source_ip.map ipEquiv.symm
    task := value.task.map (idEquiv _ _).symm }
  left_inv value := by cases value; simp
  right_inv value := by cases value; simp

def accessTokenToSdk (value : veoveo_hub.veoveo_hub_sdk.identity.AccessTokenSubject) :
    Option RequestCompositionSdk.identity.AccessTokenSubject := do
  let issued ← timestamp? value.issued_at
  let notBefore ← optional timestamp? value.not_before
  let expires ← timestamp? value.expires_at
  pure {
    issuer := idEquiv _ _ value.issuer
    subject := idEquiv _ _ value.subject
    oauth_client_id := idEquiv _ _ value.oauth_client_id
    session_family := value.session_family.map (stringEquiv _)
    audience := stringEquiv _ value.audience
    work_context := idEquiv _ _ value.work_context
    invocation_mode := invocationModeEquiv value.invocation_mode
    initiator := value.initiator.map (idEquiv _ _)
    delegation_id := value.delegation_id.map (idEquiv _ _)
    scopes := orderedSet (idEquiv _ _) value.scopes
    jwt_id := value.jwt_id.map (stringEquiv _)
    issued_at := issued
    not_before := notBefore
    expires_at := expires }

def accessTokenToHub (value : RequestCompositionSdk.identity.AccessTokenSubject) :
    veoveo_hub.veoveo_hub_sdk.identity.AccessTokenSubject := {
  issuer := (idEquiv _ _).symm value.issuer
  subject := (idEquiv _ _).symm value.subject
  oauth_client_id := (idEquiv _ _).symm value.oauth_client_id
  session_family := value.session_family.map (stringEquiv _).symm
  audience := (stringEquiv _).symm value.audience
  work_context := (idEquiv _ _).symm value.work_context
  invocation_mode := invocationModeEquiv.symm value.invocation_mode
  initiator := value.initiator.map (idEquiv _ _).symm
  delegation_id := value.delegation_id.map (idEquiv _ _).symm
  scopes := unorderedSet (idEquiv _ _) value.scopes
  jwt_id := value.jwt_id.map (stringEquiv _).symm
  issued_at := hubTimestamp value.issued_at
  not_before := value.not_before.map hubTimestamp
  expires_at := hubTimestamp value.expires_at }

def contextToSdk (value : veoveo_hub.veoveo_hub_sdk.internal_auth.request.HubRequestContext) :
    Option RequestCompositionSdk.internal_auth.request.HubRequestContext := do
  let token ← accessTokenToSdk value.access_token
  let principal ← principalToSdk value.principal
  pure {
    execution := executionEquiv value.execution
    audit := auditEquiv value.audit
    access_token := token
    principal := principal }

def contextToHub (value : RequestCompositionSdk.internal_auth.request.HubRequestContext) :
    veoveo_hub.veoveo_hub_sdk.internal_auth.request.HubRequestContext := {
  execution := executionEquiv.symm value.execution
  audit := auditEquiv.symm value.audit
  access_token := accessTokenToHub value.access_token
  principal := principalToHub value.principal }

def actingEquiv : veoveo_hub.veoveo_hub_sdk.internal_auth.ActingServer ≃
    RequestCompositionSdk.internal_auth.ActingServer where
  toFun value := {
    server := idEquiv _ _ value.server
    deployment := idEquiv _ _ value.deployment
    client := idEquiv _ _ value.client }
  invFun value := {
    server := (idEquiv _ _).symm value.server
    deployment := (idEquiv _ _).symm value.deployment
    client := (idEquiv _ _).symm value.client }
  left_inv value := by cases value; rfl
  right_inv value := by cases value; rfl

def taskEquiv : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegatingTask ≃
    RequestCompositionSdk.internal_auth.exchange.DelegatingTask where
  toFun value := {
    server := idEquiv _ _ value.server
    deployment := idEquiv _ _ value.deployment
    task := stringEquiv _ value.task }
  invFun value := {
    server := (idEquiv _ _).symm value.server
    deployment := (idEquiv _ _).symm value.deployment
    task := (stringEquiv _).symm value.task }
  left_inv value := by cases value; rfl
  right_inv value := by cases value; rfl

def delegationToSdk (value : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegationContext) :
    Option RequestCompositionSdk.internal_auth.exchange.DelegationContext := do
  let expires ← timestamp? value.expires_at
  pure {
    actors := vectorEquiv actingEquiv value.actors
    task := value.task.map taskEquiv
    expires_at := expires }

def delegationToHub (value : RequestCompositionSdk.internal_auth.exchange.DelegationContext) :
    veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegationContext := {
  actors := (vectorEquiv actingEquiv).symm value.actors
  task := value.task.map taskEquiv.symm
  expires_at := hubTimestamp value.expires_at }

def requestToSdk (value : veoveo_hub.veoveo_hub_sdk.internal_auth.ForwardedRequest) :
    Option RequestCompositionSdk.internal_auth.ForwardedRequest := do
  let actor ← principalToSdk value.actor
  let context ← optional contextToSdk value.request_context
  let delegation ← optional delegationToSdk value.delegation_context
  let expires ← timestamp? value.expires_at
  pure {
    profile := stringEquiv _ value.profile
    server := idEquiv _ _ value.server
    deployment := idEquiv _ _ value.deployment
    audience := idEquiv _ _ value.audience
    client := idEquiv _ _ value.client
    actor := actor
    authority := authorityToSdk value.authority
    permissions := orderedSet (idEquiv _ _) value.permissions
    request_context := context
    acting_server := value.acting_server.map actingEquiv
    acting_task := value.acting_task.map (stringEquiv _)
    delegation_context := delegation
    expires_at := expires }

def requestToHub (value : RequestCompositionSdk.internal_auth.ForwardedRequest) :
    veoveo_hub.veoveo_hub_sdk.internal_auth.ForwardedRequest := {
  profile := (stringEquiv _).symm value.profile
  server := (idEquiv _ _).symm value.server
  deployment := (idEquiv _ _).symm value.deployment
  audience := (idEquiv _ _).symm value.audience
  client := (idEquiv _ _).symm value.client
  actor := principalToHub value.actor
  authority := authorityToHub value.authority
  permissions := unorderedSet (idEquiv _ _) value.permissions
  request_context := value.request_context.map contextToHub
  acting_server := value.acting_server.map actingEquiv.symm
  acting_task := value.acting_task.map (stringEquiv _).symm
  delegation_context := value.delegation_context.map delegationToHub
  expires_at := hubTimestamp value.expires_at }

def identityToSdk (value : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalIdentity) :
    Option RequestCompositionSdk.internal_auth.HubInternalIdentity := do
  let actor ← principalToSdk value.actor
  let context ← optional contextToSdk value.request_context
  let delegation ← optional delegationToSdk value.delegation_context
  let issued ← timestamp? value.issued_at
  let notBefore ← timestamp? value.not_before
  let expires ← timestamp? value.expires_at
  pure {
    issuer := idEquiv _ _ value.issuer
    profile := stringEquiv _ value.profile
    server := idEquiv _ _ value.server
    deployment := idEquiv _ _ value.deployment
    audience := idEquiv _ _ value.audience
    client := idEquiv _ _ value.client
    actor := actor
    authority := authorityToSdk value.authority
    permissions := orderedSet (idEquiv _ _) value.permissions
    request_context := context
    acting_server := value.acting_server.map actingEquiv
    acting_task := value.acting_task.map (stringEquiv _)
    delegation_context := delegation
    jwt_id := stringEquiv _ value.jwt_id
    issued_at := issued
    not_before := notBefore
    expires_at := expires }

def identityToHub (value : RequestCompositionSdk.internal_auth.HubInternalIdentity) :
    veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalIdentity := {
  issuer := (idEquiv _ _).symm value.issuer
  profile := (stringEquiv _).symm value.profile
  server := (idEquiv _ _).symm value.server
  deployment := (idEquiv _ _).symm value.deployment
  audience := (idEquiv _ _).symm value.audience
  client := (idEquiv _ _).symm value.client
  actor := principalToHub value.actor
  authority := authorityToHub value.authority
  permissions := unorderedSet (idEquiv _ _) value.permissions
  request_context := value.request_context.map contextToHub
  acting_server := value.acting_server.map actingEquiv.symm
  acting_task := value.acting_task.map (stringEquiv _).symm
  delegation_context := value.delegation_context.map delegationToHub
  jwt_id := (stringEquiv _).symm value.jwt_id
  issued_at := hubTimestamp value.issued_at
  not_before := hubTimestamp value.not_before
  expires_at := hubTimestamp value.expires_at }

def issuerEquiv : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer ≃
    RequestCompositionSdk.internal_auth.HubInternalTokenIssuer where
  toFun value := {issuer := idEquiv _ _ value.issuer, signing_key := opaqueEquiv _ _ value.signing_key}
  invFun value := {issuer := (idEquiv _ _).symm value.issuer, signing_key := (opaqueEquiv _ _).symm value.signing_key}
  left_inv value := by cases value; rfl
  right_inv value := by cases value; rfl

def errorToSdk : veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError →
    RequestCompositionSdk.internal_auth.InternalTokenError
  | .DeploymentMismatch => .DeploymentMismatch
  | .EmptyPrivateKey => .EmptyPrivateKey
  | .InvalidSigningKey => .InvalidSigningKey
  | .InvalidIssuer => .InvalidIssuer
  | .KeyIdNotThumbprint text => .KeyIdNotThumbprint text
  | .InvalidTokenId => .InvalidTokenId
  | .InvalidKeyId => .InvalidKeyId
  | .EmptyTrustBundle => .EmptyTrustBundle
  | .MissingKeyId => .MissingKeyId
  | .DuplicateKeyId text => .DuplicateKeyId text
  | .UnknownKeyId text => .UnknownKeyId text
  | .UnsupportedTrustKey => .UnsupportedTrustKey
  | .UnsupportedTokenAlgorithm algorithm => .UnsupportedTokenAlgorithm (algorithmEquiv algorithm)
  | .UnsupportedTokenType => .UnsupportedTokenType
  | .ExpiredDelegation => .ExpiredDelegation
  | .DelegationDepthExceeded => .DelegationDepthExceeded
  | .AudienceMismatch expected actual => .AudienceMismatch (idEquiv _ _ expected) (idEquiv _ _ actual)
  | .SubjectPrincipalMismatch => .SubjectPrincipalMismatch
  | .InconsistentClaims => .InconsistentClaims
  | .PermissionOutsideAudience permission => .PermissionOutsideAudience (idEquiv _ _ permission)
  | .InvalidRequestContext => .InvalidRequestContext
  | .InvalidTimestamp field seconds => .InvalidTimestamp field seconds
  | .Identifier error => .Identifier (opaqueEquiv _ _ error)
  | .TrustBundleJson error => .TrustBundleJson (opaqueEquiv _ _ error)
  | .Jwt error => .Jwt (opaqueEquiv _ _ error)

def errorToHub : RequestCompositionSdk.internal_auth.InternalTokenError →
    veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError
  | .DeploymentMismatch => .DeploymentMismatch
  | .EmptyPrivateKey => .EmptyPrivateKey
  | .InvalidSigningKey => .InvalidSigningKey
  | .InvalidIssuer => .InvalidIssuer
  | .KeyIdNotThumbprint text => .KeyIdNotThumbprint text
  | .InvalidTokenId => .InvalidTokenId
  | .InvalidKeyId => .InvalidKeyId
  | .EmptyTrustBundle => .EmptyTrustBundle
  | .MissingKeyId => .MissingKeyId
  | .DuplicateKeyId text => .DuplicateKeyId text
  | .UnknownKeyId text => .UnknownKeyId text
  | .UnsupportedTrustKey => .UnsupportedTrustKey
  | .UnsupportedTokenAlgorithm algorithm => .UnsupportedTokenAlgorithm (algorithmEquiv.symm algorithm)
  | .UnsupportedTokenType => .UnsupportedTokenType
  | .ExpiredDelegation => .ExpiredDelegation
  | .DelegationDepthExceeded => .DelegationDepthExceeded
  | .AudienceMismatch expected actual => .AudienceMismatch ((idEquiv _ _).symm expected) ((idEquiv _ _).symm actual)
  | .SubjectPrincipalMismatch => .SubjectPrincipalMismatch
  | .InconsistentClaims => .InconsistentClaims
  | .PermissionOutsideAudience permission => .PermissionOutsideAudience ((idEquiv _ _).symm permission)
  | .InvalidRequestContext => .InvalidRequestContext
  | .InvalidTimestamp field seconds => .InvalidTimestamp field seconds
  | .Identifier error => .Identifier ((opaqueEquiv _ _).symm error)
  | .TrustBundleJson error => .TrustBundleJson ((opaqueEquiv _ _).symm error)
  | .Jwt error => .Jwt ((opaqueEquiv _ _).symm error)

def issuedToSdk (value : veoveo_hub.veoveo_hub_sdk.internal_auth.IssuedHubInternalToken) :
    Option RequestCompositionSdk.internal_auth.IssuedHubInternalToken := do
  let token ← identityToSdk value.identity
  pure {bearer_token := value.bearer_token, identity := token}

def issuedToHub (value : RequestCompositionSdk.internal_auth.IssuedHubInternalToken) :
    veoveo_hub.veoveo_hub_sdk.internal_auth.IssuedHubInternalToken := {
  bearer_token := value.bearer_token, identity := identityToHub value.identity }

end
end SdkInternalAuthBridge
