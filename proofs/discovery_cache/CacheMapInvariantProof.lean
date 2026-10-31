import CacheKeyOrderProof
import CacheCompleteProof

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof CacheStartSpecification CacheStartProof CacheCompleteSpecification
open CacheKeyOrderProof

namespace CacheMapInvariantProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Strict native-model key order, with complete values retained in each row. -/
def Ordered {V : Type} (rows : List (Key × V)) : Prop :=
  rows.Pairwise (fun left right => Less left.1 right.1)

def Unique {V : Type} (rows : List (Key × V)) : Prop := (rows.map Prod.fst).Nodup

def KeysSatisfy {V : Type} (predicate : Key → Prop) (rows : List (Key × V)) : Prop :=
  ∀ row ∈ rows, predicate row.1

theorem ordered_unique {V : Type} (rows : List (Key × V)) (ordered : Ordered rows) :
    Unique rows := by
  induction rows with
  | nil => simp [Unique]
  | cons head rest recursive =>
    rcases List.pairwise_cons.mp ordered with ⟨before, tailOrdered⟩
    simp only [Unique, List.map_cons, List.nodup_cons]
    refine ⟨?_, recursive tailOrdered⟩
    intro member
    rcases List.mem_map.mp member with ⟨other, inRest, same⟩
    have strict := before other inRest
    rw [same] at strict
    exact less_irrefl _ strict

theorem erase_members {V : Type} (key : Key) (rows : List (Key × V)) (row : Key × V)
    (member : row ∈ erase key rows) : row ∈ rows := by
  induction rows with
  | nil => simpa [erase] using member
  | cons head rest recursive =>
    by_cases same : head.1 = key
    · have inRest : row ∈ rest := by simpa [erase, same] using member
      exact List.mem_cons_of_mem head inRest
    · have located : row = head ∨ row ∈ erase key rest := by
        simpa [erase, same] using member
      rcases located with equal | inRest
      · exact List.mem_cons.mpr (Or.inl equal)
      · exact List.mem_cons_of_mem head (recursive inRest)

theorem erase_ordered {V : Type} (key : Key) (rows : List (Key × V))
    (ordered : Ordered rows) : Ordered (erase key rows) := by
  induction rows with
  | nil => simp [erase, Ordered]
  | cons head rest recursive =>
    rcases List.pairwise_cons.mp ordered with ⟨before, tailOrdered⟩
    by_cases same : head.1 = key
    · simpa [erase, same, Ordered] using tailOrdered
    · simp only [erase, if_neg same]
      exact List.pairwise_cons.mpr
        ⟨fun other member => before other (erase_members key rest other member),
          recursive tailOrdered⟩

theorem erase_size_le {V : Type} (key : Key) (rows : List (Key × V)) :
    (erase key rows).length ≤ rows.length := by
  induction rows with
  | nil => simp [erase]
  | cons head rest recursive =>
    by_cases same : head.1 = key <;> simp [erase, same] <;> omega

theorem put_members {V : Type} (key : Key) (value : V) (rows : List (Key × V)) (row : Key × V)
    (member : row ∈ put key value rows) : row = (key, value) ∨ row ∈ rows := by
  induction rows with
  | nil => simpa [put] using member
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    cases compared : keyOrder key oldKey with
    | lt =>
      simpa [put, compared, List.mem_cons] using member
    | eq =>
      have same : key = oldKey := (keyOrder_eq key oldKey).mp compared
      subst oldKey
      have located : row = (key, value) ∨ row ∈ rest := by simpa [put, compared] using member
      exact located.imp_right (List.mem_cons_of_mem (key, oldValue))
    | gt =>
      have located : row = (oldKey, oldValue) ∨ row ∈ put key value rest := by
        simpa [put, compared] using member
      rcases located with equal | inRest
      · exact Or.inr (List.mem_cons.mpr (Or.inl equal))
      · exact (recursive inRest).imp_right (List.mem_cons_of_mem (oldKey, oldValue))

