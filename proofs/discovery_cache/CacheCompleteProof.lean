import CacheCompleteSpecification

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof CacheStartSpecification CacheStartProof CacheCompleteSpecification

namespace CacheCompleteProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Exact modeled UUID/Option guard, derived from the actual generated dictionary
and primitive definitions; the whole complete result is not assumed. -/
theorem ticket_guard_exact (pending : Option Ticket) (request : Ticket) :
    core.cmp.PartialEq.ne.trait_default
      (core.option.Option.Insts.CoreCmpPartialEqOption
        (core.cmp.PartialEqShared uuid.Uuid.Insts.CoreCmpPartialEqUuid)) pending (some request) =
      .ok (decide (pending ≠ some request)) := by
  cases pending <;>
    simp [core.cmp.PartialEq.ne.trait_default, core.cmp.PartialEq.ne.default,
      core.option.Option.Insts.CoreCmpPartialEqOption,
      core.option.Option.Insts.CoreCmpPartialEqOption.eq,
      core.cmp.PartialEqShared, core.cmp.impls.PartialEqShared.eq,
      uuid.Uuid.Insts.CoreCmpPartialEqUuid,
      uuid.Uuid.Insts.CoreCmpPartialEqUuid.eq, bind_tc_eq, bind_ok]

/-- The SDK's actual extracted initializer is 5000ms and from_millis preserves
nanoseconds exactly. Neither the own constant nor conversion is a success axiom. -/
theorem ttl_delay_exact :
    core.time.Duration.from_millis veoveo_hub_sdk.protocol.PRIVATE_CATALOG_TTL_MS =
      .ok retryDelay := by
  simp [core.time.Duration.from_millis, veoveo_hub_sdk.protocol.PRIVATE_CATALOG_TTL_MS,
    retryDelay]

private theorem borrow_function :
    @HubDiscoveryCacheModel.core.borrow.Borrow.Blanket.borrow Key =
      (fun key => Result.ok key) := by rfl

private theorem native_get {V : Type} (entries : List (Key × V)) (key : Key) :
    alloc.collections.btree.map.BTreeMap.get
      alloc.alloc.Global.Insts.CoreAllocAllocatorClone (core.borrow.Borrow.Blanket Key)
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd entries key =
      .ok (lookup key entries) := by
  simp only [alloc.collections.btree.map.BTreeMap.get]
  rw [borrow_function]
  exact map_get_exact entries key

private theorem native_remove {V : Type} (entries : List (Key × V)) (key : Key) :
    alloc.collections.btree.map.BTreeMap.remove
      alloc.alloc.Global.Insts.CoreAllocAllocatorClone (core.borrow.Borrow.Blanket Key)
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd entries key =
      .ok (lookup key entries, erase key entries) := by
  simp only [alloc.collections.btree.map.BTreeMap.remove]
  rw [borrow_function]
  exact map_remove_exact entries key

private theorem native_contains {V : Type} (entries : List (Key × V)) (key : Key) :
    alloc.collections.btree.map.BTreeMap.contains_key
      alloc.alloc.Global.Insts.CoreAllocAllocatorClone (core.borrow.Borrow.Blanket Key)
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd entries key =
      .ok (lookup key entries).isSome := by
  simp only [alloc.collections.btree.map.BTreeMap.contains_key]
  rw [borrow_function, map_get_exact]
  simp only [bind_tc_eq, bind_ok]

private def nativeSize (size : Nat) (bound : size ≤ Usize.max) : Usize :=
  Usize.ofNatCore size (by scalar_tac)

private theorem native_length {K V : Type} (entries : List (K × V))
    (bound : entries.length ≤ Usize.max) :
    alloc.collections.btree.map.BTreeMap.len alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      entries = .ok (nativeSize entries.length bound) := by
  simp [alloc.collections.btree.map.BTreeMap.len, bound, nativeSize]

/-- Minimum is the first row only on an independently justified sorted map.
The exact primitive equation itself holds for every finite list. -/
theorem pop_first_exact {V : Type} (entries : List (Key × V)) :
    alloc.collections.btree.map.BTreeMap.pop_first alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd entries =
      .ok (entries.head?, entries.tail) := by
  cases entries <;> rfl

