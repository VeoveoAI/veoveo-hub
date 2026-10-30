import SDKAccessProof

open Aeneas Aeneas.Std veoveo_hub SDKAccessProof

namespace SDKAccessCorollaries
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

theorem matching_membership {T A : Type} (evaluate : T → Bool)
    (entries : List (T × A)) (output : A) :
    output ∈ ResolutionProof.matching evaluate entries ↔
      ∃ template, (template, output) ∈ entries ∧ evaluate template = true := by
  induction entries with
  | nil => simp
  | cons entry rest recursive =>
    rcases entry with ⟨template, access⟩
    cases matched : evaluate template <;>
      simp [ResolutionProof.matching_cons, matched, recursive] <;> aesop

theorem agreement_membership {A : Type} (hits : List A) (output : A) :
    (∃ tail, hits = output :: tail ∧ ∀ other ∈ tail, other = output) ↔
      output ∈ hits ∧ ∀ other ∈ hits, other = output := by
  constructor
  · rintro ⟨tail, rfl, agree⟩
    refine ⟨by simp, ?_⟩
    intro other member
    rcases List.mem_cons.mp member with same | member
    · exact same
    · exact agree other member
  · rintro ⟨member, agree⟩
    cases hits with
    | nil => simp at member
    | cons first rest =>
      have same : first = output := agree first (by simp)
      subst first
      exact ⟨rest, rfl, fun other member => agree other (List.mem_cons_of_mem _ member)⟩

variable {K L S Q N CKey : Type}
variable (parsed : Template → Option Parsed)
variable (decodeParsed : Parsed → Uri → Option (AuthorizationProof.DecodedView K L S Q CKey))
variable (text : TemplateProof.TextModel S L)
variable (keyModel : ResolutionProof.Enumeration CKey N)
variable (queryModel : TemplateProof.QueryModel Q N)

def TemplateMatches (uri : Uri) (template : Template) : Prop :=
  AuthorizationProof.DecodedAllowed text queryModel keyModel (decode parsed decodeParsed uri template)

/-- Exact entries take precedence. Otherwise at least one RAW matching entry
must exist, and every RAW matching occurrence must agree on the full value. -/
def ResourceDeclares (uri : Uri) (exact : Option Access)
    (entries : List (Template × Access)) (output : Access) : Prop :=
  exact = some output ∨ exact = none ∧
    (∃ template, (template, output) ∈ entries ∧
      TemplateMatches parsed decodeParsed text keyModel queryModel uri template) ∧
    ∀ template access, (template, access) ∈ entries →
      TemplateMatches parsed decodeParsed text keyModel queryModel uri template → access = output

def Declares (declaration : Declaration) (target : Target) (output : Access) : Prop :=
  match target with
  | .Tool tool => lookupRaw declaration.requirements.tools tool = some output
  | .Prompt prompt => lookupRaw declaration.requirements.prompts prompt = some output
  | .ResourceTemplate template => lookupRaw declaration.requirements.resource_templates template = some output
  | .Resource uri => ResourceDeclares parsed decodeParsed text keyModel queryModel uri
      (lookupRaw declaration.requirements.resources uri) declaration.requirements.resource_templates output

theorem resource_result_some_iff (uri : Uri) (exact : Option Access)
    (entries : List (Template × Access)) (output : Access) :
    resourceResult (parsed := parsed) (decodeParsed := decodeParsed) (text := text)
      (keyModel := keyModel) (queryModel := queryModel) uri exact entries = some output ↔
      ResourceDeclares parsed decodeParsed text keyModel queryModel uri exact entries output := by
  cases exact with
  | some access => simp [resourceResult, ResourceDeclares]
  | none =>
    simp [resourceResult, ResourceDeclares]
    rw [ResolutionProof.finish_success_iff, agreement_membership]
    have membership (value : Access) :
        value ∈ ResolutionProof.matching
          (fun template => decide (TemplateMatches parsed decodeParsed text keyModel queryModel uri template))
          entries ↔ ∃ template, (template, value) ∈ entries ∧
            TemplateMatches parsed decodeParsed text keyModel queryModel uri template := by
      simpa only [decide_eq_true_eq] using
        matching_membership
          (fun template => decide (TemplateMatches parsed decodeParsed text keyModel queryModel uri template))
          entries value
    constructor
    · rintro ⟨present, agree⟩
      refine ⟨(membership output).mp present, ?_⟩
      intro template access member matched
      exact agree access ((membership access).mpr ⟨template, member, matched⟩)
    · rintro ⟨present, agree⟩
      refine ⟨(membership output).mpr present, ?_⟩
      intro access member
      rcases (membership access).mp member with ⟨template, member, matched⟩
      exact agree template access member matched

