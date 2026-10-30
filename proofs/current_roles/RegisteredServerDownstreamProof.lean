import RegisteredServerAdmissionProof
import CurrentPermissionsProof

open Aeneas Aeneas.Std veoveo_hub hub_authority_proof
open RegisteredServerPolicyProof RegisteredServerDecisionProof RegisteredServerAdmissionProof

namespace RegisteredServerDownstreamProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Permissions := Finset PermissionName
abbrev KernelDecision := authority.Decision Permissions control_plane.policy.PolicyReasonCode Decision
abbrev Route := authority.Route PermissionName

/- Source correspondence to mcp::authorization::narrow_policy_decision:
mem::take empties the metadata grant field BEFORE calling the extracted kernel.
This is a mathematical wrapper projection; the async/native wrapper itself is
not extracted here. Both extracted decision functions below are called directly. -/
def projectDecision (base : Decision) : KernelDecision :=
  { allowed := decide (base.effect = .Allow), reason := base.reason,
    granted := base.granted, metadata := { base with granted := ∅ } }

def reconstruct (output : KernelDecision) : Decision :=
  { output.metadata with
    effect := if output.allowed then .Allow else .Deny
    reason := output.reason
    granted := output.granted }

/- All correlation/requirement fields, including the sampled timestamp, are
retained. Only effect/reason/granted are reconstructed by the native wrapper. -/
def correlation (decision : Decision) : Decision :=
  { decision with effect := .Deny, reason := .PolicyDeny, granted := ∅ }

def narrowed (base : Decision) (managed : Bool)
    (delegation : Option (Permissions × Route)) : Decision :=
  reconstruct (AuthorityProof.expectedDecision CurrentPermissionsProof.permissionSets
    (projectDecision base) managed delegation .PolicyDeny .MissingPermission)

def narrowTrace (ops : authority.Permissions Permissions PermissionName) (base : Decision)
    (managed : Bool) (delegation : Option (Permissions × Route)) : Result Decision := do
  let output ← authority.narrow ops (projectDecision base) managed delegation .PolicyDeny .MissingPermission
  .ok (reconstruct output)

theorem narrow_trace_exact (ops : authority.Permissions Permissions PermissionName)
    (law : AuthorityProof.LawfulPermissions ops CurrentPermissionsProof.permissionSets)
    (base : Decision) (managed : Bool) (delegation : Option (Permissions × Route)) :
    narrowTrace ops base managed delegation = .ok (narrowed base managed delegation) := by
  unfold narrowTrace
  rw [AuthorityProof.narrow_exact CurrentPermissionsProof.permissionSets ops law]
  simp only [bind_ok, bind_tc_ok, narrowed]

theorem narrowed_admitted_iff (base : Decision) (managed : Bool)
    (delegation : Option (Permissions × Route)) :
    (narrowed base managed delegation).effect = .Allow ↔
      managed = true ∧ base.effect = .Allow ∧ ∀ carried route,
        delegation = some (carried, route) →
        AuthorityProof.RouteAllowed CurrentPermissionsProof.permissionSets carried
          (base.granted ∩ carried) route := by
  cases managed <;> cases effect : base.effect <;> cases delegation with
  | none => simp [narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision, effect]
  | some pair =>
    rcases pair with ⟨carried, route⟩
    by_cases permitted : AuthorityProof.RouteAllowed CurrentPermissionsProof.permissionSets
        carried (base.granted ∩ carried) route <;>
      simp_all [narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision,
        CurrentPermissionsProof.permissionSets]

theorem narrowed_correlation (base : Decision) (managed : Bool)
    (delegation : Option (Permissions × Route)) :
    correlation (narrowed base managed delegation) = correlation base := by
  cases managed <;> cases effect : base.effect <;> cases delegation with
  | none => simp [correlation, narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision, effect]
  | some pair =>
    rcases pair with ⟨carried, route⟩
    by_cases permitted : AuthorityProof.RouteAllowed CurrentPermissionsProof.permissionSets
        carried (base.granted ∩ carried) route <;>
      simp_all [correlation, narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision,
        CurrentPermissionsProof.permissionSets]

theorem narrowed_managed_false (base : Decision) (delegation : Option (Permissions × Route)) :
    narrowed base false delegation = { base with effect := .Deny, reason := .PolicyDeny } := by
  simp [narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision]

theorem narrowed_base_denied (base : Decision) (delegation : Option (Permissions × Route))
    (denied : base.effect = .Deny) : narrowed base true delegation = base := by
  cases base
  simp_all [narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision]

