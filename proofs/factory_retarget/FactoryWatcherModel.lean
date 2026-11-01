import FactoryRetargetModel

/-! Manual composition of guarded watcher operations with the unchanged Factory
model. Normal returned operations in one Inner only, not an async/SQL extraction.
Native correspondence is restricted to valid parsed identities and, whenever
Factory.apply is invoked, an existing backing row without external row writes.
New native allocation after a genuinely missing parent row is outside this model.
Logical preparation/resolve events do not verify every parser/tempdir/native read.
-/
namespace FactoryRetarget.Watcher

structure RunnerKey where
  repository : String
  branch : String
  link : String
  deriving DecidableEq, Repr

/-- Failure storage includes watched deployment; mutex and current SQL keys do not. -/
structure FailureKey where
  watchedDeployment : String
  runner : RunnerKey
  deriving DecidableEq, Repr

structure Failure where
  linkedRepository : String
  parentRevision : String
  message : String
  failedAt : Timestamp
  deriving DecidableEq, Repr

/-- Exactly the native Runner projection, distinct from the complete backing Row. -/
structure Projection where
  key : RunnerKey
  linkedRepository : String
  trackedId : String
  trackedName : String
  parentRevision : Option String
  parentPusher : Option String
  linkedRevision : Option String
  linkedPusher : Option String
  deriving DecidableEq, Repr

def project (r : Row) : Option Projection :=
  match r.link, r.linkedRepository, r.trackedBranch, r.trackedName with
  | some link, some repository, some tracked, some name =>
      some ⟨⟨r.repository, r.branch, link⟩, repository, tracked, name,
        r.revision, r.pusher, r.linkedRevision, r.linkedPusher⟩
  | _, _, _, _ => none

structure WatchState (P : Type) where
  factory : FactoryRetarget.State P
  failures : FailureKey → Option Failure

/-- Independent SQL key predicate; uniqueness is the actual instance constraint. -/
def RowAt (key : RunnerKey) (s : FactoryRetarget.State P) (d : Deployment) (r : Row) : Prop :=
  s.rows d = some r ∧ r.repository = key.repository ∧ r.branch = key.branch ∧ r.link = some key.link

def UniqueKey (s : FactoryRetarget.State P) : Prop :=
  ∀ key d₁ d₂ r₁ r₂, RowAt key s d₁ r₁ → RowAt key s d₂ r₂ → d₁ = d₂ ∧ r₁ = r₂

/-- Explicit native SELECT/parse contract, not a premise asserting a desired output.
Err provides no usable projection; None means absence, not a fabricated empty row. -/
def ReadAgrees (key : RunnerKey) (s : FactoryRetarget.State P)
    (result : Except Error (Option Projection)) : Prop :=
  match result with
  | .error _ => True
  | .ok none => ¬∃ d r, RowAt key s d r
  | .ok (some current) => current.key = key ∧
      ∃ d r, RowAt key s d r ∧ project r = some current

structure LinkedPush where
  revision : String
  pusher : String
  deriving DecidableEq, Repr

structure Notice where
  watchedDeployment : String
  repository : String
  pushed : Option LinkedPush
  deriving DecidableEq, Repr

structure Arguments where
  parent : RunnerKey
  parentRevision : String
  parentPusher : String
  watchedDeployment : String
  linkedRepository : String
  linkedRevision : String
  linkedPusher : String
  trackedId : String
  deriving DecidableEq, Repr

/-- Source-returned own tree and actual Factory native/build observations. This
contains no already-correct Request: the continuation constructs all its fields. -/
structure Source (P : Type) where
  ownTree : String
  ownEntries : List String
  factory : Observations P

structure BranchAnswer where
  name : String
  deleted : Bool
  deriving DecidableEq, Repr

structure Cancellation where
  afterLock : Bool
  beforeChange : Bool
  beforePublication : Bool
  deriving DecidableEq, Repr

def CancellationCoherent (c : Cancellation) : Prop :=
  (c.afterLock = true → c.beforeChange = true) ∧
    (c.beforeChange = true → c.beforePublication = true)

structure Publication where
  committed : Bool
  reply : Except Error Unit
  failedAt : Timestamp
  deriving DecidableEq, Repr

def PublicationCoherent (p : Publication) : Prop := p.reply = .ok () → p.committed = true

/-- Parent membership/program is supplied by the existing parent preparation, not
asserted to be the latest Lore head. This slice does not repair saved parent retries. -/
structure ParentInput where
  key : RunnerKey
  watchedDeployment : String
  requestedRepository : String
  parentBranchId : String
  pinnedRevision : String
  follows : Bool
  revision : String
  pusher : String
  ownTree : String
  ownEntries : List String
  deriving DecidableEq, Repr

