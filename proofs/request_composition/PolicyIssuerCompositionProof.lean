import DelegatedPolicyClientProof
import RegisteredSurfacesProof
import SdkInternalAuthCallerProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub IteratorProof
open SdkInternalAuthBridge

namespace PolicyIssuerCompositionProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Server := SnapshotAssemblyProof.Server
abbrev Subject := SnapshotAssemblyProof.Subject
abbrev Context := SnapshotAssemblyProof.Context
abbrev Installation := DelegatedPolicyClientProof.Installation
abbrev Permission := DelegatedPolicyClientProof.Permission
abbrev Snapshot := SnapshotAssemblyProof.Snapshot
abbrev Slug := SelectedCollectionProof.Slug
abbrev Client := SnapshotAssemblyProof.Client
abbrev TextFits := InternalAuthContextProof.TextRepresentable

/-- Raw source fields that may supply the request context. These bounds assert
no execution match, authentication, Task liveness, policy grant or issuer result. -/
def SourceContextPhysical (subject : Subject) : Prop :=
  match subject.delegation with
  | none => AccessTokenPhysical subject.access_token ∧ PrincipalPhysical subject.principal
  | some info => ContextPhysical info.request_context

def ContextText (context : Context) : Prop :=
  TextFits context.principal.subject.value ∧ TextFits context.access_token.issuer.value ∧
    TextFits context.access_token.oauth_client_id.value

def SourceContextText (subject : Subject) : Prop :=
  match subject.delegation with
  | none => TextFits subject.principal.subject.value ∧ TextFits subject.access_token.issuer.value ∧
      TextFits subject.access_token.oauth_client_id.value
  | some info => ContextText info.request_context

/-- Physical scalar/allocation bounds on source observations only. Rendered
scope allocation is bounded for every duplicate-free enumeration; no arbitrary
Finset enumeration is identified with native BTreeSet order. -/
structure SubjectBounds (subject : Subject) (requested : HubTime) : Prop where
  actor_physical : PrincipalPhysical subject.actor
  context_physical : SourceContextPhysical subject
  inherited_physical : ∀ info ∈ subject.delegation, ∀ inherited ∈ info.context,
    DelegationPhysical inherited
  cap_physical : ∀ info ∈ subject.delegation, TimestampPhysical info.expires_at
  requested_physical : TimestampPhysical requested
  actor_id : TextFits subject.actor.id.value
  actor_subject : TextFits subject.actor.subject.value
  scopes : ∀ scope ∈ subject.actor.scopes, TextFits scope.value
  rendered_scopes : ∀ entries : List _root_.veoveo_types.names.ScopeName,
    entries.toFinset = subject.actor.scopes → entries.Nodup →
      TextFits (String.intercalate " " (entries.map (·.value)))
  context_text : SourceContextText subject

/-- The permission bound is on declaration entries, before current grants or
optional delegation narrowing have been computed. Surfaces are carried intact
but are not another authorization or issuance rule. -/
structure ServerBounds (row : Server) : Prop where
  server : TextFits row.slug.value
  audience : TextFits row.endpoint.value
  declarations : ∀ entry ∈ row.authorization.permissions.val, TextFits entry.permission.value

private theorem context_sources (subject : Subject) (context : Context)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context) :
    match subject.delegation with
    | none => context.access_token = subject.access_token ∧ context.principal = subject.principal
    | some info => context = info.request_context := by
  cases delegated : subject.delegation with
  | none =>
    cases execution : subject.execution with
    | none => simp [SnapshotAssemblyProof.expectedContext, delegated, execution] at accepted
    | some value =>
      have same : ({
          execution := value
          audit := subject.audit
          access_token := subject.access_token
          principal := subject.principal } : Context) = context := by
        simpa [SnapshotAssemblyProof.expectedContext, delegated, execution] using accepted
      subst context
      exact ⟨rfl, rfl⟩
  | some info =>
    simp only [SnapshotAssemblyProof.expectedContext, delegated] at accepted
    split at accepted
    · cases accepted
    · simpa using accepted.symm

