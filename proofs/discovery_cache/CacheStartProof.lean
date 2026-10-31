import CacheStartSpecification

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof CacheStartSpecification

namespace CacheStartProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

private theorem exact_of_total {T : Type} (operation : Result T) (value : T)
    (total : operation ⦃ result => result = value ⦄) : operation = .ok value := by
  cases operation using Result.cases with
  | ret returned => exact congrArg Result.ok ((WP.spec_ok returned).mp total)
  | vis effect continuation => exact False.elim ((WP.spec_vis effect continuation).mp total)
  | div => exact False.elim (WP.spec_div.mp total)

/-- The generated key clone preserves all five fields, including all 32 hash
bytes. No assumed own clone outcome or whole-key primitive is substituted. -/
theorem key_clone_exact (key : Key) :
    mcp.discovery.DiscoveryCacheKey.Insts.CoreCloneClone.clone key = .ok key := by
  have arrayTotal := core.array.CloneArray.clone_spec core.clone.CloneU8
    key.authorization_fingerprint (by intro value member; rfl)
  have arrayExact := exact_of_total _ _
    (WP.spec_mono arrayTotal (by intro output same; exact same.symm))
  have generation : lift (core.clone.impls.CloneU64.clone key.catalog_generation) =
      (Result.ok key.catalog_generation : Result U64) := by rfl
  have optional : core.option.Option.Insts.CoreCloneClone.clone
      veoveo_hub_sdk.hub.tool_name.LinkName.Insts.CoreCloneClone key.link = .ok key.link := by
    cases key.link <;>
      simp only [core.option.Option.Insts.CoreCloneClone.clone,
        veoveo_hub_sdk.hub.tool_name.LinkName.Insts.CoreCloneClone,
        veoveo_hub_sdk.hub.tool_name.LinkName.Insts.CoreCloneClone.clone,
        alloc.string.String.Insts.CoreCloneClone.clone, bind_tc_eq, bind_ok]
  unfold mcp.discovery.DiscoveryCacheKey.Insts.CoreCloneClone.clone
  rw [generation]
  simp only [veoveo_types.identity.PrincipalId.Insts.CoreCloneClone.clone,
    veoveo_types.platform_names.ServerSlug.Insts.CoreCloneClone.clone,
    alloc.string.String.Insts.CoreCloneClone.clone, bind_tc_eq, bind_ok]
  rw [arrayExact]
  simp only [bind_tc_eq, bind_ok]
  rw [optional]
  simp only [bind_tc_eq, bind_ok]

/-- Exact whole-map update, including the prior insertion result. The prior value
is oldAtInsertion, not assumed equal to lookup on an unordered list. -/
theorem map_insert_exact {V : Type} (entries : List (Key × V)) (key : Key) (value : V) :
    NativeMap.insertBy keyCmp entries key value =
      .ok (oldAtInsertion key entries, put key value entries) := by
  induction entries with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    rw [NativeMap.insertBy, key_comparison_exact]
    simp only [bind_tc_eq, bind_ok]
    cases compared : keyOrder key oldKey <;>
      simp [put, oldAtInsertion, compared, recursive, bind_tc_eq, bind_ok]
    all_goals rfl

/-- Accepted insertion always makes this complete key return the supplied ticket.
This read property does not require sortedness or uniqueness of the input list. -/
theorem put_reads_inserted {V : Type} (entries : List (Key × V)) (key : Key) (value : V) :
    lookup key (put key value entries) = some value := by
  induction entries with
  | nil => simp [put, lookup]
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    cases compared : keyOrder key oldKey with
    | lt => simp [put, compared, lookup]
    | eq =>
      have same : key = oldKey := (keyOrder_eq key oldKey).mp compared
      subst oldKey
      simp [put, compared, lookup]
    | gt =>
      have different : oldKey ≠ key := by
        intro same
        have equal := (keyOrder_eq key oldKey).mpr same.symm
        rw [equal] at compared
        contradiction
      simp [put, compared, lookup, different, recursive]

/-- Insertion preserves unrelated whole values, not merely membership/counts. -/
theorem put_frames {V : Type} (entries : List (Key × V)) (key other : Key) (value : V)
    (different : other ≠ key) :
    lookup other (put key value entries) = lookup other entries := by
  induction entries with
  | nil => simp [put, lookup, Ne.symm different]
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    cases compared : keyOrder key oldKey with
    | lt => simp [put, compared, lookup, Ne.symm different]
    | eq =>
      have same : key = oldKey := (keyOrder_eq key oldKey).mp compared
      subst oldKey
      simp [put, compared, lookup, Ne.symm different]
    | gt => simp [put, compared, lookup, recursive]

