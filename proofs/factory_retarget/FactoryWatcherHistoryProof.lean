import FactoryWatcherProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget.Watcher
variable {P : Type} [DecidableEq P]

/-- The actual parent query error convention: resolve receives None, not a
fabricated old row, and the operation does not turn into a query-error no-op. -/
def queriedExisting (result : Except Error (Option Projection)) : Option Projection :=
  match result with | .ok value => value | .error _ => none

/-- A request assembled from parent source inputs and the native resolved target,
not a Request supplied as a desired-output premise. -/
def parentRequest (input : ParentInput) (resolved : Resolved P) : Request :=
  ⟨resolved.target, input.revision, input.pusher, input.ownTree, input.ownEntries⟩

theorem parent_request_fields (input : ParentInput) (resolved : Resolved P) :
    (parentRequest input resolved).target = resolved.target ∧
    (parentRequest input resolved).revision = input.revision ∧
    (parentRequest input resolved).pusher = input.pusher ∧
    (parentRequest input resolved).ownTree = input.ownTree ∧
    (parentRequest input resolved).ownEntries = input.ownEntries := by
  exact ⟨rfl, rfl, rfl, rfl, rfl⟩

/-- No Factory-success premise: query/resolve traces and swallowed Factory result
are preserved, and publication remains independently committed-or-not. -/
theorem parent_ready_channels (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (bound : s.factory.rows d = some backing)
    (resolved : Resolved P) (active : o.cancellation.afterLock = false)
    (change : o.cancellation.beforeChange = false) (publication : o.cancellation.beforePublication = false)
    (resolution : o.resolve input (queriedExisting o.query) = .ok resolved) :
    let q := parentRequest input resolved
    let output := parentLink d backing input o s bound
    output.exit = .prepared ∧ output.factoryOutcome = some (apply d backing q resolved.factory s.factory) ∧
    output.state.factory = (apply d backing q resolved.factory s.factory).state ∧
    output.state.failures = statusAfter s.failures (publicationKey input.key input.watchedDeployment)
      none o.publication.committed ∧
    Trace.logicalResolve input (queriedExisting o.query) ∈ output.trace := by
  cases query : o.query <;> cases committed : o.publication.committed <;>
    simp_all [queriedExisting, parentLink, parentRequest, active, change, publication,
      attempt, publish, close]
  all_goals funext other; simp [statusAfter]

/-- Immutable identity entails the source SQL key fields as well as deployment. -/
theorem identity_key_fields (a b : Row) (same : identity a = identity b) :
    a.deployment = b.deployment ∧ a.repository = b.repository ∧ a.branch = b.branch ∧ a.link = b.link := by
  simp only [identity, Prod.mk.injEq] at same
  exact ⟨same.1, same.2.2.1, same.2.2.2.1, same.2.2.2.2.1⟩

theorem identity_row_matches (key : RunnerKey) (a b : Row) (same : identity a = identity b)
    (keyFields : a.repository = key.repository ∧ a.branch = key.branch ∧ a.link = some key.link) :
    b.repository = key.repository ∧ b.branch = key.branch ∧ b.link = some key.link := by
  have fields := identity_key_fields a b same
  exact ⟨fields.2.1.symm.trans keyFields.1, fields.2.2.1.symm.trans keyFields.2.1,
    fields.2.2.2.symm.trans keyFields.2.2⟩

/-- Every after-row of an identity-preserving operation has an actual before-row;
this derives row existence instead of assuming an after-state invariant. -/
theorem previous_row (before after : FactoryRetarget.State P) (preserved : IdentityFrame before after)
    (d : Deployment) (row : Row) (bound : after.rows d = some row) :
    ∃ original, before.rows d = some original ∧ identity row = identity original := by
  have same := preserved d
  cases stored : before.rows d with
  | none => simp [bound, stored] at same
  | some original =>
      refine ⟨original, rfl, ?_⟩
      apply Option.some.inj
      simpa only [bound, stored, Option.map_some] using same

/-- Native schema uniqueness is preserved by actual immutable row-key effects.
The full after-row values at equal deployment are equal because rows is a map. -/
theorem unique_of_identity (before after : FactoryRetarget.State P)
    (unique : UniqueKey before) (preserved : IdentityFrame before after) : UniqueKey after := by
  intro key d₁ d₂ r₁ r₂ first second
  obtain ⟨old₁, stored₁, identity₁⟩ := previous_row before after preserved d₁ r₁ first.1
  obtain ⟨old₂, stored₂, identity₂⟩ := previous_row before after preserved d₂ r₂ second.1
  have fields₁ := identity_row_matches key r₁ old₁ identity₁ first.2
  have fields₂ := identity_row_matches key r₂ old₂ identity₂ second.2
  have same := unique key d₁ d₂ old₁ old₂ ⟨stored₁, fields₁⟩ ⟨stored₂, fields₂⟩
  refine ⟨same.1, ?_⟩
  have firstAtSecond : after.rows d₂ = some r₁ := by simpa only [same.1] using first.1
  exact Option.some.inj (firstAtSecond.symm.trans second.1)

/-- The actual apply identity theorem supplies the next SQL uniqueness premise. -/
theorem apply_preserves_unique (d : Deployment) (backing : Row) (q : Request)
    (o : Observations P) (s : FactoryRetarget.State P) (unique : UniqueKey s) :
    UniqueKey (apply d backing q o s).state :=
  unique_of_identity s _ unique (apply_identity d backing q o s)

theorem identity_keeps_row_at (key : RunnerKey) (d : Deployment) (before after : FactoryRetarget.State P)
    (backing next : Row) (old : RowAt key before d backing)
    (stored : after.rows d = some next) (same : identity next = identity backing) :
    RowAt key after d next := by
  exact ⟨stored, identity_row_matches key backing next same.symm old.2⟩

/-- Independent complete Factory-value specification; the request is constructed
inside parentLink. Native resolution/policy correctness remains an explicit input
observation contract, not an assertion that a requested retarget already happened. -/
def FactoryValues (d : Deployment) (backing : Row) (q : Request) (o : Observations P)
    (before after : FactoryRetarget.State P) : Prop :=
  after.rows d = some (successRow backing q o (choice backing q o.prepared o.readNative)) ∧
  after.sandboxes d = nativeOutput backing q o ∧
  after.providers d = providerOutput backing q o (before.providers d) ∧
  after.createRequests d = requestOutput backing q o (before.createRequests d) ∧ Frame d before after

/-- Accepted parent operation, complete outputs and full visible client projection.
Only native resolution/preparation/admission observations are premises; success is
constructed. Arbitrary status commit/reply and unrelated full state are retained. -/
theorem parent_full_output (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (resolved : Resolved P)
    (bound : s.factory.rows d = some backing) (key : backing.deployment = d) (runner : Runner backing)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false)
    (resolution : o.resolve input (queriedExisting o.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative) :
    let q := parentRequest input resolved
    let output := parentLink d backing input o s bound
    output.exit = .prepared ∧
    (apply d backing q resolved.factory s.factory).error = none ∧
    output.factoryOutcome = some (apply d backing q resolved.factory s.factory) ∧
    FactoryValues d backing q resolved.factory s.factory output.state.factory ∧
    (output.state.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
      backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ ∧
    output.state.failures = statusAfter s.failures (publicationKey input.key input.watchedDeployment) none o.publication.committed := by
  let q := parentRequest input resolved
  have normal := no_fault_capacity_accepts d backing q resolved.factory s.factory bound noFault capacity
  have values := successful_outputs_complete d backing q resolved.factory s.factory bound key runner normal
  have channels := parent_ready_channels d backing input o s bound resolved active change publication resolution
  refine ⟨channels.1, normal, channels.2.1, ?_, ?_, channels.2.2.2.1⟩
  · rw [channels.2.2.1]
    exact values
  · rw [channels.2.2.1, values.1]
    simp [successRow, client, q, parentRequest]

/-- Current projection and schema uniqueness follow the actual parent operation,
including its full row/frame effects; not an assumed invariant of its after-state. -/
theorem parent_preserves_key_and_unique (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (resolved : Resolved P)
    (bound : RowAt input.key s.factory d backing) (unique : UniqueKey s.factory)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false)
    (resolution : o.resolve input (queriedExisting o.query) = .ok resolved) :
    UniqueKey (parentLink d backing input o s bound.1).state.factory ∧
    IdentityFrame s.factory (parentLink d backing input o s bound.1).state.factory := by
  have channels := parent_ready_channels d backing input o s bound.1 resolved active change publication resolution
  rw [channels.2.2.1]
  exact ⟨apply_preserves_unique d backing _ resolved.factory s.factory unique,
    apply_identity d backing _ resolved.factory s.factory⟩

/-- Unrelated status keys are framed by the exact complete keyed write, whether
publication committed, failed without commit, or lost its successful reply. -/
theorem status_other_key (before : FailureKey → Option Failure) (key other : FailureKey)
    (value : Option Failure) (committed : Bool) (different : other ≠ key) :
    statusAfter before key value committed other = before other := by
  simp [statusAfter, different]

theorem parent_status_other (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (bound : s.factory.rows d = some backing)
    (resolved : Resolved P) (other : FailureKey)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false)
    (resolution : o.resolve input (queriedExisting o.query) = .ok resolved)
    (different : other ≠ publicationKey input.key input.watchedDeployment) :
    (parentLink d backing input o s bound).state.failures other = s.failures other := by
  have channels := parent_ready_channels d backing input o s bound resolved active change publication resolution
  rw [channels.2.2.2.1]
  exact status_other_key s.failures _ other none o.publication.committed different

/-- Expected projection of the actual accepted parent operation. -/
def parentProjection (input : ParentInput) (resolved : Resolved P) : Projection :=
  ⟨input.key, resolved.target.repository, resolved.target.trackedBranch, resolved.target.trackedName,
    some input.revision, some input.pusher, some resolved.target.revision, some resolved.target.pusher⟩

theorem success_identity (backing : Row) (q : Request) (o : Observations P) (path : Choice) :
    identity (successRow backing q o path) = identity backing := by
  simp [successRow, identity]

theorem parent_success_projection (d : Deployment) (backing : Row) (input : ParentInput)
    (resolved : Resolved P) (s : FactoryRetarget.State P) (bound : RowAt input.key s d backing)
    (path : Choice) :
    project (successRow backing (parentRequest input resolved) resolved.factory path) =
      some (parentProjection input resolved) := by
  rcases bound with ⟨_, repository, branch, link⟩
  simp [project, successRow, parentRequest, parentProjection, repository, branch, link]

/-- General current-row source consequence, used after an actual parent step.
Query errors remain logged no-ops; a successful projection is derived from schema
uniqueness and read consistency, never supplied as a desired post-state. -/
theorem obsolete_after_current_row (d : Deployment) (backing : Row) (candidate expected : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P)
    (unique : UniqueKey s.factory) (bound : RowAt candidate.key s.factory d backing)
    (projected : project backing = some expected) (read : ReadAgrees candidate.key s.factory o.query)
    (active : o.cancellation.afterLock = false)
    (different : expected.linkedRepository ≠ notice.repository ∨ expected.trackedId ≠ candidate.trackedId) :
    (resume d backing candidate notice o s).state = s ∧
      (resume d backing candidate notice o s).factoryOutcome = none := by
  cases queried : o.query with
  | error error =>
      exact ⟨(query_error_noop d backing candidate notice o s error active queried).1,
        (query_error_noop d backing candidate notice o s error active queried).2.1⟩
  | ok value =>
      cases value with
      | none =>
          have missing : ¬∃ other row, RowAt candidate.key s.factory other row := by
            simpa [ReadAgrees, queried] using read
          exact False.elim (missing ⟨d, backing, bound⟩)
      | some current =>
          have actual := read_current_projection candidate.key s.factory d backing current unique bound
            (by simpa [queried] using read)
          have same : current = expected := Option.some.inj (actual.symm.trans projected)
          subst current
          have noop := obsolete_noop d backing candidate expected notice o s active queried different
          exact ⟨noop.1, noop.2.1⟩

/-- Actual parent retarget followed by a queued old candidate. The entire B
Factory/failure state is preserved, not merely its target field or result tag. -/
theorem parent_retarget_then_obsolete (d : Deployment) (backing : Row) (input : ParentInput)
    (parent : ParentObservations P) (s : WatchState P) (resolved : Resolved P)
    (candidate : Projection) (notice : Notice) (later : ResumeObservations P)
    (bound : RowAt input.key s.factory d backing) (key : backing.deployment = d)
    (runner : Runner backing) (unique : UniqueKey s.factory)
    (active : parent.cancellation.afterLock = false) (change : parent.cancellation.beforeChange = false)
    (publication : parent.cancellation.beforePublication = false)
    (resolution : parent.resolve input (queriedExisting parent.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative)
    (sameKey : candidate.key = input.key) (laterActive : later.cancellation.afterLock = false)
    (different : resolved.target.repository ≠ notice.repository ∨ resolved.target.trackedBranch ≠ candidate.trackedId)
    (laterRead : ReadAgrees candidate.key
      (parentLink d backing input parent s bound.1).state.factory later.query) :
    let q := parentRequest input resolved
    let next := successRow backing q resolved.factory (choice backing q resolved.factory.prepared resolved.factory.readNative)
    let first := parentLink d backing input parent s bound.1
    let second := resume d next candidate notice later first.state
    first.state.factory.rows d = some next ∧ second.state = first.state ∧ second.factoryOutcome = none ∧
    (second.state.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
      backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ := by
  let q := parentRequest input resolved
  let next := successRow backing q resolved.factory (choice backing q resolved.factory.prepared resolved.factory.readNative)
  let first := parentLink d backing input parent s bound.1
  have complete := parent_full_output d backing input parent s resolved bound.1 key runner active change publication resolution noFault capacity
  have preserved := parent_preserves_key_and_unique d backing input parent s resolved bound unique active change publication resolution
  have stored : first.state.factory.rows d = some next := complete.2.2.2.1.1
  have afterBound : RowAt candidate.key first.state.factory d next := by
    rw [sameKey]
    exact identity_keeps_row_at input.key d s.factory first.state.factory backing next bound stored
      (success_identity backing q resolved.factory _)
  have projected : project next = some (parentProjection input resolved) :=
    parent_success_projection d backing input resolved s.factory bound _
  have noop := obsolete_after_current_row d next candidate (parentProjection input resolved) notice later first.state
    preserved.1 afterBound projected laterRead laterActive different
  refine ⟨stored, noop.1, noop.2, ?_⟩
  rw [noop.1]
  exact complete.2.2.2.2.1

/-- Parent query failure still invokes native resolve(None), can apply successfully
in this existing-row domain, and does not masquerade as a whole-operation no-op. -/
theorem parent_query_error_resolves_none (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P) (bound : s.factory.rows d = some backing)
    (error : Error) (resolved : Resolved P) (query : o.query = .error error)
    (active : o.cancellation.afterLock = false) (change : o.cancellation.beforeChange = false)
    (publication : o.cancellation.beforePublication = false)
    (resolution : o.resolve input none = .ok resolved) :
    (parentLink d backing input o s bound).factoryOutcome =
      some (apply d backing (parentRequest input resolved) resolved.factory s.factory) ∧
    Trace.logicalResolve input none ∈ (parentLink d backing input o s bound).trace ∧
    Trace.log .query error ∈ (parentLink d backing input o s bound).trace := by
  have channels := parent_ready_channels d backing input o s bound resolved active change publication
    (by simpa [queriedExisting, query] using resolution)
  refine ⟨channels.2.1, ?_, ?_⟩
  · simpa [queriedExisting, query] using channels.2.2.2.2
  · cases committed : o.publication.committed <;>
      simp [parentLink, query, active, resolution, change, publication, attempt, publish, close, committed]

/-- Point SQL key and projection agree by actual row fields. -/
theorem project_row_at_key (key : RunnerKey) (s : FactoryRetarget.State P)
    (d : Deployment) (row : Row) (current : Projection)
    (bound : RowAt key s d row) (projected : project row = some current) : current.key = key := by
  have fields := project_fields row current projected
  have repository := fields.1.trans bound.2.1
  have branch := fields.2.1.trans bound.2.2.1
  have link : current.key.link = key.link := Option.some.inj (fields.2.2.1.symm.trans bound.2.2.2)
  cases keyValue : current.key
  cases key
  simp_all

/-- Actual parent-program advance followed by a previously captured same-target
candidate. Current metadata, own-entry response and incoming linked push build the
second Request; old captured revision/pusher equality is never required. -/
theorem parent_program_then_relevant (d : Deployment) (backing : Row) (input : ParentInput)
    (parent : ParentObservations P) (s : WatchState P) (resolved : Resolved P)
    (candidate : Projection) (notice : Notice) (later : ResumeObservations P)
    (incoming : LinkedPush) (source : Source P)
    (bound : RowAt input.key s.factory d backing) (key : backing.deployment = d)
    (captured : project backing = some candidate) (unique : UniqueKey s.factory)
    (active : parent.cancellation.afterLock = false) (change : parent.cancellation.beforeChange = false)
    (publication : parent.cancellation.beforePublication = false)
    (resolution : parent.resolve input (queriedExisting parent.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative)
    (sameRepository : resolved.target.repository = candidate.linkedRepository)
    (sameTracked : resolved.target.trackedBranch = candidate.trackedId)
    (incomingRepository : notice.repository = resolved.target.repository)
    (pushed : notice.pushed = some incoming)
    (laterActive : later.cancellation.afterLock = false) (laterChange : later.cancellation.beforeChange = false)
    (laterPublication : later.cancellation.beforePublication = false)
    (laterRead : ReadAgrees candidate.key (parentLink d backing input parent s bound.1).state.factory later.query)
    (readSucceeds : ∃ current, later.query = .ok (some current))
    (prepared : later.prepare (arguments (parentProjection input resolved) notice input.revision input.pusher incoming) = .ok source)
    (noFaultLater : source.factory.fault = none) (capacityLater : CapacityAllowed source.factory source.factory.readNative) :
    let parentQ := parentRequest input resolved
    let next := successRow backing parentQ resolved.factory (choice backing parentQ resolved.factory.prepared resolved.factory.readNative)
    let first := parentLink d backing input parent s bound.1
    let args := arguments (parentProjection input resolved) notice input.revision input.pusher incoming
    let q := request (parentProjection input resolved) input.revision input.pusher incoming (preparedName (parentProjection input resolved) args later).1 source
    let second := resume d next candidate notice later first.state
    changed backing parentQ = false ∧ first.state.factory.rows d = some next ∧ second.exit = .prepared ∧
    (apply d next q source.factory first.state.factory).error = none ∧
    second.factoryOutcome = some (apply d next q source.factory first.state.factory) ∧
    FactoryValues d next q source.factory first.state.factory second.state.factory ∧
    (second.state.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
      backing.repository, some resolved.target.repository, some incoming.pusher⟩ ∧
    second.state.failures = statusAfter first.state.failures
      (publicationKey candidate.key notice.watchedDeployment) none later.publication.committed ∧
    Frame d s.factory second.state.factory ∧ Trace.logicalPrepare args ∈ second.trace := by
  let parentQ := parentRequest input resolved
  let next := successRow backing parentQ resolved.factory (choice backing parentQ resolved.factory.prepared resolved.factory.readNative)
  let first := parentLink d backing input parent s bound.1
  let args := arguments (parentProjection input resolved) notice input.revision input.pusher incoming
  let q := request (parentProjection input resolved) input.revision input.pusher incoming
    (preparedName (parentProjection input resolved) args later).1 source
  have runner := project_runner backing candidate captured
  have fields := project_fields backing candidate captured
  have unchanged : changed backing parentQ = false := by
    apply (changed_false_iff_same_target backing parentQ).mpr
    constructor
    · simpa [parentQ, parentRequest, sameRepository] using fields.2.2.2.1
    · simpa [parentQ, parentRequest, sameTracked] using fields.2.2.2.2.1
  have sameKey := project_row_at_key input.key s.factory d backing candidate bound captured
  have complete := parent_full_output d backing input parent s resolved bound.1 key runner active change publication resolution noFault capacity
  have preserved := parent_preserves_key_and_unique d backing input parent s resolved bound unique active change publication resolution
  have stored : first.state.factory.rows d = some next := complete.2.2.2.1.1
  have afterBound : RowAt candidate.key first.state.factory d next := by
    rw [sameKey]
    exact identity_keeps_row_at input.key d s.factory first.state.factory backing next bound stored
      (success_identity backing parentQ resolved.factory _)
  have projected : project next = some (parentProjection input resolved) :=
    parent_success_projection d backing input resolved s.factory bound _
  obtain ⟨current, read⟩ := readSucceeds
  have actual := read_current_projection candidate.key first.state.factory d next current preserved.1 afterBound
    (by simpa [read] using laterRead)
  have currentEq : current = parentProjection input resolved := Option.some.inj (actual.symm.trans projected)
  subst current
  have nextKey : next.deployment = d := by simpa [next, successRow] using key
  have outputs := relevant_full_output d next candidate (parentProjection input resolved) notice later first.state
    input.revision input.pusher incoming source preserved.1 afterBound nextKey laterRead laterActive laterChange
    laterPublication read incomingRepository.symm sameTracked rfl rfl pushed prepared noFaultLater capacityLater
  refine ⟨unchanged, stored, outputs.1, outputs.2.1, outputs.2.2.1, ?_, ?_, outputs.2.2.2.2.2.2.2.1, ?_, outputs.2.2.2.2.2.2.2.2.2⟩
  · exact ⟨outputs.2.2.2.1, outputs.2.2.2.2.1, outputs.2.2.2.2.2.1,
      outputs.2.2.2.2.2.2.1, outputs.2.2.2.2.2.2.2.2.1⟩
  · rw [outputs.2.2.2.1]
    simp [client, successRow, next, q, request, parentProjection]
  · exact frame_trans complete.2.2.2.1.2.2.2.2 outputs.2.2.2.2.2.2.2.2.1

#print axioms parent_ready_channels
#print axioms parent_full_output
#print axioms apply_preserves_unique
#print axioms parent_retarget_then_obsolete
#print axioms parent_program_then_relevant
#print axioms parent_query_error_resolves_none
#print axioms parent_status_other

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``queriedExisting, ``parentRequest, ``FactoryValues, ``parentProjection,
      ``parent_request_fields, ``parent_ready_channels, ``identity_key_fields,
      ``identity_row_matches, ``previous_row, ``unique_of_identity,
      ``apply_preserves_unique, ``identity_keeps_row_at, ``parent_full_output,
      ``parent_preserves_key_and_unique, ``status_other_key, ``parent_status_other,
      ``success_identity, ``parent_success_projection, ``obsolete_after_current_row,
      ``parent_retarget_then_obsolete, ``parent_query_error_resolves_none,
      ``project_row_at_key, ``parent_program_then_relevant] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected watcher history axiom in {declaration}: {unexpected}"

end FactoryRetarget.Watcher
