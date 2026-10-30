import GrantedPermissionsProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

namespace RegisteredServerPolicyProof
noncomputable section

abbrev Target := control_plane.policy.PolicyTarget
abbrev Requirement := veoveo_hub_sdk.registration.authorization.RequirementTarget
abbrev Access := veoveo_hub_sdk.registration.authorization.RouteAccess
abbrev Declaration := veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration
abbrev Slug := veoveo_types.platform_names.ServerSlug
abbrev Request := policy.evaluation.PolicyRequest
abbrev Decision := control_plane.policy.PolicyDecision
abbrev Timestamp := chrono.datetime.DateTime chrono.offset.utc.Utc chrono.offset.utc.Utc

/- Declarative target projection, including the negative and server/Task cases.
No relation to a separately supplied decide_server argument is assumed here. -/
def projectedTarget : Target → Option (Slug × Option Requirement)
  | .Server server | .Task server _ | .PlatformTask server _ => some (server, none)
  | .Tool server tool => some (server, some (.Tool tool))
  | .Resource server uri => some (server, some (.Resource uri))
  | .ResourceTemplate server uri => some (server, some (.ResourceTemplate uri))
  | .Prompt server prompt => some (server, some (.Prompt prompt))
  | .Hub | .Owner _ | .Unadmitted _ => none

theorem registered_target_exact (target : Target) :
    policy.evaluation.registered_target target = .ok (projectedTarget target) := by
  cases target <;> rfl

theorem registered_target_coherent (target : Target) (server : Slug)
    (requirement : Option Requirement) :
    policy.evaluation.registered_target target = .ok (some (server, requirement)) ↔
      projectedTarget target = some (server, requirement) := by
  rw [registered_target_exact]
  simp

/- The SDK operation is intentionally left native. This trace proves which
catalog/target arguments the ACTUAL Hub helper supplies, not SDK route semantics. -/
def declaredTrace (lookup : Slug → Result (Option Declaration)) (target : Target) :
    Result (Option Access) := do
  match projectedTarget target with
  | some (server, some requirement) =>
    let declaration ← lookup server
    match declaration with
    | none => .ok none
    | some declaration =>
      veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration requirement
  | _ => .ok none

theorem declared_route_trace {C : Type} (ops : policy.catalog.PolicyCatalogView C)
    (catalog : C) (target : Target) :
    policy.evaluation.declared_route_access ops catalog target =
      declaredTrace (ops.server_authorization catalog) target := by
  cases target <;>
    simp only [policy.evaluation.declared_route_access, policy.evaluation.registered_target,
      projectedTarget, declaredTrace, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
      bind_ok, bind_tc_ok, bind_tc_eq]
  all_goals try rfl
  all_goals
    apply congrArg (fun continuation =>
      Aeneas.Std.bind (ops.server_authorization catalog _) continuation)
    funext declaration
    cases declaration <;> rfl

/- Closed kernel metadata spellings; validation is the reviewed ordinary native
parser model, not a claim that the Core parser implementation was extracted. -/
def kernelSpelling : contract.actions.HubAction → _root_.String
  | .ToolsList => "tools_list"
  | .ToolsCall => "tools_call"
  | .ResourcesList => "resources_list"
  | .ResourcesTemplatesList => "resources_templates_list"
  | .ResourcesRead => "resources_read"
  | .SubscriptionsListen => "subscriptions_listen"
  | .PromptsList => "prompts_list"
  | .PromptsGet => "prompts_get"
  | .CompletionComplete => "completion_complete"
  | .TasksGet => "tasks_get"
  | .TasksUpdate => "tasks_update"
  | .TasksCancel => "tasks_cancel"
  | .AdminRead => "admin_read"
  | .AdminWrite => "admin_write"

