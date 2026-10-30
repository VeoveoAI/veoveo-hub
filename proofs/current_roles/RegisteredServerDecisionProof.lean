import RegisteredServerPolicyProof

open Aeneas Aeneas.Std veoveo_hub RegisteredServerPolicyProof NativeSetProof

namespace RegisteredServerDecisionProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Installation := control_plane.authorization_config.InstallationAuthorization
abbrev PermissionName := veoveo_types.permission_names.PermissionName
abbrev Principal := veoveo_hub_sdk.identity.Principal
abbrev Version := Option veoveo_types.identity.PolicyVersion

def eligible : contract.actions.PolicyAction → Bool
  | .Kernel .AdminRead | .Kernel .AdminWrite | .Registered _ => false
  | .Kernel _ => true

theorem kernel_eligible_exact (action : contract.actions.HubAction) :
    contract.actions.HubAction.targets_registered_server action = .ok (eligible (.Kernel action)) := by
  cases action <;>
    simp [contract.actions.HubAction.targets_registered_server,
      contract.actions.HubAction.mcp_method, core.option.Option.is_some, eligible, bind_ok]

/- This set is derived from the qualified actual role/risk evaluator. -/
def grants (installation : Installation) (declaration : Declaration) (principal : Principal) :
    Finset PermissionName :=
  GrantedPermissionsProof.expectedPermissions installation
    (GrantedPermissionsProof.selectedRoles installation
      (HeldRoleProof.expectedHeld principal installation.grants.val)) declaration

theorem grants_exact (installation : Installation) (declaration : Declaration) (principal : Principal) :
    policy.grants.granted_permissions installation declaration principal =
      .ok (grants installation declaration principal) :=
  GrantedPermissionsProof.granted_permissions_exact installation declaration principal

theorem grants_membership (installation : Installation) (declaration : Declaration)
    (principal : Principal) (permission : PermissionName) :
    permission ∈ grants installation declaration principal ↔
      GrantedPermissionsProof.Grants installation principal declaration permission := by
  have result := GrantedPermissionsProof.granted_permissions_membership installation declaration principal
  rw [grants_exact] at result
  simp only [WP.spec_ok] at result
  exact result permission

/- Observed native operations, not route semantics or an authorized flag.
The lookup/access arguments retain their actual independent target/server binding. -/
def observedRoute (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) (target : Target) : Option Access :=
  match projectedTarget target with
  | some (server, some requirement) => (declarations server).bind (fun declaration => access declaration requirement)
  | _ => none

theorem declared_route_observed {C : Type} (ops : policy.catalog.PolicyCatalogView C)
    (catalog : C) (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access)
    (lookup : ∀ server, ops.server_authorization catalog server = .ok (declarations server))
    (nativeAccess : ∀ declaration requirement,
      veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration requirement =
        .ok (access declaration requirement)) (target : Target) :
    policy.evaluation.declared_route_access ops catalog target =
      .ok (observedRoute declarations access target) := by
  rw [declared_route_trace]
  unfold declaredTrace observedRoute
  cases projectedTarget target with
  | none => rfl
  | some pair =>
    rcases pair with ⟨server, requirement⟩
    cases requirement with
    | none => rfl
    | some requirement =>
      dsimp only
      rw [lookup]
      simp only [bind_ok, bind_tc_ok]
      cases declarations server <;> simp [nativeAccess]

def missingReason : Requirement → control_plane.policy.PolicyReasonCode
  | .Tool _ => .UnknownTool
  | .Prompt _ => .UnknownPrompt
  | .Resource _ | .ResourceTemplate _ => .UndeclaredTarget

def finish (request : Request) (version : Version) (sample : Timestamp)
    (allowed : Bool) (required : Option PermissionName) (held : Finset PermissionName) : Decision :=
  { expectedDecision request (if allowed then .Allow else .Deny)
      (if allowed then .PolicyAllow else .MissingPermission) version none sample with
    permission := required
    granted := if allowed then held else ∅ }

/- Independent supplied server/requirement and request.target are intentional.
Only the later coherent-call corollary may identify them via registered_target. -/
def expectedServer (request : Request) (server : Slug) (requirement : Option Requirement)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) : Decision :=
  if eligible request.action then
    match declarations server with
    | none => expectedDecision request .Deny .UnknownServer version none sample
    | some declaration =>
      let held := grants installation declaration request.principal
      match requirement with
      | none => finish request version sample (decide (held ≠ ∅)) none held
      | some target =>
        match observedRoute declarations access request.target with
        | none => expectedDecision request .Deny (missingReason target) version none sample
        | some .Authenticated => finish request version sample true none held
        | some (.Permission permission) =>
          finish request version sample (decide (permission ∈ held)) (some permission) held
  else expectedDecision request .Deny .PolicyDeny version none sample