theorem put_ordered {V : Type} (key : Key) (value : V) (rows : List (Key × V))
    (ordered : Ordered rows) : Ordered (put key value rows) := by
  induction rows with
  | nil => simp [put, Ordered]
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    rcases List.pairwise_cons.mp ordered with ⟨before, tailOrdered⟩
    cases compared : keyOrder key oldKey with
    | lt =>
      simp only [put, compared]
      apply List.pairwise_cons.mpr
      refine ⟨?_, ordered⟩
      intro other member
      rcases List.mem_cons.mp member with equal | inRest
      · subst other; exact compared
      · exact less_trans compared (before other inRest)
    | eq =>
      have same : key = oldKey := (keyOrder_eq key oldKey).mp compared
      subst oldKey
      simp only [put, compared]
      exact List.pairwise_cons.mpr ⟨before, tailOrdered⟩
    | gt =>
      have after : Less oldKey key := (greater_iff_reverse key oldKey).mp compared
      simp only [put, compared]
      apply List.pairwise_cons.mpr
      refine ⟨?_, recursive tailOrdered⟩
      intro other member
      rcases put_members key value rest other member with equal | inRest
      · subst other; exact after
      · exact before other inRest

/-- On ordered maps the actual old insertion return agrees with full lookup.
The sorted premise is necessary; no arbitrary-list correspondence is assumed. -/
theorem insertion_old_lookup {V : Type} (key : Key) (rows : List (Key × V))
    (ordered : Ordered rows) : oldAtInsertion key rows = lookup key rows := by
  induction rows with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    rcases List.pairwise_cons.mp ordered with ⟨before, tailOrdered⟩
    cases compared : keyOrder key oldKey with
    | lt =>
      have absent : key ∉ ((oldKey, oldValue) :: rest).map Prod.fst := by
        intro member
        rcases List.mem_map.mp member with ⟨other, inRows, same⟩
        rcases List.mem_cons.mp inRows with equal | inRest
        · subst other
          exact less_ne compared same.symm
        · have reverse := before other inRest
          rw [same] at reverse
          exact less_asymm compared reverse
      have missing := lookup_none_of_absent ((oldKey, oldValue) :: rest) key absent
      simpa only [oldAtInsertion, compared] using missing.symm
    | eq =>
      have same : key = oldKey := (keyOrder_eq key oldKey).mp compared
      subst oldKey
      simp [oldAtInsertion, compared, lookup]
    | gt =>
      have different : oldKey ≠ key := less_ne ((greater_iff_reverse key oldKey).mp compared)
      simp [oldAtInsertion, compared, lookup, different, recursive tailOrdered]

theorem put_length_at_insertion {V : Type} (key : Key) (value : V) (rows : List (Key × V)) :
    (put key value rows).length = rows.length +
      (if (oldAtInsertion key rows).isSome then 0 else 1) := by
  induction rows with
  | nil => simp [put, oldAtInsertion]
  | cons head rest recursive =>
    rcases head with ⟨oldKey, oldValue⟩
    cases compared : keyOrder key oldKey <;>
      simp [put, oldAtInsertion, compared, recursive] <;> omega

theorem put_length {V : Type} (key : Key) (value : V) (rows : List (Key × V))
    (ordered : Ordered rows) :
    (put key value rows).length = rows.length + (if (lookup key rows).isSome then 0 else 1) := by
  rw [put_length_at_insertion, insertion_old_lookup key rows ordered]

theorem put_existing_size {V : Type} (key : Key) (value old : V) (rows : List (Key × V))
    (ordered : Ordered rows) (found : lookup key rows = some old) :
    (put key value rows).length = rows.length := by
  rw [put_length key value rows ordered, found]
  simp

theorem tail_ordered {V : Type} (rows : List (Key × V)) (ordered : Ordered rows) :
    Ordered rows.tail := by
  cases rows with
  | nil => simp [Ordered]
  | cons head rest => exact (List.pairwise_cons.mp ordered).2

theorem room_ordered {V : Type} (key : Key) (rows : List (Key × V))
    (ordered : Ordered rows) : Ordered (room key rows) := by
  unfold room
  split
  · exact tail_ordered rows ordered
  · exact ordered

