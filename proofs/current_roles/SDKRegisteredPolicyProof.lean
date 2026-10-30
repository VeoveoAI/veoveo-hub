import SDKAccessCorollaries
import RegisteredServerDownstreamProof

open Aeneas Aeneas.Std veoveo_hub hub_authority_proof
open RegisteredServerPolicyProof RegisteredServerDecisionProof RegisteredServerAdmissionProof

namespace SDKRegisteredPolicyProof
noncomputable section

variable {K L S Q N CKey : Type} [DecidableEq K]
variable (rootEq : core.cmp.PartialEq K K)
variable (segments : sdk_authorization_proof.decision.Segment S L)
variable (keys : sdk_authorization_proof.decision.Cursor CKey N)
variable (query : sdk_authorization_proof.decision.AllowedQuery Q N)
variable (parsed : SDKAccessProof.Template → Option SDKAccessProof.Parsed)
variable (decodeParsed : SDKAccessProof.Parsed → SDKAccessProof.Uri →
  Option (AuthorizationProof.DecodedView K L S Q CKey))
variable (parseLaw : ∀ template,
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse template =
    .ok (parsed template))
variable (templateTrace : ∀ value uri,
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches value uri =
    AuthorizationProof.decodedTrace rootEq segments keys query (decodeParsed value uri))
variable (rootLaw : ResolutionProof.LawfulEq rootEq)
variable (text : TemplateProof.TextModel S L)
variable (segmentLaw : TemplateProof.LawfulSegment segments text)
variable (keyModel : ResolutionProof.Enumeration CKey N)
variable (keyLaw : ResolutionProof.LawfulCursor keys keyModel)
variable (queryModel : TemplateProof.QueryModel Q N)
variable (queryLaw : TemplateProof.LawfulQuery query queryModel)

/-- This interpretation is raw declaration lookup plus the SDK's mathematical
full-value agreement specification; it is not supplied as an access oracle. -/
def routes : Declaration → Requirement → Option Access :=
  SDKAccessProof.expectedAccess (parsed := parsed) (decodeParsed := decodeParsed)
    (text := text) (keyModel := keyModel) (queryModel := queryModel)

theorem routes_declares (declaration : Declaration) (requirement : Requirement) (output : Access) :
    routes parsed decodeParsed text keyModel queryModel declaration requirement = some output ↔
      SDKAccessCorollaries.Declares parsed decodeParsed text keyModel queryModel
        declaration requirement output :=
  SDKAccessCorollaries.expected_some_iff parsed decodeParsed text keyModel queryModel declaration requirement output

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw in
theorem actual_access : ∀ declaration requirement,
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration requirement =
      .ok (routes parsed decodeParsed text keyModel queryModel declaration requirement) := by
  intro declaration requirement
  exact SDKAccessProof.access_exact rootEq segments keys query parsed decodeParsed parseLaw
    templateTrace rootLaw text segmentLaw keyModel keyLaw queryModel queryLaw declaration requirement

