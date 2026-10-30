import SDKResolverAdapterProof
import NativeSetProof
import CollectionProof
import AuthorizationProof

open Aeneas Aeneas.Std veoveo_hub

namespace SDKAccessProof
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

abbrev Access := veoveo_hub_sdk.registration.authorization.RouteAccess
abbrev Declaration := veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration
abbrev Target := veoveo_hub_sdk.registration.authorization.RequirementTarget
abbrev Template := veoveo_types.resource_template.ResourceTemplateUri
abbrev Uri := veoveo_types.resource.ResourceUri
abbrev Parsed := veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate

/-- Ordinary first-key lookup over raw model entries. No matcher, grant or route
eligibility is part of this projection. Native map invariants remain separate. -/
def lookupRaw {K V : Type} [DecidableEq K] (entries : List (K × V)) (query : K) : Option V :=
  entries.findSome? fun entry => if entry.1 = query then some entry.2 else none

theorem map_get_exact {K V : Type} [DecidableEq K]
    (operation : K → K → Result Ordering) (order : NativeSetProof.Comparison K)
    (law : NativeSetProof.LawfulComparison operation order)
    (entries : List (K × V)) (query : K) :
    CurrentRolesSDK.mapGetBy (fun key => .ok key) operation entries query =
      .ok (lookupRaw entries query) := by
  unfold NativeSetProof.LawfulComparison at law
  induction entries with
  | nil => simp [CurrentRolesSDK.mapGetBy, lookupRaw]
  | cons entry rest recursive =>
    rcases entry with ⟨key, value⟩
    simp only [CurrentRolesSDK.mapGetBy, law, bind_ok, lookupRaw, List.findSome?_cons]
    cases compared : order.compare key query with
    | eq =>
      have same := (order.equal_iff key query).mp compared
      simp [compared, same]
    | lt =>
      have different : key ≠ query := by
        intro same
        have equal := (order.equal_iff key query).mpr same
        simp [compared] at equal
      simpa [compared, different, lookupRaw] using recursive
    | gt =>
      have different : key ≠ query := by
        intro same
        have equal := (order.equal_iff key query).mpr same
        simp [compared] at equal
      simpa [compared, different, lookupRaw] using recursive

theorem tagged_lookup_exact {V : Type} (tag : Nat)
    (entries : List (CurrentRolesId tag × V)) (query : CurrentRolesId tag) :
    CurrentRolesSDK.mapGetBy (fun key => .ok key)
      (fun left right => .ok (compare left.value right.value)) entries query =
      .ok (lookupRaw entries query) := by
  exact map_get_exact _ (NativeSetProof.taggedComparison tag) (fun _ _ => rfl) entries query

def toolLookup := alloc.collections.btree.map.BTreeMap.get
  (V := Access) alloc.alloc.Global.Insts.CoreAllocAllocatorClone
  (core.borrow.Borrow.Blanket veoveo_hub_sdk.identity.LocalToolName)
  veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCmpOrd
  veoveo_hub_sdk.identity.LocalToolName.Insts.CoreCmpOrd

def promptLookup := alloc.collections.btree.map.BTreeMap.get
  (V := Access) alloc.alloc.Global.Insts.CoreAllocAllocatorClone
  (core.borrow.Borrow.Blanket veoveo_hub_sdk.identity.PromptName)
  veoveo_hub_sdk.identity.PromptName.Insts.CoreCmpOrd
  veoveo_hub_sdk.identity.PromptName.Insts.CoreCmpOrd

def templateLookup := alloc.collections.btree.map.BTreeMap.get
  (V := Access) alloc.alloc.Global.Insts.CoreAllocAllocatorClone
  (core.borrow.Borrow.Blanket Template)
  veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCmpOrd
  veoveo_types.resource_template.ResourceTemplateUri.Insts.CoreCmpOrd

def resourceLookup := alloc.collections.btree.map.BTreeMap.get
  (V := Access) alloc.alloc.Global.Insts.CoreAllocAllocatorClone
  (core.borrow.Borrow.Blanket Uri)
  veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd
  veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd

theorem tool_lookup_exact (entries : List (veoveo_hub_sdk.identity.LocalToolName × Access))
    (query : veoveo_hub_sdk.identity.LocalToolName) :
    toolLookup entries query = .ok (lookupRaw entries query) := by
  exact tagged_lookup_exact 8 entries query

theorem prompt_lookup_exact (entries : List (veoveo_hub_sdk.identity.PromptName × Access))
    (query : veoveo_hub_sdk.identity.PromptName) :
    promptLookup entries query = .ok (lookupRaw entries query) := by
  exact tagged_lookup_exact 9 entries query