theorem put_size_bound {V : Type} (entries : List (Key × V)) (key : Key) (value : V) :
    (put key value entries).length ≤ entries.length + 1 := by
  induction entries with
  | nil => simp [put]
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    cases compared : keyOrder key oldKey <;> simp [put, compared] <;> omega

/-- If inserting a previously absent key, key-order insertion cannot replace
anything even on an unordered list. -/
theorem put_absent_size {V : Type} (entries : List (Key × V)) (key : Key) (value : V)
    (absent : key ∉ entries.map Prod.fst) :
    (put key value entries).length = entries.length + 1 := by
  induction entries with
  | nil => simp [put]
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    simp only [List.map_cons, List.mem_cons, not_or] at absent
    cases compared : keyOrder key oldKey with
    | lt => simp [put, compared]
    | eq => exact False.elim (absent.1 ((keyOrder_eq key oldKey).mp compared))
    | gt => simp [put, compared, recursive absent.2]

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

private theorem retry_predicate_exact {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (now : N) (retry : Instant) :
    mcp.discovery.SurfaceState.start.closure.Insts.CoreOpsFunctionFnOnceTupleShared0InstantBool.call_once
      clone eq clock fresh now retry =
      (do let sampled ← clock.call_once now ()
          .ok (decide (retry.nanoseconds > sampled.nanoseconds))) := by
  unfold mcp.discovery.SurfaceState.start.closure.Insts.CoreOpsFunctionFnOnceTupleShared0InstantBool.call_once
  apply congrArg (fun continuation => clock.call_once now () >>= continuation)
  funext sampled
  simp [core.cmp.PartialOrd.gt.trait_default, core.cmp.PartialOrd.gt.default,
    core.cmp.PartialOrd.gt_body,
    tokio.time.instant.Instant.Insts.CoreCmpPartialOrdInstant,
    tokio.time.instant.Instant.Insts.CoreCmpPartialOrdInstant.partial_cmp,
    bind_ok, _root_.compare_gt_iff_gt]

private theorem retry_option_exact {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (now : N) (retry : Option Instant) :
    core.option.Option.is_some_and
      (mcp.discovery.SurfaceState.start.closure.Insts.CoreOpsFunctionFnOnceTupleShared0InstantBool
        clone eq clock fresh) retry now =
      (match retry with
       | none => .ok false
       | some deadline => do
           let sampled ← clock.call_once now ()
           .ok (decide (deadline.nanoseconds > sampled.nanoseconds))) := by
  cases retry with
  | none => rfl
  | some deadline =>
    exact retry_predicate_exact clone eq clock fresh now deadline

/-- Complete actual start equation. Callback Results remain arbitrary (including
failure/divergence); no normal-return or fresh-ticket axiom is required here.
Physical is an input-representation bound, never an assumed own result. -/
theorem start_exact {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (coalesce : Bool) (now : N) (ticket : R) (physical : Physical state key) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key coalesce now ticket =
      startOutcome state key coalesce (clock.call_once now ()) (fresh.call_once ticket ()) := by
  have emptyBound : 0 ≤ Usize.max := by scalar_tac
  unfold mcp.discovery.SurfaceState.start startOutcome
  by_cases older : key.catalog_generation.val < state.generation.val
  · simp [UScalar.lt_equiv, older]
  · by_cases newer : key.catalog_generation.val > state.generation.val
    · cases coalesce <;>
        simp [UScalar.lt_equiv, UScalar.le_equiv, older, newer, prepared, cooling,
          alloc.collections.btree.map.BTreeMap.clear, native_get, native_contains,
          native_length ([] : List (Key × Ticket)) emptyBound, nativeSize, retry_option_exact,
          key_clone_exact, alloc.collections.btree.map.BTreeMap.insert, map_insert_exact,
          put, oldAtInsertion, lookup, mcp.discovery.MAX_CACHE_ENTRIES_PER_SURFACE,
          bind_tc_eq, bind_ok]
    · have bound : state.in_flight.length ≤ Usize.max := physical.resolve_left newer
      cases coalesce <;>
        simp [UScalar.lt_equiv, UScalar.le_equiv, older, newer, prepared, cooling,
          alloc.collections.btree.map.BTreeMap.clear, native_get, native_contains,
          native_length state.in_flight bound, nativeSize, retry_option_exact,
          key_clone_exact, alloc.collections.btree.map.BTreeMap.insert, map_insert_exact,
          mcp.discovery.MAX_CACHE_ENTRIES_PER_SURFACE, bind_tc_eq, bind_ok]
      all_goals rfl

/-- Old-generation rejection depends on no callback result and changes nothing.
This is result independence, not an instrumented native invocation-count claim. -/
theorem old_generation {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (coalesce : Bool) (now : N) (ticket : R)
    (older : key.catalog_generation.val < state.generation.val) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key coalesce now ticket =
      .ok (none, state) := by
  simp [mcp.discovery.SurfaceState.start, UScalar.lt_equiv, older]

/-- Higher generation resets all maps; normal acceptance depends only on this
operation's ticket Result. No post-failure native state is asserted. -/
theorem newer_generation {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (coalesce : Bool) (now : N) (ticket : R)
    (newer : key.catalog_generation.val > state.generation.val) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key coalesce now ticket =
      (do let request ← fresh.call_once ticket ()
          .ok (some { key, request, denied := 0#u32 },
            { generation := key.catalog_generation
              entries := []
              in_flight := [(key, request)]
              failed := [] })) := by
  rw [start_exact clone eq clock fresh state key coalesce now ticket (Or.inl newer)]
  have notOlder : ¬key.catalog_generation.val < state.generation.val := by omega
  cases coalesce <;> simp [startOutcome, prepared, newer, notOlder, cooling, lookup, put]

/-- Even a non-coalesced replacement of an existing key is refused at capacity;
clock/fresh Results are irrelevant and all state is preserved. -/
theorem uncoalesced_capacity {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (now : N) (ticket : R) (same : key.catalog_generation = state.generation)
    (bound : state.in_flight.length ≤ Usize.max) (capacity : state.in_flight.length ≥ 4096) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key false now ticket =
      .ok (none, state) := by
  rw [start_exact clone eq clock fresh state key false now ticket (Or.inr bound)]
  simp [startOutcome, prepared, cooling, same, capacity]

/-- Coalesced in-flight work refuses after any cooldown check. No ticket is
sampled; a clock result can be needed first if a retry record also exists. -/
theorem pending_refused {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (now : N) (ticket : R) (same : key.catalog_generation = state.generation)
    (bound : state.in_flight.length ≤ Usize.max) (pending : (lookup key state.in_flight).isSome = true) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key true now ticket =
      (do let blocked ← cooling state key true (clock.call_once now ())
          .ok (none, state)) := by
  rw [start_exact clone eq clock fresh state key true now ticket (Or.inr bound)]
  simp [startOutcome, prepared, same, pending]

/-- Independent raw retry readiness: coalescing is off, no retry is stored,
or the sampled clock reaches the stored deadline. No own result is assumed. -/
def RetryReady {T : Type} (state : State T) (key : Key) (coalesce : Bool)
    (clock : Result Instant) : Prop :=
  coalesce = false ∨ lookup key state.failed = none ∨
    ∃ retry sampled, lookup key state.failed = some retry ∧ clock = .ok sampled ∧
      retry.nanoseconds ≤ sampled.nanoseconds

theorem cooling_ready {T : Type} (state : State T) (key : Key) (coalesce : Bool)
    (clock : Result Instant) (ready : RetryReady state key coalesce clock) :
    cooling state key coalesce clock = .ok false := by
  rcases ready with off | absent | ⟨retry, sampled, found, observed, reached⟩
  · simp [cooling, off]
  · cases coalesce <;> simp [cooling, absent]
  · cases coalesce <;> simp [cooling, found, observed, not_lt_of_ge reached, bind_tc_eq, bind_ok]

/-- A retry deadline in the future refuses before pending/capacity handling.
No condition on the ticket callback, existing work or cache payloads is required. -/
theorem cooldown_refused {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (now : N) (ticket : R) (same : key.catalog_generation = state.generation)
    (bound : state.in_flight.length ≤ Usize.max) (retry sampled : Instant)
    (found : lookup key state.failed = some retry)
    (observed : clock.call_once now () = .ok sampled)
    (future : retry.nanoseconds > sampled.nanoseconds) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key true now ticket =
      .ok (none, state) := by
  rw [start_exact clone eq clock fresh state key true now ticket (Or.inr bound)]
  simp [startOutcome, prepared, same, cooling, found, observed, future, bind_tc_eq, bind_ok]

/-- Source-ready acceptance retains the exact lazy ticket bind. This equation
therefore propagates ticket failure/divergence and exposes the complete success
payload and map update, rather than assuming constructor success. -/
theorem ready_start {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (coalesce : Bool) (now : N) (ticket : R)
    (same : key.catalog_generation = state.generation)
    (bound : state.in_flight.length ≤ Usize.max)
    (retryReady : RetryReady state key coalesce (clock.call_once now ()))
    (notPending : (coalesce && (lookup key state.in_flight).isSome) = false)
    (capacity : state.in_flight.length < 4096) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key coalesce now ticket =
      (do let request ← fresh.call_once ticket ()
          .ok (some { key, request, denied := 0#u32 },
            { state with in_flight := put key request state.in_flight })) := by
  rw [start_exact clone eq clock fresh state key coalesce now ticket (Or.inr bound)]
  have cooled := cooling_ready state key coalesce (clock.call_once now ()) retryReady
  simp [startOutcome, prepared, same, cooled, notPending, Nat.not_le.mpr capacity]

/-- Given only source-ready facts and a typed ticket observation, actual start
returns the complete denied-zero fetch and exact full updated state. -/
theorem ready_accepted {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (coalesce : Bool) (now : N) (ticket : R)
    (same : key.catalog_generation = state.generation)
    (bound : state.in_flight.length ≤ Usize.max)
    (retryReady : RetryReady state key coalesce (clock.call_once now ()))
    (notPending : (coalesce && (lookup key state.in_flight).isSome) = false)
    (capacity : state.in_flight.length < 4096) (request : Ticket)
    (observed : fresh.call_once ticket () = .ok request) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key coalesce now ticket =
      .ok (some { key, request, denied := 0#u32 },
        { state with in_flight := put key request state.in_flight }) ∧
      lookup key (put key request state.in_flight) = some request ∧
      (put key request state.in_flight).length ≤ 4096 ∧
      ∀ other, other ≠ key →
        lookup other (put key request state.in_flight) = lookup other state.in_flight := by
  refine ⟨?_, put_reads_inserted _ _ _, ?_, fun other different => put_frames _ _ _ _ different⟩
  · rw [ready_start clone eq clock fresh state key coalesce now ticket same bound
      retryReady notPending capacity, observed]
    simp only [bind_tc_eq, bind_ok]
  · have size := put_size_bound state.in_flight key request
    omega

/-- Non-coalesced acceptance needs no clock observation and no condition on
stored retry/pending records. This specialization guards the complete Bool
notPending condition: falsely requiring coalesce=true would reject this proof. -/
theorem uncoalesced_accepted {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (now : N) (ticket : R) (same : key.catalog_generation = state.generation)
    (bound : state.in_flight.length ≤ Usize.max) (capacity : state.in_flight.length < 4096)
    (request : Ticket) (observed : fresh.call_once ticket () = .ok request) :
    mcp.discovery.SurfaceState.start clone eq clock fresh state key false now ticket =
      .ok (some { key, request, denied := 0#u32 },
        { state with in_flight := put key request state.in_flight }) ∧
      lookup key (put key request state.in_flight) = some request ∧
      (put key request state.in_flight).length ≤ 4096 ∧
      ∀ other, other ≠ key →
        lookup other (put key request state.in_flight) = lookup other state.in_flight := by
  exact ready_accepted clone eq clock fresh state key false now ticket same bound
    (Or.inl rfl) (by rfl) capacity request observed

end
end CacheStartProof

#print axioms CacheStartProof.key_clone_exact
#print axioms CacheStartProof.map_insert_exact
#print axioms CacheStartProof.start_exact
#print axioms CacheStartProof.old_generation
#print axioms CacheStartProof.newer_generation
#print axioms CacheStartProof.uncoalesced_capacity
#print axioms CacheStartProof.pending_refused
#print axioms CacheStartProof.ready_start
#print axioms CacheStartProof.cooldown_refused
#print axioms CacheStartProof.ready_accepted
#print axioms CacheStartProof.uncoalesced_accepted

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheStartProof.key_clone_exact, ``CacheStartProof.map_insert_exact,
      ``CacheStartProof.put_reads_inserted, ``CacheStartProof.put_frames,
      ``CacheStartProof.put_size_bound, ``CacheStartProof.put_absent_size,
      ``CacheStartProof.start_exact, ``CacheStartProof.old_generation,
      ``CacheStartProof.newer_generation, ``CacheStartProof.uncoalesced_capacity,
      ``CacheStartProof.pending_refused, ``CacheStartProof.cooling_ready,
      ``CacheStartProof.cooldown_refused, ``CacheStartProof.ready_start,
      ``CacheStartProof.ready_accepted, ``CacheStartProof.uncoalesced_accepted] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected cache-start axiom in {declaration}: {dependencies}"