section Catalog
variable {Catalog : Type} (ops : policy.catalog.PolicyCatalogView Catalog) (catalog : Catalog)
variable (installation : Installation) (declarations : Slug → Option Declaration) (sample : Timestamp)
variable (lookup : ∀ slug, ops.server_authorization catalog slug = .ok (declarations slug))
variable (installed : ops.installation_authorization catalog = .ok installation)
variable (clock : chrono.offset.utc.Utc.now = .ok sample)

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw lookup installed clock in
/-- The old whole-access premise is constructed from actual SDK access. Raw
server/requirement arguments remain independent of request.target. -/
theorem decide_server_exact (request : Request) (server : Slug)
    (requirement : Option Requirement) (version : Version) :
    policy.evaluation.decide_server ops catalog request server requirement version =
      .ok (expectedServer request server requirement version sample installation declarations
        (routes parsed decodeParsed text keyModel queryModel)) := by
  exact RegisteredServerDecisionProof.decide_server_exact ops catalog request server requirement version
    installation declarations (routes parsed decodeParsed text keyModel queryModel) sample lookup installed
    (actual_access rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
      segmentLaw keyModel keyLaw queryModel queryLaw) clock

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw lookup installed clock in
theorem admitted_iff (request : Request) (server : Slug)
    (requirement : Option Requirement) (version : Version) :
    policy.evaluation.decide_server ops catalog request server requirement version ⦃ decision =>
      decision.effect = .Allow ↔ Admitted request server requirement installation declarations
        (routes parsed decodeParsed text keyModel queryModel) ⦄ := by
  exact RegisteredServerAdmissionProof.actual_admitted_iff ops catalog installation declarations
    (routes parsed decodeParsed text keyModel queryModel) sample lookup installed
    (actual_access rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
      segmentLaw keyModel keyLaw queryModel queryLaw) clock request server requirement version

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw lookup installed clock in
/-- Coherent target binding is explicit; accepted permission membership is
derived from actual grants and actual SDK access, not assumed for an Allowed input. -/
theorem permission_consistency (request : Request) (server : Slug) (requirement : Requirement)
    (version : Version) (declaration : Declaration) (permission : PermissionName)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : SDKAccessCorollaries.Declares parsed decodeParsed text keyModel queryModel
      declaration requirement (.Permission permission)) :
    policy.evaluation.decide_server ops catalog request server (some requirement) version ⦃ decision =>
      decision.permission = some permission ∧
        (decision.effect = .Allow ↔
          GrantedPermissionsProof.Grants installation request.principal declaration permission) ∧
        (decision.effect = .Allow → permission ∈ decision.granted) ∧
        (decision.effect = .Deny → decision.granted = ∅) ⦄ := by
  exact RegisteredServerAdmissionProof.actual_permission_consistency ops catalog installation declarations
    (routes parsed decodeParsed text keyModel queryModel) sample lookup installed
    (actual_access rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
      segmentLaw keyModel keyLaw queryModel queryLaw) clock request server requirement version declaration
    permission enabled coherent selected
    ((routes_declares parsed decodeParsed text keyModel queryModel declaration requirement
      (.Permission permission)).mpr required)

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw lookup installed clock in
theorem permission_downstream
    (permissionOps : authority.Permissions RegisteredServerDownstreamProof.Permissions PermissionName)
    (permissionLaw : AuthorityProof.LawfulPermissions permissionOps CurrentPermissionsProof.permissionSets)
    (request : Request) (server : Slug) (requirement : Requirement) (version : Version)
    (declaration : Declaration) (permission : PermissionName) (managed : Bool)
    (carried : RegisteredServerDownstreamProof.Permissions)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (required : SDKAccessCorollaries.Declares parsed decodeParsed text keyModel queryModel
      declaration requirement (.Permission permission)) :
    RegisteredServerDownstreamProof.serverTrace ops catalog permissionOps request server (some requirement)
      version managed (some (carried, .Permission permission)) ⦃ output =>
      output.permission = some permission ∧
        (output.effect = .Allow ↔ managed = true ∧
          GrantedPermissionsProof.Grants installation request.principal declaration permission ∧
          permission ∈ carried) ∧
        (output.effect = .Allow → output.granted =
          grants installation declaration request.principal ∩ carried ∧ permission ∈ output.granted) ∧
        RegisteredServerDownstreamProof.correlation output = RegisteredServerDownstreamProof.correlation
          (expectedServer request server (some requirement) version sample installation declarations
            (routes parsed decodeParsed text keyModel queryModel)) ⦄ := by
  exact RegisteredServerDownstreamProof.actual_permission_downstream ops catalog permissionOps permissionLaw
    request server requirement version installation declarations
    (routes parsed decodeParsed text keyModel queryModel) sample lookup installed
    (actual_access rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
      segmentLaw keyModel keyLaw queryModel queryLaw) clock declaration permission managed carried
    enabled coherent selected
    ((routes_declares parsed decodeParsed text keyModel queryModel declaration requirement
      (.Permission permission)).mpr required)

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw lookup installed clock in
theorem downstream_trace
    (permissionOps : authority.Permissions RegisteredServerDownstreamProof.Permissions PermissionName)
    (permissionLaw : AuthorityProof.LawfulPermissions permissionOps CurrentPermissionsProof.permissionSets)
    (request : Request) (server : Slug) (requirement : Option Requirement) (version : Version)
    (managed : Bool)
    (delegation : Option (RegisteredServerDownstreamProof.Permissions × RegisteredServerDownstreamProof.Route)) :
    RegisteredServerDownstreamProof.serverTrace ops catalog permissionOps request server requirement
      version managed delegation =
      .ok (RegisteredServerDownstreamProof.narrowed
        (expectedServer request server requirement version sample installation declarations
          (routes parsed decodeParsed text keyModel queryModel)) managed delegation) := by
  exact RegisteredServerDownstreamProof.server_trace_exact ops catalog permissionOps permissionLaw
    request server requirement version installation declarations
    (routes parsed decodeParsed text keyModel queryModel) sample lookup installed
    (actual_access rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
      segmentLaw keyModel keyLaw queryModel queryLaw) clock managed delegation

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw lookup installed clock in
/-- An actual coherent Authenticated route accepts with zero current grants,
including an empty carried set; every other output field is exact. -/
theorem authenticated_zero_grants
    (permissionOps : authority.Permissions RegisteredServerDownstreamProof.Permissions PermissionName)
    (permissionLaw : AuthorityProof.LawfulPermissions permissionOps CurrentPermissionsProof.permissionSets)
    (request : Request) (server : Slug) (requirement : Requirement) (version : Version)
    (declaration : Declaration) (carried : RegisteredServerDownstreamProof.Permissions)
    (enabled : eligible request.action = true)
    (coherent : policy.evaluation.registered_target request.target = .ok (some (server, some requirement)))
    (selected : declarations server = some declaration)
    (authenticated : SDKAccessCorollaries.Declares parsed decodeParsed text keyModel queryModel
      declaration requirement .Authenticated)
    (empty : grants installation declaration request.principal = ∅) :
    RegisteredServerDownstreamProof.serverTrace ops catalog permissionOps request server (some requirement)
      version true (some (carried, .Authenticated)) =
      .ok (expectedDecision request .Allow .PolicyAllow version none sample) := by
  have authenticatedEq := (routes_declares parsed decodeParsed text keyModel queryModel
    declaration requirement .Authenticated).mpr authenticated
  rw [downstream_trace rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
    segmentLaw keyModel keyLaw queryModel queryLaw ops catalog installation declarations sample lookup
    installed clock permissionOps permissionLaw request server (some requirement) version true
    (some (carried, .Authenticated))]
  rw [RegisteredServerDownstreamProof.expected_authenticated_downstream request server requirement
    version sample installation declarations (routes parsed decodeParsed text keyModel queryModel)
    declaration carried enabled coherent selected authenticatedEq]
  have route := coherent_route request.target server requirement declarations
    (routes parsed decodeParsed text keyModel queryModel) declaration coherent selected
  simp [expectedServer, enabled, selected, route, authenticatedEq, finish, empty, expectedDecision]
end Catalog

#print axioms actual_access
#print axioms routes_declares
#print axioms decide_server_exact
#print axioms admitted_iff
#print axioms permission_consistency
#print axioms permission_downstream
#print axioms downstream_trace
#print axioms authenticated_zero_grants

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := #[``chrono.offset.utc.Utc.now,
    ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
    ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  let declarationAxioms ← Lean.collectAxioms ``routes_declares
  unless (declarationAxioms.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
    throwError "unreviewed declaration interpretation axioms: {declarationAxioms}"
  let accessAxioms ← Lean.collectAxioms ``actual_access
  let accessAllowed := standard ++
    #[``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
      ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  unless (accessAxioms.filter (fun axiomName => !accessAllowed.contains axiomName)).isEmpty do
    throwError "unreviewed SDK access axioms: {accessAxioms}"
  for declaration in #[``decide_server_exact, ``admitted_iff,
      ``permission_consistency, ``permission_downstream, ``downstream_trace,
      ``authenticated_zero_grants] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !(standard ++ native).contains axiomName)).isEmpty do
      throwError "unreviewed SDK-policy composition axioms in {declaration}: {found}"

end
end SDKRegisteredPolicyProof
