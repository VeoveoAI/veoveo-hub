import FactorySavedParentModel
import FactoryWatcherHistoryProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget.SavedParent
open Watcher
variable {P : Type}

/-- Independent native condition: no diagnostic lookup or revision ordering. -/
def NativeMatches (input : Input) (branch : NativeBranch) : Prop :=
  branch.id = input.branchId ∧ branch.name = input.parent.branch ∧
  branch.deleted = false ∧ branch.latest = input.revision

/-- Exact complete keyed SQL observation, independent of its acknowledgement. -/
def preparationsAfter (before : PreparationKey → Option PreparationFailure)
    (key : PreparationKey) (value : Option PreparationFailure) (committed : Bool) :=
  fun other => if committed = true ∧ other = key then value else before other

def sourceEvents (attempt : Attempt) : List Event :=
  [.sourceAttempt attempt.input, .sourceReply (attempt.source.map (fun _ => ())),
    .acquired attempt.input.parent]

def nativeEvents (attempt : Attempt) (watcher : String) (o : Observations P) : List Event :=
  sourceEvents attempt ++ [.nativeAttempt watcher attempt.input.parent.repository attempt.input.branchId,
    .nativeReply o.nativeBranch]

/-- Concrete successful admission data, matching the actual handler construction.
Neither ParentInput nor a successful after-state is a premise. -/
def derivedReady (input : Input) (source : LinkedSource) (o : Observations P)
    (s : SavedState P) (events : List Event) : Ready P :=
  let (cleared, published) := publishPreparation (preparationKey input) none o.preparationSQL s
  ⟨input.parent, parentInput input source, o.link, cleared, events ++ published⟩

theorem begin_fields (input : Input) (origin : Origin) (read : Except String LinkedSource) :
    (begin input origin read).input = input ∧ (begin input origin read).origin = origin ∧
    (begin input origin read).source = read := ⟨rfl, rfl, rfl⟩

theorem constructed_parent_fields (input : Input) (source : LinkedSource) :
    (parentInput input source).key = ⟨input.parent.repository, input.parent.branch, source.linkName⟩ ∧
    (parentInput input source).watchedDeployment = input.watchedDeployment ∧
    (parentInput input source).requestedRepository = source.requestedRepository ∧
    (parentInput input source).parentBranchId = input.branchId ∧
    (parentInput input source).pinnedRevision = source.pinnedRevision ∧
    (parentInput input source).follows = source.follows ∧
    (parentInput input source).revision = input.revision ∧
    (parentInput input source).pusher = input.pusher ∧
    (parentInput input source).ownTree = source.ownTree ∧
    (parentInput input source).ownEntries = source.ownEntries := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem publication_exact (key : PreparationKey) (value : Option PreparationFailure)
    (o : Publication) (s : SavedState P) :
    (publishPreparation key value o s).1.watcher = s.watcher ∧
    (publishPreparation key value o s).1.preparations =
      preparationsAfter s.preparations key value o.committed := ⟨rfl, rfl⟩

theorem publication_other_key (before : PreparationKey → Option PreparationFailure)
    (key other : PreparationKey) (value : Option PreparationFailure) (committed : Bool)
    (different : other ≠ key) : preparationsAfter before key value committed other = before other := by
  simp [preparationsAfter, different]

theorem ready_fields (input : Input) (source : LinkedSource) (o : Observations P)
    (s : SavedState P) (events : List Event) :
    (derivedReady input source o s events).input = parentInput input source ∧
    (derivedReady input source o s events).link = o.link ∧
    (derivedReady input source o s events).state.watcher = s.watcher ∧
    (derivedReady input source o s events).state.preparations =
      preparationsAfter s.preparations (preparationKey input) none o.preparationSQL.committed := by
  exact ⟨rfl, rfl, rfl, rfl⟩