/- Exact finite ASCII data, checked by Lean's kernel. No native_decide,
parser assumption or replacement spelling is used. -/
def kernelCodes : contract.actions.HubAction → List Nat
  | .ToolsList => [116, 111, 111, 108, 115, 95, 108, 105, 115, 116]
  | .ToolsCall => [116, 111, 111, 108, 115, 95, 99, 97, 108, 108]
  | .ResourcesList => [114, 101, 115, 111, 117, 114, 99, 101, 115, 95, 108, 105, 115, 116]
  | .ResourcesTemplatesList => [114, 101, 115, 111, 117, 114, 99, 101, 115, 95, 116, 101, 109, 112, 108, 97, 116, 101, 115, 95, 108, 105, 115, 116]
  | .ResourcesRead => [114, 101, 115, 111, 117, 114, 99, 101, 115, 95, 114, 101, 97, 100]
  | .SubscriptionsListen => [115, 117, 98, 115, 99, 114, 105, 112, 116, 105, 111, 110, 115, 95, 108, 105, 115, 116, 101, 110]
  | .PromptsList => [112, 114, 111, 109, 112, 116, 115, 95, 108, 105, 115, 116]
  | .PromptsGet => [112, 114, 111, 109, 112, 116, 115, 95, 103, 101, 116]
  | .CompletionComplete => [99, 111, 109, 112, 108, 101, 116, 105, 111, 110, 95, 99, 111, 109, 112, 108, 101, 116, 101]
  | .TasksGet => [116, 97, 115, 107, 115, 95, 103, 101, 116]
  | .TasksUpdate => [116, 97, 115, 107, 115, 95, 117, 112, 100, 97, 116, 101]
  | .TasksCancel => [116, 97, 115, 107, 115, 95, 99, 97, 110, 99, 101, 108]
  | .AdminRead => [97, 100, 109, 105, 110, 95, 114, 101, 97, 100]
  | .AdminWrite => [97, 100, 109, 105, 110, 95, 119, 114, 105, 116, 101]

theorem kernel_spelling_bytes (action : contract.actions.HubAction) :
    (kernelSpelling action).toByteArray.toList =
      (kernelCodes action).map UInt8.ofNat := by
  cases action <;> decide +kernel

theorem kernel_spelling_chars (action : contract.actions.HubAction) :
    _root_.String.ofList ((kernelCodes action).map Char.ofNat) = kernelSpelling action := by
  cases action <;> decide +kernel

def actionLabel : contract.actions.PolicyAction → veoveo_types.actions.ActionName
  | .Kernel action => ⟨kernelSpelling action⟩
  | .Registered handle => handle.actionName

theorem action_name_exact (action : contract.actions.PolicyAction) :
    contract.actions.PolicyAction.name action = .ok (actionLabel action) := by
  cases action with
  | Kernel action =>
    have bytes := kernel_spelling_bytes action
    have chars := kernel_spelling_chars action
    cases action <;> simp only [kernelSpelling, kernelCodes] at bytes chars
    all_goals
      simp only [contract.actions.PolicyAction.name, contract.actions.HubAction.as_str,
        contract.actions.HubAction.Insts.Veoveo_typesVocabularyVocabulary.as_str, bind_ok]
      simp only [veoveo_types.actions.ActionName.parse, Shared0T.Insts.CoreConvertAsRef,
        Shared0T.Insts.CoreConvertAsRef.as_ref, Str.Insts.CoreConvertAsRefStr,
        Str.Insts.CoreConvertAsRefStr.as_ref, bind_ok, bind_tc_ok, bind_tc_eq]
      simp [-UScalar.bv_toNat, -U8.bv_toNat, Str, toStr, bytes, chars, UScalar.val,
        actionLabel, kernelSpelling, core.result.Result.expect, bind_ok, bind_tc_ok, bind_tc_eq]
  | Registered handle =>
    simp [contract.actions.PolicyAction.name, actionLabel,
      veoveo_types.actions.ActionHandle.name,
      veoveo_types.actions.ActionName.Insts.CoreCloneClone.clone, bind_ok, bind_tc_ok]

theorem target_clone_exact (target : Target) :
    control_plane.policy.PolicyTarget.Insts.CoreCloneClone.clone target = .ok target := by
  cases target <;>
    simp only [control_plane.policy.PolicyTarget.Insts.CoreCloneClone.clone,
      veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.PromptName.Insts.CoreCloneClone.clone,
      veoveo_hub_sdk.identity.CanonicalTaskId.Insts.CoreCloneClone.clone,
      veoveo_types.resource.ResourceUri.Insts.CoreCloneClone.clone,
      veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCloneClone.clone,
      veoveo_types.task.TaskId.Insts.CoreCloneClone.clone,
      contract.catalog.AdmittedPolicyTarget.Insts.CoreCloneClone.clone,
      contract.catalog.TargetAuditResource.Insts.CoreCloneClone.clone,
      control_plane.policy.DecodedPolicyTarget.Insts.CoreCloneClone.clone,
      veoveo_types.extensions.AdmittedExtensions.Insts.CoreCloneClone.clone,
      veoveo_types.extensions.ExtensionName.Insts.CoreCloneClone.clone,
      serde_json.value.Value.Insts.CoreCloneClone.clone, bind_ok, bind_tc_ok] <;> rfl

