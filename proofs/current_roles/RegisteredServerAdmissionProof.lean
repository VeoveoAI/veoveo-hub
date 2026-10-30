import RegisteredServerDecisionProof

open Aeneas Aeneas.Std veoveo_hub RegisteredServerPolicyProof RegisteredServerDecisionProof

namespace RegisteredServerAdmissionProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

def Satisfies (installation : Installation) (principal : Principal) (declaration : Declaration)
    (requirement : Option Requirement) (route : Option Access) : Prop :=
  match requirement with
  | none => ∃ permission, GrantedPermissionsProof.Grants installation principal declaration permission
  | some _ => match route with
    | none => False
    | some .Authenticated => True
    | some (.Permission permission) =>
      GrantedPermissionsProof.Grants installation principal declaration permission

/- Declarative admission, using the independently proved role/risk predicate.
No arbitrary Allowed decision or computed grant-set premise is supplied. -/
def Admitted (request : Request) (server : Slug) (requirement : Option Requirement)
    (installation : Installation) (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) : Prop :=
  eligible request.action = true ∧ ∃ declaration,
    declarations server = some declaration ∧
    Satisfies installation request.principal declaration requirement
      (observedRoute declarations access request.target)

theorem grants_nonempty (installation : Installation) (declaration : Declaration)
    (principal : Principal) :
    grants installation declaration principal ≠ ∅ ↔
      ∃ permission, GrantedPermissionsProof.Grants installation principal declaration permission := by
  constructor
  · intro nonempty
    obtain ⟨permission, member⟩ := Finset.nonempty_iff_ne_empty.mpr nonempty
    exact ⟨permission, (grants_membership installation declaration principal permission).mp member⟩
  · rintro ⟨permission, granted⟩ empty
    have member := (grants_membership installation declaration principal permission).mpr granted
    simpa [empty] using member

theorem expected_allow_iff (request : Request) (server : Slug) (requirement : Option Requirement)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) :
    (expectedServer request server requirement version sample installation declarations access).effect = .Allow ↔
      Admitted request server requirement installation declarations access := by
  cases enabled : eligible request.action with
  | false => simp [expectedServer, enabled, Admitted, expectedDecision]
  | true =>
    cases declarationEq : declarations server with
    | none => simp [expectedServer, enabled, declarationEq, Admitted, expectedDecision]
    | some declaration =>
      cases requirement with
      | none =>
        by_cases empty : grants installation declaration request.principal = ∅ <;>
          simp [expectedServer, enabled, declarationEq, Admitted, Satisfies, finish,
            expectedDecision, ← grants_nonempty, empty]
      | some target =>
        cases routeEq : observedRoute declarations access request.target with
        | none => simp [expectedServer, enabled, declarationEq, routeEq, Admitted, Satisfies, expectedDecision]
        | some route =>
          cases route with
          | Authenticated =>
            simp [expectedServer, enabled, declarationEq, routeEq, Admitted, Satisfies, finish, expectedDecision]
          | Permission permission =>
            by_cases member : permission ∈ grants installation declaration request.principal <;>
              simp [expectedServer, enabled, declarationEq, routeEq, Admitted, Satisfies,
                finish, expectedDecision, ← grants_membership, member]

theorem expected_denied_empty (request : Request) (server : Slug) (requirement : Option Requirement)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) :
    let decision := expectedServer request server requirement version sample installation declarations access
    decision.effect = .Deny → decision.granted = ∅ := by
  dsimp only
  cases enabled : eligible request.action with
  | false => simp [expectedServer, enabled, expectedDecision]
  | true =>
    cases declarationEq : declarations server with
    | none => simp [expectedServer, enabled, declarationEq, expectedDecision]
    | some declaration =>
      cases requirement with
      | none =>
        by_cases empty : grants installation declaration request.principal = ∅ <;>
          simp [expectedServer, enabled, declarationEq, finish, expectedDecision, empty]
      | some target =>
        cases routeEq : observedRoute declarations access request.target with
        | none => simp [expectedServer, enabled, declarationEq, routeEq, expectedDecision]
        | some route =>
          cases route with
          | Authenticated => simp [expectedServer, enabled, declarationEq, routeEq, finish, expectedDecision]
          | Permission permission =>
            by_cases member : permission ∈ grants installation declaration request.principal <;>
              simp [expectedServer, enabled, declarationEq, routeEq, finish, expectedDecision, member]