theorem room_length {V : Type} (key : Key) (rows : List (Key × V)) :
    (room key rows).length =
      if rows.length ≥ 4096 ∧ lookup key rows = none then rows.length - 1 else rows.length := by
  by_cases needsRoom : rows.length ≥ 4096 ∧ lookup key rows = none
  · simp only [room, if_pos needsRoom]
    cases rows <;> simp
  · simp [room, needsRoom]

/-- Native capacity eviction plus exact ordered replacement preserves the bound;
replacement at capacity does not spuriously add a duplicate key. -/
theorem put_room_capacity {V : Type} (key : Key) (value : V) (rows : List (Key × V))
    (ordered : Ordered rows) (bound : rows.length ≤ 4096) :
    (put key value (room key rows)).length ≤ 4096 := by
  by_cases capacity : rows.length ≥ 4096
  · have full : rows.length = 4096 := by omega
    cases found : lookup key rows with
    | none =>
      have size : (room key rows).length = 4095 := by
        rw [room_length]
        simp [capacity, found, full]
      have inserted := put_size_bound (room key rows) key value
      omega
    | some previous =>
      have unchanged : room key rows = rows := by simp [room, found]
      rw [unchanged, put_existing_size key value previous rows ordered found]
      exact bound
  · have unchanged : room key rows = rows := by simp [room, capacity]
    rw [unchanged]
    have inserted := put_size_bound rows key value
    omega

/-- The named eviction victim really is the first/minimum key under the actual
checked key comparator, conditional on the native ordered representation. -/
theorem victim_minimum {V : Type} (key : Key) (rows : List (Key × V))
    (ordered : Ordered rows) (selected : Key) (named : victim key rows = some selected) :
    (∃ value, (selected, value) ∈ rows) ∧
      ∀ other ∈ rows, other.1 = selected ∨ Less selected other.1 := by
  unfold victim at named
  split at named
  · cases rows with
    | nil => simp at named
    | cons head rest =>
      rcases head with ⟨headKey, headValue⟩
      have same : headKey = selected := by simpa using named
      subst headKey
      refine ⟨⟨headValue, List.mem_cons_self⟩, ?_⟩
      intro other member
      rcases List.mem_cons.mp member with equal | inRest
      · subst other; exact Or.inl rfl
      · exact Or.inr ((List.pairwise_cons.mp ordered).1 other inRest)
  · simp at named

theorem erase_keys {V : Type} (predicate : Key → Prop) (key : Key) (rows : List (Key × V))
    (allKeys : KeysSatisfy predicate rows) : KeysSatisfy predicate (erase key rows) := by
  intro row member
  exact allKeys row (erase_members key rows row member)

theorem put_keys {V : Type} (predicate : Key → Prop) (key : Key) (value : V)
    (rows : List (Key × V)) (allKeys : KeysSatisfy predicate rows) (newKey : predicate key) :
    KeysSatisfy predicate (put key value rows) := by
  intro row member
  rcases put_members key value rows row member with equal | old
  · subst row; exact newKey
  · exact allKeys row old

theorem room_keys {V : Type} (predicate : Key → Prop) (key : Key) (rows : List (Key × V))
    (allKeys : KeysSatisfy predicate rows) : KeysSatisfy predicate (room key rows) := by
  unfold room
  split
  · cases rows with
    | nil => simpa [KeysSatisfy] using allKeys
    | cons head rest =>
      intro row member
      exact allKeys row (List.mem_cons_of_mem head member)
  · exact allKeys

/-- Actual insertion primitive with full previous value, sorted/unique output,
exact inserted value and unrelated whole-value lookup preservation. -/
theorem actual_insert_contract {V : Type} (key : Key) (value : V) (rows : List (Key × V))
    (ordered : Ordered rows) :
    alloc.collections.btree.map.BTreeMap.insert alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd rows key value =
      .ok (lookup key rows, put key value rows) ∧
      Ordered (put key value rows) ∧ Unique (put key value rows) ∧
      lookup key (put key value rows) = some value ∧
      ∀ other, other ≠ key → lookup other (put key value rows) = lookup other rows := by
  have sorted := put_ordered key value rows ordered
  refine ⟨?_, sorted, ordered_unique _ sorted, put_reads_inserted _ _ _,
    fun other different => put_frames _ _ _ _ different⟩
  change NativeMap.insertBy keyCmp rows key value = _
  rw [map_insert_exact, insertion_old_lookup key rows ordered]

