import FactoryWatcherHistoryProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher
variable {P : Type} [DecidableEq P]

/-- Resolve refusal can still successfully apply recorded content. Its requested
linked repository and full failure payload remain independent of returned target. -/
theorem recorded_fallback_channels (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (bound : s.factory.rows d = some backing)
    (current : Projection) (fallback : Resolved P) (error : Error)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false) (query : o.query = .ok (some current))
    (resolution : o.resolve input (some current) = .error error)
    (recorded : o.recorded input current = some fallback) :
    let q := parentRequest input fallback
    let output := parentLink d backing input o s bound
    output.exit = .prepared ∧ output.factoryOutcome = some (apply d backing q fallback.factory s.factory) ∧
    output.state.factory = (apply d backing q fallback.factory s.factory).state ∧
    output.state.failures = statusAfter s.failures (publicationKey input.key input.watchedDeployment)
      (some ⟨input.requestedRepository, input.revision, error.message, o.publication.failedAt⟩) o.publication.committed ∧
    Trace.logicalRecorded input current ∈ output.trace := by
  cases committed : o.publication.committed <;>
    simp [parentLink, query, active, resolution, recorded, attempt, change, publication,
      publish, close, committed, parentRequest]
  all_goals funext other; simp [statusAfter]

theorem successful_fallback_records_resolve_failure (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (bound : s.factory.rows d = some backing)
    (key : backing.deployment = d) (runner : Runner backing)
    (current : Projection) (fallback : Resolved P) (error : Error)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false) (query : o.query = .ok (some current))
    (resolution : o.resolve input (some current) = .error error)
    (recorded : o.recorded input current = some fallback)
    (noFault : fallback.factory.fault = none) (capacity : CapacityAllowed fallback.factory fallback.factory.readNative) :
    let q := parentRequest input fallback
    let output := parentLink d backing input o s bound
    (apply d backing q fallback.factory s.factory).error = none ∧
    FactoryValues d backing q fallback.factory s.factory output.state.factory ∧
    (output.state.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
      backing.repository, some fallback.target.repository, some fallback.target.pusher⟩ ∧
    output.state.failures = statusAfter s.failures (publicationKey input.key input.watchedDeployment)
      (some ⟨input.requestedRepository, input.revision, error.message, o.publication.failedAt⟩) o.publication.committed := by
  let q := parentRequest input fallback
  have accepted := no_fault_capacity_accepts d backing q fallback.factory s.factory bound noFault capacity
  have values := successful_outputs_complete d backing q fallback.factory s.factory bound key runner accepted
  have channels := recorded_fallback_channels d backing input o s bound current fallback error active change publication query resolution recorded
  refine ⟨accepted, ?_, ?_, channels.2.2.2.1⟩
  · rw [channels.2.2.1]
    exact values
  · rw [channels.2.2.1, values.1]
    simp [successRow, client, q, parentRequest]

/-- Exact failure publication after logical preparation refusal; Factory state is
untouched but the full status may commit even when its acknowledgement is lost. -/
theorem preparation_refusal_channels (d : Deployment) (backing : Row) (candidate current : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (revision pusher : String) (linked : LinkedPush) (error : Error)
    (active : o.cancellation.afterLock = false) (publication : o.cancellation.beforePublication = false)
    (read : o.query = .ok (some current)) (repository : current.linkedRepository = notice.repository)
    (tracked : current.trackedId = candidate.trackedId)
    (parentRevision : current.parentRevision = some revision) (parentPusher : current.parentPusher = some pusher)
    (pushed : notice.pushed = some linked)
    (prepared : o.prepare (arguments current notice revision pusher linked) = .error error) :
    let output := resume d backing candidate notice o s
    output.exit = .preparationRefused error ∧ output.factoryOutcome = none ∧ output.state.factory = s.factory ∧
    output.state.failures = statusAfter s.failures (publicationKey candidate.key notice.watchedDeployment)
      (some ⟨notice.repository, revision, error.message, o.publication.failedAt⟩) o.publication.committed ∧
    Trace.log .preparation error ∈ output.trace := by
  cases committed : o.publication.committed <;>
    simp [resume, active, read, repository, tracked, parentRevision, parentPusher, pushed,
      prepared, publication, publish, close, committed]
  all_goals funext other; simp [statusAfter]

/-- A later committed write replaces the whole previous failure at this key,
including linked identity, revision, message and timestamp. -/
theorem status_last_commit (before : FailureKey → Option Failure) (key : FailureKey)
    (old next : Option Failure) (committed : Bool) :
    statusAfter (statusAfter before key old committed) key next true = statusAfter before key next true := by
  funext other
  by_cases same : other = key <;> simp [statusAfter, same]

/-- The existing ready operation swallows a Factory error while reporting logical
preparation success and clearing the watcher status according to native commit.
The failed full row remains the exact inherited stopped-prefix result. -/
theorem factory_failure_is_not_preparation_failure (d : Deployment) (backing : Row)
    (candidate current : Projection) (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (revision pusher : String) (linked : LinkedPush) (source : Source P) (error : Error)
    (bound : s.factory.rows d = some backing)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false) (read : o.query = .ok (some current))
    (repository : current.linkedRepository = notice.repository) (tracked : current.trackedId = candidate.trackedId)
    (parentRevision : current.parentRevision = some revision) (parentPusher : current.parentPusher = some pusher)
    (pushed : notice.pushed = some linked)
    (prepared : o.prepare (arguments current notice revision pusher linked) = .ok source)
    (failed : (apply d backing
      (request current revision pusher linked (preparedName current (arguments current notice revision pusher linked) o).1 source)
      source.factory s.factory).error = some error) :
    let q := request current revision pusher linked (preparedName current (arguments current notice revision pusher linked) o).1 source
    let output := resume d backing candidate notice o s
    output.exit = .prepared ∧ output.factoryOutcome = some (apply d backing q source.factory s.factory) ∧
    output.state.failures = statusAfter s.failures (publicationKey candidate.key notice.watchedDeployment) none o.publication.committed ∧
    ∃ before first later, program backing q source.factory source.factory.readNative = before ++ first :: later ∧
      ClearPrefix d backing q source.factory before s.factory ∧
      observedError d source.factory first (foldEffects d backing q source.factory before s.factory) = some error ∧
      output.state.factory.rows d = some (refusedRow source.factory first error (rowFold before backing)) := by
  let q := request current revision pusher linked
    (preparedName current (arguments current notice revision pusher linked) o).1 source
  have channels := ready_resume_channels d backing candidate current notice o s revision pusher linked source
    active change publication read repository tracked parentRevision parentPusher pushed prepared
  obtain ⟨before, first, later, shape, clear, refused, row⟩ :=
    failed_row_prefix d backing q source.factory _ s.factory error bound failed
  refine ⟨channels.1, channels.2.1, channels.2.2.2.1, before, first, later, shape, clear, refused, ?_⟩
  rw [channels.2.2.1]
  exact row

/-- Explicit completed SQL deletion boundary, not the whole native remove protocol.
The only operation described here is the row delete, framing all other rows. -/
def rowDeleted (d : Deployment) (s : WatchState P) : WatchState P :=
  { s with factory := { s.factory with rows := fun other => if other = d then none else s.factory.rows other } }

theorem completed_delete_has_no_key (d : Deployment) (backing : Row) (key : RunnerKey)
    (s : WatchState P) (bound : RowAt key s.factory d backing) (unique : UniqueKey s.factory) :
    ¬∃ other row, RowAt key (rowDeleted d s).factory other row := by
  rintro ⟨other, row, stored, fields⟩
  by_cases same : other = d
  · subst other
    simp [rowDeleted] at stored
  · have before : s.factory.rows other = some row := by simpa [rowDeleted, same] using stored
    have id := (unique key d other backing row bound ⟨before, fields⟩).1
    exact same id.symm

theorem completed_delete_then_obsolete (d : Deployment) (backing : Row) (candidate : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (bound : RowAt candidate.key s.factory d backing) (unique : UniqueKey s.factory)
    (read : ReadAgrees candidate.key (rowDeleted d s).factory o.query) :
    (resume d backing candidate notice o (rowDeleted d s)).state = rowDeleted d s ∧
    (resume d backing candidate notice o (rowDeleted d s)).factoryOutcome = none :=
  missing_key_no_resurrection d backing candidate notice o (rowDeleted d s) read
    (completed_delete_has_no_key d backing candidate.key s bound unique)

/-- Actual preparation refusal followed by an actual successful parent operation.
The second binding and old status are derived from the first call. The first
publication may commit despite a lost reply; the second may fail without clearing
it. Whole-payload clearing is conditional on the second commit, never inferred
from Factory success. Different watched deployments are distinct status keys. -/
theorem refusal_then_parent_history (d : Deployment) (backing : Row)
    (candidate current : Projection) (notice : Notice) (firstObs : ResumeObservations P)
    (s : WatchState P) (revision pusher : String) (linked : LinkedPush) (error : Error)
    (input : ParentInput) (parent : ParentObservations P) (resolved : Resolved P)
    (bound : s.factory.rows d = some backing) (key : backing.deployment = d) (runner : Runner backing)
    (active : firstObs.cancellation.afterLock = false)
    (publication : firstObs.cancellation.beforePublication = false)
    (read : firstObs.query = .ok (some current)) (repository : current.linkedRepository = notice.repository)
    (tracked : current.trackedId = candidate.trackedId)
    (parentRevision : current.parentRevision = some revision) (parentPusher : current.parentPusher = some pusher)
    (pushed : notice.pushed = some linked)
    (refused : firstObs.prepare (arguments current notice revision pusher linked) = .error error)
    (parentActive : parent.cancellation.afterLock = false) (parentChange : parent.cancellation.beforeChange = false)
    (parentPublication : parent.cancellation.beforePublication = false)
    (resolution : parent.resolve input (queriedExisting parent.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative)
    (sameStatusKey : publicationKey input.key input.watchedDeployment = publicationKey candidate.key notice.watchedDeployment) :
    let first := resume d backing candidate notice firstObs s
    let failedKey := publicationKey candidate.key notice.watchedDeployment
    let failure := some (Failure.mk notice.repository revision error.message firstObs.publication.failedAt)
    first.exit = .preparationRefused error ∧
    first.state.failures = statusAfter s.failures failedKey failure firstObs.publication.committed ∧
    ∃ nextBinding : first.state.factory.rows d = some backing,
      let second := parentLink d backing input parent first.state nextBinding
      second.exit = .prepared ∧
      FactoryValues d backing (parentRequest input resolved) resolved.factory first.state.factory second.state.factory ∧
      (second.state.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
        backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ ∧
      second.state.failures = statusAfter (statusAfter s.failures failedKey failure firstObs.publication.committed)
        failedKey none parent.publication.committed ∧
      (parent.publication.committed = true → second.state.failures = statusAfter s.failures failedKey none true) ∧
      ∀ other, other ≠ failedKey → second.state.failures other = s.failures other := by
  let first := resume d backing candidate notice firstObs s
  let failedKey := publicationKey candidate.key notice.watchedDeployment
  let failure := some (Failure.mk notice.repository revision error.message firstObs.publication.failedAt)
  have refusedChannels := preparation_refusal_channels d backing candidate current notice firstObs s revision pusher linked error
    active publication read repository tracked parentRevision parentPusher pushed refused
  have nextBinding : first.state.factory.rows d = some backing := by
    rw [refusedChannels.2.2.1]
    exact bound
  refine ⟨refusedChannels.1, refusedChannels.2.2.2.1, nextBinding, ?_⟩
  have full := parent_full_output d backing input parent first.state resolved nextBinding key runner
    parentActive parentChange parentPublication resolution noFault capacity
  let second := parentLink d backing input parent first.state nextBinding
  have status : second.state.failures = statusAfter (statusAfter s.failures failedKey failure firstObs.publication.committed)
      failedKey none parent.publication.committed := by
    rw [full.2.2.2.2.2, sameStatusKey, refusedChannels.2.2.2.1]
  refine ⟨full.1, full.2.2.2.1, full.2.2.2.2.1, status, ?_, ?_⟩
  · intro committed
    rw [status, committed, status_last_commit]
  · intro other different
    rw [status]
    rw [status_other_key _ failedKey other none parent.publication.committed different]
    exact status_other_key s.failures failedKey other failure firstObs.publication.committed different

/-- Frame keys include the watched deployment, even though the runner mutex does
not. A different watched deployment is necessarily an unrelated failure key. -/
theorem different_watched_deployment (key other : FailureKey)
    (different : other.watchedDeployment ≠ key.watchedDeployment) : other ≠ key := by
  intro same
  exact different (congrArg FailureKey.watchedDeployment same)

#print axioms recorded_fallback_channels
#print axioms successful_fallback_records_resolve_failure
#print axioms preparation_refusal_channels
#print axioms factory_failure_is_not_preparation_failure
#print axioms completed_delete_then_obsolete
#print axioms refusal_then_parent_history

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``recorded_fallback_channels, ``successful_fallback_records_resolve_failure,
      ``preparation_refusal_channels, ``status_last_commit,
      ``factory_failure_is_not_preparation_failure, ``rowDeleted,
      ``completed_delete_has_no_key, ``completed_delete_then_obsolete,
      ``refusal_then_parent_history, ``different_watched_deployment] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected watcher status/recovery axiom in {declaration}: {unexpected}"

end FactoryRetarget.Watcher
