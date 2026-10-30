import HubCurrentRoles
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

namespace SDKResolverAdapterProof

universe u v w z

/-- Field-exact adapters preserve arbitrary operations, not only successful ones. -/
theorem cursor_operation {C : Type u} {I : Type v}
    (ops : veoveo_hub_sdk.registration.authorization_decision.Cursor C I) (state : C) :
    (CurrentRolesSDK.cursorToSDK ops).next state = ops.next state := rfl

theorem matcher_operation {M : Type u} {T : Type v}
    (ops : veoveo_hub_sdk.registration.authorization_decision.Matcher M T)
    (state : M) (template : T) :
    (CurrentRolesSDK.matcherToSDK ops).«matches» state template =
      ops.«matches» state template := rfl

/-- This connects two Lean definitions. The external-call model delegates to the
SDK-owned generated resolver; it does not prove Rust compilation/linking. -/
theorem resolver_model_link {T : Type u} {A : Type v} {C : Type w} {M : Type z}
    (eq : core.cmp.PartialEq A A) (copy : core.marker.Copy A)
    (cursor : veoveo_hub_sdk.registration.authorization_decision.Cursor C (T × A))
    (matcher : veoveo_hub_sdk.registration.authorization_decision.Matcher M T)
    (exact : Option A) (candidates : C) (state : M) :
    veoveo_hub_sdk.registration.authorization_decision.resolve eq copy cursor matcher
      exact candidates state =
      sdk_authorization_proof.decision.resolve eq copy
        (CurrentRolesSDK.cursorToSDK cursor) (CurrentRolesSDK.matcherToSDK matcher)
        exact candidates state := rfl

/-- Actual extracted parse/Option/closure wiring, retaining native failure and
divergence. Neither native operation is assigned a fabricated result. -/
theorem native_matcher_trace
    (uri : veoveo_hub_sdk.registration.authorization.NativeMatcher)
    (template : veoveo_types.resource_template.ResourceTemplateUri) :
    veoveo_hub_sdk.registration.authorization.NativeMatcher.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionMatcherShared0ResourceTemplateUri.matches
      uri template = (do
        let parsed ← veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse
          template
        match parsed with
        | none => .ok false
        | some value =>
          veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches value uri) := by
  simp only [
    veoveo_hub_sdk.registration.authorization.NativeMatcher.Insts.Veoveo_hub_sdkRegistrationAuthorization_decisionMatcherShared0ResourceTemplateUri.matches,
    core.option.Option.is_some_and,
    veoveo_hub_sdk.registration.authorization.MatcherNativeMatcherShared0ResourceTemplateUri.matches.closure.Insts.CoreOpsFunctionFnOnceTupleAuthorizationTemplateBool,
    veoveo_hub_sdk.registration.authorization.MatcherNativeMatcherShared0ResourceTemplateUri.matches.closure.Insts.CoreOpsFunctionFnOnceTupleAuthorizationTemplateBool.call_once]
  apply congrArg (fun continuation : Option
      veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate → Result Bool =>
    Aeneas.Std.bind
      (veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse template)
      continuation)
  funext parsed
  cases parsed <;> rfl

#print axioms cursor_operation
#print axioms matcher_operation
#print axioms resolver_model_link
#print axioms native_matcher_trace

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``cursor_operation, ``matcher_operation, ``resolver_model_link,
      ``veoveo_hub_sdk.registration.authorization_decision.resolve] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !standard.contains axiomName)).isEmpty do
      throwError "unreviewed resolver-model axioms in {declaration}: {found}"
  let native := standard ++
    #[``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.parse,
      ``veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate.matches]
  for declaration in #[``native_matcher_trace,
      ``veoveo_hub_sdk.registration.authorization.AuthorizationDeclaration.access] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !native.contains axiomName)).isEmpty do
      throwError "unreviewed SDK access axioms in {declaration}: {found}"

end SDKResolverAdapterProof
