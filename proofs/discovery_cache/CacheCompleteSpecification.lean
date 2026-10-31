import CacheStartProof

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof CacheStartSpecification

namespace CacheCompleteSpecification
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Minimum-key eviction is represented by the list head only on sorted native
map representations. This independent room operation also makes the exact
whole-list update observable without silently assuming sortedness. -/
def room {V : Type} (key : Key) (entries : List (Key × V)) : List (Key × V) :=
  if entries.length ≥ 4096 ∧ lookup key entries = none then entries.tail else entries

def victim {V : Type} (key : Key) (entries : List (Key × V)) : Option Key :=
  if entries.length ≥ 4096 ∧ lookup key entries = none then entries.head?.map Prod.fst else none

/-- The actual library vector comparison is retained for arbitrary PartialEq
implementations. It can fail/diverge and need not equal structural Lean equality.
The own complete closure is not assumed successful or made opaque. -/
def changed {T : Type} (eq : core.cmp.PartialEq T T)
    (old : Option (mcp.discovery.CachedItems T)) (items : alloc.vec.Vec T)
    (denied : U32) : Result Bool :=
  match old with
  | none => .ok true
  | some previous => do
    let different ← alloc.vec.partial_eq.PartialEqVec.ne eq previous.items items
    if different then .ok true else .ok (previous.denied != denied)

/-- Exactly five seconds in nanoseconds; no millisecond projection of Instant. -/
def retryDelay : core.time.Duration := ⟨5000000000⟩

/-- Physical list/usize bounds only on the map whose length is read. A stale
fetch returns before either map is measured, so needs no representation premise. -/
def Physical {T : Type} (state : State T) (fetch : Fetch)
    (items : Option (alloc.vec.Vec T)) : Prop :=
  lookup fetch.key state.in_flight ≠ some fetch.request ∨
    match items with
    | none => state.failed.length ≤ Usize.max
    | some _ => state.entries.length ≤ Usize.max

/-- Independent complete state/result contract. Stale means complete no-op;
`false` alone does not. Failed discovery keeps cached values, but removes the
current ticket and writes a retry record after the native clock/addition bind.
Successful discovery moves the entire supplied vector and denied count, removes
its ticket/retry, and preserves unrelated values except the named room victim.

Clock, vector comparison and native Instant.add keep their Result fail/div.
There is no assertion about native post-panic mutex state or effect counts. -/
def completeOutcome {T : Type} (eq : core.cmp.PartialEq T T) (state : State T)
    (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (clock : Result Instant) :
    Result (Bool × State T) :=
  if lookup fetch.key state.in_flight ≠ some fetch.request then .ok (false, state)
  else
    match items with
    | none => do
      let sampled ← clock
      let deadline ← tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add
        sampled retryDelay
      .ok (false, { state with
        in_flight := erase fetch.key state.in_flight
        failed := put fetch.key deadline (room fetch.key state.failed) })
    | some values => do
      let different ← changed eq (lookup fetch.key state.entries) values fetch.denied
      .ok (different, { state with
        entries := put fetch.key ({ items := values, denied := fetch.denied } : mcp.discovery.CachedItems T)
          (room fetch.key state.entries)
        in_flight := erase fetch.key state.in_flight
        failed := erase fetch.key state.failed })

end
end CacheCompleteSpecification