/-- Actual erase primitive retains order/uniqueness, never grows the map, and
frames every unrelated complete value. No invariant-after-state is assumed. -/
theorem actual_remove_contract {V : Type} (key : Key) (rows : List (Key × V))
    (ordered : Ordered rows) :
    alloc.collections.btree.map.BTreeMap.remove alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      (core.borrow.Borrow.Blanket Key) mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd rows key =
      .ok (lookup key rows, erase key rows) ∧
      Ordered (erase key rows) ∧ Unique (erase key rows) ∧
      (erase key rows).length ≤ rows.length ∧
      lookup key (erase key rows) = none ∧
      ∀ other, other ≠ key → lookup other (erase key rows) = lookup other rows := by
  have sorted := erase_ordered key rows ordered
  refine ⟨?_, sorted, ordered_unique _ sorted,
    erase_size_le _ _, erase_removed _ _ (ordered_unique rows ordered),
    fun other different => erase_other _ _ _ different⟩
  simp only [alloc.collections.btree.map.BTreeMap.remove]
  have borrow : @HubDiscoveryCacheModel.core.borrow.Borrow.Blanket.borrow Key =
      (fun key => Result.ok key) := by rfl
  rw [borrow]
  exact map_remove_exact _ _

/-- Actual minimum-removal primitive preserves strict ordering/uniqueness and
returns the complete first row, not a key-only summary. -/
theorem actual_pop_contract {V : Type} (rows : List (Key × V)) (ordered : Ordered rows) :
    alloc.collections.btree.map.BTreeMap.pop_first alloc.alloc.Global.Insts.CoreAllocAllocatorClone
      mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd rows = .ok (rows.head?, rows.tail) ∧
      Ordered rows.tail ∧ Unique rows.tail := by
  have sorted := tail_ordered rows ordered
  exact ⟨CacheCompleteProof.pop_first_exact rows, sorted, ordered_unique _ sorted⟩

end
end CacheMapInvariantProof

#print axioms CacheMapInvariantProof.ordered_unique
#print axioms CacheMapInvariantProof.put_ordered
#print axioms CacheMapInvariantProof.insertion_old_lookup
#print axioms CacheMapInvariantProof.put_room_capacity
#print axioms CacheMapInvariantProof.victim_minimum
#print axioms CacheMapInvariantProof.actual_insert_contract
#print axioms CacheMapInvariantProof.actual_remove_contract
#print axioms CacheMapInvariantProof.actual_pop_contract

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheMapInvariantProof.ordered_unique,
      ``CacheMapInvariantProof.erase_members, ``CacheMapInvariantProof.erase_ordered,
      ``CacheMapInvariantProof.erase_size_le, ``CacheMapInvariantProof.put_members,
      ``CacheMapInvariantProof.put_ordered, ``CacheMapInvariantProof.insertion_old_lookup,
      ``CacheMapInvariantProof.put_length_at_insertion, ``CacheMapInvariantProof.put_length,
      ``CacheMapInvariantProof.put_existing_size, ``CacheMapInvariantProof.tail_ordered,
      ``CacheMapInvariantProof.room_ordered, ``CacheMapInvariantProof.room_length,
      ``CacheMapInvariantProof.put_room_capacity, ``CacheMapInvariantProof.victim_minimum,
      ``CacheMapInvariantProof.erase_keys, ``CacheMapInvariantProof.put_keys,
      ``CacheMapInvariantProof.room_keys, ``CacheMapInvariantProof.actual_insert_contract,
      ``CacheMapInvariantProof.actual_remove_contract, ``CacheMapInvariantProof.actual_pop_contract] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected cache map-invariant axiom in {declaration}: {dependencies}"