theorem tenant_clone_exact (tenant : Option veoveo_types.identity.TenantId) :
    core.option.Option.Insts.CoreCloneClone.clone
      veoveo_types.identity.TenantId.Insts.CoreCloneClone tenant = .ok tenant := by
  cases tenant <;>
    simp [core.option.Option.Insts.CoreCloneClone.clone,
      veoveo_types.identity.TenantId.Insts.CoreCloneClone,
      veoveo_types.identity.TenantId.Insts.CoreCloneClone.clone, bind_ok, bind_tc_ok]

/- The observed clock value is explicit. No fixed timestamp, freshness or native
clock implementation theorem is introduced. All decision fields are stated. -/
def expectedDecision (request : Request) (effect : control_plane.policy.PolicyEffect)
    (reason : control_plane.policy.PolicyReasonCode)
    (version : Option veoveo_types.identity.PolicyVersion)
    (rule : Option control_plane.ids.PolicyRuleId) (sample : Timestamp) : Decision where
  effect := effect
  reason := reason
  evaluated_at := sample
  profile := request.profile
  action := actionLabel request.action
  target := request.target
  principal := some request.principal.id
  tenant := request.principal.tenant
  policy_version := version
  rule_id := rule
  permission := none
  granted := ∅
  trace_id := request.trace_id

/- Result-trace equality retains native failure/divergence propagation too. -/
theorem decision_trace (request : Request) (effect : control_plane.policy.PolicyEffect)
    (reason : control_plane.policy.PolicyReasonCode)
    (version : Option veoveo_types.identity.PolicyVersion)
    (rule : Option control_plane.ids.PolicyRuleId) :
    policy.evaluation.decision request effect reason version rule = (do
      let sample ← chrono.offset.utc.Utc.now
      .ok (expectedDecision request effect reason version rule sample)) := by
  simp only [policy.evaluation.decision,
    veoveo_hub_sdk.identity.HubProfileId.Insts.CoreCloneClone.clone, action_name_exact,
    target_clone_exact, veoveo_types.identity.PrincipalId.Insts.CoreCloneClone.clone,
    tenant_clone_exact, alloc.collections.btree.set.BTreeSetTGlobal.new,
    control_plane.ids.TraceId.Insts.CoreCloneClone.clone,
    alloc.string.String.Insts.CoreCloneClone.clone, bind_ok, bind_tc_ok, bind_tc_eq]
  rfl

theorem decision_exact (request : Request) (effect : control_plane.policy.PolicyEffect)
    (reason : control_plane.policy.PolicyReasonCode)
    (version : Option veoveo_types.identity.PolicyVersion)
    (rule : Option control_plane.ids.PolicyRuleId) (sample : Timestamp)
    (clock : chrono.offset.utc.Utc.now = .ok sample) :
    policy.evaluation.decision request effect reason version rule =
      .ok (expectedDecision request effect reason version rule sample) := by
  rw [decision_trace, clock]
  simp only [bind_ok, bind_tc_ok]

theorem deny_exact (request : Request) (reason : control_plane.policy.PolicyReasonCode)
    (version : Option veoveo_types.identity.PolicyVersion) (sample : Timestamp)
    (clock : chrono.offset.utc.Utc.now = .ok sample) :
    policy.evaluation.deny request reason version =
      .ok (expectedDecision request .Deny reason version none sample) := by
  unfold policy.evaluation.deny
  exact decision_exact request .Deny reason version none sample clock

#print axioms registered_target_exact
#print axioms registered_target_coherent
#print axioms declared_route_trace
#print axioms kernel_spelling_bytes
#print axioms kernel_spelling_chars
#print axioms action_name_exact
#print axioms target_clone_exact
#print axioms tenant_clone_exact
#print axioms decision_trace
#print axioms decision_exact
#print axioms deny_exact

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let groups := #[
    (#[``registered_target_exact, ``registered_target_coherent,
       ``kernel_spelling_bytes, ``kernel_spelling_chars, ``action_name_exact,
       ``target_clone_exact, ``tenant_clone_exact], standard),
    (#[``declared_route_trace], standard ++
       #[``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
    ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]),
    (#[``decision_trace, ``decision_exact, ``deny_exact], standard ++
       #[``chrono.offset.utc.Utc.now])]
  for (declarations, allowed) in groups do
    for declaration in declarations do
      let found ← Lean.collectAxioms declaration
      let unexpected := found.filter (fun axiomName => !allowed.contains axiomName)
      unless unexpected.isEmpty do
        throwError "unreviewed proof axioms in {declaration}: {unexpected}"

end
end RegisteredServerPolicyProof
