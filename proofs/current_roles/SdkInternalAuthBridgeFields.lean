import SdkInternalAuthBridgeRecords

open Aeneas Aeneas.Std

namespace SdkInternalAuthBridge
noncomputable section

/-- Independent same-field relations: they prevent a pair of mutually inverse
adapters from silently exchanging fields or enum meanings. -/
def IdFields {hubTag sdkTag : Nat} (hub : CurrentRolesId hubTag)
    (sdk : RequestCompositionSdk.NativeId sdkTag) : Prop := hub.value = sdk.value

def OptionalFields {H S : Type} (fields : H → S → Prop) : Option H → Option S → Prop
  | none, none => True
  | some hub, some sdk => fields hub sdk
  | _, _ => False

def TimestampFields (hub : HubTime) (sdk : RequestCompositionSdk.NativeTimestamp) : Prop :=
  hub.1 = sdk.seconds.val ∧ hub.2 = sdk.nanoseconds.val

/-- Membership is extensional; traversal normalization is stated separately. -/
def SetFields {H S : Type} (fields : H → S → Prop) (hub : Finset H)
    (sdk : RequestCompositionSdk.OrderedSet S) : Prop :=
  ∀ entry, entry ∈ sdk.values ↔ ∃ original ∈ hub, fields original entry

def GroupRoleFields : veoveo_hub.veoveo_hub_sdk.identity.GroupRole →
    RequestCompositionSdk.identity.GroupRole → Prop
  | .Read, .Read | .Write, .Write | .Admin, .Admin => True
  | _, _ => False

def KindFields : veoveo_hub.veoveo_hub_sdk.identity.PrincipalKind →
    RequestCompositionSdk.identity.PrincipalKind → Prop
  | .User, .User | .Service, .Service => True
  | _, _ => False

def AssuranceFields : veoveo_hub.veoveo_hub_sdk.identity.PrincipalAssurance →
    RequestCompositionSdk.identity.PrincipalAssurance → Prop
  | .UsPerson, .UsPerson => True

def ModeFields : veoveo_hub.veoveo_types.provenance.InvocationMode →
    RequestCompositionSdk.veoveo_types.provenance.InvocationMode → Prop
  | .Direct, .Direct | .Delegated, .Delegated | .Automated, .Automated => True
  | _, _ => False

def MembershipFields : veoveo_hub.veoveo_types.authority.WorkContextMembershipLevel →
    RequestCompositionSdk.veoveo_types.authority.WorkContextMembershipLevel → Prop
  | .Viewer, .Viewer | .Contributor, .Contributor | .Custodian, .Custodian | .Owner, .Owner => True
  | _, _ => False

def AccessLevelFields : veoveo_hub.veoveo_types.authority.AccessLevel →
    RequestCompositionSdk.veoveo_types.authority.AccessLevel → Prop
  | .Read, .Read | .Write, .Write | .Admin, .Admin => True
  | _, _ => False

def SubjectFields : veoveo_hub.veoveo_types.identity.AccessSubject →
    RequestCompositionSdk.veoveo_types.identity.AccessSubject → Prop
  | .Principal hub, .Principal sdk => IdFields hub sdk
  | .Group hub, .Group sdk => IdFields hub sdk
  | _, _ => False

def ProvenanceFields : veoveo_hub.veoveo_types.provenance.InvocationProvenance →
    RequestCompositionSdk.veoveo_types.provenance.InvocationProvenance → Prop
  | .Direct hub, .Direct sdk => IdFields hub sdk
  | .Delegated hubInitiator hubDelegation, .Delegated sdkInitiator sdkDelegation =>
      IdFields hubInitiator sdkInitiator ∧ IdFields hubDelegation sdkDelegation
  | .Automated, .Automated => True
  | _, _ => False

structure GroupMembershipFields (hub : veoveo_hub.veoveo_hub_sdk.identity.GroupMembership)
    (sdk : RequestCompositionSdk.identity.GroupMembership) : Prop where
  group : IdFields hub.group sdk.group
  role : GroupRoleFields hub.role sdk.role