theorem template_lookup_exact (entries : List (Template × Access)) (query : Template) :
    templateLookup entries query = .ok (lookupRaw entries query) := by
  exact tagged_lookup_exact 12 entries query

theorem resource_lookup_exact (entries : List (Uri × Access)) (query : Uri) :
    resourceLookup entries query = .ok (lookupRaw entries query) := by
  exact tagged_lookup_exact 13 entries query

def routeEq : core.cmp.PartialEq Access Access :=
  core.cmp.PartialEqShared
    veoveo_hub_sdk.registration.authorization.RouteAccess.Insts.CoreCmpPartialEqRouteAccess

theorem route_equality : ResolutionProof.LawfulEq routeEq := by
  constructor <;> intro left right
  · simp [routeEq, core.cmp.PartialEqShared, core.cmp.impls.PartialEqShared.eq,
      veoveo_hub_sdk.registration.authorization.RouteAccess.Insts.CoreCmpPartialEqRouteAccess.eq]
  · simp [routeEq, core.cmp.PartialEqShared, core.cmp.impls.PartialEqShared.ne,
      core.cmp.PartialEq.ne.trait_default, core.cmp.PartialEq.ne.default,
      veoveo_hub_sdk.registration.authorization.RouteAccess.Insts.CoreCmpPartialEqRouteAccess.eq]

def entriesCursor : sdk_authorization_proof.decision.Cursor
    (List (Template × Access)) (Template × Access) :=
  CurrentRolesSDK.cursorToSDK
    alloc.collections.btree.map.IterAResourceTemplateUriRouteAccess.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionCursorPairSharedAResourceTemplateUriSharedARouteAccess

def entriesModel : ResolutionProof.Enumeration
    (List (Template × Access)) (Template × Access) := { remaining := id }

theorem entries_cursor_law : ResolutionProof.LawfulCursor entriesCursor entriesModel := by
  intro entries
  cases entries <;> simp [entriesCursor, entriesModel, CurrentRolesSDK.cursorToSDK,
    alloc.collections.btree.map.IterAResourceTemplateUriRouteAccess.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionCursorPairSharedAResourceTemplateUriSharedARouteAccess.next]

def nativeMatcher : sdk_authorization_proof.decision.Matcher Uri Template :=
  CurrentRolesSDK.matcherToSDK
    veoveo_hub_sdk.registration.authorization.NativeMatcher.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionMatcherShared0ResourceTemplateUri

section DecodedNative
variable {K L S Q N C : Type}
variable (rootEq : core.cmp.PartialEq K K)
variable (segments : sdk_authorization_proof.decision.Segment S L)
variable (keys : sdk_authorization_proof.decision.Cursor C N)
variable (query : sdk_authorization_proof.decision.AllowedQuery Q N)
variable (parsed : Template → Option Parsed)
variable (decodeParsed : Parsed → Uri → Option (AuthorizationProof.DecodedView K L S Q C))

/-- Parsing and native projection stay explicit. The native-template law below
calls actual imported generated root/path/query helpers, not an allowed flag. -/
def decode (uri : Uri) (template : Template) :
    Option (AuthorizationProof.DecodedView K L S Q C) :=
  (parsed template).bind (fun value => decodeParsed value uri)

variable (parseLaw : ∀ template,
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse template =
    .ok (parsed template))
variable (templateTrace : ∀ value uri,
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches value uri =
    AuthorizationProof.decodedTrace rootEq segments keys query (decodeParsed value uri))

include parseLaw templateTrace in
theorem native_matcher_decoded_trace (uri : Uri) :
    AuthorizationProof.NativeMatcherTrace rootEq segments keys query nativeMatcher uri
      (decode parsed decodeParsed uri) := by
  intro template
  change
    veoveo_hub_sdk.registration.authorization.NativeMatcher.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionMatcherShared0ResourceTemplateUri.matches
      uri template = _
  rw [SDKResolverAdapterProof.native_matcher_trace, parseLaw]
  simp only [bind_tc_ok, bind_ok]
  cases result : parsed template with
  | none => simp [decode, result, AuthorizationProof.decodedTrace]
  | some value => simpa [decode, result] using templateTrace value uri

variable [DecidableEq K]
variable (rootLaw : ResolutionProof.LawfulEq rootEq)
variable (text : TemplateProof.TextModel S L)
variable (segmentLaw : TemplateProof.LawfulSegment segments text)
variable (keyModel : ResolutionProof.Enumeration C N)
variable (keyLaw : ResolutionProof.LawfulCursor keys keyModel)
variable (queryModel : TemplateProof.QueryModel Q N)
variable (queryLaw : TemplateProof.LawfulQuery query queryModel)

