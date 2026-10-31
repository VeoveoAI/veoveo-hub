import RequestCompositionHub.Funs
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std RequestCompositionHub

namespace SnapshotAssemblyProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Server := registration.snapshot.RegisteredServer
abbrev Snapshot := registration.snapshot.RegistrationSnapshot
abbrev Servers := alloc.collections.btree.map.BTreeMap
  veoveo_types.execution.DeploymentId (alloc.sync.Arc Server) alloc.alloc.Global
abbrev Namespaces := alloc.collections.btree.map.BTreeMap
  veoveo_types.platform_names.ServerSlug registration.snapshot.Namespace alloc.alloc.Global
abbrev Subject := auth.verified.AuthenticatedSubject
abbrev Context := veoveo_hub_sdk.internal_auth.request.HubRequestContext
abbrev Client := mcp.upstream_authorized_http.HubAuthorizedHttpClient

/-- Universal namespace-interface factor. No completion, ordering, or valid-route
assumption is built into this function or the native helper symbol. -/
def ofWith (derive : Servers → Result Namespaces) (servers : Servers) : Result Snapshot := do
  let namespaces ← derive servers
  .ok { servers, namespaces }

theorem ofWith_success_iff (derive : Servers → Result Namespaces)
    (servers : Servers) (snapshot : Snapshot) :
    ofWith derive servers = .ok snapshot ↔
      snapshot.servers = servers ∧ derive servers = .ok snapshot.namespaces := by
  cases snapshot
  cases h : derive servers <;> simp [ofWith, h, eq_comm]

theorem ofWith_arbitrary_success (servers : Servers) (namespaces : Namespaces) :
    ofWith (fun _ => .ok namespaces) servers = .ok { servers, namespaces } := by
  simp [ofWith]

/-- Syntactic connection to the untouched extracted source, deliberately retaining
its unresolved own-code helper symbol in the dependency closure. -/
theorem extracted_of_connection (servers : Servers) :
    registration.snapshot.RegistrationSnapshot.of servers =
      ofWith registration.snapshot.RegistrationSnapshot.build_namespaces servers := by
  rfl

theorem extracted_of_success (servers : Servers) (snapshot : Snapshot)
    (success : registration.snapshot.RegistrationSnapshot.of servers = .ok snapshot) :
    snapshot.servers = servers ∧
      registration.snapshot.RegistrationSnapshot.build_namespaces servers =
        .ok snapshot.namespaces := by
  rw [extracted_of_connection] at success
  exact (ofWith_success_iff _ _ _).mp success

/-- Direct calls construct current context; delegated calls retain their supplied
original context and reject only an explicitly mismatched current execution. -/
def expectedContext (subject : Subject) :
    core.result.Result Context veoveo_hub_sdk.internal_auth.InternalTokenError :=
  match subject.delegation with
  | none => match subject.execution with
    | none => .Err .InvalidRequestContext
    | some execution => .Ok {
        execution, audit := subject.audit, access_token := subject.access_token,
        principal := subject.principal }
  | some delegation =>
    if subject.execution.any (fun execution => decide (execution ≠ delegation.request_context.execution))
    then .Err .InvalidRequestContext else .Ok delegation.request_context

theorem request_context_exact (subject : Subject) :
    auth.verified.AuthenticatedSubject.request_context subject = .ok (expectedContext subject) := by
  cases delegation : subject.delegation <;> cases execution : subject.execution <;>
    simp [auth.verified.AuthenticatedSubject.request_context, expectedContext,
      delegation, execution, core.option.Option.as_ref,
      core.option.Option.Insts.CoreCloneClone.clone,
      core.option.Option.ok_or, core.result.Result.Insts.CoreOpsTry.branch,
      core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual,
      core.convert.FromSame, core.option.Option.is_some_and,
      auth.verified.AuthenticatedSubject.request_context.closure.Insts.CoreOpsFunctionFnOnceTupleShared1ExecutionContextBool,
      auth.verified.AuthenticatedSubject.request_context.closure.Insts.CoreOpsFunctionFnOnceTupleShared1ExecutionContextBool.call_once,
      core.cmp.impls.PartialEqShared.ne,
      core.cmp.PartialEq.ne.trait_default, core.cmp.PartialEq.ne.default,
      veoveo_hub_sdk.execution.ExecutionContext.Insts.CoreCmpPartialEqExecutionContext,
      veoveo_hub_sdk.execution.ExecutionContext.Insts.CoreCmpPartialEqExecutionContext.eq,
      veoveo_hub_sdk.execution.ExecutionContext.Insts.CoreCloneClone,
      veoveo_hub_sdk.execution.ExecutionContext.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.request.HubRequestContext.Insts.CoreCloneClone.clone,
      veoveo_audit_contract.model.AuditRequest.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.AccessTokenSubject.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone,
      bind_ok, bind_tc_ok]
  all_goals split <;> rfl

