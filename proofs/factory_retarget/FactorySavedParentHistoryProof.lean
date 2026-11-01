import FactorySavedParentProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget.SavedParent
open Watcher
variable {P : Type}

/-- Structural prefix domain transfer follows the actual immutable row effects
and keyed diagnostic clear. It does not assume the desired after-state domain. -/
theorem quiescent_transfer (d : Deployment) (backing next : Row) (input : Input)
    (source : LinkedSource) (before after : SavedState P)
    (domain : QuiescentPrefix d backing input source before)
    (preserved : IdentityFrame before.watcher.factory after.watcher.factory)
    (stored : after.watcher.factory.rows d = some next)
    (sameIdentity : identity next = identity backing) (committed : Bool)
    (diagnostics : after.watcher.failures = statusAfter before.watcher.failures
      (publicationKey (parentInput input source).key input.watchedDeployment) none committed) :
    QuiescentPrefix d next input source after := by
  classical
  refine ⟨identity_keeps_row_at (parentInput input source).key d before.watcher.factory
    after.watcher.factory backing next domain.1 stored sameIdentity,
    unique_of_identity before.watcher.factory after.watcher.factory domain.2.1 preserved, ?_, ?_⟩
  · intro other row found repository branch
    obtain ⟨original, originalAt, same⟩ := previous_row before.watcher.factory after.watcher.factory preserved other row found
    have fields := identity_key_fields row original same
    have originalLink := domain.2.2.1 other original originalAt
      (fields.2.1.symm.trans repository) (fields.2.2.1.symm.trans branch)
    exact fields.2.2.2.trans originalLink
  · intro key failure present deployment repository branch
    have original : before.watcher.failures key = some failure := by
      rw [diagnostics] at present
      by_cases cleared : committed = true ∧ key = publicationKey (parentInput input source).key input.watchedDeployment
      · simp [statusAfter, cleared] at present
      · simpa [statusAfter, cleared] using present
    exact domain.2.2.2 key failure original deployment repository branch