structure GrantFields (hub : veoveo_hub.veoveo_types.authority.WorkContextGrant)
    (sdk : RequestCompositionSdk.veoveo_types.authority.WorkContextGrant) : Prop where
  subject : SubjectFields hub.subject sdk.subject
  level : AccessLevelFields hub.level sdk.level

structure OutputPolicyFields (hub : veoveo_hub.veoveo_types.authority.WorkContextOutputPolicy)
    (sdk : RequestCompositionSdk.veoveo_types.authority.WorkContextOutputPolicy) : Prop where
  owner : SubjectFields hub.owner sdk.owner
  initial_grants : List.Forall₂ GrantFields hub.initial_grants.val sdk.initial_grants.val
  classification : OptionalFields IdFields hub.classification sdk.classification
  data_labels : SetFields IdFields hub.data_labels sdk.data_labels

structure AuthorityFields (hub : veoveo_hub.veoveo_types.authority.InvocationAuthority)
    (sdk : RequestCompositionSdk.veoveo_types.authority.InvocationAuthority) : Prop where
  work_context : IdFields hub.work_context sdk.work_context
  tenant : IdFields hub.tenant sdk.tenant
  membership : MembershipFields hub.membership sdk.membership
  policy_revision : IdFields hub.policy_revision sdk.policy_revision
  output_policy : OutputPolicyFields hub.output_policy sdk.output_policy
  provenance : ProvenanceFields hub.provenance sdk.provenance

structure PrincipalFields (hub : veoveo_hub.veoveo_hub_sdk.identity.Principal)
    (sdk : RequestCompositionSdk.identity.Principal) : Prop where
  id : IdFields hub.id sdk.id
  kind : KindFields hub.kind sdk.kind
  issuer : IdFields hub.issuer sdk.issuer
  subject : IdFields hub.subject sdk.subject
  tenant : OptionalFields IdFields hub.tenant sdk.tenant
  groups : SetFields IdFields hub.groups sdk.groups
  group_roles : SetFields GroupMembershipFields hub.group_roles sdk.group_roles
  roles : SetFields IdFields hub.roles sdk.roles
  scopes : SetFields IdFields hub.scopes sdk.scopes
  data_labels : SetFields IdFields hub.data_labels sdk.data_labels
  assurances : SetFields AssuranceFields hub.assurances sdk.assurances
  authenticated_at : OptionalFields TimestampFields hub.authenticated_at sdk.authenticated_at

def IpFields : veoveo_hub.core.net.ip_addr.IpAddr → RequestCompositionSdk.core.net.ip_addr.IpAddr → Prop
  | .V4 hub, .V4 sdk => IdFields hub sdk
  | .V6 hub, .V6 sdk => IdFields hub sdk
  | _, _ => False

structure AuditFields (hub : veoveo_hub.veoveo_audit_contract.model.AuditRequest)
    (sdk : RequestCompositionSdk.veoveo_audit_contract.model.AuditRequest) : Prop where
  id : IdFields hub.id sdk.id
  trace_id : IdFields hub.trace_id sdk.trace_id
  span_id : IdFields hub.span_id sdk.span_id
  source_ip : OptionalFields IpFields hub.source_ip sdk.source_ip
  task : OptionalFields IdFields hub.task sdk.task