theorem permission_contains_exact (held : Finset PermissionName) (permission : PermissionName) :
    alloc.collections.btree.set.BTreeSet.contains alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      (core.borrow.Borrow.Blanket PermissionName)
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd held permission =
        .ok (decide (permission ∈ held)) := by
  have orderLaw : LawfulComparison
      veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd.cmp (taggedComparison 11) :=
    fun left right => (permission_operations_exact left right).1
  simpa using set_contains_exact alloc.alloc.Global.Insts.CoreAllocAllocatorClone
    (core.borrow.Borrow.Blanket PermissionName) id (by intro value; rfl)
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd
    veoveo_types.permission_names.PermissionName.Insts.CoreCmpOrd (taggedComparison 11)
    orderLaw held permission

@[simp] theorem expected_granted_empty (request : Request)
    (effect : control_plane.policy.PolicyEffect) (reason : control_plane.policy.PolicyReasonCode)
    (version : Version) (rule : Option control_plane.ids.PolicyRuleId) (sample : Timestamp) :
    (expectedDecision request effect reason version rule sample).granted = ∅ := rfl

theorem decide_server_exact {C : Type} (ops : policy.catalog.PolicyCatalogView C)
    (catalog : C) (request : Request) (server : Slug) (requirement : Option Requirement)
    (version : Version) (installation : Installation) (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) (sample : Timestamp)
    (lookup : ∀ slug, ops.server_authorization catalog slug = .ok (declarations slug))
    (installed : ops.installation_authorization catalog = .ok installation)
    (nativeAccess : ∀ declaration target,
      veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
        .ok (access declaration target))
    (clock : chrono.offset.utc.Utc.now = .ok sample) :
    policy.evaluation.decide_server ops catalog request server requirement version =
      .ok (expectedServer request server requirement version sample installation declarations access) := by
  have route := declared_route_observed ops catalog declarations access lookup nativeAccess request.target
  cases actionEq : request.action with
  | Registered handle =>
    simp [policy.evaluation.decide_server, actionEq, expectedServer, eligible,
      deny_exact request _ _ sample clock]
  | Kernel action =>
    cases enabled : eligible (.Kernel action) with
    | false =>
      simp [policy.evaluation.decide_server, actionEq, kernel_eligible_exact, enabled,
        expectedServer, deny_exact request _ _ sample clock, bind_ok]
    | true =>
      cases declarationEq : declarations server with
      | none =>
        simp [policy.evaluation.decide_server, actionEq, kernel_eligible_exact, enabled,
          lookup, declarationEq, expectedServer, deny_exact request _ _ sample clock, bind_ok]
      | some declaration =>
        cases requirement with
        | none =>
          by_cases empty : grants installation declaration request.principal = ∅ <;>
            simp [policy.evaluation.decide_server, actionEq, kernel_eligible_exact, enabled,
              lookup, declarationEq, installed, grants_exact,
              alloc.collections.btree.set.BTreeSet.is_empty, expectedServer, finish, empty,
              decision_exact request _ _ _ _ sample clock, bind_ok, bind_tc_ok]
        | some target =>
          cases routeEq : observedRoute declarations access request.target with
          | none =>
            cases target <;>
              simp [policy.evaluation.decide_server, actionEq, kernel_eligible_exact, enabled,
                lookup, declarationEq, installed, grants_exact, route, routeEq,
                expectedServer, missingReason, deny_exact request _ _ sample clock, bind_ok]
          | some routeAccess =>
            cases routeAccess with
            | Authenticated =>
              simp [policy.evaluation.decide_server, actionEq, kernel_eligible_exact, enabled,
                lookup, declarationEq, installed, grants_exact, route, routeEq,
                expectedServer, finish, decision_exact request _ _ _ _ sample clock, bind_ok]
            | Permission permission =>
              by_cases member : permission ∈ grants installation declaration request.principal <;>
                simp [policy.evaluation.decide_server, actionEq, kernel_eligible_exact, enabled,
                  lookup, declarationEq, installed, grants_exact, route, routeEq,
                  veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone.clone,
                  permission_contains_exact, expectedServer, finish, member,
                  decision_exact request _ _ _ _ sample clock, bind_ok]

#print axioms kernel_eligible_exact
#print axioms grants_exact
#print axioms grants_membership
#print axioms declared_route_observed
#print axioms expected_granted_empty
#print axioms permission_contains_exact
#print axioms decide_server_exact

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``kernel_eligible_exact, ``grants_exact, ``grants_membership,
      ``permission_contains_exact, ``expected_granted_empty] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unexpected pure proof axiom in {declaration}: {found}"
  let native := #[``chrono.offset.utc.Utc.now,
    ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
    ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  let routeAxioms ← Lean.collectAxioms ``declared_route_observed
  unless (routeAxioms.filter (fun axiomName =>
      !(standard ++ #[``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
    ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]).contains axiomName)).isEmpty do
    throwError "unexpected route observation axiom: {routeAxioms}"
  let found ← Lean.collectAxioms ``decide_server_exact
  unless (found.filter (fun axiomName => !(standard ++ native).contains axiomName)).isEmpty do
    throwError "unexpected decision proof axiom: {found}"

end
end RegisteredServerDecisionProof
