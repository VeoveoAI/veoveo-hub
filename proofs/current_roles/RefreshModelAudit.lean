import HubCurrentRoles
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

/- Actual transparent predicates and closure: ordinary nominal equality and the
existing timestamp/checked scalar models, with no native outcome axiom. -/
#print axioms store.refresh.RefreshFamily.bound_to
#print axioms store.refresh.deliverable
#print axioms store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool.call_once
#print axioms veoveo_hub_sdk.identity.HubRefreshFamilyId.Insts.CoreCmpPartialEqHubRefreshFamilyId.eq

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``store.refresh.RefreshFamily.bound_to, ``store.refresh.deliverable,
      ``store.refresh.deliverable.closure.Insts.CoreOpsFunctionFnOnceTupleShared1RefreshDeliveryEnvelopeBool.call_once,
      ``veoveo_hub_sdk.identity.HubRefreshFamilyId.Insts.CoreCmpPartialEqHubRefreshFamilyId.eq] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh model axiom in {declaration}: {found}"