structure AccessTokenFields (hub : veoveo_hub.veoveo_hub_sdk.identity.AccessTokenSubject)
    (sdk : RequestCompositionSdk.identity.AccessTokenSubject) : Prop where
  issuer : IdFields hub.issuer sdk.issuer
  subject : IdFields hub.subject sdk.subject
  oauth_client_id : IdFields hub.oauth_client_id sdk.oauth_client_id
  session_family : OptionalFields (fun hub sdk => hub.value = sdk) hub.session_family sdk.session_family
  audience : hub.audience.value = sdk.audience
  work_context : IdFields hub.work_context sdk.work_context
  invocation_mode : ModeFields hub.invocation_mode sdk.invocation_mode
  initiator : OptionalFields IdFields hub.initiator sdk.initiator
  delegation_id : OptionalFields IdFields hub.delegation_id sdk.delegation_id
  scopes : SetFields IdFields hub.scopes sdk.scopes
  jwt_id : OptionalFields (fun hub sdk => hub.value = sdk) hub.jwt_id sdk.jwt_id
  issued_at : TimestampFields hub.issued_at sdk.issued_at
  not_before : OptionalFields TimestampFields hub.not_before sdk.not_before
  expires_at : TimestampFields hub.expires_at sdk.expires_at

structure ContextFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.request.HubRequestContext)
    (sdk : RequestCompositionSdk.internal_auth.request.HubRequestContext) : Prop where
  composition : hub.execution.composition.value = sdk.execution.composition.value
  audit : AuditFields hub.audit sdk.audit
  access_token : AccessTokenFields hub.access_token sdk.access_token
  principal : PrincipalFields hub.principal sdk.principal

structure ActingFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.ActingServer)
    (sdk : RequestCompositionSdk.internal_auth.ActingServer) : Prop where
  server : IdFields hub.server sdk.server
  deployment : IdFields hub.deployment sdk.deployment
  client : IdFields hub.client sdk.client

structure TaskFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegatingTask)
    (sdk : RequestCompositionSdk.internal_auth.exchange.DelegatingTask) : Prop where
  server : IdFields hub.server sdk.server
  deployment : IdFields hub.deployment sdk.deployment
  task : hub.task.value = sdk.task

structure DelegationFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.exchange.DelegationContext)
    (sdk : RequestCompositionSdk.internal_auth.exchange.DelegationContext) : Prop where
  actors : List.Forall₂ ActingFields hub.actors.val sdk.actors.val
  task : OptionalFields TaskFields hub.task sdk.task
  expires_at : TimestampFields hub.expires_at sdk.expires_at

structure RequestFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.ForwardedRequest)
    (sdk : RequestCompositionSdk.internal_auth.ForwardedRequest) : Prop where
  profile : hub.profile.value = sdk.profile
  server : IdFields hub.server sdk.server
  deployment : IdFields hub.deployment sdk.deployment
  audience : IdFields hub.audience sdk.audience
  client : IdFields hub.client sdk.client
  actor : PrincipalFields hub.actor sdk.actor
  authority : AuthorityFields hub.authority sdk.authority
  permissions : SetFields IdFields hub.permissions sdk.permissions
  request_context : OptionalFields ContextFields hub.request_context sdk.request_context
  acting_server : OptionalFields ActingFields hub.acting_server sdk.acting_server
  acting_task : OptionalFields (fun hub sdk => hub.value = sdk) hub.acting_task sdk.acting_task
  delegation_context : OptionalFields DelegationFields hub.delegation_context sdk.delegation_context
  expires_at : TimestampFields hub.expires_at sdk.expires_at

structure IdentityFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalIdentity)
    (sdk : RequestCompositionSdk.internal_auth.HubInternalIdentity) : Prop where
  issuer : IdFields hub.issuer sdk.issuer
  profile : hub.profile.value = sdk.profile
  server : IdFields hub.server sdk.server
  deployment : IdFields hub.deployment sdk.deployment
  audience : IdFields hub.audience sdk.audience
  client : IdFields hub.client sdk.client
  actor : PrincipalFields hub.actor sdk.actor
  authority : AuthorityFields hub.authority sdk.authority
  permissions : SetFields IdFields hub.permissions sdk.permissions
  request_context : OptionalFields ContextFields hub.request_context sdk.request_context
  acting_server : OptionalFields ActingFields hub.acting_server sdk.acting_server
  acting_task : OptionalFields (fun hub sdk => hub.value = sdk) hub.acting_task sdk.acting_task
  delegation_context : OptionalFields DelegationFields hub.delegation_context sdk.delegation_context
  jwt_id : hub.jwt_id.value = sdk.jwt_id
  issued_at : TimestampFields hub.issued_at sdk.issued_at
  not_before : TimestampFields hub.not_before sdk.not_before
  expires_at : TimestampFields hub.expires_at sdk.expires_at

