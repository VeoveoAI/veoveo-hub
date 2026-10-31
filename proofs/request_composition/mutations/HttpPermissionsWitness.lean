import RequestCompositionHub.Funs
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std RequestCompositionHub

-- Only the raw re-extracted model is imported. No failed assembly/route theorem
-- or its artifact may be imported into this independent semantic witness.
namespace SnapshotAssemblyProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p
abbrev Server := registration.snapshot.RegisteredServer
abbrev Subject := auth.verified.AuthenticatedSubject
abbrev Context := veoveo_hub_sdk.internal_auth.request.HubRequestContext
abbrev Client := mcp.upstream_authorized_http.HubAuthorizedHttpClient

-- Byte-for-byte independent complete-value specification from the committed
-- baseline leaf; the package runner checks the exact source substring.
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

end
end SnapshotAssemblyProof

namespace HttpPermissionsWitness
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p
open SnapshotAssemblyProof
abbrev Permission := veoveo_types.permission_names.PermissionName
abbrev Execution := veoveo_hub_sdk.execution.ExecutionContext
abbrev Grants := alloc.collections.btree.set.BTreeSet Permission alloc.alloc.Global

def directSubject (base : Subject) (execution : Execution) : Subject :=
  { base with delegation := none, execution := some execution }

def directContext (base : Subject) (execution : Execution) : Context := {
  execution, audit := base.audit, access_token := base.access_token,
  principal := base.principal }

def supplied (permission : Permission) : Grants := {permission}

/-- The real direct-context operation accepts this constructed subject. There is
no assumed accepted-context premise; all other subject payloads remain intact. -/
theorem direct_context_valid (base : Subject) (execution : Execution) :
    auth.verified.AuthenticatedSubject.request_context (directSubject base execution) =
      .ok (.Ok (directContext base execution)) := by
  simp [auth.verified.AuthenticatedSubject.request_context, directSubject, directContext, core.option.Option.as_ref,
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

/-- Kernel-checked execution of the actual re-extracted source mutant yields the
complete client. Transport/issuer/profile are carried values, never called. -/
theorem mutant_full_client
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (base : Subject) (execution : Execution)
    (permission : Permission) :
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
      http issuer profile server (directSubject base execution) (supplied permission) =
      .ok (.Ok (expectedClient http issuer profile server (directSubject base execution)
        ∅ (directContext base execution))) := by
  unfold mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
  rw [direct_context_valid]
  simp [expectedClient, directSubject, audit.AuthenticatedSubject.acting_task,
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
      alloc.collections.btree.set.BTreeSetTGlobal.new, bind_ok, bind_tc_ok]

/-- Nonempty supplied input is erased by the actual mutant, rather than refused.
No additional authentication or authorization rule is asserted by this fixture. -/
theorem permission_erased
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (base : Subject) (execution : Execution)
    (permission : Permission) :
    permission ∈ supplied permission ∧
      permission ∉ (expectedClient http issuer profile server (directSubject base execution)
        ∅ (directContext base execution)).permissions := by
  simp [supplied, expectedClient]

/-- The accepted full result contradicts the unchanged independent preservation
specification for every typed supplied singleton. Both Result layers are retained. -/
theorem mutant_violates_preservation
    (http : mcp.upstream_http.UpstreamTransport)
    (issuer : veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer)
    (profile : veoveo_hub_sdk.identity.HubProfileId)
    (server : Server) (base : Subject) (execution : Execution)
    (permission : Permission) :
    mcp.upstream_authorized_http.HubAuthorizedHttpClient.new
      http issuer profile server (directSubject base execution) (supplied permission) ≠
      .ok (.Ok (expectedClient http issuer profile server (directSubject base execution)
        (supplied permission) (directContext base execution))) := by
  rw [mutant_full_client]
  intro equal
  have clients :
      expectedClient http issuer profile server (directSubject base execution)
        ∅ (directContext base execution) =
      expectedClient http issuer profile server (directSubject base execution)
        (supplied permission) (directContext base execution) := by
    rw [Result.ok.injEq] at equal
    injection equal with clients
  have grants := congrArg (fun c : Client => c.permissions) clients
  change (∅ : Grants) = supplied permission at grants
  have absent : permission ∉ (∅ : Grants) := by simp
  apply absent
  rw [grants]
  simp [supplied]

#print axioms direct_context_valid
#print axioms mutant_full_client
#print axioms permission_erased
#print axioms mutant_violates_preservation
end
end HttpPermissionsWitness

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``HttpPermissionsWitness.direct_context_valid,
      ``HttpPermissionsWitness.permission_erased] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed direct-context/permission axiom in {declaration}: {found}"
  -- Actual new occurs in these theorem types. Its refused branch contains only
  -- these reviewed native formatting fences; accepted proofs do not call them.
  let allowed := standard ++ #[
    ``RequestCompositionHub.alloc.string.ToString.Blanket.to_string,
    ``RequestCompositionHub.veoveo_hub_sdk.internal_auth.InternalTokenError.Insts.CoreFmtDisplay.fmt]
  for declaration in #[``HttpPermissionsWitness.mutant_full_client,
      ``HttpPermissionsWitness.mutant_violates_preservation] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed mutant constructor axiom in {declaration}: {found}"