/-- The real closure's Vec.ne Result and denied comparison are preserved exactly.
Arbitrary PartialEq.ne is not replaced by structural inequality or !eq. -/
theorem changed_option_exact {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (previous : Option (mcp.discovery.CachedItems T)) (values : alloc.vec.Vec T)
    (fetch : Fetch) :
    core.option.Option.is_none_or
      (mcp.discovery.SurfaceState.complete.closure.Insts.CoreOpsFunctionFnOnceTupleShared2CachedItemsBool
        clone eq clock) previous (values, fetch) = changed eq previous values fetch.denied := by
  cases previous <;> rfl

/-- At capacity an absent key loses precisely the named head. Other entire
values are framed; no claim about maintaining native order is assumed here. -/
theorem room_frames {V : Type} (entries : List (Key × V)) (inserted other : Key)
    (safe : victim inserted entries ≠ some other) :
    lookup other (room inserted entries) = lookup other entries := by
  by_cases needsRoom : entries.length ≥ 4096 ∧ lookup inserted entries = none
  · simp only [victim, if_pos needsRoom] at safe
    simp only [room, if_pos needsRoom]
    cases entries with
    | nil => rfl
    | cons head rest =>
      rcases head with ⟨key, value⟩
      have different : key ≠ other := by simpa using safe
      simp [lookup, different]
  · simp [room, needsRoom]

/-- Complete equation on all physically represented branch inputs. Arbitrary
clock/element-comparison/addition fail/div Results remain in their native order.
Stale bypass needs no length, clock or element-comparison success premise. -/
theorem complete_exact {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N)
    (physical : CacheCompleteSpecification.Physical state fetch items) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch items now =
      completeOutcome eq state fetch items (clock.call_once now ()) := by
  unfold mcp.discovery.SurfaceState.complete completeOutcome
  rw [native_get]
  simp only [bind_tc_eq, bind_ok]
  rw [ticket_guard_exact]
  simp only [bind_tc_eq, bind_ok]
  by_cases stale : lookup fetch.key state.in_flight ≠ some fetch.request
  · simp [stale]
  · have bound := physical.resolve_left stale
    cases items with
    | none =>
      by_cases capacity : state.failed.length ≥ 4096 <;>
        cases previous : lookup fetch.key state.failed <;>
        simp [stale, capacity, previous, native_remove, native_get, native_contains,
        native_length state.failed bound, nativeSize, pop_first_exact,
        key_clone_exact, alloc.collections.btree.map.BTreeMap.insert, map_insert_exact,
        ttl_delay_exact, mcp.discovery.MAX_CACHE_ENTRIES_PER_SURFACE,
        room, bind_tc_eq, bind_ok]
      all_goals rfl
    | some values =>
      by_cases capacity : state.entries.length ≥ 4096 <;>
        cases previous : lookup fetch.key state.entries <;>
        simp [stale, capacity, previous, native_remove, native_get, native_contains,
        native_length state.entries bound, nativeSize, pop_first_exact,
        changed_option_exact, key_clone_exact,
        alloc.collections.btree.map.BTreeMap.insert, map_insert_exact,
        mcp.discovery.MAX_CACHE_ENTRIES_PER_SURFACE, room, changed, bind_tc_eq, bind_ok]
      all_goals rfl

/-- A missing or wrong ticket is an exact whole-state no-op for successful or
failed payloads, even if callback/comparison Results would fail or diverge. -/
theorem stale_complete {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N)
    (stale : lookup fetch.key state.in_flight ≠ some fetch.request) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch items now =
      .ok (false, state) := by
  rw [complete_exact clone eq clock state fetch items now (Or.inl stale)]
  simp [completeOutcome, stale]

/-- Successful discovery moves the complete vector and denied count and clears
the ticket/retry. The comparison Result is retained, not assumed normal. Clock
and native time addition are not bound in this success branch. -/
theorem successful_complete {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (values : alloc.vec.Vec T) (now : N)
    (current : lookup fetch.key state.in_flight = some fetch.request)
    (bound : state.entries.length ≤ Usize.max) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch (some values) now =
      (do let different ← changed eq (lookup fetch.key state.entries) values fetch.denied
          .ok (different, { state with
            entries := put fetch.key ({ items := values, denied := fetch.denied } : mcp.discovery.CachedItems T)
              (room fetch.key state.entries)
            in_flight := erase fetch.key state.in_flight
            failed := erase fetch.key state.failed })) := by
  rw [complete_exact clone eq clock state fetch (some values) now (Or.inr bound)]
  simp [completeOutcome, current]

/-- A new cached entry accepts without evaluating any element comparison or
clock Result. Its complete payload and denied count survive insertion. -/
theorem new_payload_accepted {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (values : alloc.vec.Vec T) (now : N)
    (current : lookup fetch.key state.in_flight = some fetch.request)
    (bound : state.entries.length ≤ Usize.max) (absent : lookup fetch.key state.entries = none) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch (some values) now =
      .ok (true, { state with
        entries := put fetch.key ({ items := values, denied := fetch.denied } : mcp.discovery.CachedItems T)
          (room fetch.key state.entries)
        in_flight := erase fetch.key state.in_flight
        failed := erase fetch.key state.failed }) ∧
      lookup fetch.key (put fetch.key ({ items := values, denied := fetch.denied } : mcp.discovery.CachedItems T)
        (room fetch.key state.entries)) = some { items := values, denied := fetch.denied } := by
  refine ⟨?_, put_reads_inserted _ _ _⟩
  rw [successful_complete clone eq clock state fetch values now current bound]
  simp [changed, absent, bind_tc_eq, bind_ok]

/-- Discovery failure preserves cached entries, consumes the current ticket and
writes the supplied normal-return retry deadline. Native addition is an explicit
observation, not a total/nonoverflow or future-deadline axiom. -/
theorem failed_complete {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (now : N) (sampled deadline : Instant)
    (current : lookup fetch.key state.in_flight = some fetch.request)
    (bound : state.failed.length ≤ Usize.max)
    (observed : clock.call_once now () = .ok sampled)
    (added : tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add sampled retryDelay =
      .ok deadline) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch none now =
      .ok (false, { state with
        in_flight := erase fetch.key state.in_flight
        failed := put fetch.key deadline (room fetch.key state.failed) }) := by
  rw [complete_exact clone eq clock state fetch none now (Or.inr bound)]
  simp [completeOutcome, current, observed, added, bind_tc_eq, bind_ok]

/-- A false changed flag still consumes the matching ticket. The vector .ne
observation is explicit, including for custom/nonreflexive element equality. -/
theorem false_flag_consumes_ticket {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (values : alloc.vec.Vec T) (now : N)
    (current : lookup fetch.key state.in_flight = some fetch.request)
    (bound : state.entries.length ≤ Usize.max) (previous : mcp.discovery.CachedItems T)
    (found : lookup fetch.key state.entries = some previous)
    (comparison : alloc.vec.partial_eq.PartialEqVec.ne eq previous.items values = .ok false)
    (sameDenied : previous.denied = fetch.denied)
    (unique : (state.in_flight.map Prod.fst).Nodup) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch (some values) now =
      .ok (false, { state with
        entries := put fetch.key ({ items := values, denied := fetch.denied } : mcp.discovery.CachedItems T)
          (room fetch.key state.entries)
        in_flight := erase fetch.key state.in_flight
        failed := erase fetch.key state.failed }) ∧
      lookup fetch.key (erase fetch.key state.in_flight) = none := by
  refine ⟨?_, erase_removed _ _ unique⟩
  rw [successful_complete clone eq clock state fetch values now current bound]
  simp [changed, found, comparison, sameDenied, bind_tc_eq, bind_ok]

/-- Unrelated successful-completion values are preserved, except the explicitly
named entries-map eviction victim. Generation preservation is part of complete_exact. -/
theorem success_frames {T : Type} (state : State T) (fetch : Fetch)
    (values : alloc.vec.Vec T) (other : Key) (different : other ≠ fetch.key)
    (notVictim : victim fetch.key state.entries ≠ some other) :
    lookup other (put fetch.key ({ items := values, denied := fetch.denied } : mcp.discovery.CachedItems T)
      (room fetch.key state.entries)) = lookup other state.entries ∧
    lookup other (erase fetch.key state.in_flight) = lookup other state.in_flight ∧
    lookup other (erase fetch.key state.failed) = lookup other state.failed := by
  refine ⟨?_, erase_other _ _ _ different, erase_other _ _ _ different⟩
  rw [put_frames _ _ _ _ different, room_frames _ _ _ notVictim]

/-- Actual supported key invalidation, then an arbitrarily late completion:
whole-state result is exactly the invalidated state, for either payload outcome.
Only native-map key uniqueness is required to rule out duplicate list records. -/
theorem discard_key_then_stale_complete {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N)
    (unique : (state.in_flight.map Prod.fst).Nodup) :
    (do let invalidated ← mcp.discovery.SurfaceState.discard_key clone eq state fetch.key
        mcp.discovery.SurfaceState.complete clone eq clock invalidated fetch items now) =
      .ok (false, { state with
        entries := erase fetch.key state.entries
        in_flight := erase fetch.key state.in_flight
        failed := erase fetch.key state.failed }) := by
  rw [discard_key_exact]
  simp only [bind_tc_eq, bind_ok]
  apply stale_complete
  have absent := erase_removed state.in_flight fetch.key unique
  simpa [absent]

end
end CacheCompleteProof

#print axioms CacheCompleteProof.ticket_guard_exact
#print axioms CacheCompleteProof.ttl_delay_exact
#print axioms CacheCompleteProof.complete_exact
#print axioms CacheCompleteProof.stale_complete
#print axioms CacheCompleteProof.successful_complete
#print axioms CacheCompleteProof.new_payload_accepted
#print axioms CacheCompleteProof.failed_complete
#print axioms CacheCompleteProof.false_flag_consumes_ticket
#print axioms CacheCompleteProof.discard_key_then_stale_complete

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheCompleteProof.ticket_guard_exact, ``CacheCompleteProof.ttl_delay_exact,
      ``CacheCompleteProof.pop_first_exact, ``CacheCompleteProof.changed_option_exact,
      ``CacheCompleteProof.room_frames, ``CacheCompleteProof.success_frames] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected cache-completion primitive/frame axiom in {declaration}: {dependencies}"
  let native := standard.push ``HubDiscoveryCacheModel.tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add
  for declaration in #[``CacheCompleteProof.complete_exact, ``CacheCompleteProof.stale_complete,
      ``CacheCompleteProof.successful_complete, ``CacheCompleteProof.new_payload_accepted,
      ``CacheCompleteProof.failed_complete, ``CacheCompleteProof.false_flag_consumes_ticket,
      ``CacheCompleteProof.discard_key_then_stale_complete] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => native.contains dependency) do
      throwError "unexpected actual-cache-completion axiom in {declaration}: {dependencies}"