structure IssuerFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (sdk : RequestCompositionSdk.internal_auth.HubInternalTokenIssuer) : Prop where
  issuer : IdFields hub.issuer sdk.issuer
  signing_key : hub.signing_key.value = sdk.signing_key.value

structure IssuedFields (hub : veoveo_hub.veoveo_hub_sdk.internal_auth.IssuedHubInternalToken)
    (sdk : RequestCompositionSdk.internal_auth.IssuedHubInternalToken) : Prop where
  bearer_token : hub.bearer_token = sdk.bearer_token
  identity : IdentityFields hub.identity sdk.identity

def AlgorithmFields : veoveo_hub.jsonwebtoken_1.algorithms.Algorithm →
    RequestCompositionSdk.jsonwebtoken.algorithms.Algorithm → Prop
  | .HS256, .HS256 => True
  | .HS384, .HS384 => True
  | .HS512, .HS512 => True
  | .ES256, .ES256 => True
  | .ES384, .ES384 => True
  | .RS256, .RS256 => True
  | .RS384, .RS384 => True
  | .RS512, .RS512 => True
  | .PS256, .PS256 => True
  | .PS384, .PS384 => True
  | .PS512, .PS512 => True
  | .EdDSA, .EdDSA => True
  | _, _ => False

/-- Constructor identity and every carried error payload are independent obligations. -/
def ErrorFields : veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError →
    RequestCompositionSdk.internal_auth.InternalTokenError → Prop
  | .DeploymentMismatch, .DeploymentMismatch => True
  | .EmptyPrivateKey, .EmptyPrivateKey => True
  | .InvalidSigningKey, .InvalidSigningKey => True
  | .InvalidIssuer, .InvalidIssuer => True
  | .KeyIdNotThumbprint htext, .KeyIdNotThumbprint stext => htext = stext
  | .InvalidTokenId, .InvalidTokenId => True
  | .InvalidKeyId, .InvalidKeyId => True
  | .EmptyTrustBundle, .EmptyTrustBundle => True
  | .MissingKeyId, .MissingKeyId => True
  | .DuplicateKeyId htext, .DuplicateKeyId stext => htext = stext
  | .UnknownKeyId htext, .UnknownKeyId stext => htext = stext
  | .UnsupportedTrustKey, .UnsupportedTrustKey => True
  | .UnsupportedTokenAlgorithm halgorithm, .UnsupportedTokenAlgorithm salgorithm => AlgorithmFields halgorithm salgorithm
  | .UnsupportedTokenType, .UnsupportedTokenType => True
  | .ExpiredDelegation, .ExpiredDelegation => True
  | .DelegationDepthExceeded, .DelegationDepthExceeded => True
  | .AudienceMismatch hexpected hactual, .AudienceMismatch sexpected sactual => IdFields hexpected sexpected ∧ IdFields hactual sactual
  | .SubjectPrincipalMismatch, .SubjectPrincipalMismatch => True
  | .InconsistentClaims, .InconsistentClaims => True
  | .PermissionOutsideAudience hpermission, .PermissionOutsideAudience spermission => IdFields hpermission spermission
  | .InvalidRequestContext, .InvalidRequestContext => True
  | .InvalidTimestamp hfield hseconds, .InvalidTimestamp sfield sseconds => hfield = sfield ∧ hseconds = sseconds
  | .Identifier herror, .Identifier serror => herror.value = serror.value
  | .TrustBundleJson herror, .TrustBundleJson serror => herror.value = serror.value
  | .Jwt herror, .Jwt serror => herror.value = serror.value
  | _, _ => False

end
end SdkInternalAuthBridge
