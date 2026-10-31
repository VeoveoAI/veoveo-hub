import CacheMapInvariantProof

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof CacheStartSpecification CacheStartProof
open CacheCompleteSpecification CacheCompleteProof CacheMapInvariantProof

namespace CacheStateInvariantProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- All complete map values are retained. Uniqueness follows from strict order;
pending and retry maps are deliberately permitted to overlap. -/
def MapOk {V : Type} (generation : U64) (rows : List (Key × V)) : Prop :=
  Ordered rows ∧ rows.length ≤ 4096 ∧
    KeysSatisfy (fun key => key.catalog_generation = generation) rows

def StateOk {T : Type} (state : State T) : Prop :=
  MapOk state.generation state.entries ∧ MapOk state.generation state.in_flight ∧
    MapOk state.generation state.failed

/-- Structural empty representation; native construction/locking refinement is
not asserted by this base case. -/
def empty (T : Type) (generation : U64) : State T :=
  { generation := generation, entries := [], in_flight := [], failed := [] }

theorem empty_ok (T : Type) (generation : U64) : StateOk (empty T generation) := by
  simp [StateOk, MapOk, empty, Ordered, KeysSatisfy]

theorem map_unique {V : Type} {generation : U64} {rows : List (Key × V)}
    (valid : MapOk generation rows) : Unique rows := ordered_unique rows valid.1

theorem map_physical {V : Type} {generation : U64} {rows : List (Key × V)}
    (valid : MapOk generation rows) : rows.length ≤ Usize.max := by
  have architecture := Usize.cMax_bound_concrete.1
  have capacity := valid.2.1
  omega

theorem lookup_member {V : Type} (key : Key) (rows : List (Key × V)) (value : V)
    (found : lookup key rows = some value) : (key, value) ∈ rows := by
  induction rows with
  | nil => simp [lookup] at found
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    by_cases same : oldKey = key
    · subst oldKey
      have values : oldValue = value := by simpa [lookup] using found
      subst oldValue
      exact List.mem_cons_self
    · have inRest := recursive (by simpa [lookup, same] using found)
      exact List.mem_cons_of_mem _ inRest

theorem lookup_generation {V : Type} {generation : U64} {rows : List (Key × V)}
    (valid : MapOk generation rows) (key : Key) (value : V)
    (found : lookup key rows = some value) : key.catalog_generation = generation :=
  valid.2.2 (key, value) (lookup_member key rows value found)

theorem erase_ok {V : Type} {generation : U64} {rows : List (Key × V)}
    (valid : MapOk generation rows) (key : Key) : MapOk generation (erase key rows) := by
  refine ⟨erase_ordered key rows valid.1, ?_, erase_keys _ key rows valid.2.2⟩
  exact (erase_size_le key rows).trans valid.2.1

theorem insert_below_ok {V : Type} {generation : U64} {rows : List (Key × V)}
    (valid : MapOk generation rows) (key : Key) (value : V)
    (sameGeneration : key.catalog_generation = generation) (space : rows.length < 4096) :
    MapOk generation (put key value rows) := by
  refine ⟨put_ordered key value rows valid.1, ?_, put_keys _ key value rows valid.2.2 sameGeneration⟩
  have growth := put_size_bound rows key value
  omega

theorem insert_room_ok {V : Type} {generation : U64} {rows : List (Key × V)}
    (valid : MapOk generation rows) (key : Key) (value : V)
    (sameGeneration : key.catalog_generation = generation) :
    MapOk generation (put key value (room key rows)) := by
  exact ⟨put_ordered key value _ (room_ordered key rows valid.1),
    put_room_capacity key value rows valid.1 valid.2.1,
    put_keys _ key value _ (room_keys _ key rows valid.2.2) sameGeneration⟩

/-- Successful bind inversion retains arbitrary visible failure/divergence in
inputs; no successful callback or desired transition is assumed. -/
private theorem bind_normal {A B : Type} {operation : Result A}
    {continuation : A → Result B} {value : B}
    (normal : Aeneas.Std.bind operation continuation = .ok value) :
    ∃ observed, operation = .ok observed ∧ continuation observed = .ok value := by
  cases operation using Result.cases with
  | ret observed => exact ⟨observed, rfl, by simpa only [bind_ok] using normal⟩
  | vis effect next =>
    rw [bind_vis] at normal
    exact False.elim (vis_not_ok normal)
  | div =>
    rw [bind_div] at normal
    exact False.elim (div_not_ok normal)

