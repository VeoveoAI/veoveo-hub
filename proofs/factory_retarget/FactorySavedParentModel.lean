import FactoryWatcherModel

/-! Manual existing-runner saved-parent preparation/publication layer.
Native snapshot/SQL/mutex correspondence and quiescent prefix are explicit;
this is not native extraction, distributed CAS or general branch allocation. -/
namespace FactoryRetarget.SavedParent
open Watcher

structure ParentKey where
  repository : String
  branch : String
  deriving DecidableEq, Repr

structure PreparationKey where
  watchedDeployment : String
  parent : ParentKey
  deriving DecidableEq, Repr

structure PreparationFailure where
  revision : String
  pusher : String
  message : String
  failedAt : Timestamp
  deriving DecidableEq, Repr

structure SavedState (P : Type) where
  watcher : WatchState P
  preparations : PreparationKey → Option PreparationFailure

/-- Native immutable revision metadata supplies these inputs; no desired Request
or after-state is accepted. The branch name is the original failure/query key. -/
structure Input where
  watchedDeployment : String
  parent : ParentKey
  branchId : String
  revision : String
  pusher : String
  deriving DecidableEq, Repr

inductive Origin
  | notification
  | savedRetry (watcher : String)
  deriving DecidableEq, Repr

/-- Accepted source domain: one prepared linked slot. General ordinary/no-run
branches and new allocation are not assigned fabricated implementations here. -/
structure LinkedSource where
  linkName : String
  requestedRepository : String
  pinnedRevision : String
  follows : Bool
  ownTree : String
  ownEntries : List String
  deriving DecidableEq, Repr

structure Attempt where
  input : Input
  origin : Origin
  source : Except String LinkedSource
  deriving DecidableEq, Repr

/-- Reading immutable source is separate from all guarded stored effects. -/
def begin (input : Input) (origin : Origin) (source : Except String LinkedSource) : Attempt :=
  ⟨input, origin, source⟩

structure NativeBranch where
  id : String
  name : String
  deleted : Bool
  latest : String
  deriving DecidableEq, Repr

/-- Parent after-lock and saved-retry post-native-call are distinct source checks.
The child carries its own runner-lock/change/publication cancellation observations. -/
structure Observations (P : Type) where
  afterLockCancelled : Bool
  afterNativeCancelled : Bool
  nativeBranch : Except Error NativeBranch
  preparationSQL : Publication
  link : ParentObservations P

inductive Event
  | sourceAttempt (input : Input)
  | sourceReply (reply : Except String Unit)
  | acquired (key : ParentKey)
  | nativeAttempt (watcher repository id : String)
  | nativeReply (reply : Except Error NativeBranch)
  | nativeWarning (error : Error)
  | sourceWarning (message : String)
  | preparationStatusAttempt (key : PreparationKey) (value : Option PreparationFailure)
  | preparationStatusReply (reply : Except Error Unit)
  | statusWarning (error : Error)
  | linkRegion (trace : List Watcher.Trace)
  | cancelled
  | released (key : ParentKey)
  deriving DecidableEq, Repr

inductive Exit | discarded | sourceRefused (message : String) | continued
  deriving DecidableEq, Repr

structure SavedOutcome (P : Type) where
  state : SavedState P
  exit : Exit
  child : Option (WatchOutcome P)
  trace : List Event

def preparationKey (input : Input) : PreparationKey :=
  ⟨input.watchedDeployment, input.parent⟩

/-- Construct actual runner key, parent program and resolver inputs from saved
source arguments. Native resolve remains Result-valued in ParentObservations. -/
def parentInput (input : Input) (source : LinkedSource) : ParentInput :=
  ⟨⟨input.parent.repository, input.parent.branch, source.linkName⟩,
    input.watchedDeployment, source.requestedRepository, input.branchId,
    source.pinnedRevision, source.follows, input.revision, input.pusher,
    source.ownTree, source.ownEntries⟩

/-- Structural BEFORE-state restriction for accepted existing-runner outputs.
Faithful native prefix reads and absent ordinary-instance removal remain explicit
interpretation conditions; this is not an invariant-after or success premise. -/
def QuiescentPrefix (d : Deployment) (backing : Row) (input : Input)
    (source : LinkedSource) (s : SavedState P) : Prop :=
  RowAt (parentInput input source).key s.watcher.factory d backing ∧
  UniqueKey s.watcher.factory ∧
  (∀ other row, s.watcher.factory.rows other = some row →
    row.repository = input.parent.repository → row.branch = input.parent.branch →
    row.link = some source.linkName) ∧
  (∀ key failure, s.watcher.failures key = some failure →
    key.watchedDeployment = input.watchedDeployment →
    key.runner.repository = input.parent.repository → key.runner.branch = input.parent.branch →
    key.runner.link = source.linkName)