def resourceResult (uri : Uri) (exact : Option Access)
    (entries : List (Template × Access)) : Option Access :=
  match exact with
  | some access => some access
  | none => ResolutionProof.finish none (ResolutionProof.matching
      (fun template => decide (AuthorizationProof.DecodedAllowed text queryModel keyModel
        (decode parsed decodeParsed uri template))) entries)

def expectedAccess (declaration : Declaration) (target : Target) : Option Access :=
  match target with
  | .Tool tool => lookupRaw declaration.requirements.tools tool
  | .Prompt prompt => lookupRaw declaration.requirements.prompts prompt
  | .ResourceTemplate template => lookupRaw declaration.requirements.resource_templates template
  | .Resource uri => resourceResult (parsed := parsed) (decodeParsed := decodeParsed)
      (text := text) (keyModel := keyModel) (queryModel := queryModel) uri
      (lookupRaw declaration.requirements.resources uri) declaration.requirements.resource_templates

include rootLaw segmentLaw keyLaw queryLaw parseLaw templateTrace in
theorem resource_resolution_correct
    (uri : Uri) (exact : Option Access) (entries : List (Template × Access)) :
    sdk_authorization_proof.decision.resolve routeEq
      (Shared0T.Insts.CoreMarkerCopy Access) entriesCursor nativeMatcher exact entries uri
      ⦃ result => result.1 = resourceResult (parsed := parsed) (decodeParsed := decodeParsed)
          (text := text) (keyModel := keyModel) (queryModel := queryModel) uri exact entries ∧
        result.2.length ≤ entries.length ∧ (exact.isSome = true → result.2 = entries) ⦄ := by
  have native := native_matcher_decoded_trace rootEq segments keys query parsed decodeParsed
    parseLaw templateTrace uri
  have matcherLaw := AuthorizationProof.derived_matcher rootEq rootLaw segments text segmentLaw
    keys keyModel keyLaw query queryModel queryLaw nativeMatcher uri
    (decode parsed decodeParsed uri) native
  have correct := ResolutionProof.resolve_correct routeEq route_equality entriesCursor entriesModel
    entries_cursor_law nativeMatcher uri _ matcherLaw (Shared0T.Insts.CoreMarkerCopy Access)
    exact entries
  cases exact <;> simpa only [resourceResult, entriesModel, id_eq] using correct

include rootLaw segmentLaw keyLaw queryLaw parseLaw templateTrace in
/-- Full access agreement, including Authenticated versus Permission, comes
from the pinned SDK resolver theorem on raw entries and the derived matcher. -/
theorem resource_resolution_success_iff
    (uri : Uri) (exact : Option Access) (entries : List (Template × Access)) :
    sdk_authorization_proof.decision.resolve routeEq
      (Shared0T.Insts.CoreMarkerCopy Access) entriesCursor nativeMatcher exact entries uri
      ⦃ result => ∀ output, result.1 = some output ↔ exact = some output ∨
        (exact = none ∧ ∃ tail,
          ResolutionProof.matching
            (fun template => decide (AuthorizationProof.DecodedAllowed text queryModel keyModel
              (decode parsed decodeParsed uri template))) entries = output :: tail ∧
          ∀ other ∈ tail, other = output) ⦄ := by
  exact AuthorizationProof.authorization_success_iff rootEq rootLaw segments text segmentLaw
    keys keyModel keyLaw query queryModel queryLaw routeEq route_equality
    (Shared0T.Insts.CoreMarkerCopy Access) entriesCursor entriesModel entries_cursor_law
    nativeMatcher uri (decode parsed decodeParsed uri)
    (native_matcher_decoded_trace rootEq segments keys query parsed decodeParsed parseLaw
      templateTrace uri) exact entries

