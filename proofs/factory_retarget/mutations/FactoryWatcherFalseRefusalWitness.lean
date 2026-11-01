import FactoryWatcherFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher.Fixture

def refusalFaultCurrent : Projection :=
  ⟨runnerKey, repositoryA, branchA, "main", some revision2, some frank, some linkedC, some carol⟩

/-- True backing/query, same event target and a usable current-source observation
are retained. The overstrict captured-parent check refuses that legitimate event
and leaves the complete first-operation outputs instead of accepting the update. -/
def WrongCapturedParentRefusal : Prop :=
  let first := firstParent false
  let second := later first
  FullValues first advancedRow (expectedNative advancedRequest)
    (expectedProviders frank repositoryA carol) [advancedRequest] advancedClient ∧
  afterRow first = advancedRow ∧ currentQuery first = .ok (some refusalFaultCurrent) ∧
  refusalFaultCurrent.linkedRepository = incoming.repository ∧ refusalFaultCurrent.trackedId = captured.trackedId ∧
  refusalFaultCurrent.parentRevision = some revision2 ∧ refusalFaultCurrent.parentPusher = some frank ∧
  captured.parentRevision = some revision1 ∧ captured.parentPusher = some alice ∧
  (prepare ⟨runnerKey, revision2, frank, uuid, repositoryA, linkedE, erin, branchA⟩).map
    (fun source => (source.ownTree, source.ownEntries, source.factory.prepared.image)) =
    .ok (newTree, newEntries, image 'c') ∧
  second.exit = .skipped ∧ second.factoryOutcome.isNone = true ∧
  second.state.factory.rows 1 = some advancedRow ∧
  second.state.factory.sandboxes 1 = expectedNative advancedRequest ∧
  second.state.factory.providers 1 = expectedProviders frank repositoryA carol ∧
  second.state.factory.createRequests 1 = [advancedRequest] ∧
  (second.state.factory.rows 1).map client = some advancedClient ∧
  second.state.failures failureKey = none ∧ second.state.failures unrelatedKey = some unrelatedFailure ∧
  second.trace = [.acquired runnerKey, .query runnerKey, .queryReply (.ok (some refusalFaultCurrent)), .released runnerKey] ∧
  advancedRow ≠ resumedRow ∧ advancedClient ≠ resumedClient

set_option maxRecDepth 4096 in
/-- False on baseline. This isolated-fault witness proves a complete wrong refusal
while the required current target/source are usable. It imports no generic proof. -/
theorem captured_parent_guard_wrong_refusal : WrongCapturedParentRefusal := by
  unfold WrongCapturedParentRefusal FullValues
  decide

set_option maxRecDepth 4096 in
/-- Stronger whole-state equation for this explicit skipped branch: not merely a
few selected fields. This does not assert anything about native cleanup. -/
theorem captured_parent_guard_whole_state_noop :
    (later (firstParent false)).state = (firstParent false).state := by
  rfl

#print axioms captured_parent_guard_wrong_refusal
#print axioms captured_parent_guard_whole_state_noop
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``captured_parent_guard_wrong_refusal, ``captured_parent_guard_whole_state_noop] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected watcher false-refusal witness axiom: {unexpected}"

end FactoryRetarget.Watcher.Fixture