theorem narrowed_granted (base : Decision) (managed : Bool) (carried : Permissions) (route : Route)
    (accepted : (narrowed base managed (some (carried, route))).effect = .Allow) :
    (narrowed base managed (some (carried, route))).granted = base.granted ∩ carried := by
  rcases (narrowed_admitted_iff base managed (some (carried, route))).mp accepted with
    ⟨enabled, allowed, permitted⟩
  have permitted := permitted carried route rfl
  simp_all [narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision,
    CurrentPermissionsProof.permissionSets]

theorem members_empty (permissions : Permissions) :
    CurrentPermissionsProof.permissionSets.members permissions = ∅ ↔ permissions = ∅ := by
  constructor
  · intro empty
    apply Finset.ext
    intro permission
    constructor
    · intro member
      have observed : permission ∈ CurrentPermissionsProof.permissionSets.members permissions := member
      rw [empty] at observed
      exact observed.elim
    · intro impossible
      simpa using impossible
  · intro empty
    subst permissions
    ext permission
    simp [CurrentPermissionsProof.permissionSets]

theorem intersection_members_empty (left right : Permissions) :
    (↑left : Set PermissionName) ∩ ↑right = ∅ ↔ left ∩ right = ∅ := by
  have representation : (↑left : Set PermissionName) ∩ ↑right =
      CurrentPermissionsProof.permissionSets.members (left ∩ right) := by
    ext permission
    simp [CurrentPermissionsProof.permissionSets]
  rw [representation, members_empty]

theorem authenticated_accepts_empty (base : Decision) (carried : Permissions)
    (allowed : base.effect = .Allow) :
    (narrowed base true (some (carried, .Authenticated))).effect = .Allow ∧
      (narrowed base true (some (carried, .Authenticated))).granted = base.granted ∩ carried := by
  have accepted : (narrowed base true (some (carried, .Authenticated))).effect = .Allow :=
    (narrowed_admitted_iff base true _).mpr
      (by simp [allowed, AuthorityProof.RouteAllowed])
  exact ⟨accepted, narrowed_granted base true carried .Authenticated accepted⟩

theorem server_admitted_iff (base : Decision) (managed : Bool) (carried : Permissions) :
    (narrowed base managed (some (carried, .Server))).effect = .Allow ↔
      managed = true ∧ base.effect = .Allow ∧ base.granted ∩ carried ≠ ∅ := by
  rw [narrowed_admitted_iff]
  simp [AuthorityProof.RouteAllowed, members_empty]

theorem permission_admitted_iff (base : Decision) (managed : Bool) (carried : Permissions)
    (permission : PermissionName) :
    (narrowed base managed (some (carried, .Permission permission))).effect = .Allow ↔
      managed = true ∧ base.effect = .Allow ∧ permission ∈ carried := by
  rw [narrowed_admitted_iff]
  simp [AuthorityProof.RouteAllowed, CurrentPermissionsProof.permissionSets]

/- Exact coherent permission-route result. Current permission consistency is
DERIVED from the qualified actual policy evaluator, not assumed for an arbitrary
Allowed input. A denied base retains its original missing-permission reason and
required Some(permission), and managed=false still takes precedence. -/
theorem expected_permission_downstream (request : Request) (server : Slug) (requirement : Requirement)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration) (access : Declaration → Requirement → Option Access)
    (declaration : Declaration) (permission : PermissionName) (managed : Bool) (carried : Permissions)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : access declaration requirement = some (.Permission permission)) :
    let base := expectedServer request server (some requirement) version sample installation declarations access
    let output := narrowed base managed (some (carried, .Permission permission))
    output.permission = some permission ∧
      (output.effect = .Allow ↔ managed = true ∧
        GrantedPermissionsProof.Grants installation request.principal declaration permission ∧
        permission ∈ carried) ∧
      (output.effect = .Allow → output.granted =
        grants installation declaration request.principal ∩ carried ∧ permission ∈ output.granted) ∧
      correlation output = correlation base := by
  have route := coherent_route request.target server requirement declarations access declaration coherent selected
  dsimp only
  by_cases member : permission ∈ grants installation declaration request.principal <;>
    cases managed <;> by_cases carriedMember : permission ∈ carried <;>
      simp [expectedServer, enabled, selected, route, required, finish,
        RegisteredServerPolicyProof.expectedDecision, narrowed, reconstruct, projectDecision,
        AuthorityProof.expectedDecision, AuthorityProof.RouteAllowed, correlation,
        CurrentPermissionsProof.permissionSets, ← grants_membership, member, carriedMember]

