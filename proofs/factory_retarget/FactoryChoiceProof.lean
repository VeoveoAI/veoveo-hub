import FactoryRetargetModel
import Lean.Util.CollectAxioms

namespace FactoryRetarget
variable {P : Type} [DecidableEq P]

/-- Independent source identity/content tests. No lifecycle implementation helper
appears in these contracts; same-name retargeting still changes target identity. -/
def SameTarget (r : Row) (q : Request) : Prop :=
  r.linkedRepository = some q.target.repository ∧
    r.trackedBranch = some q.target.trackedBranch

def SameContent (r : Row) (q : Request) : Prop :=
  r.ownTree = some q.ownTree ∧ r.linkedRevision = some q.target.revision

/-- Exact compared argv and attachment names. URL/UUID/script/native payload
interpretation remains the existing manual source-correspondence boundary. -/
def comparedCommand (r : Row) (q : Request) (p : Prepared P) : List String :=
  ["/bin/sh", "-c", p.runnerScript, "runner",
    (p.loreBase.setPath ("/" ++ r.repository)).render, r.branch,
    (p.loreBase.setPath ("/" ++ q.target.repository)).render,
    q.target.trackedBranch] ++ q.ownEntries

def comparedProviders (r : Row) (p : Prepared P) : List String :=
  [sandboxName r p ++ "-registration", sandboxName r p ++ "-lore"] ++ p.providers

/-- Precisely the non-image fields compared by native sorted-provider equality,
command equality, resources, driver and canonical policy. Perm retains duplicate
multiplicity; environment/restart policy and unspecified remainders are absent. -/
def ComparedFields (r : Row) (q : Request) (p : Prepared P)
    (spec : Configuration P) (template : Template P) : Prop :=
  template.resources = p.resources ∧ template.driver = p.driver ∧
    spec.policy = some p.policy ∧ spec.providers.Perm (comparedProviders r p) ∧
    spec.command = comparedCommand r q p