private theorem returned_state {A T : Type} {left right : A} {expected next : State T}
    (normal : Result.ok (left, expected) = Result.ok (right, next)) : expected = next :=
  congrArg Prod.snd (Result.ok_injective normal)

theorem prepared_ok {T : Type} (state : State T) (key : Key) (valid : StateOk state) :
    StateOk (prepared state key) ∧ state.generation.val ≤ (prepared state key).generation.val := by
  unfold prepared
  split
  · rename_i newer
    refine ⟨empty_ok T key.catalog_generation, ?_⟩
    change state.generation.val ≤ key.catalog_generation.val
    omega
  · exact ⟨valid, Nat.le_refl _⟩

theorem prepared_generation {T : Type} (state : State T) (key : Key)
    (notOlder : ¬ key.catalog_generation.val < state.generation.val) :
    key.catalog_generation = (prepared state key).generation := by
  unfold prepared
  split
  · rfl
  · rename_i notNewer
    apply UScalar.eq_imp
    omega

/-- Full state closure for the independently checked exact start outcome.
The only observation premise is that the complete operation returned normally. -/
theorem start_outcome_ok {T : Type} (state : State T) (key : Key) (coalesce : Bool)
    (clock : Result Instant) (fresh : Result Ticket) (result : Option Fetch) (next : State T)
    (valid : StateOk state)
    (normal : startOutcome state key coalesce clock fresh = .ok (result, next)) :
    StateOk next ∧ state.generation.val ≤ next.generation.val := by
  unfold startOutcome at normal
  split at normal
  · have same := returned_state normal
    subst next
    exact ⟨valid, Nat.le_refl _⟩
  · rename_i notOlder
    have preparedValid := prepared_ok state key valid
    have sameGeneration := prepared_generation state key notOlder
    simp only [bind_tc_eq] at normal
    obtain ⟨blocked, _, resumed⟩ := bind_normal normal
    cases blocked with
    | true =>
      simp only [↓reduceIte] at resumed
      have same := returned_state resumed
      subst next
      exact preparedValid
    | false =>
      simp only [Bool.false_eq_true, ↓reduceIte] at resumed
      split at resumed
      · have same := returned_state resumed
        subst next
        exact preparedValid
      · split at resumed
        · have same := returned_state resumed
          subst next
          exact preparedValid
        · rename_i notPending notFull
          obtain ⟨ticket, _, inserted⟩ := bind_normal resumed
          have same := returned_state inserted
          subst next
          refine ⟨⟨preparedValid.1.1, ?_, preparedValid.1.2.2⟩, preparedValid.2⟩
          exact insert_below_ok preparedValid.1.2.1 key ticket sameGeneration (by omega)

/-- Actual extracted start; finite tickets are unrestricted and clock/fresh
callbacks may fail or diverge outside this normal-return relation. -/
theorem start_preserves {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (state : State T) (key : Key)
    (coalesce : Bool) (now : N) (ticket : R) (result : Option Fetch) (next : State T)
    (valid : StateOk state)
    (normal : mcp.discovery.SurfaceState.start clone eq clock fresh state key coalesce now ticket =
      .ok (result, next)) : StateOk next ∧ state.generation.val ≤ next.generation.val := by
  rw [start_exact clone eq clock fresh state key coalesce now ticket
    (Or.inr (map_physical valid.2.1))] at normal
  exact start_outcome_ok state key coalesce _ _ result next valid normal

/-- Complete preserves generation, all three capacities/orderings and each
stored key's generation. Generic Vec.ne and native addition Results are retained. -/
theorem complete_outcome_ok {T : Type} (eq : core.cmp.PartialEq T T)
    (state : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T))
    (clock : Result Instant) (result : Bool) (next : State T) (valid : StateOk state)
    (normal : completeOutcome eq state fetch items clock = .ok (result, next)) :
    StateOk next ∧ next.generation = state.generation := by
  unfold completeOutcome at normal
  split at normal
  · have same := returned_state normal
    subst next
    exact ⟨valid, rfl⟩
  · rename_i notStale
    have current : lookup fetch.key state.in_flight = some fetch.request := by
      exact Classical.not_not.mp notStale
    have sameGeneration := lookup_generation valid.2.1 fetch.key fetch.request current
    cases items with
    | none =>
      simp only [bind_tc_eq] at normal
      obtain ⟨sampled, _, observed⟩ := bind_normal normal
      obtain ⟨deadline, _, stored⟩ := bind_normal observed
      have same := returned_state stored
      subst next
      exact ⟨⟨valid.1, erase_ok valid.2.1 fetch.key,
        insert_room_ok valid.2.2 fetch.key deadline sameGeneration⟩, rfl⟩
    | some values =>
      simp only [bind_tc_eq] at normal
      obtain ⟨different, _, stored⟩ := bind_normal normal
      have same := returned_state stored
      subst next
      exact ⟨⟨insert_room_ok valid.1 fetch.key _ sameGeneration,
        erase_ok valid.2.1 fetch.key, erase_ok valid.2.2 fetch.key⟩, rfl⟩

