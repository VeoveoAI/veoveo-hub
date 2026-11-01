import FactoryWatcherModel
import FactoryRecoveryProof
import FactoryChoiceProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher
variable {P : Type} [DecidableEq P]

/-- Independent keyed SQL write specification, including unknown commit. -/
def statusAfter (before : FailureKey → Option Failure) (key : FailureKey)
    (value : Option Failure) (committed : Bool) : FailureKey → Option Failure :=
  fun other => if committed = true ∧ other = key then value else before other

theorem publish_exact (key : FailureKey) (value : Option Failure)
    (observation : Publication) (s : WatchState P) :
    (publish key value observation s).1.factory = s.factory ∧
    (publish key value observation s).1.failures = statusAfter s.failures key value observation.committed := by
  cases committed : observation.committed <;> simp [publish, committed]
  all_goals funext other; simp [statusAfter]

omit [DecidableEq P] in
/-- Native SELECT and uniqueness yield the actual backing-row projection. -/
theorem read_current_projection (key : RunnerKey) (s : FactoryRetarget.State P)
    (d : Deployment) (backing : Row) (current : Projection)
    (unique : UniqueKey s) (bound : RowAt key s d backing)
    (read : ReadAgrees key s (.ok (some current))) : project backing = some current := by
  obtain ⟨_, other, row, stored, projected⟩ := read
  have same := unique key d other backing row bound stored
  simpa [same.2] using projected

theorem project_runner (r : Row) (current : Projection) (projected : project r = some current) :
    Runner r := by
  cases link : r.link <;> cases repo : r.linkedRepository <;>
    cases tracked : r.trackedBranch <;> cases name : r.trackedName <;>
      simp_all [project, Runner]

/-- All nullable and key fields of the native projection, not a hash/summary. -/
theorem project_fields (r : Row) (current : Projection) (projected : project r = some current) :
    current.key.repository = r.repository ∧ current.key.branch = r.branch ∧
    r.link = some current.key.link ∧ r.linkedRepository = some current.linkedRepository ∧
    r.trackedBranch = some current.trackedId ∧ r.trackedName = some current.trackedName ∧
    current.parentRevision = r.revision ∧ current.parentPusher = r.pusher ∧
    current.linkedRevision = r.linkedRevision ∧ current.linkedPusher = r.linkedPusher := by
  cases link : r.link <;> cases repo : r.linkedRepository <;>
    cases tracked : r.trackedBranch <;> cases name : r.trackedName <;>
      simp_all [project]
  all_goals subst current; simp

/-- No-op refers to complete stored state, not to absence of query/log attempts. -/
theorem query_error_noop (d : Deployment) (backing : Row) (candidate : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P) (error : Error)
    (active : o.cancellation.afterLock = false) (read : o.query = .error error) :
    (resume d backing candidate notice o s).state = s ∧
    (resume d backing candidate notice o s).factoryOutcome = none ∧
    (resume d backing candidate notice o s).trace =
      [.acquired candidate.key, .query candidate.key, .queryReply (.error error),
        .log .query error, .released candidate.key] := by
  simp [resume, active, read, close]

