import CacheKeyFrameProof

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof

namespace CacheStartSpecification
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Fetch := mcp.discovery.DiscoveryFetch
abbrev Ticket := uuid.Uuid
abbrev Instant := tokio.time.instant.Instant

/-- Whole-value insertion in the independently named, full-field key order.
An equal native key retains its old key representation and replaces only its
value, matching BTreeMap insertion. Native ordering/refinement remains explicit. -/
def put {V : Type} (key : Key) (value : V) : List (Key × V) → List (Key × V)
  | [] => [(key, value)]
  | (oldKey, oldValue) :: rest =>
    match keyOrder key oldKey with
    | .lt => (key, value) :: (oldKey, oldValue) :: rest
    | .eq => (oldKey, value) :: rest
    | .gt => (oldKey, oldValue) :: put key value rest

/-- Exact key-order map insertion returns its previous value independently of
whether this particular key was present; no arbitrary native enumeration chosen. -/
def oldAtInsertion {V : Type} (key : Key) : List (Key × V) → Option V
  | [] => none
  | (oldKey, oldValue) :: rest =>
    match keyOrder key oldKey with
    | .lt => none
    | .eq => some oldValue
    | .gt => oldAtInsertion key rest

/-- Physical finite-list/usize correspondence only, not a new cache admission
rule. Higher-generation work first clears this map, so needs no old-map bound. -/
def Physical {T : Type} (state : State T) (key : Key) : Prop :=
  key.catalog_generation.val > state.generation.val ∨ state.in_flight.length ≤ Usize.max

/-- A strictly newer generation clears all three maps before any subsequent
checks. Equal generation preserves the full original state. Older work is handled
before this value is used. -/
def prepared {T : Type} (state : State T) (key : Key) : State T :=
  if key.catalog_generation.val > state.generation.val then
    { generation := key.catalog_generation, entries := [], in_flight := [], failed := [] }
  else state

/-- The clock Result is bound only for a coalesced request with a retry record.
No native no-argument clock axiom or total-callback premise is introduced. -/
def cooling {T : Type} (state : State T) (key : Key) (coalesce : Bool)
    (clock : Result Instant) : Result Bool :=
  if coalesce then
    match lookup key state.failed with
    | none => .ok false
    | some retry => do
      let sampled ← clock
      .ok (decide (retry.nanoseconds > sampled.nanoseconds))
  else .ok false

/-- Independent observable whole-state contract. Clock/fresh arguments are
operation-specific Result observations, possibly fail/div. An unused observation
is not bound. This specifies value/dependency ordering, not an instrumented effect
trace or a post-panic native mutex state. -/
def startOutcome {T : Type} (state : State T) (key : Key) (coalesce : Bool)
    (clock : Result Instant) (fresh : Result Ticket) :
    Result (Option Fetch × State T) :=
  if key.catalog_generation.val < state.generation.val then .ok (none, state)
  else do
    let next := prepared state key
    let blocked ← cooling next key coalesce clock
    if blocked then .ok (none, next)
    else if coalesce && (lookup key next.in_flight).isSome then .ok (none, next)
    else if next.in_flight.length ≥ 4096 then .ok (none, next)
    else do
      let ticket ← fresh
      .ok (some { key := key, request := ticket, denied := 0#u32 },
        { next with in_flight := put key ticket next.in_flight })

end
end CacheStartSpecification
