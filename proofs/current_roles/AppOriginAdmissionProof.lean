import RegisteredServerAdmissionProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub
open RegisteredServerPolicyProof RegisteredServerDecisionProof

namespace AppOriginAdmissionProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Independent full decision for the fifth registered target. Declaration absence
and current permission absence remain distinct refusals. Surface metadata is not
an additional admission rule. -/
def expected (request : Request) (version : Version) (sample : Timestamp)
    (installation : Installation) (declaration : Declaration) : Decision :=
  match declaration.requirements.app_origin with
  | none => expectedDecision request .Deny .UndeclaredTarget version none sample
  | some .Authenticated => finish request version sample true none
      (grants installation declaration request.principal)
  | some (.Permission permission) => finish request version sample
      (decide (GrantedPermissionsProof.Grants installation request.principal declaration permission))
      (some permission) (grants installation declaration request.principal)

theorem access_exact (declaration : Declaration) :
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration .AppOrigin =
      .ok declaration.requirements.app_origin := by
  cases declared : declaration.requirements.app_origin <;>
    simp [veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access,
      core.option.Option.as_ref, declared, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
      bind_ok, bind_tc_ok]

section Actual
variable {C : Type} (ops : policy.catalog.PolicyCatalogView C) (catalog : C)
variable (request : Request) (server : Slug) (version : Version)
variable (installation : Installation) (declaration : Declaration) (sample : Timestamp)
variable (action : request.action = .Kernel .AppOrigin)
variable (target : request.target = .AppOrigin server)
variable (lookup : ops.server_authorization catalog server = .ok (some declaration))
variable (installed : ops.installation_authorization catalog = .ok installation)
variable (clock : chrono.offset.utc.Utc.now = .ok sample)

include action target lookup installed clock in
theorem actual_exact :
    policy.evaluation.decide_server ops catalog request server (some .AppOrigin) version =
      .ok (expected request version sample installation declaration) := by
  have route : policy.evaluation.declared_route_access ops catalog request.target =
      .ok declaration.requirements.app_origin := by
    rw [target, declared_route_trace]
    simp [declaredTrace, projectedTarget, lookup, access_exact, bind_ok, bind_tc_ok]
  cases declared : declaration.requirements.app_origin with
  | none =>
    simp [policy.evaluation.decide_server, action, kernel_eligible_exact, eligible,
      lookup, installed, grants_exact, route, declared, expected,
      deny_exact request _ _ sample clock, bind_ok, bind_tc_ok]
  | some access =>
    cases access with
    | Authenticated =>
      simp [policy.evaluation.decide_server, action, kernel_eligible_exact, eligible,
        lookup, installed, grants_exact, route, declared, expected, finish,
        decision_exact request _ _ _ _ sample clock, bind_ok, bind_tc_ok]
    | Permission permission =>
      have membership := grants_membership installation declaration request.principal permission
      by_cases granted : GrantedPermissionsProof.Grants installation request.principal declaration permission <;>
        simp [policy.evaluation.decide_server, action, kernel_eligible_exact, eligible,
          lookup, installed, grants_exact, route, declared, expected, finish,
          veoveo_types.permission_names.PermissionName.Insts.CoreCloneClone.clone,
          permission_contains_exact, membership, granted,
          decision_exact request _ _ _ _ sample clock, bind_ok, bind_tc_ok]

include action target lookup installed clock in
theorem authenticated_zero_grants
    (declared : declaration.requirements.app_origin = some .Authenticated)
    (empty : grants installation declaration request.principal = ∅) :
    policy.evaluation.decide_server ops catalog request server (some .AppOrigin) version =
      .ok (expectedDecision request .Allow .PolicyAllow version none sample) := by
  rw [actual_exact ops catalog request server version installation declaration sample
    action target lookup installed clock]
  simp [expected, declared, finish, empty, expectedDecision]

include action target lookup installed clock in
theorem permission_accepted (permission : PermissionName)
    (declared : declaration.requirements.app_origin = some (.Permission permission))
    (granted : GrantedPermissionsProof.Grants installation request.principal declaration permission) :
    policy.evaluation.decide_server ops catalog request server (some .AppOrigin) version =
      .ok { (expectedDecision request .Allow .PolicyAllow version none sample) with
        permission := some permission
        granted := grants installation declaration request.principal } := by
  rw [actual_exact ops catalog request server version installation declaration sample
    action target lookup installed clock]
  simp [expected, declared, granted, finish]

include action target lookup installed clock in
theorem permission_refused (permission : PermissionName)
    (declared : declaration.requirements.app_origin = some (.Permission permission))
    (missing : ¬ GrantedPermissionsProof.Grants installation request.principal declaration permission) :
    policy.evaluation.decide_server ops catalog request server (some .AppOrigin) version =
      .ok { (expectedDecision request .Deny .MissingPermission version none sample) with
        permission := some permission } := by
  rw [actual_exact ops catalog request server version installation declaration sample
    action target lookup installed clock]
  simp [expected, declared, missing, finish, expectedDecision]

include action target lookup installed clock in
theorem undeclared_refused (undeclared : declaration.requirements.app_origin = none) :
    policy.evaluation.decide_server ops catalog request server (some .AppOrigin) version =
      .ok (expectedDecision request .Deny .UndeclaredTarget version none sample) := by
  rw [actual_exact ops catalog request server version installation declaration sample
    action target lookup installed clock]
  simp [expected, undeclared]
end Actual
end
end AppOriginAdmissionProof

#print axioms AppOriginAdmissionProof.access_exact
#print axioms AppOriginAdmissionProof.actual_exact
#print axioms AppOriginAdmissionProof.authenticated_zero_grants
#print axioms AppOriginAdmissionProof.permission_accepted
#print axioms AppOriginAdmissionProof.permission_refused
#print axioms AppOriginAdmissionProof.undeclared_refused
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  -- AppOrigin's runtime arm only borrows its declared Option. The syntactic
  -- audit also traverses other arms of the shared access/decision definitions,
  -- which retain the existing resource-template parser/matcher boundary.
  let resourceBoundary := #[
    ``_root_.veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
    ``_root_.veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  let foundAccess ← Lean.collectAxioms ``AppOriginAdmissionProof.access_exact
  unless (foundAccess.filter (fun n => !(standard ++ resourceBoundary).contains n)).isEmpty do
    throwError "unexpected app-origin access axiom: {foundAccess}"
  for declaration in #[``AppOriginAdmissionProof.actual_exact,
      ``AppOriginAdmissionProof.authenticated_zero_grants,
      ``AppOriginAdmissionProof.permission_accepted, ``AppOriginAdmissionProof.permission_refused,
      ``AppOriginAdmissionProof.undeclared_refused] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun n => !(standard ++ resourceBoundary ++ #[``_root_.chrono.offset.utc.Utc.now]).contains n)).isEmpty do
      throwError "unexpected app-origin admission axiom in {declaration}: {found}"