/- Normal caller coherence is DERIVED from the actual registered_target result,
then identifies the declaration used for grants and the SDK route query. -/
theorem coherent_route (target : Target) (server : Slug) (requirement : Requirement)
    (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) (declaration : Declaration)
    (coherent : policy.evaluation.registered_target target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration) :
    observedRoute declarations access target = access declaration requirement := by
  have projected := (registered_target_coherent target server (some requirement)).mp coherent
  simp [observedRoute, projected, selected]

theorem expected_permission_fields (request : Request) (server : Slug) (requirement : Requirement)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access)
    (declaration : Declaration) (permission : PermissionName)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : access declaration requirement = some (.Permission permission)) :
    let decision := expectedServer request server (some requirement) version sample installation declarations access
    decision.permission = some permission ∧
      (decision.effect = .Allow ↔
        GrantedPermissionsProof.Grants installation request.principal declaration permission) ∧
      (decision.effect = .Allow → permission ∈ decision.granted) ∧
      (decision.effect = .Deny → decision.granted = ∅) := by
  have route := coherent_route request.target server requirement declarations access declaration coherent selected
  dsimp only
  by_cases member : permission ∈ grants installation declaration request.principal <;>
    simp [expectedServer, enabled, selected, route, required, finish, expectedDecision,
      ← grants_membership, member]

section Actual
variable {C : Type} (ops : policy.catalog.PolicyCatalogView C) (catalog : C)
variable (installation : Installation) (declarations : Slug → Option Declaration)
variable (access : Declaration → Requirement → Option Access) (sample : Timestamp)
variable (lookup : ∀ slug, ops.server_authorization catalog slug = .ok (declarations slug))
variable (installed : ops.installation_authorization catalog = .ok installation)
variable (nativeAccess : ∀ declaration target,
  veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
    .ok (access declaration target))
variable (clock : chrono.offset.utc.Utc.now = .ok sample)

include lookup installed nativeAccess clock in
theorem actual_admitted_iff (request : Request) (server : Slug) (requirement : Option Requirement)
    (version : Version) :
    policy.evaluation.decide_server ops catalog request server requirement version ⦃ decision =>
      decision.effect = .Allow ↔ Admitted request server requirement installation declarations access ⦄ := by
  rw [decide_server_exact ops catalog request server requirement version installation declarations access sample
    lookup installed nativeAccess clock]
  simp only [WP.spec_ok]
  exact expected_allow_iff request server requirement version sample installation declarations access

include lookup installed nativeAccess clock in
theorem actual_denied_empty (request : Request) (server : Slug) (requirement : Option Requirement)
    (version : Version) :
    policy.evaluation.decide_server ops catalog request server requirement version ⦃ decision =>
      decision.effect = .Deny → decision.granted = ∅ ⦄ := by
  rw [decide_server_exact ops catalog request server requirement version installation declarations access sample
    lookup installed nativeAccess clock]
  simp only [WP.spec_ok]
  exact expected_denied_empty request server requirement version sample installation declarations access

include lookup installed nativeAccess clock in
theorem actual_permission_consistency (request : Request) (server : Slug) (requirement : Requirement)
    (version : Version) (declaration : Declaration) (permission : PermissionName)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : access declaration requirement = some (.Permission permission)) :
    policy.evaluation.decide_server ops catalog request server (some requirement) version ⦃ decision =>
      decision.permission = some permission ∧
        (decision.effect = .Allow ↔
          GrantedPermissionsProof.Grants installation request.principal declaration permission) ∧
        (decision.effect = .Allow → permission ∈ decision.granted) ∧
        (decision.effect = .Deny → decision.granted = ∅) ⦄ := by
  rw [decide_server_exact ops catalog request server (some requirement) version installation declarations access sample
    lookup installed nativeAccess clock]
  simp only [WP.spec_ok]
  exact expected_permission_fields request server requirement version sample installation declarations access
    declaration permission enabled coherent selected required
end Actual

#print axioms grants_nonempty
#print axioms expected_allow_iff
#print axioms expected_denied_empty
#print axioms coherent_route
#print axioms expected_permission_fields
#print axioms actual_admitted_iff
#print axioms actual_denied_empty
#print axioms actual_permission_consistency

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``grants_nonempty, ``expected_allow_iff, ``expected_denied_empty,
      ``coherent_route, ``expected_permission_fields] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unexpected pure admission axiom in {declaration}: {found}"
  let allowed := standard ++ #[``chrono.offset.utc.Utc.now,
    ``veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access]
  for declaration in #[``actual_admitted_iff, ``actual_denied_empty, ``actual_permission_consistency] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unexpected native admission axiom in {declaration}: {found}"

end
end RegisteredServerAdmissionProof