theorem expected_some_iff (declaration : Declaration) (target : Target) (output : Access) :
    expectedAccess (parsed := parsed) (decodeParsed := decodeParsed) (text := text)
      (keyModel := keyModel) (queryModel := queryModel) declaration target = some output ↔
      Declares parsed decodeParsed text keyModel queryModel declaration target output := by
  cases target with
  | Tool tool => rfl
  | Prompt prompt => rfl
  | ResourceTemplate template => rfl
  | Resource uri => exact resource_result_some_iff parsed decodeParsed text keyModel queryModel uri _ _ output

variable [DecidableEq K]
variable (rootEq : core.cmp.PartialEq K K)
variable (segments : sdk_authorization_proof.decision.Segment S L)
variable (keys : sdk_authorization_proof.decision.Cursor CKey N)
variable (query : sdk_authorization_proof.decision.AllowedQuery Q N)
variable (parseLaw : ∀ template,
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse template = .ok (parsed template))
variable (templateTrace : ∀ value uri,
  veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches value uri =
    AuthorizationProof.decodedTrace rootEq segments keys query (decodeParsed value uri))
variable (rootLaw : ResolutionProof.LawfulEq rootEq)
variable (segmentLaw : TemplateProof.LawfulSegment segments text)
variable (keyLaw : ResolutionProof.LawfulCursor keys keyModel)
variable (queryLaw : TemplateProof.LawfulQuery query queryModel)

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw in
theorem access_some_iff (declaration : Declaration) (target : Target) (output : Access) :
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target =
      .ok (some output) ↔ Declares parsed decodeParsed text keyModel queryModel declaration target output := by
  rw [access_exact rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
    segmentLaw keyModel keyLaw queryModel queryLaw, Result.ok.injEq]
  exact expected_some_iff parsed decodeParsed text keyModel queryModel declaration target output

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw in
theorem access_none_iff (declaration : Declaration) (target : Target) :
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration target = .ok none ↔
      ¬ ∃ output, Declares parsed decodeParsed text keyModel queryModel declaration target output := by
  rw [access_exact rootEq segments keys query parsed decodeParsed parseLaw templateTrace rootLaw text
    segmentLaw keyModel keyLaw queryModel queryLaw, Result.ok.injEq]
  constructor
  · intro absent ⟨output, declared⟩
    have present := (expected_some_iff parsed decodeParsed text keyModel queryModel declaration target output).mpr declared
    simp [absent] at present
  · intro absent
    cases result : expectedAccess (parsed := parsed) (decodeParsed := decodeParsed) (text := text)
        (keyModel := keyModel) (queryModel := queryModel) declaration target with
    | none => rfl
    | some output =>
      exact False.elim (absent ⟨output,
        (expected_some_iff parsed decodeParsed text keyModel queryModel declaration target output).mp result⟩)

include parseLaw templateTrace rootLaw segmentLaw keyLaw queryLaw in
theorem resource_conflict (declaration : Declaration) (uri : Uri)
    (first second : Template) (left right : Access)
    (noExact : lookupRaw declaration.requirements.resources uri = none)
    (firstEntry : (first, left) ∈ declaration.requirements.resource_templates)
    (secondEntry : (second, right) ∈ declaration.requirements.resource_templates)
    (firstMatch : TemplateMatches parsed decodeParsed text keyModel queryModel uri first)
    (secondMatch : TemplateMatches parsed decodeParsed text keyModel queryModel uri second)
    (different : left ≠ right) :
    veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access declaration (.Resource uri) = .ok none := by
  apply (access_none_iff parsed decodeParsed text keyModel queryModel rootEq segments keys query parseLaw
    templateTrace rootLaw segmentLaw keyLaw queryLaw declaration (.Resource uri)).mpr
  rintro ⟨output, declared⟩
  simp [Declares, ResourceDeclares, noExact] at declared
  exact different ((declared.2 first left firstEntry firstMatch).trans
    (declared.2 second right secondEntry secondMatch).symm)

#print axioms matching_membership
#print axioms agreement_membership
#print axioms resource_result_some_iff
#print axioms expected_some_iff
#print axioms access_some_iff
#print axioms access_none_iff
#print axioms resource_conflict

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let native := standard ++
    #[``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
      ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  for declaration in #[``matching_membership, ``agreement_membership,
      ``resource_result_some_iff, ``expected_some_iff] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed declaration-spec axioms in {declaration}: {found}"
  for declaration in #[``access_some_iff, ``access_none_iff, ``resource_conflict] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !native.contains axiomName)).isEmpty do
      throwError "unreviewed public access axioms in {declaration}: {found}"

end
end SDKAccessCorollaries