/- A valid Authenticated route accepts even when the actual derived G, the
carried set, or their intersection is empty. The entire other record survives. -/
theorem expected_authenticated_downstream (request : Request) (server : Slug) (requirement : Requirement)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration) (access : Declaration → Requirement → Option Access)
    (declaration : Declaration) (carried : Permissions)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (authenticated : access declaration requirement = some .Authenticated) :
    let base := expectedServer request server (some requirement) version sample installation declarations access
    narrowed base true (some (carried, .Authenticated)) =
      { base with granted := grants installation declaration request.principal ∩ carried } := by
  have route := coherent_route request.target server requirement declarations access declaration coherent selected
  simp [expectedServer, enabled, selected, route, authenticated, narrowed, reconstruct,
    projectDecision, AuthorityProof.expectedDecision, AuthorityProof.RouteAllowed,
    CurrentPermissionsProof.permissionSets, finish, RegisteredServerPolicyProof.expectedDecision]

/- The registered_target premise covers Server, Task and PlatformTask through
the checked projection. No arbitrary nonempty-G premise is needed: narrowing
accepts exactly a nonempty intersection of actual current grants and carried. -/
theorem expected_server_downstream (request : Request) (server : Slug)
    (version : Version) (sample : Timestamp) (installation : Installation)
    (declarations : Slug → Option Declaration) (access : Declaration → Requirement → Option Access)
    (declaration : Declaration) (managed : Bool) (carried : Permissions)
    (enabled : eligible request.action = true)
    (_coherent : policy.evaluation.registered_target request.target = .ok (some (server, none)))
    (selected : declarations server = some declaration) :
    let base := expectedServer request server none version sample installation declarations access
    let output := narrowed base managed (some (carried, .Server))
    (output.effect = .Allow ↔ managed = true ∧
      grants installation declaration request.principal ∩ carried ≠ ∅) ∧
      (output.effect = .Allow → output.granted =
        grants installation declaration request.principal ∩ carried) := by
  dsimp only
  cases managed <;>
    by_cases empty : grants installation declaration request.principal = ∅ <;>
    by_cases intersectionEmpty : grants installation declaration request.principal ∩ carried = ∅ <;>
    simp_all [expectedServer, finish, RegisteredServerPolicyProof.expectedDecision,
      narrowed, reconstruct, projectDecision, AuthorityProof.expectedDecision,
      AuthorityProof.RouteAllowed, CurrentPermissionsProof.permissionSets,
      intersection_members_empty]

def serverTrace {C : Type} (catalogOps : policy.catalog.PolicyCatalogView C) (catalog : C)
    (permissionOps : authority.Permissions Permissions PermissionName) (request : Request)
    (server : Slug) (requirement : Option Requirement) (version : Version)
    (managed : Bool) (delegation : Option (Permissions × Route)) : Result Decision := do
  let base ← policy.evaluation.decide_server catalogOps catalog request server requirement version
  narrowTrace permissionOps base managed delegation

theorem server_trace_exact {C : Type} (catalogOps : policy.catalog.PolicyCatalogView C) (catalog : C)
    (permissionOps : authority.Permissions Permissions PermissionName)
    (permissionLaw : AuthorityProof.LawfulPermissions permissionOps CurrentPermissionsProof.permissionSets)
    (request : Request) (server : Slug) (requirement : Option Requirement) (version : Version)
    (installation : Installation) (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) (sample : Timestamp)
    (lookup : ∀ slug, catalogOps.server_authorization catalog slug = .ok (declarations slug))
    (installed : catalogOps.installation_authorization catalog = .ok installation)
    (nativeAccess : ∀ declaration target,
      veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
        .ok (access declaration target))
    (clock : chrono.offset.utc.Utc.now = .ok sample)
    (managed : Bool) (delegation : Option (Permissions × Route)) :
    serverTrace catalogOps catalog permissionOps request server requirement version managed delegation =
      .ok (narrowed (expectedServer request server requirement version sample installation declarations access)
        managed delegation) := by
  unfold serverTrace
  rw [decide_server_exact catalogOps catalog request server requirement version installation declarations access
    sample lookup installed nativeAccess clock]
  simpa only [bind_ok, bind_tc_ok] using
    narrow_trace_exact permissionOps permissionLaw
      (expectedServer request server requirement version sample installation declarations access)
      managed delegation