/-- Actual successful first application derives the next row, its Runner shape,
immutable deployment and the entire prefix domain needed by a later duplicate. -/
theorem complete_preserves_domain [DecidableEq P] (d : Deployment) (backing : Row)
    (input : Input) (source : LinkedSource) (o : Observations P) (s : SavedState P)
    (events : List Event) (resolved : Resolved P)
    (domain : QuiescentPrefix d backing input source s) (key : backing.deployment = d) (runner : Runner backing)
    (active : o.link.cancellation.afterLock = false) (change : o.link.cancellation.beforeChange = false)
    (publication : o.link.cancellation.beforePublication = false)
    (resolution : o.link.resolve (parentInput input source) (queriedExisting o.link.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative) :
    let ready := derivedReady input source o s events
    let first := completeExisting d backing ready domain.1.1
    let next := successRow backing (parentRequest (parentInput input source) resolved)
      resolved.factory (choice backing (parentRequest (parentInput input source) resolved) resolved.factory.prepared resolved.factory.readNative)
    QuiescentPrefix d next input source first.state ∧ next.deployment = d ∧ Runner next := by
  let ready := derivedReady input source o s events
  let first := completeExisting d backing ready domain.1.1
  let q := parentRequest (parentInput input source) resolved
  let next := successRow backing q resolved.factory (choice backing q resolved.factory.prepared resolved.factory.readNative)
  obtain ⟨_bound, _continued, _hasChild, _exactChild, _childResult, _normal, values, _visible, diagnostics, _preparations⟩ :=
    complete_source_full d backing input source o s events resolved domain key runner active change publication resolution noFault capacity
  have preserved := parent_preserves_key_and_unique d backing (parentInput input source) o.link s.watcher resolved
    domain.1 domain.2.1 active change publication resolution
  have identityFrame : IdentityFrame s.watcher.factory first.state.watcher.factory := preserved.2
  have stored : first.state.watcher.factory.rows d = some next := values.1
  have sameIdentity : identity next = identity backing := success_identity backing q resolved.factory _
  have nextDomain := quiescent_transfer d backing next input source s first.state domain identityFrame
    stored sameIdentity o.link.publication.committed diagnostics
  have linked : backing.link = some source.linkName := by simpa [parentInput] using domain.1.2.2.2
  refine ⟨nextDomain, key, ?_⟩
  simp [Runner, successRow, linked]

/-- Actual newer notification preparation/application precedes the guarded finish
of an already-read saved attempt. The old result can be Ok OR Err. No correct
first Request/state is given: enter constructs Ready, then the original child
supplies all complete outputs. The native rejection is an explicit observation,
not a claim that a mutex prevents Lore from moving. -/
theorem newer_parent_then_saved [DecidableEq P] (d : Deployment) (backing : Row)
    (input : Input) (source : LinkedSource) (parent : Observations P) (s : SavedState P)
    (resolved : Resolved P) (domain : QuiescentPrefix d backing input source s)
    (key : backing.deployment = d) (runner : Runner backing)
    (active : parent.afterLockCancelled = false)
    (childActive : parent.link.cancellation.afterLock = false)
    (change : parent.link.cancellation.beforeChange = false)
    (publication : parent.link.cancellation.beforePublication = false)
    (resolution : parent.link.resolve (parentInput input source) (queriedExisting parent.link.query) = .ok resolved)
    (noFault : resolved.factory.fault = none) (capacity : CapacityAllowed resolved.factory resolved.factory.readNative)
    (old : Input) (watcher : String) (oldSource : Except String LinkedSource)
    (later : Observations P) (branch : NativeBranch) (read : later.nativeBranch = .ok branch)
    (sameParent : old.parent = input.parent) (sameId : old.branchId = input.branchId)
    (nativeCurrent : NativeMatches input branch) (advanced : input.revision ≠ old.revision) :
    let fresh := begin input .notification (.ok source)
    let ready := derivedReady input source parent s (sourceEvents fresh)
    let child := Watcher.parentLink d backing (parentInput input source) parent.link s.watcher domain.1.1
    let applied := apply d backing (parentRequest (parentInput input source) resolved) resolved.factory s.watcher.factory
    branch.id = old.branchId ∧ branch.name = old.parent.branch ∧
    enter fresh parent s = .ready ready ∧
    ∃ bound : ready.state.watcher.factory.rows d = some backing,
      let first := completeExisting d backing ready bound
      first.exit = .continued ∧ first.child.isSome = true ∧ first.child = some child ∧
      child.factoryOutcome = some applied ∧ applied.error = none ∧
      FactoryValues d backing (parentRequest (parentInput input source) resolved)
        resolved.factory s.watcher.factory first.state.watcher.factory ∧
      (first.state.watcher.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
        backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ ∧
      first.state.watcher.failures = statusAfter s.watcher.failures
        (publicationKey (parentInput input source).key input.watchedDeployment) none parent.link.publication.committed ∧
      first.state.preparations = preparationsAfter s.preparations (preparationKey input)
        none parent.preparationSQL.committed ∧
      ∃ last, enter (begin old (.savedRetry watcher) oldSource) later first.state = .finished last ∧
        last.exit = .discarded ∧ last.child = none ∧ last.state = first.state ∧
        (last.state.watcher.factory.rows d).map client = some ⟨backing.deployment, some input.pusher,
          backing.repository, some resolved.target.repository, some resolved.target.pusher⟩ ∧
        ∀ diagnostic failure, s.watcher.failures diagnostic = some failure →
          parent.link.publication.committed = false →
          first.state.watcher.failures diagnostic = some failure ∧ last.state.watcher.failures diagnostic = some failure := by
  let fresh := begin input .notification (.ok source)
  let ready := derivedReady input source parent s (sourceEvents fresh)
  obtain ⟨bound, continued, hasChild, exactChild, childResult, normal, values, visible, diagnostics, preparations⟩ :=
    complete_source_full d backing input source parent s (sourceEvents fresh) resolved domain key runner
      childActive change publication resolution noFault capacity
  let first := completeExisting d backing ready bound
  have oldId : branch.id = old.branchId := nativeCurrent.1.trans sameId.symm
  have oldName : branch.name = old.parent.branch :=
    nativeCurrent.2.1.trans (congrArg ParentKey.branch sameParent).symm
  have obsoleteHead : branch.latest ≠ old.revision := by
    intro same
    exact advanced (nativeCurrent.2.2.2.symm.trans same)
  refine ⟨oldId, oldName, notification_source_ready input source parent s active, bound, continued, hasChild, exactChild, childResult, normal,
    values, visible, diagnostics, preparations, ?_⟩
  let attempt := begin old (.savedRetry watcher) oldSource
  let events := if later.afterLockCancelled then sourceEvents attempt ++ [.cancelled]
    else nativeEvents attempt watcher later
  let last : SavedOutcome P := close old.parent ⟨first.state, .discarded, none, events⟩
  refine ⟨last, ?_, rfl, rfl, rfl, visible, ?_⟩
  · exact obsolete_enter attempt watcher later first.state branch rfl read
      (Or.inr (Or.inr (Or.inr obsoleteHead)))
  · intro diagnostic failure present uncommitted
    have ignored : statusAfter s.watcher.failures
        (publicationKey (parentInput input source).key input.watchedDeployment) none
        parent.link.publication.committed = s.watcher.failures := by
      rw [uncommitted]
      funext other
      simp [Watcher.statusAfter]
    have same : first.state.watcher.failures = s.watcher.failures := by
      change (completeExisting d backing (derivedReady input source parent s (sourceEvents fresh)) bound).state.watcher.failures = s.watcher.failures
      exact diagnostics.trans ignored
    exact ⟨by rw [same]; exact present, by change first.state.watcher.failures diagnostic = some failure; rw [same]; exact present⟩

/-- The same-branch/head-change specialization requires no ordering on revision
hashes. Branch truth is the native observation at the old attempt's guarded call. -/
theorem changed_head_rejects (old : Input) (watcher : String) (source : Except String LinkedSource)
    (o : Observations P) (s : SavedState P) (branch : NativeBranch)
    (read : o.nativeBranch = .ok branch) (head : branch.latest ≠ old.revision) :
    ∃ result, enter (begin old (.savedRetry watcher) source) o s = .finished result ∧
      result.state = s ∧ result.exit = .discarded ∧ result.child = none := by
  let attempt := begin old (.savedRetry watcher) source
  let events := if o.afterLockCancelled then sourceEvents attempt ++ [.cancelled]
    else nativeEvents attempt watcher o
  refine ⟨close old.parent ⟨s, .discarded, none, events⟩, ?_, rfl, rfl, rfl⟩
  exact obsolete_enter attempt watcher o s branch rfl read (Or.inr (Or.inr (Or.inr head)))

/-- A deleted old branch may still report the original name AND revision. This
rejects even without a head mismatch and preserves an arbitrary live replacement
row at that name; it never issues removal by the saved name. -/
theorem deleted_name_reuse_rejects (old : Input) (watcher : String)
    (source : Except String LinkedSource) (o : Observations P) (s : SavedState P)
    (branch : NativeBranch) (read : o.nativeBranch = .ok branch)
    (sameId : branch.id = old.branchId) (sameName : branch.name = old.parent.branch)
    (sameHead : branch.latest = old.revision) (deleted : branch.deleted = true) :
    branch.id = old.branchId ∧ branch.name = old.parent.branch ∧ branch.latest = old.revision ∧
    ∃ result, enter (begin old (.savedRetry watcher) source) o s = .finished result ∧
      result.state = s ∧ result.exit = .discarded ∧ result.child = none := by
  let attempt := begin old (.savedRetry watcher) source
  let events := if o.afterLockCancelled then sourceEvents attempt ++ [.cancelled]
    else nativeEvents attempt watcher o
  refine ⟨sameId, sameName, sameHead, close old.parent ⟨s, .discarded, none, events⟩, ?_, rfl, rfl, rfl⟩
  exact obsolete_enter attempt watcher o s branch rfl read (Or.inr (Or.inr (Or.inl deleted)))

/-- The newer failure is an ACTUAL source-error publication, including its full
payload and uncertain commit. The subsequent stale Ok/Err preparation cannot
change that result, even if any diagnostic row still requests the old revision. -/
theorem current_error_then_obsolete (input : Input) (watcher message : String)
    (parent : Observations P) (s : SavedState P) (current : NativeBranch)
    (active : parent.afterLockCancelled = false) (native : parent.nativeBranch = .ok current)
    (matching : NativeMatches input current) (afterNative : parent.afterNativeCancelled = false)
    (old : Input) (oldWatcher : String) (oldSource : Except String LinkedSource)
    (later : Observations P) (oldBranch : NativeBranch) (read : later.nativeBranch = .ok oldBranch)
    (obsolete : oldBranch.id ≠ old.branchId ∨ oldBranch.name ≠ old.parent.branch ∨
      oldBranch.deleted = true ∨ oldBranch.latest ≠ old.revision) :
    ∃ first, enter (begin input (.savedRetry watcher) (.error message)) parent s = .finished first ∧
      first.exit = .sourceRefused message ∧ first.child = none ∧ first.state.watcher = s.watcher ∧
      first.state.preparations = preparationsAfter s.preparations (preparationKey input)
        (some ⟨input.revision, input.pusher, message, parent.preparationSQL.failedAt⟩) parent.preparationSQL.committed ∧
      ∃ last, enter (begin old (.savedRetry oldWatcher) oldSource) later first.state = .finished last ∧
        last.state = first.state ∧ last.exit = .discarded ∧ last.child = none := by
  obtain ⟨first, entered, refused, noChild, unchanged, preparation, _warning, _reply⟩ :=
    current_source_error input watcher message parent s current active native matching afterNative
  refine ⟨first, entered, refused, noChild, unchanged, preparation, ?_⟩
  let attempt := begin old (.savedRetry oldWatcher) oldSource
  let events := if later.afterLockCancelled then sourceEvents attempt ++ [.cancelled]
    else nativeEvents attempt oldWatcher later
  refine ⟨close old.parent ⟨first.state, .discarded, none, events⟩, ?_, rfl, rfl, rfl⟩
  exact obsolete_enter attempt oldWatcher later first.state oldBranch rfl read obsolete

/-- Actual parent application clears diagnostics, then a same-head saved retry
is still admitted and produces complete outputs. Its next row/domain is derived
from the first real child, never supplied as a desired intermediate invariant. -/
theorem normal_then_current_duplicate [DecidableEq P] (d : Deployment) (backing : Row)
    (input : Input) (source : LinkedSource) (parent later : Observations P)
    (s : SavedState P) (firstResolved secondResolved : Resolved P)
    (domain : QuiescentPrefix d backing input source s) (key : backing.deployment = d) (runner : Runner backing)
    (active : parent.afterLockCancelled = false)
    (childActive : parent.link.cancellation.afterLock = false)
    (change : parent.link.cancellation.beforeChange = false)
    (publication : parent.link.cancellation.beforePublication = false)
    (resolution : parent.link.resolve (parentInput input source) (queriedExisting parent.link.query) = .ok firstResolved)
    (noFault : firstResolved.factory.fault = none)
    (capacity : CapacityAllowed firstResolved.factory firstResolved.factory.readNative)
    (cleared : parent.link.publication.committed = true) (preparationCleared : parent.preparationSQL.committed = true)
    (watcher : String) (branch : NativeBranch) (laterActive : later.afterLockCancelled = false)
    (read : later.nativeBranch = .ok branch) (matching : NativeMatches input branch)
    (afterNative : later.afterNativeCancelled = false)
    (laterChildActive : later.link.cancellation.afterLock = false)
    (laterChange : later.link.cancellation.beforeChange = false)
    (laterPublication : later.link.cancellation.beforePublication = false)
    (laterResolution : later.link.resolve (parentInput input source) (queriedExisting later.link.query) = .ok secondResolved)
    (laterNoFault : secondResolved.factory.fault = none)
    (laterCapacity : CapacityAllowed secondResolved.factory secondResolved.factory.readNative) :
    let fresh := begin input .notification (.ok source)
    let ready := derivedReady input source parent s (sourceEvents fresh)
    let first := completeExisting d backing ready domain.1.1
    let firstChild := Watcher.parentLink d backing (parentInput input source) parent.link s.watcher domain.1.1
    let firstApplied := apply d backing (parentRequest (parentInput input source) firstResolved) firstResolved.factory s.watcher.factory
    let next := successRow backing (parentRequest (parentInput input source) firstResolved)
      firstResolved.factory (choice backing (parentRequest (parentInput input source) firstResolved)
        firstResolved.factory.prepared firstResolved.factory.readNative)
    enter fresh parent s = .ready ready ∧ first.exit = .continued ∧ first.child = some firstChild ∧
    firstChild.factoryOutcome = some firstApplied ∧ firstApplied.error = none ∧
    FactoryValues d backing (parentRequest (parentInput input source) firstResolved)
      firstResolved.factory s.watcher.factory first.state.watcher.factory ∧
    first.state.watcher.failures (publicationKey (parentInput input source).key input.watchedDeployment) = none ∧
    first.state.preparations (preparationKey input) = none ∧
    ∃ nextDomain : QuiescentPrefix d next input source first.state,
      next.deployment = d ∧ Runner next ∧
      let attempt := begin input (.savedRetry watcher) (.ok source)
      let nextReady := derivedReady input source later first.state (nativeEvents attempt watcher later)
      let child := Watcher.parentLink d next (parentInput input source) later.link first.state.watcher nextDomain.1.1
      let applied := apply d next (parentRequest (parentInput input source) secondResolved)
        secondResolved.factory first.state.watcher.factory
      enter attempt later first.state = .ready nextReady ∧
      ∃ bound : nextReady.state.watcher.factory.rows d = some next,
        let output := completeExisting d next nextReady bound
        output.exit = .continued ∧ output.child.isSome = true ∧ output.child = some child ∧
        child.factoryOutcome = some applied ∧ applied.error = none ∧
        FactoryValues d next (parentRequest (parentInput input source) secondResolved)
          secondResolved.factory first.state.watcher.factory output.state.watcher.factory ∧
        (output.state.watcher.factory.rows d).map client = some ⟨next.deployment, some input.pusher,
          next.repository, some secondResolved.target.repository, some secondResolved.target.pusher⟩ ∧
        output.state.watcher.failures = statusAfter first.state.watcher.failures
          (publicationKey (parentInput input source).key input.watchedDeployment) none later.link.publication.committed ∧
        output.state.preparations = preparationsAfter first.state.preparations (preparationKey input)
          none later.preparationSQL.committed := by
  let fresh := begin input .notification (.ok source)
  let ready := derivedReady input source parent s (sourceEvents fresh)
  let first := completeExisting d backing ready domain.1.1
  let next := successRow backing (parentRequest (parentInput input source) firstResolved)
    firstResolved.factory (choice backing (parentRequest (parentInput input source) firstResolved)
      firstResolved.factory.prepared firstResolved.factory.readNative)
  obtain ⟨_bound, continued, _hasChild, exactChild, childResult, normal, values, _visible, diagnostics, preparations⟩ :=
    complete_source_full d backing input source parent s (sourceEvents fresh) firstResolved domain key runner
      childActive change publication resolution noFault capacity
  obtain ⟨nextDomain, nextKey, nextRunner⟩ := complete_preserves_domain d backing input source parent s
    (sourceEvents fresh) firstResolved domain key runner childActive change publication resolution noFault capacity
  refine ⟨notification_source_ready input source parent s active, continued, exactChild, childResult, normal,
    values, ?_, ?_, nextDomain, nextKey, nextRunner, ?_⟩
  · rw [diagnostics]
    simp [statusAfter, cleared]
  · rw [preparations]
    simp [preparationsAfter, preparationCleared]
  · exact current_head_full d next input watcher source later first.state branch secondResolved
      nextDomain nextKey nextRunner laterActive read matching afterNative laterChildActive laterChange
      laterPublication laterResolution laterNoFault laterCapacity

#print axioms quiescent_transfer
#print axioms complete_preserves_domain
#print axioms newer_parent_then_saved
#print axioms changed_head_rejects
#print axioms deleted_name_reuse_rejects
#print axioms current_error_then_obsolete
#print axioms normal_then_current_duplicate

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``quiescent_transfer, ``complete_preserves_domain, ``newer_parent_then_saved,
      ``changed_head_rejects, ``deleted_name_reuse_rejects, ``current_error_then_obsolete,
      ``normal_then_current_duplicate] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected saved-parent history axiom in {declaration}: {unexpected}"

end FactoryRetarget.SavedParent