inductive LogKind | query | preparation | factory | status | resolve
  deriving DecidableEq, Repr

inductive Trace
  | acquired (key : RunnerKey)
  | query (key : RunnerKey)
  | queryReply (reply : Except Error (Option Projection))
  | logicalPrepare (args : Arguments)
  | prepareReply (reply : Except Error Unit)
  | nameParse (tracked : String) (valid : Bool)
  | nameRead (args : Arguments)
  | nameReply (reply : Except Error BranchAnswer)
  | factoryAttempt (deployment : Deployment) (request : Request)
  | factoryReturn (error : Option Error) (events : List Event)
  | statusAttempt (key : FailureKey) (value : Option Failure)
  | statusReply (reply : Except Error Unit)
  | logicalResolve (input : ParentInput) (existing : Option Projection)
  | resolveReply (reply : Except Error Unit)
  | logicalRecorded (input : ParentInput) (existing : Projection)
  | recordedReply (available : Bool)
  | log (kind : LogKind) (error : Error)
  | cancelled
  | released (key : RunnerKey)
  deriving DecidableEq, Repr

inductive Exit
  | skipped
  | preparationRefused (error : Error)
  | prepared
  deriving DecidableEq, Repr

/-- Internal model observations, not a new production return protocol. Factory
error remains separate from preparation success (native change_runner swallows it). -/
structure WatchOutcome (P : Type) where
  state : WatchState P
  exit : Exit
  factoryOutcome : Option (Outcome P)
  trace : List Trace

structure ResumeObservations (P : Type) where
  query : Except Error (Option Projection)
  prepare : Arguments → Except Error (Source P)
  trackedIdParses : String → Bool
  branch : Arguments → Except Error BranchAnswer
  cancellation : Cancellation
  publication : Publication

def request (current : Projection) (revision pusher : String) (linked : LinkedPush)
    (name : String) (source : Source P) : Request :=
  ⟨⟨current.linkedRepository, current.trackedId, name, linked.pusher, linked.revision⟩,
    revision, pusher, source.ownTree, source.ownEntries⟩

def arguments (current : Projection) (notice : Notice) (revision pusher : String)
    (linked : LinkedPush) : Arguments :=
  ⟨current.key, revision, pusher, notice.watchedDeployment, notice.repository,
    linked.revision, linked.pusher, current.trackedId⟩

def publicationKey (key : RunnerKey) (deployment : String) : FailureKey := ⟨deployment, key⟩

def publish (key : FailureKey) (value : Option Failure) (observation : Publication)
    (s : WatchState P) : WatchState P × List Trace :=
  let after := if observation.committed then
    { s with failures := fun other => if other = key then value else s.failures other } else s
  let trace := [.statusAttempt key value, .statusReply observation.reply]
  let logs := match observation.reply with | .ok _ => [] | .error error => [.log .status error]
  (after, trace ++ logs)

def close (key : RunnerKey) (outcome : WatchOutcome P) : WatchOutcome P :=
  { outcome with trace := outcome.trace ++ [.released key] }

/-- The one place this layer invokes the already checked Factory operation. -/
def attempt [DecidableEq P] (d : Deployment) (backing : Row) (q : Request)
    (o : Observations P) (cancel : Bool) (s : WatchState P) :
    WatchState P × Option (Outcome P) × List Trace :=
  if cancel then (s, none, [.cancelled]) else
    let result := apply d backing q o s.factory
    let logs := match result.error with | none => [] | some error => [.log .factory error]
    ({ s with factory := result.state }, some result,
      [.factoryAttempt d q, .factoryReturn result.error result.events] ++ logs)

def preparedName (current : Projection) (args : Arguments) (o : ResumeObservations P) : String × List Trace :=
  if o.trackedIdParses current.trackedId then
    let result := o.branch args
    let name := match result with | .ok branch => branch.name | .error _ => current.trackedName
    -- Native reapply does not reject a successful branch result marked deleted.
    (name, [.nameParse current.trackedId true, .nameRead args, .nameReply result])
  else (current.trackedName, [.nameParse current.trackedId false])