/-- Source success or failure is not handled until this native branch condition
and both actual saved-retry cancellation observations permit it. -/
theorem current_enter (attempt : Attempt) (watcher : String) (o : Observations P)
    (s : SavedState P) (branch : NativeBranch)
    (origin : attempt.origin = .savedRetry watcher) (active : o.afterLockCancelled = false)
    (read : o.nativeBranch = .ok branch) (matching : NativeMatches attempt.input branch)
    (afterNative : o.afterNativeCancelled = false) :
    enter attempt o s = handleSource attempt o s (nativeEvents attempt watcher o) := by
  rcases matching with ⟨id, name, live, head⟩
  simp [enter, origin, active, read, id, name, live, head, afterNative,
    sourceEvents, nativeEvents]

theorem notification_enter (attempt : Attempt) (o : Observations P) (s : SavedState P)
    (origin : attempt.origin = .notification) (active : o.afterLockCancelled = false) :
    enter attempt o s = handleSource attempt o s (sourceEvents attempt) := by
  simp [enter, origin, active, sourceEvents]

/-- No row, uniqueness or prefix restriction: arbitrary membership and BOTH
failure maps are retained, including genuine uncleared diagnostic rows. -/
theorem obsolete_enter (attempt : Attempt) (watcher : String) (o : Observations P)
    (s : SavedState P) (branch : NativeBranch) (origin : attempt.origin = .savedRetry watcher)
    (read : o.nativeBranch = .ok branch)
    (obsolete : branch.id ≠ attempt.input.branchId ∨ branch.name ≠ attempt.input.parent.branch ∨
      branch.deleted = true ∨ branch.latest ≠ attempt.input.revision) :
    let events := if o.afterLockCancelled then sourceEvents attempt ++ [.cancelled]
      else nativeEvents attempt watcher o
    enter attempt o s = .finished (close attempt.input.parent ⟨s, .discarded, none, events⟩) := by
  cases active : o.afterLockCancelled <;>
    simp [enter, origin, read, obsolete, active, sourceEvents, nativeEvents]

theorem native_error_enter (attempt : Attempt) (watcher : String) (o : Observations P)
    (s : SavedState P) (error : Error) (origin : attempt.origin = .savedRetry watcher)
    (read : o.nativeBranch = .error error) :
    let events := if o.afterLockCancelled then sourceEvents attempt ++ [.cancelled]
      else nativeEvents attempt watcher o ++ [.nativeWarning error]
    enter attempt o s = .finished (close attempt.input.parent ⟨s, .discarded, none, events⟩) := by
  cases active : o.afterLockCancelled <;>
    simp [enter, origin, read, active, sourceEvents, nativeEvents]

theorem after_lock_cancellation (attempt : Attempt) (o : Observations P) (s : SavedState P)
    (cancelled : o.afterLockCancelled = true) :
    enter attempt o s = .finished (close attempt.input.parent
      ⟨s, .discarded, none, sourceEvents attempt ++ [.cancelled]⟩) := by
  simp [enter, cancelled, sourceEvents]

theorem after_native_cancellation (attempt : Attempt) (watcher : String) (o : Observations P)
    (s : SavedState P) (branch : NativeBranch) (origin : attempt.origin = .savedRetry watcher)
    (active : o.afterLockCancelled = false) (read : o.nativeBranch = .ok branch)
    (matching : NativeMatches attempt.input branch) (cancelled : o.afterNativeCancelled = true) :
    enter attempt o s = .finished (close attempt.input.parent
      ⟨s, .discarded, none, nativeEvents attempt watcher o ++ [.cancelled]⟩) := by
  rcases matching with ⟨id, name, live, head⟩
  simp [enter, origin, active, read, id, name, live, head, cancelled, sourceEvents, nativeEvents]

/-- This internal decomposition is in the SAME held parent guard. Conditional
row evidence disappears on a finished branch; no row is presumed for discard. -/
theorem finished_composition [DecidableEq P] (d : Deployment) (backing : Row)
    (attempt : Attempt) (o : Observations P) (s : SavedState P) (result : SavedOutcome P)
    (finished : enter attempt o s = .finished result)
    (existing : ∀ work, enter attempt o s = .ready work → work.state.watcher.factory.rows d = some backing) :
    finishExisting d backing attempt o s existing = result := by
  unfold finishExisting
  split
  · rename_i returned selected
    exact Entered.finished.inj (selected.symm.trans finished)
  · rename_i work selected
    have impossible := selected.symm.trans finished
    cases impossible