private theorem context_physical (subject : Subject) (context : Context)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context)
    (bounds : SourceContextPhysical subject) : ContextPhysical context := by
  have sources := context_sources subject context accepted
  cases delegated : subject.delegation with
  | none =>
    simp only [delegated, SourceContextPhysical] at bounds
    simp only [delegated] at sources
    exact ⟨sources.1.symm ▸ bounds.1, sources.2.symm ▸ bounds.2⟩
  | some info =>
    simp only [delegated, SourceContextPhysical] at bounds
    simp only [delegated] at sources
    simpa only [sources] using bounds

private theorem context_text (subject : Subject) (context : Context)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context)
    (bounds : SourceContextText subject) : ContextText context := by
  have sources := context_sources subject context accepted
  cases delegated : subject.delegation with
  | none =>
    simp only [delegated, SourceContextText] at bounds
    simp only [delegated] at sources
    simpa only [ContextText, sources.1, sources.2] using bounds
  | some info =>
    simp only [delegated, SourceContextText] at bounds
    simp only [delegated] at sources
    simpa only [sources] using bounds

/-- Exact constructed client, with current grants and carried ceiling derived
from the same installation, registration row and current subject. -/
abbrev policyClient (installation : Installation) (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (context : Context) : Client :=
  SnapshotAssemblyProof.expectedClient http issuer profile row subject
    (DelegatedPolicyClientProof.expectedPermissions installation row subject) context

def policyRequest (installation : Installation) (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (context : Context) (requested : HubTime) : HubRequest :=
  SdkInternalAuthCallerProof.callerRequest
    (policyClient installation http issuer profile row subject context) requested

private theorem client_physical (installation : Installation)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (context : Context) (requested : HubTime)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context)
    (bounds : SubjectBounds subject requested) :
    SdkInternalAuthCallerProof.CallerPhysical
      (policyClient installation http issuer profile row subject context) requested := by
  refine ⟨bounds.actor_physical, context_physical subject context accepted bounds.context_physical,
    ?_, bounds.requested_physical, ?_⟩
  · intro inherited member
    cases delegated : subject.delegation with
    | none => simp [policyClient, SnapshotAssemblyProof.expectedClient, delegated] at member
    | some info =>
      exact bounds.inherited_physical info (by simp [delegated]) inherited
        (by simpa [policyClient, SnapshotAssemblyProof.expectedClient, delegated] using member)
  · intro cap member
    cases delegated : subject.delegation with
    | none => simp [policyClient, SnapshotAssemblyProof.expectedClient, delegated] at member
    | some info =>
      have same : info.expires_at = cap := by
        simpa [policyClient, SnapshotAssemblyProof.expectedClient, delegated] using member
      exact same ▸ bounds.cap_physical info (by simp [delegated])

/-- Declaration-origin membership discharges the computed permission text bound.
Neither a supplied grant result nor a validator-success premise is used. -/
theorem request_text (installation : Installation) (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (context : Context) (requested : HubTime)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context)
    (subjectBounds : SubjectBounds subject requested) (serverBounds : ServerBounds row) :
    HubIssueTextDomain (policyRequest installation http issuer profile row subject context requested) := by
  refine ⟨serverBounds.server, ?_, subjectBounds.actor_id, subjectBounds.actor_subject,
    serverBounds.audience, subjectBounds.scopes, subjectBounds.rendered_scopes, ?_⟩
  · intro permission member
    change permission ∈ DelegatedPolicyClientProof.expectedPermissions installation row subject at member
    obtain ⟨granted, _⟩ :=
      (DelegatedPolicyClientProof.expected_permissions_membership installation row subject permission).mp member
    rcases granted with ⟨entry, declared, same, _⟩
    simpa only [same] using serverBounds.declarations entry declared
  · intro actual member
    have same : context = actual := by
      simpa [policyRequest, SdkInternalAuthCallerProof.callerRequest, policyClient,
        SnapshotAssemblyProof.expectedClient] using member
    subst actual
    exact context_text subject context accepted subjectBounds.context_text

/-- MCP construction errors and SDK caller results remain distinct. A Caller
value can itself be an exact SDK refusal; it does not assert token issuance. -/
inductive RowResult where
  | ContextError : rmcp.model.ErrorData → RowResult
  | Caller : SdkInternalAuthCallerProof.CallerResult → RowResult

/-- Exact independent MCP error payload, with native formatting observations
left fallible. No particular formatted text or formatter success is assumed. -/
def contextFailure (error : veoveo_hub_sdk.internal_auth.InternalTokenError) : Result RowResult := do
  let text ← _root_.alloc.string.ToString.Blanket.to_string
    _root_.veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay error
  .ok (.ContextError { code := (-32603)#i32, message := .Owned text, data := none })

/-- Source-reviewed composition of extracted subcalls, not an extraction of the
async acquisition/transport wrapper. Complete native effects remain in binds. -/
def issueFromRow (installation : Installation) (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (requested : HubTime) : Result RowResult :=
  Aeneas.Std.bind (do
    let granted ← policy.grants.granted_permissions installation row.authorization subject.principal
    let permissions ← match subject.delegation with
      | none => .ok granted
      | some info => delegation.SubjectDelegation.narrow info granted
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new http issuer profile row subject permissions)
    (fun result => match result with
      | .Err error => .ok (.ContextError error)
      | .Ok client => do
        let issued ← mcp.upstream_authorized_http.HubAuthorizedHttpClient.issue_bearer_token_until client requested
        .ok (.Caller issued))

def selectedIssue (selected : Snapshot) (slug : Slug) (installation : Installation)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (subject : Subject)
    (requested : HubTime) : Result (Option RowResult) := do
  let found ← registration.snapshot.RegistrationSnapshot.get selected slug
  match found with
  | none => .ok none
  | some row => do
    let result ← issueFromRow installation http issuer profile row subject requested
    .ok (some result)

private theorem new_refused (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (permissions : Finset Permission) (error : veoveo_hub_sdk.internal_auth.InternalTokenError)
    (refused : SnapshotAssemblyProof.expectedContext subject = .Err error) :
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new http issuer profile row subject permissions =
      (do
        let text ← _root_.alloc.string.ToString.Blanket.to_string
          _root_.veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay error
        .ok (.Err { code := (-32603)#i32, message := .Owned text, data := none })) := by
  unfold mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
  rw [SnapshotAssemblyProof.request_context_exact, refused]
  simp only [veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.registration.declaration.ResourceIndicator.Insts.CoreConvertFromShared0ServerEndpoint.from,
    url.Url.Insts.CoreCloneClone.clone,
    veoveo_types.platform_names.OAuthClientId.Insts.CoreCloneClone.clone,
    veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone,
    veoveo_types.authority.InvocationAuthority.Insts.CoreCloneClone.clone,
    core.result.Result.map_err,
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure.Insts.CoreOpsFunctionFnOnceTupleInternalTokenErrorErrorData,
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure.Insts.CoreOpsFunctionFnOnceTupleInternalTokenErrorErrorData.call_once,
    mcp_support.mcp_internal, core.convert.IntoFrom, core.convert.FromSame,
    _root_.rmcp.model.ErrorData.internal_error,
    _root_.veoveo_hub.alloc.borrow.CowAStrString.Insts.CoreConvertFromString,
    _root_.alloc.borrow.CowAStrString.Insts.CoreConvertFromString.from,
    bind_ok, bind_tc_eq, Aeneas.Std.bind_assoc]
  apply congrArg (fun continuation => Aeneas.Std.bind
    (_root_.alloc.string.ToString.Blanket.to_string
      _root_.veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay error) continuation)
  funext text
  simp [core.result.Result.Insts.CoreOpsTry.branch,
    core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual,
    core.convert.FromSame, _root_.alloc.borrow.CowAStrString.Insts.CoreConvertFromString.from,
    bind_ok, bind_tc_ok]

theorem row_refused (installation : Installation) (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (requested : HubTime) (error : veoveo_hub_sdk.internal_auth.InternalTokenError)
    (refused : SnapshotAssemblyProof.expectedContext subject = .Err error) :
    issueFromRow installation http issuer profile row subject requested = contextFailure error := by
  unfold issueFromRow
  rw [GrantedPermissionsProof.granted_permissions_exact]
  simp only [bind_ok, bind_tc_eq]
  cases delegated : subject.delegation with
  | none =>
    simp only [delegated, bind_ok]
    rw [new_refused http issuer profile row subject _ error refused]
    simp only [contextFailure, bind_tc_eq, Aeneas.Std.bind_assoc, bind_ok]
  | some info =>
    simp only [delegated]
    rw [DelegatedPolicyClientProof.subject_narrow_exact]
    simp only [bind_ok]
    rw [new_refused http issuer profile row subject _ error refused]
    simp only [contextFailure, bind_tc_eq, Aeneas.Std.bind_assoc, bind_ok]

/-- Accepted context is only this lemma's branch discriminator, not the final
composition theorem's premise. All issuer outcomes and primitive effects remain. -/
theorem row_accepted (installation : Installation) (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (row : Server) (subject : Subject)
    (context : Context) (requested : HubTime)
    (accepted : SnapshotAssemblyProof.expectedContext subject = .Ok context)
    (subjectBounds : SubjectBounds subject requested) (serverBounds : ServerBounds row) :
    ∃ sdk, requestToSdk (policyRequest installation http issuer profile row subject context requested) = some sdk ∧
      RequestFields (policyRequest installation http issuer profile row subject context requested) sdk ∧
      RequestCanonical sdk ∧
      requestToHub sdk = policyRequest installation http issuer profile row subject context requested ∧
      issueFromRow installation http issuer profile row subject requested =
        (do let result ← SdkInternalAuthCallerProof.callerOutcome issuer sdk; .ok (.Caller result)) := by
  obtain ⟨sdk, converted, fields, canonical, roundtrip, outcome⟩ :=
    SdkInternalAuthCallerProof.caller_exact
      (policyClient installation http issuer profile row subject context) requested
      (client_physical installation http issuer profile row subject context requested accepted subjectBounds)
      (request_text installation http issuer profile row subject context requested accepted subjectBounds serverBounds)
  refine ⟨sdk, converted, fields, canonical, roundtrip, ?_⟩
  have constructed := (DelegatedPolicyClientProof.policy_client_exact
    installation http issuer profile row subject context accepted).1
  have operation : (do
      let granted ← policy.grants.granted_permissions installation row.authorization subject.principal
      let permissions ← match subject.delegation with
        | none => .ok granted
        | some info => delegation.SubjectDelegation.narrow info granted
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new http issuer profile row subject permissions) =
      .ok (.Ok (policyClient installation http issuer profile row subject context)) := by
    cases delegated : subject.delegation <;>
      simpa only [delegated, bind_tc_eq, Aeneas.Std.bind_assoc] using constructed
  unfold issueFromRow
  cases delegated : subject.delegation <;>
    simp only [delegated, bind_tc_eq, bind_ok] at operation ⊢
  all_goals
    rw [operation]
    simp only [bind_ok]
    rw [outcome]
  all_goals rfl

private theorem component_member (components : List SelectedCollectionProof.Entry) (slug : Slug)
    (component : SelectedRouteContextProof.Component)
    (found : SelectedRouteContextProof.componentLookup components slug = some component) :
    (slug, component) ∈ components := by
  induction components with
  | nil => simp [SelectedRouteContextProof.componentLookup] at found
  | cons head tail recursive =>
    rcases head with ⟨key, value⟩
    by_cases same : key = slug
    · have equal : value = component := by simpa [SelectedRouteContextProof.componentLookup, same] using found
      subst key
      subst value
      exact List.mem_cons_self
    · exact List.mem_cons_of_mem _ (recursive (by simpa [SelectedRouteContextProof.componentLookup, same] using found))

private theorem resolved_member (snapshot selected : Snapshot)
    (composition : SelectedMapCollectionProof.Composition) (slug : Slug)
    (selectedSuccess : registration.snapshot.RegistrationSnapshot.selected snapshot composition = .ok selected)
    (key : SelectedCollectionProof.Deployment) (row : Server)
    (route : SelectedRouteContextProof.resolveRoute snapshot composition.components slug = some (key, row)) :
    (key, row) ∈ selected.servers := by
  have image := SelectedMapCollectionProof.selected_success_complete snapshot composition selected selectedSuccess
  apply (image.2.2 key row).mpr
  apply (SelectedCollectionProof.emitted_membership snapshot composition.components key row).mp
  unfold SelectedCollectionProof.emitted
  apply List.mem_filterMap.mpr
  unfold SelectedRouteContextProof.resolveRoute at route
  cases found : SelectedRouteContextProof.componentLookup composition.components slug with
  | none => simp [found] at route
  | some component =>
    exact ⟨(slug, component), component_member _ _ _ found, by simpa only [found] using route⟩

/-- One supplied generation determines route, complete source row, current
principal grants, optional carried ceiling, client and SDK request. The whole
operation includes absent-route and context-refusal branches. No supplied client,
computed grant result, issuer success or desired outcome is a premise.

This is a prepared-stack composition of extracted subcalls, not the async
catalog-acquisition/HTTP transport wrapper, TTL selection, native set/UTF-8/Chrono
refinement, authentication or policy freshness. Native SDK154 equivalence to the
prepared checked SDK implementation remains a separate boundary. -/
theorem constructed_policy_issue_exact {S I : Type}
    (ops : Aeneas.Std.core.iter.traits.collect.IntoIterator S Server I)
    (source : S) (initial : I) (model : Enumeration I Server)
    (cursor : CursorLaw ops.iteratorInst model) (opened : ops.into_iter source = .ok initial)
    (valid : model.valid initial) (generation : catalog.HubCatalog)
    (constructed : registration.snapshot.RegistrationSnapshot.new ops source = .ok generation.registrations)
    (composition : SelectedMapCollectionProof.Composition) (selected : Snapshot) (slug : Slug)
    (selectedSuccess : registration.snapshot.RegistrationSnapshot.selected generation.registrations composition = .ok selected)
    (unique : SelectedRouteContextProof.CompositionKeysUnique composition)
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId) (subject : Subject) (requested : HubTime)
    (subjectBounds : SubjectBounds subject requested)
    (rowBounds : ∀ row ∈ model.remaining initial, ServerBounds row) :
    registration.snapshot.RegistrationSnapshot.get selected slug =
      .ok ((SelectedRouteContextProof.resolveRoute generation.registrations composition.components slug).map Prod.snd) ∧
    match SelectedRouteContextProof.resolveRoute generation.registrations composition.components slug with
    | none => selectedIssue selected slug generation.control_plane_value.authorization http issuer profile subject requested = .ok none
    | some (key, row) =>
      row.slug = slug ∧ row.deployment = key ∧
      (∃ before original after, model.remaining initial = before ++ original :: after ∧
        row = original ∧ RegisteredSurfacesProof.surfacesPayload row = RegisteredSurfacesProof.surfacesPayload original ∧
        RegisteredSurfacesProof.SurfaceFields original.surfaces (RegisteredSurfacesProof.surfacesPayload row)) ∧
      (∀ permission, permission ∈ DelegatedPolicyClientProof.expectedPermissions
        generation.control_plane_value.authorization row subject ↔
        DelegatedPolicyClientProof.Eligible generation.control_plane_value.authorization row subject permission) ∧
      match SnapshotAssemblyProof.expectedContext subject with
      | .Err error => selectedIssue selected slug generation.control_plane_value.authorization http issuer profile subject requested =
          (do let result ← contextFailure error; .ok (some result))
      | .Ok context => ∃ sdk,
          requestToSdk (policyRequest generation.control_plane_value.authorization http issuer profile row subject context requested) = some sdk ∧
          RequestFields (policyRequest generation.control_plane_value.authorization http issuer profile row subject context requested) sdk ∧
          RequestCanonical sdk ∧ requestToHub sdk =
            policyRequest generation.control_plane_value.authorization http issuer profile row subject context requested ∧
          selectedIssue selected slug generation.control_plane_value.authorization http issuer profile subject requested =
            (do let result ← SdkInternalAuthCallerProof.callerOutcome issuer sdk; .ok (some (.Caller result))) := by
  have getter := SelectedRouteContextProof.selected_get_exact generation.registrations composition selected slug selectedSuccess unique
  refine ⟨getter, ?_⟩
  cases route : SelectedRouteContextProof.resolveRoute generation.registrations composition.components slug with
  | none => simp [selectedIssue, getter, route, bind_ok, bind_tc_ok]
  | some pair =>
    rcases pair with ⟨key, row⟩
    have coherent := SnapshotConstructorProof.new_success_coherent ops source initial model cursor opened valid generation.registrations constructed
    obtain ⟨sameSlug, sameDeployment⟩ := SelectedRouteContextProof.resolved_route_fields
      generation.registrations composition slug key row unique coherent route
    have member := resolved_member generation.registrations selected composition slug selectedSuccess key row route
    have provenance := RegisteredSurfacesProof.constructed_selected_surfaces ops source initial model cursor opened valid
      generation.registrations constructed composition selected selectedSuccess key row member
    have sourceMember : row ∈ model.remaining initial := by
      rcases provenance with ⟨before, original, after, occurrence, same, _⟩
      rw [occurrence, same]
      simp
    refine ⟨sameSlug, sameDeployment, provenance,
      DelegatedPolicyClientProof.expected_permissions_membership _ row subject, ?_⟩
    cases contextResult : SnapshotAssemblyProof.expectedContext subject with
    | Err error =>
      simp only [selectedIssue, getter, route, Option.map_some, bind_ok, bind_tc_ok]
      rw [row_refused _ http issuer profile row subject requested error contextResult]
    | Ok context =>
      obtain ⟨sdk, converted, fields, canonical, roundtrip, issued⟩ := row_accepted
        generation.control_plane_value.authorization http issuer profile row subject context requested
        contextResult subjectBounds (rowBounds row sourceMember)
      refine ⟨sdk, converted, fields, canonical, roundtrip, ?_⟩
      simp only [selectedIssue, getter, route, Option.map_some, bind_ok, bind_tc_ok]
      rw [issued]
      simp only [bind_tc_eq, Aeneas.Std.bind_assoc, bind_ok]

#print axioms request_text
#print axioms row_refused
#print axioms row_accepted
#print axioms constructed_policy_issue_exact
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[
    ``_root_.registration.snapshot.RegistrationSnapshot.build_namespaces,
    ``_root_.alloc.string.ToString.Blanket.to_string,
    ``_root_.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt,
    ``RequestCompositionSdk.internal_auth.ensure_jwt_crypto_provider,
    ``RequestCompositionSdk.chrono.offset.utc.Utc.now,
    ``RequestCompositionSdk.uuid.v7.Uuid.now_v7,
    ``RequestCompositionSdk.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionSdk.identity.JwtId.parse,
    ``RequestCompositionSdk.internal_auth.HubInternalTokenIssuer.sign_forwarded]
  for declaration in #[``PolicyIssuerCompositionProof.request_text,
      ``PolicyIssuerCompositionProof.row_refused, ``PolicyIssuerCompositionProof.row_accepted,
      ``PolicyIssuerCompositionProof.constructed_policy_issue_exact] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun n => !(standard ++ native).contains n)).isEmpty do
      throwError "unreviewed policy/issuer composition dependency in {declaration}: {found}"
end
end PolicyIssuerCompositionProof