/-- Starts at actual guard acquisition. Parsing the candidate repository and mutex
acquisition/serialization are native correspondence boundaries. A valid existing
backing Row is required only for branches that invoke inherited Factory.apply. -/
def resume [DecidableEq P] (d : Deployment) (backing : Row) (candidate : Projection)
    (notice : Notice) (o : ResumeObservations P) (s : WatchState P) : WatchOutcome P :=
  let key := candidate.key
  let start := [.acquired key]
  let finish := close key
  if o.cancellation.afterLock then finish ⟨s, .skipped, none, start ++ [.cancelled]⟩ else
  let queried := start ++ [.query key, .queryReply o.query]
  match o.query with
  | .error error => finish ⟨s, .skipped, none, queried ++ [.log .query error]⟩
  | .ok none => finish ⟨s, .skipped, none, queried⟩
  | .ok (some current) =>
      if current.linkedRepository ≠ notice.repository ∨ current.trackedId ≠ candidate.trackedId then
        finish ⟨s, .skipped, none, queried⟩ else
      let metadata := current
      match metadata.parentRevision, metadata.parentPusher with
      | some revision, some pusher =>
          let incoming := notice.pushed.orElse (fun _ =>
            metadata.linkedRevision.bind (fun linked => metadata.linkedPusher.map (fun person => ⟨linked, person⟩)))
          match incoming with
          | none => finish ⟨s, .skipped, none, queried⟩
          | some linked =>
              let args := arguments metadata notice revision pusher linked
              let preparing := queried ++ [.logicalPrepare args]
              match o.prepare args with
              | .error error =>
                  let events := preparing ++ [.prepareReply (.error error)]
                  if o.cancellation.beforePublication then
                    finish ⟨s, .preparationRefused error, none, events ++ [.cancelled]⟩ else
                  let failure := Failure.mk notice.repository revision error.message o.publication.failedAt
                  let (after, published) := publish (publicationKey key notice.watchedDeployment)
                    (some failure) o.publication s
                  finish ⟨after, .preparationRefused error, none, events ++ [.log .preparation error] ++ published⟩
              | .ok source =>
                  let (name, nameTrace) := preparedName metadata args o
                  let q := request metadata revision pusher linked name source
                  let (changed, result, events) := attempt d backing q source.factory o.cancellation.beforeChange s
                  let beforePublish := preparing ++ [.prepareReply (.ok ())] ++ nameTrace ++ events
                  if o.cancellation.beforePublication then
                    finish ⟨changed, .prepared, result, beforePublish ++ [.cancelled]⟩ else
                  let (after, published) := publish (publicationKey key notice.watchedDeployment) none o.publication changed
                  finish ⟨after, .prepared, result, beforePublish ++ published⟩
      | _, _ => finish ⟨s, .skipped, none, queried⟩

structure Resolved (P : Type) where
  target : Target
  factory : Observations P

structure ParentObservations (P : Type) where
  query : Except Error (Option Projection)
  resolve : ParentInput → Option Projection → Except Error (Resolved P)
  recorded : ParentInput → Projection → Option (Resolved P)
  cancellation : Cancellation
  publication : Publication

/-- Existing-row part of run_links, including query-Err-as-None and recorded
fallback after resolve failure. No new-row native allocation claim is made. -/
def parentLink [DecidableEq P] (d : Deployment) (backing : Row) (input : ParentInput)
    (o : ParentObservations P) (s : WatchState P)
    (_existing : s.factory.rows d = some backing) : WatchOutcome P :=
  let key := input.key
  let start := [.acquired key]
  let finish := close key
  if o.cancellation.afterLock then finish ⟨s, .skipped, none, start ++ [.cancelled]⟩ else
  let existing := match o.query with | .ok value => value | .error _ => none
  let queryLogs := match o.query with | .ok _ => [] | .error error => [.log .query error]
  let queried := start ++ [.query key, .queryReply o.query] ++ queryLogs
  let resolution := o.resolve input existing
  let resolving := queried ++ [.logicalResolve input existing,
    .resolveReply (resolution.map (fun _ => ()))]
  let (chosen, failure, fallbackTrace) := match resolution with
    | .ok resolved => (some resolved, none, [])
    | .error error =>
        let fallback := existing.bind (o.recorded input)
        let trace := match existing with
          | none => []
          | some current => [Trace.logicalRecorded input current, Trace.recordedReply fallback.isSome]
        (fallback, some (Failure.mk input.requestedRepository input.revision error.message o.publication.failedAt), trace)
  let (changed, result, effects) := match chosen with
    | none => (s, none, [])
    | some resolved => attempt d backing
        ⟨resolved.target, input.revision, input.pusher, input.ownTree, input.ownEntries⟩
        resolved.factory o.cancellation.beforeChange s
  let trace := resolving ++ fallbackTrace ++ effects
  if o.cancellation.beforePublication then finish ⟨changed, .prepared, result, trace ++ [Trace.cancelled]⟩ else
  let logs := match resolution with | .ok _ => [] | .error error => [Trace.log .resolve error]
  let (after, published) := publish (publicationKey key input.watchedDeployment) failure o.publication changed
  finish ⟨after, .prepared, result, trace ++ logs ++ published⟩

end FactoryRetarget.Watcher