/-- One native SQL outcome, not a transaction for the whole parent operation.
Error reply does not imply rollback and is logged independently of stored value. -/
def publishPreparation (key : PreparationKey) (value : Option PreparationFailure)
    (o : Publication) (s : SavedState P) : SavedState P × List Event :=
  let after := { s with preparations := fun other =>
    if o.committed = true ∧ other = key then value else s.preparations other }
  let logs := match o.reply with | .ok _ => [] | .error error => [Event.statusWarning error]
  (after, [.preparationStatusAttempt key value, .preparationStatusReply o.reply] ++ logs)

def close (key : ParentKey) (outcome : SavedOutcome P) : SavedOutcome P :=
  { outcome with trace := outcome.trace ++ [.released key] }

/-- Concrete data returned after admission and preparation-clear publication.
The actual operation constructs input/state/observations here, not a desired
Request or an arbitrary effect callback supplied by a theorem premise. Native
parent guard stays held across this internal decomposition; it is no new yield. -/
structure Ready (P : Type) where
  parent : ParentKey
  input : ParentInput
  link : ParentObservations P
  state : SavedState P
  trace : List Event

inductive Entered (P : Type)
  | finished (outcome : SavedOutcome P)
  | ready (work : Ready P)

/-- This preparation result handler needs no runner row. Even an empty state can
publish a current read error. Successful preparation returns concrete child data
without inventing how native missing-row allocation would succeed. -/
def handleSource (attempt : Attempt) (o : Observations P) (s : SavedState P)
    (events : List Event) : Entered P :=
  match attempt.source with
  | .error message =>
      let failure := PreparationFailure.mk attempt.input.revision attempt.input.pusher
        message o.preparationSQL.failedAt
      let (after, published) := publishPreparation (preparationKey attempt.input) (some failure) o.preparationSQL s
      .finished (close attempt.input.parent
        ⟨after, .sourceRefused message, none, events ++ [.sourceWarning message] ++ published⟩)
  | .ok source =>
      let (cleared, published) := publishPreparation (preparationKey attempt.input) none o.preparationSQL s
      .ready ⟨attempt.input.parent, parentInput attempt.input source, o.link, cleared, events ++ published⟩

/-- Immutable result completion and actual guarded admission/publication. This
function is total for arbitrary stored state, including no row at any deployment.
Native validation precedes BOTH source outcomes; no failure table is consulted. -/
def enter (attempt : Attempt) (o : Observations P) (s : SavedState P) : Entered P :=
  let key := attempt.input.parent
  let start := [Event.sourceAttempt attempt.input,
    Event.sourceReply (attempt.source.map (fun _ => ())), Event.acquired key]
  let discard := fun trace => Entered.finished (close key ⟨s, .discarded, none, trace⟩)
  if o.afterLockCancelled then discard (start ++ [.cancelled]) else
  match attempt.origin with
  | .notification => handleSource attempt o s start
  | .savedRetry watcher =>
      let queried := start ++ [.nativeAttempt watcher attempt.input.parent.repository attempt.input.branchId,
        .nativeReply o.nativeBranch]
      match o.nativeBranch with
      | .error error => discard (queried ++ [.nativeWarning error])
      | .ok branch =>
          if branch.id ≠ attempt.input.branchId ∨ branch.name ≠ attempt.input.parent.branch ∨
              branch.deleted = true ∨ branch.latest ≠ attempt.input.revision then
            discard queried
          else if o.afterNativeCancelled then discard (queried ++ [.cancelled])
          else handleSource attempt o s queried

/-- This existing-row region alone requires actual row evidence. The successful
native prefix is interpreted only in QuiescentPrefix; omitted prefix reads are
not claimed verified. The original child and all its outputs remain unchanged. -/
def completeExisting [DecidableEq P] (d : Deployment) (backing : Row) (ready : Ready P)
    (bound : ready.state.watcher.factory.rows d = some backing) : SavedOutcome P :=
  let child := Watcher.parentLink d backing ready.input ready.link ready.state.watcher bound
  close ready.parent ⟨{ ready.state with watcher := child.state }, .continued,
    some child, ready.trace ++ [.linkRegion child.trace]⟩

/-- Optional composition API: row evidence is needed ONLY if admission returns
Ready, never on stale/cancelled/native-error/source-error completed branches.
No implementation of missing-row success is fabricated. -/
def finishExisting [DecidableEq P] (d : Deployment) (backing : Row) (attempt : Attempt)
    (o : Observations P) (s : SavedState P)
    (existing : ∀ ready, enter attempt o s = .ready ready →
      ready.state.watcher.factory.rows d = some backing) : SavedOutcome P :=
  match selected : enter attempt o s with
  | .finished result => result
  | .ready ready => completeExisting d backing ready (existing ready selected)

end FactoryRetarget.SavedParent