/-- An explicit source error publishes the complete actual payload even with no
runner. An Err reply may accompany a committed write; no later cancellation
observation is invented to roll back that already-started SQL statement. -/
theorem current_source_error (input : Input) (watcher message : String) (o : Observations P)
    (s : SavedState P) (branch : NativeBranch) (active : o.afterLockCancelled = false)
    (read : o.nativeBranch = .ok branch) (matching : NativeMatches input branch)
    (afterNative : o.afterNativeCancelled = false) :
    let attempt := begin input (.savedRetry watcher) (.error message)
    let failure := PreparationFailure.mk input.revision input.pusher message o.preparationSQL.failedAt
    ∃ result, enter attempt o s = .finished result ∧ result.exit = .sourceRefused message ∧
      result.child = none ∧ result.state.watcher = s.watcher ∧
      result.state.preparations = preparationsAfter s.preparations (preparationKey input)
        (some failure) o.preparationSQL.committed ∧
      Event.sourceWarning message ∈ result.trace ∧
      Event.preparationStatusReply o.preparationSQL.reply ∈ result.trace := by
  dsimp only
  let attempt := begin input (.savedRetry watcher) (.error message)
  rw [current_enter attempt watcher o s branch rfl active read matching afterNative]
  cases reply : o.preparationSQL.reply <;>
    simp [handleSource, attempt, begin, publishPreparation,
      close, reply, nativeEvents, sourceEvents]
  all_goals rfl

/-- Concrete Ready is derived from current admission, not assumed correct. -/
theorem current_source_ready (input : Input) (watcher : String) (source : LinkedSource)
    (o : Observations P) (s : SavedState P) (branch : NativeBranch)
    (active : o.afterLockCancelled = false) (read : o.nativeBranch = .ok branch)
    (matching : NativeMatches input branch) (afterNative : o.afterNativeCancelled = false) :
    let attempt := begin input (.savedRetry watcher) (.ok source)
    enter attempt o s = .ready (derivedReady input source o s (nativeEvents attempt watcher o)) := by
  dsimp only
  let attempt := begin input (.savedRetry watcher) (.ok source)
  rw [current_enter attempt watcher o s branch rfl active read matching afterNative]
  rfl

theorem notification_source_ready (input : Input) (source : LinkedSource)
    (o : Observations P) (s : SavedState P) (active : o.afterLockCancelled = false) :
    let attempt := begin input .notification (.ok source)
    enter attempt o s = .ready (derivedReady input source o s (sourceEvents attempt)) := by
  dsimp only
  rw [notification_enter _ o s rfl active]
  rfl

