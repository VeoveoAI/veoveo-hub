import FactoryWatcherFixture
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher.Fixture

/-- The real current row/query are not changed by the fault. Only the metadata
used AFTER the same-target relevance check is changed to the old candidate. -/
def metadataFaultCurrent : Projection :=
  ⟨runnerKey, repositoryA, branchA, "main", some revision2, some frank, some linkedC, some carol⟩

def metadataFaultRow : Row := expectedRow repositoryA branchA "main" revision1 alice linkedE erin oldTree

def metadataFaultClient : Client := ⟨1, some alice, parentRepository, some repositoryA, some erin⟩

/-- Independent complete wrong result. It retains actual parent2/Frank backing
and SQL query, but visibly runs parent1/Alice source entries with its image and
writes the old parent metadata. Full client/provider/native/request values are
checked alongside the precise logical preparation arguments. -/
def WrongCurrentMetadataOutcome : Prop :=
  let first := firstParent false
  let second := later first
  FullValues first advancedRow (expectedNative advancedRequest)
    (expectedProviders frank repositoryA carol) [advancedRequest] advancedClient ∧
  afterRow first = advancedRow ∧ currentQuery first = .ok (some metadataFaultCurrent) ∧
  metadataFaultCurrent.linkedRepository = incoming.repository ∧ metadataFaultCurrent.trackedId = captured.trackedId ∧
  FullValues second metadataFaultRow (expectedNative obsoleteProgramRequest)
    (expectedProviders alice repositoryA erin) [advancedRequest, obsoleteProgramRequest] metadataFaultClient ∧
  second.factoryOutcome.isSome = true ∧
  second.factoryOutcome.bind (fun output => output.error) = none ∧
  (second.state.factory.createRequests 1).map (fun request => request.spec.command) =
    [expectedArgv repositoryA branchA newEntries, expectedArgv repositoryA branchA oldEntries] ∧
  Trace.logicalPrepare ⟨runnerKey, revision1, alice, uuid, repositoryA, linkedE, erin, branchA⟩ ∈ second.trace ∧
  metadataFaultRow ≠ resumedRow ∧ metadataFaultClient ≠ resumedClient ∧
  obsoleteProgramRequest.spec.command ≠ currentRequest.spec.command

set_option maxRecDepth 4096 in
/-- False on the baseline; kernel-checked only in the isolated metadata-fault
package. This witnesses the actual whole wrong represented output, not a proof
elaboration failure or a failed rewrite. -/
theorem lost_current_metadata_wrong_result : WrongCurrentMetadataOutcome := by
  unfold WrongCurrentMetadataOutcome FullValues
  decide

#print axioms lost_current_metadata_wrong_result
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let found ← Lean.collectAxioms ``lost_current_metadata_wrong_result
  let unexpected := found.filter (fun name => !allowed.contains name)
  unless unexpected.isEmpty do
    throwError "unexpected watcher wrong-metadata witness axiom: {unexpected}"

end FactoryRetarget.Watcher.Fixture