theorem absent_noop (d : Deployment) (backing : Row) (candidate : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (active : o.cancellation.afterLock = false) (read : o.query = .ok none) :
    (resume d backing candidate notice o s).state = s ∧
    (resume d backing candidate notice o s).factoryOutcome = none ∧
    (resume d backing candidate notice o s).trace =
      [.acquired candidate.key, .query candidate.key, .queryReply (.ok none), .released candidate.key] := by
  simp [resume, active, read, close]

theorem obsolete_noop (d : Deployment) (backing : Row) (candidate current : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (active : o.cancellation.afterLock = false) (read : o.query = .ok (some current))
    (different : current.linkedRepository ≠ notice.repository ∨ current.trackedId ≠ candidate.trackedId) :
    (resume d backing candidate notice o s).state = s ∧
    (resume d backing candidate notice o s).factoryOutcome = none ∧
    (resume d backing candidate notice o s).trace =
      [.acquired candidate.key, .query candidate.key, .queryReply (.ok (some current)), .released candidate.key] := by
  simp [resume, active, read, different, close]

/-- Changing captured parent/fallback metadata cannot change the entire returned
operation, provided key and event tracked-ID stay the same. -/
theorem capture_independent (d : Deployment) (backing : Row) (a b : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (key : a.key = b.key) (tracked : a.trackedId = b.trackedId) :
    resume d backing a notice o s = resume d backing b notice o s := by
  simp only [resume, key, tracked]

omit [DecidableEq P] in
/-- Successful branch results supply their name even when marked deleted, exactly
as reapply does; this is not the stricter resolve policy. -/
theorem successful_name (current : Projection) (args : Arguments) (o : ResumeObservations P)
    (name : String) (deleted : Bool) (parsed : o.trackedIdParses current.trackedId = true)
    (read : o.branch args = .ok ⟨name, deleted⟩) :
    (preparedName current args o).1 = name := by
  simp [preparedName, parsed, read]

theorem constructed_current_arguments (current : Projection) (notice : Notice)
    (revision pusher : String) (linked : LinkedPush) :
    (arguments current notice revision pusher linked).parent = current.key ∧
    (arguments current notice revision pusher linked).parentRevision = revision ∧
    (arguments current notice revision pusher linked).parentPusher = pusher ∧
    (arguments current notice revision pusher linked).linkedRepository = notice.repository ∧
    (arguments current notice revision pusher linked).linkedRevision = linked.revision ∧
    (arguments current notice revision pusher linked).linkedPusher = linked.pusher := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl⟩

omit [DecidableEq P] in
theorem constructed_current_request (current : Projection) (revision pusher : String)
    (linked : LinkedPush) (name : String) (source : Source P) :
    let q := request current revision pusher linked name source
    q.revision = revision ∧ q.pusher = pusher ∧
    q.target.repository = current.linkedRepository ∧ q.target.trackedBranch = current.trackedId ∧
    q.target.trackedName = name ∧ q.target.pusher = linked.pusher ∧ q.target.revision = linked.revision ∧
    q.ownTree = source.ownTree ∧ q.ownEntries = source.ownEntries := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- Exact inner application/status channel, without assuming Factory succeeds.
A Factory error can coexist with watcher preparation success and status clear. -/
theorem ready_resume_channels (d : Deployment) (backing : Row) (candidate current : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (revision pusher : String) (linked : LinkedPush) (source : Source P)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false)
    (read : o.query = .ok (some current))
    (repository : current.linkedRepository = notice.repository) (tracked : current.trackedId = candidate.trackedId)
    (parentRevision : current.parentRevision = some revision) (parentPusher : current.parentPusher = some pusher)
    (pushed : notice.pushed = some linked)
    (prepared : o.prepare (arguments current notice revision pusher linked) = .ok source) :
    let args := arguments current notice revision pusher linked
    let q := request current revision pusher linked (preparedName current args o).1 source
    let result := resume d backing candidate notice o s
    result.exit = .prepared ∧ result.factoryOutcome = some (apply d backing q source.factory s.factory) ∧
    result.state.factory = (apply d backing q source.factory s.factory).state ∧
    result.state.failures = statusAfter s.failures (publicationKey candidate.key notice.watchedDeployment)
      none o.publication.committed ∧ Trace.logicalPrepare args ∈ result.trace := by
  cases named : preparedName current (arguments current notice revision pusher linked) o with
  | mk name events =>
      cases committed : o.publication.committed <;>
        simp [resume, active, read, repository, tracked, parentRevision, parentPusher, pushed,
          prepared, named, attempt, change, publication, publish, committed, close]
      all_goals funext other; simp [statusAfter]

/-- Complete independent Factory outputs from generated current-state inputs.
SQL/current-row consistency and schema identity are visible native contracts; no
correct Request, successful watcher return or desired after-state is supplied. -/
theorem relevant_full_output (d : Deployment) (backing : Row) (candidate current : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (revision pusher : String) (linked : LinkedPush) (source : Source P)
    (unique : UniqueKey s.factory) (bound : RowAt candidate.key s.factory d backing)
    (key : backing.deployment = d) (consistent : ReadAgrees candidate.key s.factory o.query)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false) (read : o.query = .ok (some current))
    (repository : current.linkedRepository = notice.repository) (tracked : current.trackedId = candidate.trackedId)
    (parentRevision : current.parentRevision = some revision) (parentPusher : current.parentPusher = some pusher)
    (pushed : notice.pushed = some linked)
    (prepared : o.prepare (arguments current notice revision pusher linked) = .ok source)
    (noFault : source.factory.fault = none) (capacity : CapacityAllowed source.factory source.factory.readNative) :
    let args := arguments current notice revision pusher linked
    let q := request current revision pusher linked (preparedName current args o).1 source
    let result := resume d backing candidate notice o s
    result.exit = .prepared ∧
    (apply d backing q source.factory s.factory).error = none ∧
    result.factoryOutcome = some (apply d backing q source.factory s.factory) ∧
    result.state.factory.rows d = some (successRow backing q source.factory (choice backing q source.factory.prepared source.factory.readNative)) ∧
    result.state.factory.sandboxes d = nativeOutput backing q source.factory ∧
    result.state.factory.providers d = providerOutput backing q source.factory (s.factory.providers d) ∧
    result.state.factory.createRequests d = requestOutput backing q source.factory (s.factory.createRequests d) ∧
    result.state.failures = statusAfter s.failures (publicationKey candidate.key notice.watchedDeployment) none o.publication.committed ∧
    Frame d s.factory result.state.factory ∧ Trace.logicalPrepare args ∈ result.trace := by
  let args := arguments current notice revision pusher linked
  let q := request current revision pusher linked (preparedName current args o).1 source
  have projected := read_current_projection candidate.key s.factory d backing current unique bound
    (by simpa only [read] using consistent)
  have runner := project_runner backing current projected
  have normal := no_fault_capacity_accepts d backing q source.factory s.factory bound.1 noFault capacity
  have outputs := successful_outputs_complete d backing q source.factory s.factory bound.1 key runner normal
  have channels := ready_resume_channels d backing candidate current notice o s revision pusher linked source
    active change publication read repository tracked parentRevision parentPusher pushed prepared
  refine ⟨channels.1, normal, channels.2.1, ?_, ?_, ?_, ?_, channels.2.2.2.1, ?_, channels.2.2.2.2⟩
  · rw [channels.2.2.1]; exact outputs.1
  · rw [channels.2.2.1]; exact outputs.2.1
  · rw [channels.2.2.1]; exact outputs.2.2.1
  · rw [channels.2.2.1]; exact outputs.2.2.2.1
  · rw [channels.2.2.1]; exact outputs.2.2.2.2

/-- Missing current SQL row cannot recreate anything, however old the candidate.
This also covers query errors and cancelled attempts without erasing their trace. -/
theorem missing_key_no_resurrection (d : Deployment) (backing : Row) (candidate : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (read : ReadAgrees candidate.key s.factory o.query)
    (absent : ¬∃ other row, RowAt candidate.key s.factory other row) :
    (resume d backing candidate notice o s).state = s ∧
      (resume d backing candidate notice o s).factoryOutcome = none := by
  cases cancelled : o.cancellation.afterLock with
  | true => simp [resume, cancelled, close]
  | false =>
      cases queried : o.query with
      | error error => exact ⟨(query_error_noop d backing candidate notice o s error cancelled queried).1,
          (query_error_noop d backing candidate notice o s error cancelled queried).2.1⟩
      | ok value =>
          cases value with
          | none => exact ⟨(absent_noop d backing candidate notice o s cancelled queried).1,
              (absent_noop d backing candidate notice o s cancelled queried).2.1⟩
          | some current =>
              have witness : ∃ other row, RowAt candidate.key s.factory other row := by
                obtain ⟨_, other, row, stored, _⟩ := (show ReadAgrees candidate.key s.factory (.ok (some current)) from by simpa [queried] using read)
                exact ⟨other, row, stored⟩
              exact False.elim (absent witness)

#print axioms read_current_projection
#print axioms project_fields
#print axioms obsolete_noop
#print axioms capture_independent
#print axioms ready_resume_channels
#print axioms relevant_full_output
#print axioms missing_key_no_resurrection

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``resume, ``parentLink, ``publish, ``attempt,
      ``publish_exact, ``read_current_projection, ``project_runner, ``project_fields,
      ``query_error_noop, ``absent_noop, ``obsolete_noop, ``capture_independent,
      ``successful_name, ``constructed_current_arguments, ``constructed_current_request,
      ``ready_resume_channels, ``relevant_full_output, ``missing_key_no_resurrection] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected watcher composition axiom in {declaration}: {unexpected}"

end FactoryRetarget.Watcher