theorem complete_preserves {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N)
    (result : Bool) (next : State T) (valid : StateOk state)
    (normal : mcp.discovery.SurfaceState.complete clone eq clock state fetch items now =
      .ok (result, next)) : StateOk next ∧ next.generation = state.generation := by
  have physical : CacheCompleteSpecification.Physical state fetch items := by
    apply Or.inr
    cases items with
    | none => exact map_physical valid.2.2
    | some _ => exact map_physical valid.1
  rw [complete_exact clone eq clock state fetch items now physical] at normal
  exact complete_outcome_ok eq state fetch items _ result next valid normal

theorem remove_cached_preserves {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (state : State T) (key : Key) (next : State T)
    (valid : StateOk state)
    (normal : mcp.discovery.SurfaceState.remove_cached clone eq state key = .ok next) :
    StateOk next ∧ next.generation = state.generation := by
  rw [remove_cached_exact] at normal
  have same := Result.ok_injective normal
  subst next
  exact ⟨⟨erase_ok valid.1 key, valid.2⟩, rfl⟩

theorem discard_key_preserves {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (state : State T) (key : Key) (next : State T)
    (valid : StateOk state)
    (normal : mcp.discovery.SurfaceState.discard_key clone eq state key = .ok next) :
    StateOk next ∧ next.generation = state.generation := by
  rw [discard_key_exact] at normal
  have same := Result.ok_injective normal
  subst next
  exact ⟨⟨erase_ok valid.1 key, erase_ok valid.2.1 key, erase_ok valid.2.2 key⟩, rfl⟩

/-- Generation fencing needs no ticket freshness premise: coherent stored keys
exclude every old-generation fetch, regardless of its payload, denied or clock. -/
theorem generation_stale_complete {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (state : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N)
    (valid : StateOk state) (older : fetch.key.catalog_generation.val < state.generation.val) :
    mcp.discovery.SurfaceState.complete clone eq clock state fetch items now = .ok (false, state) := by
  apply stale_complete
  intro current
  have same := lookup_generation valid.2.1 fetch.key fetch.request current
  rw [same] at older
  exact (Nat.lt_irrefl _ older)

end
end CacheStateInvariantProof

#print axioms CacheStateInvariantProof.empty_ok
#print axioms CacheStateInvariantProof.map_physical
#print axioms CacheStateInvariantProof.start_preserves
#print axioms CacheStateInvariantProof.complete_preserves
#print axioms CacheStateInvariantProof.remove_cached_preserves
#print axioms CacheStateInvariantProof.discard_key_preserves
#print axioms CacheStateInvariantProof.generation_stale_complete

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheStateInvariantProof.empty_ok, ``CacheStateInvariantProof.map_unique,
      ``CacheStateInvariantProof.map_physical, ``CacheStateInvariantProof.lookup_member,
      ``CacheStateInvariantProof.lookup_generation, ``CacheStateInvariantProof.erase_ok,
      ``CacheStateInvariantProof.insert_below_ok, ``CacheStateInvariantProof.insert_room_ok,
      ``CacheStateInvariantProof.prepared_ok, ``CacheStateInvariantProof.prepared_generation,
      ``CacheStateInvariantProof.start_outcome_ok, ``CacheStateInvariantProof.start_preserves,
      ``CacheStateInvariantProof.remove_cached_preserves, ``CacheStateInvariantProof.discard_key_preserves] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected cache state invariant axiom in {declaration}: {dependencies}"
  let native := standard.push ``HubDiscoveryCacheModel.tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add
  for declaration in #[``CacheStateInvariantProof.complete_outcome_ok,
      ``CacheStateInvariantProof.complete_preserves, ``CacheStateInvariantProof.generation_stale_complete] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => native.contains dependency) do
      throwError "unexpected actual-completion state axiom in {declaration}: {dependencies}"