include rootLaw segmentLaw keyLaw queryLaw parseLaw templateTrace in
/-- Exact total public access for all four target kinds. Both absence and
complete access values are outputs; no whole-access interpretation is assumed. -/
theorem access_exact (declaration : Declaration) (target : Target) :
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
      .ok (expectedAccess (parsed := parsed) (decodeParsed := decodeParsed) (text := text)
        (keyModel := keyModel) (queryModel := queryModel) declaration target) := by
  apply CollectionProof.exact_of_total
  unfold veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access
  cases target with
  | Tool tool =>
    change (do
      let found ← toolLookup declaration.requirements.tools tool
      let branch ← core.option.Option.Insts.CoreOpsTry_traitTry.branch found
      match branch with
      | .Continue value => .ok (some value)
      | .Break residual =>
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual Access residual)
      ⦃ result => result = _ ⦄
    rw [tool_lookup_exact]
    cases found : lookupRaw declaration.requirements.tools tool <;>
      simp [bind_ok, bind_tc_ok, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
        expectedAccess, found]
  | Prompt prompt =>
    change (do
      let found ← promptLookup declaration.requirements.prompts prompt
      let branch ← core.option.Option.Insts.CoreOpsTry_traitTry.branch found
      match branch with
      | .Continue value => .ok (some value)
      | .Break residual =>
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual Access residual)
      ⦃ result => result = _ ⦄
    rw [prompt_lookup_exact]
    cases found : lookupRaw declaration.requirements.prompts prompt <;>
      simp [bind_ok, bind_tc_ok, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
        expectedAccess, found]
  | ResourceTemplate template =>
    change (do
      let found ← templateLookup declaration.requirements.resource_templates template
      let branch ← core.option.Option.Insts.CoreOpsTry_traitTry.branch found
      match branch with
      | .Continue value => .ok (some value)
      | .Break residual =>
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual Access residual)
      ⦃ result => result = _ ⦄
    rw [template_lookup_exact]
    cases found : lookupRaw declaration.requirements.resource_templates template <;>
      simp [bind_ok, bind_tc_ok, core.option.Option.Insts.CoreOpsTry_traitTry.branch,
        core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
        expectedAccess, found]
  | Resource uri =>
    have lookupEq : alloc.collections.btree.map.BTreeMap.get
        alloc.alloc.Global.Insts.CoreAllocAllocatorClone (core.borrow.Borrow.Blanket Uri)
        veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd
        veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd
        declaration.requirements.resources uri =
        .ok (lookupRaw declaration.requirements.resources uri) :=
      resource_lookup_exact declaration.requirements.resources uri
    simp only [lookupEq, alloc.collections.btree.map.BTreeMap.iter, bind_ok, bind_tc_ok,
      bind_tc_eq, Aeneas.Std.bind_assoc,
      veoveo_hub_sdk.registration.authorization_decision.resolve]
    apply WP.spec_bind (resource_resolution_correct rootEq segments keys query parsed decodeParsed
      parseLaw templateTrace rootLaw text segmentLaw keyModel keyLaw queryModel queryLaw uri
      (lookupRaw declaration.requirements.resources uri) declaration.requirements.resource_templates)
    intro result correct
    rcases result with ⟨found, remaining⟩
    cases found <;> simpa [core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
      bind_ok, bind_tc_ok, expectedAccess] using correct.1
end DecodedNative

/-- Exact resource access bypasses native parsing/matching altogether. This
statement has no parse/decode/template or cursor-totality hypothesis. -/
theorem resource_exact_hit (declaration : Declaration) (uri : Uri) (access : Access)
    (hit : lookupRaw declaration.requirements.resources uri = some access) :
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration
      (.Resource uri) = .ok (some access) := by
  unfold veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access
  have lookupEq : alloc.collections.btree.map.BTreeMap.get
      alloc.alloc.Global.Insts.CoreAllocAllocatorClone (core.borrow.Borrow.Blanket Uri)
      veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd
      veoveo_types.resource.ResourceUri.Insts.CoreCmpOrd
      declaration.requirements.resources uri =
      .ok (lookupRaw declaration.requirements.resources uri) :=
    resource_lookup_exact declaration.requirements.resources uri
  simp only [lookupEq, hit,
    alloc.collections.btree.map.BTreeMap.iter, bind_ok, bind_tc_ok,
    veoveo_hub_sdk.registration.authorization_decision.resolve,
    sdk_authorization_proof.decision.resolve]
  simp [core.option.Option.Insts.CoreOpsTry_traitTry.branch,
    core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual]

#print axioms map_get_exact
#print axioms tagged_lookup_exact
#print axioms tool_lookup_exact
#print axioms prompt_lookup_exact
#print axioms template_lookup_exact
#print axioms resource_lookup_exact
#print axioms route_equality
#print axioms entries_cursor_law
#print axioms native_matcher_decoded_trace
#print axioms resource_resolution_correct
#print axioms resource_resolution_success_iff
#print axioms access_exact
#print axioms resource_exact_hit

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := standard ++
    #[``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
      ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  for declaration in #[``map_get_exact, ``tagged_lookup_exact, ``tool_lookup_exact,
      ``prompt_lookup_exact, ``template_lookup_exact, ``resource_lookup_exact,
      ``route_equality, ``entries_cursor_law] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed primitive proof axioms in {declaration}: {found}"
  for declaration in #[``native_matcher_decoded_trace, ``resource_resolution_correct,
      ``resource_resolution_success_iff, ``access_exact, ``resource_exact_hit] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !native.contains axiomName)).isEmpty do
      throwError "unreviewed access proof axioms in {declaration}: {found}"

end
end SDKAccessProof