/-- Independent full value specification for the real HTTP constructor, after
its actual source-context validation succeeds. Permissions are preserved exactly as supplied; current-policy freshness is a
separate linkage obligation. No nonempty-grant requirement is invented. -/
def expectedClient (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (subject : Subject)
    (permissions : alloc.collections.btree.set.BTreeSet
      veoveo_types.permission_names.PermissionName alloc.alloc.Global)
    (context : Context) : Client := {
  http, issuer, profile, server := server.slug, deployment := server.deployment,
  audience := server.endpoint, client := subject.access_token.oauth_client_id,
  actor := subject.actor, authority := subject.authority, permissions,
  request_context := context,
  acting_server := subject.delegation.map (·.acting_server),
  acting_task := subject.delegation.bind (·.task),
  delegation_context := subject.delegation.bind (·.context),
  delegation_expires_at := subject.delegation.map (·.expires_at) }

theorem http_new_exact
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (subject : Subject)
    (permissions : alloc.collections.btree.set.BTreeSet
      veoveo_types.permission_names.PermissionName alloc.alloc.Global)
    (context : Context) (accepted : expectedContext subject = .Ok context) :
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
      http issuer profile server subject permissions =
      .ok (.Ok (expectedClient http issuer profile server subject permissions context)) := by
  unfold mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
  rw [request_context_exact, accepted]
  cases delegation : subject.delegation with
  | none =>
    simp [expectedClient, delegation, audit.AuthenticatedSubject.acting_task,
      veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.registration.declaration.ResourceIndicator.Insts.CoreConvertFromShared0ServerEndpoint.from,
      url.Url.Insts.CoreCloneClone.clone,
      veoveo_types.platform_names.OAuthClientId.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone,
      veoveo_types.authority.InvocationAuthority.Insts.CoreCloneClone.clone,
      core.result.Result.map_err, core.result.Result.Insts.CoreOpsTry.branch,
      core.option.Option.as_ref, core.option.Option.map, core.option.Option.and_then,
      core.option.OptionShared0T.cloned, core.option.Option.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone,
      veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone,
      veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_1.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationActingServer,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_1.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationActingServer.call_once,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_2.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationOptionDelegationContext,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_2.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationOptionDelegationContext.call_once,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_3.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationDateTimeUtcUtc,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_3.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationDateTimeUtcUtc.call_once,
      bind_ok, bind_tc_ok]
  | some info =>
    cases task : info.task <;> cases delegatedContext : info.context <;>
      simp [expectedClient, delegation, task, delegatedContext, audit.AuthenticatedSubject.acting_task,
      veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.registration.declaration.ResourceIndicator.Insts.CoreConvertFromShared0ServerEndpoint.from,
      url.Url.Insts.CoreCloneClone.clone,
      veoveo_types.platform_names.OAuthClientId.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.Principal.Insts.CoreCloneClone.clone,
      veoveo_types.authority.InvocationAuthority.Insts.CoreCloneClone.clone,
      core.result.Result.map_err, core.result.Result.Insts.CoreOpsTry.branch,
      core.option.Option.as_ref, core.option.Option.map, core.option.Option.and_then,
      core.option.OptionShared0T.cloned, core.option.Option.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.internal_auth.ActingServer.Insts.CoreCloneClone,
      veoveo_hub_sdk.identity.OpaqueTaskId.Insts.CoreCloneClone,
      veoveo_hub_sdk.internal_auth.exchange.DelegationContext.Insts.CoreCloneClone,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_1.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationActingServer,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_1.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationActingServer.call_once,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_2.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationOptionDelegationContext,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_2.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationOptionDelegationContext.call_once,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_3.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationDateTimeUtcUtc,
      mcp.upstream_authorized_http.HubAuthorizedHttpClient.new.closure_3.Insts.CoreOpsFunctionFnOnceTupleShared0SubjectDelegationDateTimeUtcUtc.call_once,
      bind_ok, bind_tc_ok]

#print axioms ofWith_success_iff
#print axioms ofWith_arbitrary_success
#print axioms extracted_of_connection
#print axioms extracted_of_success
#print axioms request_context_exact
#print axioms http_new_exact
end
end SnapshotAssemblyProof

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SnapshotAssemblyProof.ofWith_success_iff,
      ``SnapshotAssemblyProof.ofWith_arbitrary_success,
      ``SnapshotAssemblyProof.request_context_exact] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed context/universal axiom in {declaration}: {found}"
  let namespaceBoundary := standard.push
    ``RequestCompositionHub.registration.snapshot.RegistrationSnapshot.build_namespaces
  for declaration in #[``SnapshotAssemblyProof.extracted_of_connection,
      ``SnapshotAssemblyProof.extracted_of_success] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !namespaceBoundary.contains axiomName)).isEmpty do
      throwError "unreviewed snapshot boundary in {declaration}: {found}"

  -- The theorem TYPE mentions actual new, whose refused branch contains the
  -- native Display/ToString callback. Its accepted-branch proof never assumes
  -- that callback succeeds or renders any particular text. No issuer/helper
  -- symbols are permitted by this HTTP-constructor audit.
  let formattingBoundary := standard ++ #[
    ``RequestCompositionHub.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionHub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt]
  let found ← Lean.collectAxioms ``SnapshotAssemblyProof.http_new_exact
  unless (found.filter (fun axiomName => !formattingBoundary.contains axiomName)).isEmpty do
    throwError "unreviewed HTTP constructor axiom: {found}"