theorem actual_permission_downstream {C : Type} (catalogOps : policy.catalog.PolicyCatalogView C)
    (catalog : C) (permissionOps : authority.Permissions Permissions PermissionName)
    (permissionLaw : AuthorityProof.LawfulPermissions permissionOps CurrentPermissionsProof.permissionSets)
    (request : Request) (server : Slug) (requirement : Requirement) (version : Version)
    (installation : Installation) (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) (sample : Timestamp)
    (lookup : ∀ slug, catalogOps.server_authorization catalog slug = .ok (declarations slug))
    (installed : catalogOps.installation_authorization catalog = .ok installation)
    (nativeAccess : ∀ declaration target,
      veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
        .ok (access declaration target))
    (clock : chrono.offset.utc.Utc.now = .ok sample)
    (declaration : Declaration) (permission : PermissionName) (managed : Bool) (carried : Permissions)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : access declaration requirement = some (.Permission permission)) :
    serverTrace catalogOps catalog permissionOps request server (some requirement) version
        managed (some (carried, .Permission permission)) ⦃ output =>
      output.permission = some permission ∧
        (output.effect = .Allow ↔ managed = true ∧
          GrantedPermissionsProof.Grants installation request.principal declaration permission ∧
          permission ∈ carried) ∧
        (output.effect = .Allow → output.granted =
          grants installation declaration request.principal ∩ carried ∧ permission ∈ output.granted) ∧
        correlation output = correlation (expectedServer request server (some requirement) version sample
          installation declarations access) ⦄ := by
  rw [server_trace_exact catalogOps catalog permissionOps permissionLaw request server (some requirement)
    version installation declarations access sample lookup installed nativeAccess clock]
  simp only [WP.spec_ok]
  exact expected_permission_downstream request server requirement version sample installation declarations
    access declaration permission managed carried enabled coherent selected required

/- Direct reuse of the qualified authority survival theorem after discharging
its former current-policy consistency premise from the ACTUAL decide_server.
The result metadata is precisely the emptied-grant native wrapper projection. -/
theorem actual_permission_survives {C : Type} (catalogOps : policy.catalog.PolicyCatalogView C)
    (catalog : C) (permissionOps : authority.Permissions Permissions PermissionName)
    (permissionLaw : AuthorityProof.LawfulPermissions permissionOps CurrentPermissionsProof.permissionSets)
    (request : Request) (server : Slug) (requirement : Requirement) (version : Version)
    (installation : Installation) (declarations : Slug → Option Declaration)
    (access : Declaration → Requirement → Option Access) (sample : Timestamp)
    (lookup : ∀ slug, catalogOps.server_authorization catalog slug = .ok (declarations slug))
    (installed : catalogOps.installation_authorization catalog = .ok installation)
    (nativeAccess : ∀ declaration target,
      veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
        .ok (access declaration target))
    (clock : chrono.offset.utc.Utc.now = .ok sample)
    (declaration : Declaration) (permission : PermissionName) (carried : Permissions)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : access declaration requirement = some (.Permission permission)) :
    policy.evaluation.decide_server catalogOps catalog request server (some requirement) version ⦃ base =>
      authority.narrow permissionOps (projectDecision base) true (some (carried, .Permission permission))
        .PolicyDeny .MissingPermission ⦃ output => output.allowed = true →
          permission ∈ output.granted ∧ output.metadata = { base with granted := ∅ } ⦄ ⦄ := by
  apply WP.spec_mono (actual_permission_consistency catalogOps catalog installation declarations access sample
    lookup installed nativeAccess clock request server requirement version declaration permission
    enabled coherent selected required)
  intro base consistent
  cases effect : base.effect with
  | Allow =>
    simpa only [CurrentPermissionsProof.permissionSets, projectDecision, effect, Set.mem_ofPred_eq] using
      AuthorityProof.accepted_permission_survives CurrentPermissionsProof.permissionSets permissionOps
        permissionLaw (projectDecision base) carried permission .PolicyDeny .MissingPermission
        (consistent.2.2.1 effect)
  | Deny =>
    rw [AuthorityProof.narrow_exact CurrentPermissionsProof.permissionSets permissionOps permissionLaw]
    simp [AuthorityProof.expectedDecision, projectDecision, effect]

#print axioms narrow_trace_exact
#print axioms narrowed_admitted_iff
#print axioms narrowed_correlation
#print axioms narrowed_managed_false
#print axioms narrowed_base_denied
#print axioms narrowed_granted
#print axioms authenticated_accepts_empty
#print axioms server_admitted_iff
#print axioms permission_admitted_iff
#print axioms expected_permission_downstream
#print axioms expected_authenticated_downstream
#print axioms expected_server_downstream
#print axioms server_trace_exact
#print axioms actual_permission_downstream
#print axioms actual_permission_survives

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``narrow_trace_exact, ``narrowed_admitted_iff, ``narrowed_correlation,
      ``narrowed_managed_false, ``narrowed_base_denied, ``narrowed_granted,
      ``authenticated_accepts_empty, ``server_admitted_iff, ``permission_admitted_iff,
      ``expected_permission_downstream, ``expected_authenticated_downstream, ``expected_server_downstream] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unexpected downstream axiom in {declaration}: {found}"
  let allowed := standard ++ #[``chrono.offset.utc.Utc.now,
    ``veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access]
  for declaration in #[``server_trace_exact, ``actual_permission_downstream, ``actual_permission_survives] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unexpected native downstream axiom in {declaration}: {found}"

end
end RegisteredServerDownstreamProof