/-- Full actual existing child application; the quiescent native prefix domain
is BEFORE-state structure, not a desired successful output or Request premise. -/
theorem complete_source_full [DecidableEq P] (d : Deployment) (backing : Row)
    (input : Input) (source : LinkedSource) (o : Observations P) (s : SavedState P)
    (events : List Event) (resolved : Resolved P)
    (domain : QuiescentPrefix d backing input source s) (key : backing.deployment = d)
    (runner : Runner backing)
    (active : o.link.cancellation.afterLock = false) (change : o.link.cancellation.beforeChange = false)
    (publication : o.link.cancellation.beforePublication = false)
    (resolution : o.link.resolve (parentInput input source) (queriedExisting o.link.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative) :
    let ready := derivedReady input source o s events
    let child := Watcher.parentLink d backing (parentInput input source) o.link s.watcher domain.1.1
    let applied := apply d backing (parentRequest (parentInput input source) resolved) resolved.factory s.watcher.factory
    ∃ bound : ready.state.watcher.factory.rows d = some backing,
      let output := completeExisting d backing ready bound
      output.exit = .continued ∧ output.child.isSome = true ∧ output.child = some child ∧
      child.factoryOutcome = some applied ∧ applied.error = none ∧
      FactoryValues d backing (parentRequest (parentInput input source) resolved)
        resolved.factory s.watcher.factory output.state.watcher.factory ∧
      (output.state.watcher.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
        backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ ∧
      output.state.watcher.failures = statusAfter s.watcher.failures
        (publicationKey (parentInput input source).key input.watchedDeployment) none o.link.publication.committed ∧
      output.state.preparations = preparationsAfter s.preparations (preparationKey input)
        none o.preparationSQL.committed := by
  let ready := derivedReady input source o s events
  have bound : ready.state.watcher.factory.rows d = some backing := domain.1.1
  have full := parent_full_output d backing (parentInput input source) o.link s.watcher resolved
    domain.1.1 key runner active change publication resolution noFault capacity
  refine ⟨bound, rfl, rfl, rfl, full.2.2.1, full.2.1, ?_, ?_, ?_, rfl⟩
  · exact full.2.2.2.1
  · exact full.2.2.2.2.1
  · exact full.2.2.2.2.2

/-- A legitimate current-head retry accepts with complete row/client/native/
provider/request values, regardless of diagnostic presence. Enter and the child
execution are composed visibly; no arbitrary Ready or correct Request is given. -/
theorem current_head_full [DecidableEq P] (d : Deployment) (backing : Row)
    (input : Input) (watcher : String) (source : LinkedSource) (o : Observations P)
    (s : SavedState P) (branch : NativeBranch) (resolved : Resolved P)
    (domain : QuiescentPrefix d backing input source s) (key : backing.deployment = d) (runner : Runner backing)
    (active : o.afterLockCancelled = false) (read : o.nativeBranch = .ok branch)
    (matching : NativeMatches input branch) (afterNative : o.afterNativeCancelled = false)
    (childActive : o.link.cancellation.afterLock = false) (change : o.link.cancellation.beforeChange = false)
    (publication : o.link.cancellation.beforePublication = false)
    (resolution : o.link.resolve (parentInput input source) (queriedExisting o.link.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative) :
    let attempt := begin input (.savedRetry watcher) (.ok source)
    let ready := derivedReady input source o s (nativeEvents attempt watcher o)
    let child := Watcher.parentLink d backing (parentInput input source) o.link s.watcher domain.1.1
    let applied := apply d backing (parentRequest (parentInput input source) resolved) resolved.factory s.watcher.factory
    enter attempt o s = .ready ready ∧
    ∃ bound : ready.state.watcher.factory.rows d = some backing,
      let output := completeExisting d backing ready bound
      output.exit = .continued ∧ output.child.isSome = true ∧ output.child = some child ∧
      child.factoryOutcome = some applied ∧ applied.error = none ∧
      FactoryValues d backing (parentRequest (parentInput input source) resolved)
        resolved.factory s.watcher.factory output.state.watcher.factory ∧
      (output.state.watcher.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
        backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ ∧
      output.state.watcher.failures = statusAfter s.watcher.failures
        (publicationKey (parentInput input source).key input.watchedDeployment) none o.link.publication.committed ∧
      output.state.preparations = preparationsAfter s.preparations (preparationKey input)
        none o.preparationSQL.committed := by
  exact ⟨current_source_ready input watcher source o s branch active read matching afterNative,
    complete_source_full d backing input source o s _ resolved domain key runner
      childActive change publication resolution noFault capacity⟩

#print axioms obsolete_enter
#print axioms native_error_enter
#print axioms after_native_cancellation
#print axioms current_source_error
#print axioms current_source_ready
#print axioms complete_source_full
#print axioms current_head_full

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``NativeMatches, ``preparationsAfter, ``sourceEvents, ``nativeEvents, ``derivedReady,
      ``begin_fields, ``constructed_parent_fields, ``publication_exact, ``publication_other_key,
      ``ready_fields, ``current_enter, ``notification_enter, ``obsolete_enter, ``native_error_enter,
      ``after_lock_cancellation, ``after_native_cancellation, ``finished_composition,
      ``current_source_error, ``current_source_ready, ``notification_source_ready,
      ``complete_source_full, ``current_head_full,
      ``begin, ``publishPreparation, ``handleSource, ``enter, ``completeExisting, ``finishExisting] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected saved-parent proof axiom in {declaration}: {unexpected}"

end FactoryRetarget.SavedParent