def KeepCondition (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : Prop :=
  ∃ old spec template, current = some old ∧ old.spec = some spec ∧
    spec.template = some template ∧ SameTarget r q ∧ SameContent r q ∧
    ComparedFields r q p spec template

def RestartCondition (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : Prop :=
  ∃ old spec template, current = some old ∧ old.spec = some spec ∧
    spec.template = some template ∧ SameTarget r q ∧
    ComparedFields r q p spec template ∧ template.image = p.image ∧ ¬SameContent r q

/-- Present workers are replaced exactly when no spec/template has the required
identity/fields plus either the same content or compatible image. Missing native
spec/template makes the existential false without inventing a default value. -/
def ReplaceCondition (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : Prop :=
  ∃ old, current = some old ∧ ¬∃ spec template,
    old.spec = some spec ∧ spec.template = some template ∧ SameTarget r q ∧
      ComparedFields r q p spec template ∧ (SameContent r q ∨ template.image = p.image)

/-- Actual retarget Boolean is linked repository OR tracked-ID difference. -/
theorem changed_false_iff_same_target (r : Row) (q : Request) :
    changed r q = false ↔ SameTarget r q := by
  simp [changed, SameTarget]

theorem beside_image_iff_compared (r : Row) (q : Request) (p : Prepared P)
    (spec : Configuration P) :
    besideImage spec (configuration r q p) = true ↔
      ∃ template, spec.template = some template ∧ ComparedFields r q p spec template := by
  cases found : spec.template <;>
    simp [besideImage, configuration, found, ComparedFields, comparedProviders,
      comparedCommand, command]

theorem runs_iff_compared_image (r : Row) (q : Request) (p : Prepared P)
    (spec : Configuration P) :
    runs spec (configuration r q p) = true ↔
      ∃ template, spec.template = some template ∧ ComparedFields r q p spec template ∧
        template.image = p.image := by
  cases found : spec.template <;>
    simp [runs, besideImage, configuration, found, ComparedFields, comparedProviders,
      comparedCommand, command, and_assoc]

theorem choice_create_iff_absent (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : choice r q p current = .create ↔ current = none := by
  cases native : current with
  | none => simp [choice]
  | some old =>
      cases spec : old.spec <;> simp [choice, spec]
      all_goals split_ifs <;> simp

set_option maxHeartbeats 600000 in
/-- Keep has precedence and does not compare the rebuilt image. -/
theorem choice_keep_iff (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : choice r q p current = .keep ↔ KeepCondition r q p current := by
  cases native : current with
  | none => simp [choice, KeepCondition]
  | some old =>
      cases spec : old.spec with
      | none => simp [choice, spec, KeepCondition]
      | some config =>
          cases template : config.template <;>
            simp [choice, spec, template, KeepCondition, SameTarget, SameContent,
              ComparedFields, comparedProviders, comparedCommand, changed, besideImage,
              runs, configuration, command, and_assoc]
          all_goals try split_ifs
          all_goals simp_all
          all_goals tauto

set_option maxHeartbeats 600000 in
/-- Compatible image alone is insufficient: keep wins for matching content. -/
theorem choice_restart_iff (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : choice r q p current = .restart ↔ RestartCondition r q p current := by
  cases native : current with
  | none => simp [choice, RestartCondition]
  | some old =>
      cases spec : old.spec with
      | none => simp [choice, spec, RestartCondition]
      | some config =>
          cases template : config.template <;>
            simp [choice, spec, template, RestartCondition, SameTarget, SameContent,
              ComparedFields, comparedProviders, comparedCommand, changed, besideImage,
              runs, configuration, command, and_assoc]
          all_goals try split_ifs
          all_goals simp_all
          all_goals tauto

set_option maxHeartbeats 600000 in
/-- Independent necessary-and-sufficient replacement condition, including all
present missing-spec/template, retarget and incompatible-field/image cases. -/
theorem choice_replace_iff (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : choice r q p current = .replace ↔ ReplaceCondition r q p current := by
  cases native : current with
  | none => simp [choice, ReplaceCondition]
  | some old =>
      cases spec : old.spec with
      | none => simp [choice, spec, ReplaceCondition]
      | some config =>
          cases template : config.template <;>
            simp [choice, spec, template, ReplaceCondition, SameTarget, SameContent,
              ComparedFields, comparedProviders, comparedCommand, changed, besideImage,
              runs, configuration, command, and_assoc]
          all_goals try split_ifs
          all_goals simp_all
          all_goals tauto

/-- Target identity differences force replacement even if every spec field and
image/content happen to match. No stronger freshness statement is implied. -/
theorem present_retarget_replaces (r : Row) (q : Request) (p : Prepared P)
    (old : Sandbox P) (different : ¬SameTarget r q) :
    choice r q p (some old) = .replace := by
  apply (choice_replace_iff r q p (some old)).mpr
  refine ⟨old, rfl, ?_⟩
  rintro ⟨spec, template, _, _, same, _⟩
  exact different same

theorem missing_spec_replaces (r : Row) (q : Request) (p : Prepared P)
    (old : Sandbox P) (missing : old.spec = none) :
    choice r q p (some old) = .replace := by
  apply (choice_replace_iff r q p (some old)).mpr
  simp [ReplaceCondition, missing]

theorem missing_template_replaces (r : Row) (q : Request) (p : Prepared P)
    (old : Sandbox P) (spec : Configuration P) (found : old.spec = some spec)
    (missing : spec.template = none) : choice r q p (some old) = .replace := by
  apply (choice_replace_iff r q p (some old)).mpr
  simp [ReplaceCondition, found, missing]

/-- Keeping identical program/content is valid even when rebuild yields a
completely different image string. The predicate never compares that field. -/
theorem keep_allows_rebuilt_image (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) (image : String)
    (eligible : KeepCondition r q p current) :
    choice r q { p with image := image } current = .keep := by
  apply (choice_keep_iff r q { p with image := image } current).mpr
  simpa [KeepCondition, ComparedFields, comparedProviders, comparedCommand, sandboxName] using eligible

/-- Permutation is the native sorted-list interpretation, including duplicates. -/
theorem choice_provider_permutation (r : Row) (q : Request) (p : Prepared P)
    (old : Sandbox P) (spec : Configuration P) (providers : List String)
    (permutation : providers.Perm spec.providers) :
    choice r q p (some { old with spec := some { spec with providers := providers } }) =
      choice r q p (some { old with spec := some spec }) := by
  have same (expected : List String) : providers.Perm expected ↔ spec.providers.Perm expected :=
    ⟨fun value => permutation.symm.trans value, fun value => permutation.trans value⟩
  cases template : spec.template <;>
    simp [choice, runs, besideImage, configuration, template, comparedProviders, same]

/-- An extra occurrence is different regardless of whether it was present before. -/
theorem providers_retain_multiplicity (provider : String) (providers : List String) :
    ¬(provider :: providers).Perm providers := by
  intro permutation
  have length := permutation.length_eq
  simp at length

theorem provider_length_difference_replaces (r : Row) (q : Request) (p : Prepared P)
    (old : Sandbox P) (spec : Configuration P) (found : old.spec = some spec)
    (different : spec.providers.length ≠ (comparedProviders r p).length) :
    choice r q p (some old) = .replace := by
  apply (choice_replace_iff r q p (some old)).mpr
  refine ⟨old, rfl, ?_⟩
  rintro ⟨config, template, bound, _, _, fields, _⟩
  have same : config = spec := Option.some.inj (bound.symm.trans found)
  subst config
  exact different fields.2.2.2.1.length_eq

/-- These fields are deliberately unexamined by the actual native comparisons.
No complete wanted-configuration equality is asserted for retained workers. -/
theorem choice_ignores_uncompared_config (r : Row) (q : Request) (p : Prepared P)
    (old : Sandbox P) (spec : Configuration P) (environment : List (String × String))
    (restartAlways : Bool) (specRemainder templateRemainder : P) :
    choice r q p (some { old with spec := some { spec with environment := environment, restartAlways := restartAlways, remainder := specRemainder, template := spec.template.map (fun value => { value with remainder := templateRemainder }) } }) =
      choice r q p (some { old with spec := some spec }) := by
  cases template : spec.template <;>
    simp [choice, runs, besideImage, configuration, template]

#print axioms choice_create_iff_absent
#print axioms choice_keep_iff
#print axioms choice_restart_iff
#print axioms choice_replace_iff
#print axioms keep_allows_rebuilt_image
#print axioms choice_provider_permutation
#print axioms provider_length_difference_replaces
#print axioms choice_ignores_uncompared_config

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``changed_false_iff_same_target, ``beside_image_iff_compared,
      ``runs_iff_compared_image, ``choice_create_iff_absent,
      ``choice_keep_iff, ``choice_restart_iff, ``choice_replace_iff,
      ``present_retarget_replaces, ``missing_spec_replaces, ``missing_template_replaces,
      ``keep_allows_rebuilt_image, ``choice_provider_permutation,
      ``providers_retain_multiplicity, ``provider_length_difference_replaces,
      ``choice_ignores_uncompared_config] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected lifecycle choice axiom in {declaration}: {unexpected}"

end FactoryRetarget
