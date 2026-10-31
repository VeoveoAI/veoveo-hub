import HubDiscoveryCacheState.Funs

open Aeneas Aeneas.Std
open HubDiscoveryCacheModel

namespace CacheKeyFrameProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Key := mcp.discovery.DiscoveryCacheKey
abbrev State := mcp.discovery.SurfaceState
abbrev keyCmp := mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd.cmp

deriving instance DecidableEq for Key

/-- Full source field order, independently named rather than an assumed result
of the extracted derive. Native string/UUID representations remain refinement
boundaries; this is the complete generated String-value model. -/
def thenOrder (first next : Ordering) : Ordering :=
  match first with | .eq => next | other => other

def lexOrder {T : Type} (order : T → T → Ordering) : List T → List T → Ordering
  | [], [] => .eq
  | [], _ :: _ => .lt
  | _ :: _, [] => .gt
  | left :: rest, right :: tail => thenOrder (order left right) (lexOrder order rest tail)

def keyOrder (left right : Key) : Ordering :=
  thenOrder (compare left.catalog_generation.val right.catalog_generation.val)
    (thenOrder (compare left.principal right.principal)
      (thenOrder (lexOrder (fun a b : U8 => compare a.val b.val)
          left.authorization_fingerprint.val right.authorization_fingerprint.val)
        (thenOrder (compare left.server right.server) (compare left.link right.link))))

theorem thenOrder_eq (first next : Ordering) :
    thenOrder first next = .eq ↔ first = .eq ∧ next = .eq := by
  cases first <;> simp [thenOrder]

theorem lexOrder_eq {T : Type} (order : T → T → Ordering)
    (law : ∀ left right, order left right = .eq ↔ left = right)
    (left right : List T) : lexOrder order left right = .eq ↔ left = right := by
  induction left generalizing right with
  | nil => cases right <;> simp [lexOrder]
  | cons head rest recursive =>
    cases right with
    | nil => simp [lexOrder]
    | cons other tail => simp [lexOrder, thenOrder_eq, law, recursive]

private theorem option_compare_eq (left right : Option _root_.String) :
    compare left right = .eq ↔ left = right := by
  cases left with
  | none => cases right <;> simp [compare]
  | some left =>
    cases right with
    | none => simp [compare]
    | some right =>
      change compare left right = .eq ↔ some left = some right
      simp only [_root_.compare_eq_iff_eq, Option.some.injEq]

theorem keyOrder_eq (left right : Key) :
    keyOrder left right = .eq ↔ left = right := by
  have scalar (left right : U8) : compare left.val right.val = .eq ↔ left = right := by
    rw [_root_.compare_eq_iff_eq]
    exact (UScalar.eq_equiv left right).symm
  simp only [keyOrder, thenOrder_eq, _root_.compare_eq_iff_eq,
    lexOrder_eq _ scalar, ← Array.eq_iff, option_compare_eq]
  rw [← UScalar.eq_equiv]
  cases left; cases right; simp

private theorem lexCompare_exact {T : Type} (operation : T → T → Result Ordering)
    (order : T → T → Ordering)
    (law : ∀ left right, operation left right = .ok (order left right))
    (left right : List T) :
    NativeMap.lexCompare operation left right = .ok (lexOrder order left right) := by
  induction left generalizing right with
  | nil => cases right <;> rfl
  | cons head rest recursive =>
    cases right with
    | nil => rfl
    | cons other tail =>
      rw [NativeMap.lexCompare, law]
      simp only [bind_tc_eq, bind_ok]
      cases compared : order head other <;> simp [lexOrder, thenOrder, compared, recursive]

/-- Derived key comparison is proved from the actual generated body and the
concrete external primitive definitions. It is not an own-success premise. -/
theorem key_comparison_exact (left right : Key) :
    keyCmp left right = .ok (keyOrder left right) := by
  have bytes (a b : U8) :
      (liftFun2 core.cmp.impls.OrdU8.cmp a b : Result Ordering) =
        .ok (compare a.val b.val) := by rfl
  have fingerprint := lexCompare_exact
    (liftFun2 core.cmp.impls.OrdU8.cmp)
    (fun a b : U8 => compare a.val b.val) bytes
    left.authorization_fingerprint.val right.authorization_fingerprint.val
  have link : core.option.Option.Insts.CoreCmpOrd.cmp
      veoveo_hub_sdk.hub.tool_name.LinkName.Insts.CoreCmpOrd left.link right.link =
      .ok (compare left.link right.link) := by
    cases left.link <;> cases right.link <;> rfl
  unfold keyCmp mcp.discovery.DiscoveryCacheKey.Insts.CoreCmpOrd.cmp
  rw [link]
  simp only [veoveo_types.identity.PrincipalId.Insts.CoreCmpOrd.cmp,
    veoveo_types.platform_names.ServerSlug.Insts.CoreCmpOrd.cmp,
    alloc.string.String.Insts.CoreCmpOrd.cmp, Array.Insts.CoreCmpOrd.cmp]
  rw [fingerprint]
  have generation : (lift (core.cmp.impls.OrdU64.cmp left.catalog_generation right.catalog_generation) : Result Ordering) =
      .ok (compare left.catalog_generation.val right.catalog_generation.val) := by rfl
  rw [generation]
  simp only [bind_tc_eq, bind_ok]
  cases first : compare left.catalog_generation.val right.catalog_generation.val <;>
    cases second : compare left.principal right.principal <;>
    cases third : lexOrder (fun a b : U8 => compare a.val b.val)
      left.authorization_fingerprint.val right.authorization_fingerprint.val <;>
    cases fourth : compare left.server right.server <;>
    simp [keyOrder, thenOrder, first, second, third, fourth, bind_ok]

/-- Independent finite-map observations. Whole values, including every payload
field, are returned; there is no projection to descriptor counts. -/
def lookup {V : Type} (query : Key) : List (Key × V) → Option V
  | [] => none
  | (key, value) :: rest => if key = query then some value else lookup query rest

def erase {V : Type} (query : Key) : List (Key × V) → List (Key × V)
  | [] => []
  | (key, value) :: rest => if key = query then rest else (key, value) :: erase query rest

theorem map_get_exact {V : Type} (entries : List (Key × V)) (query : Key) :
    NativeMap.getBy (fun key => .ok key) keyCmp entries query =
      .ok (lookup query entries) := by
  induction entries with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨key, value⟩
    rw [NativeMap.getBy]
    simp only [bind_tc_eq, bind_ok]
    rw [key_comparison_exact]
    simp only [bind_tc_eq, bind_ok]
    by_cases same : key = query
    · subst key
      have reflexive : keyOrder query query = .eq := (keyOrder_eq query query).mpr rfl
      simp [lookup, reflexive]
    · have different := mt (keyOrder_eq key query).mp same
      simp [lookup, same, different, recursive]

theorem map_remove_exact {V : Type} (entries : List (Key × V)) (query : Key) :
    NativeMap.removeBy (fun key => .ok key) keyCmp entries query =
      .ok (lookup query entries, erase query entries) := by
  induction entries with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨key, value⟩
    rw [NativeMap.removeBy]
    simp only [bind_tc_eq, bind_ok]
    rw [key_comparison_exact]
    simp only [bind_tc_eq, bind_ok]
    by_cases same : key = query
    · subst key
      have reflexive : keyOrder query query = .eq := (keyOrder_eq query query).mpr rfl
      simp [lookup, erase, reflexive]
    · have different := mt (keyOrder_eq key query).mp same
      simp [lookup, erase, same, different, recursive, bind_ok]

theorem erase_other {V : Type} (entries : List (Key × V)) (removed query : Key)
    (different : query ≠ removed) :
    lookup query (erase removed entries) = lookup query entries := by
  induction entries with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨key, value⟩
    by_cases same : key = removed
    · subst key
      simp [erase, lookup, Ne.symm different]
    · simp only [erase, if_neg same]
      simp only [lookup]
      split <;> simp_all

theorem lookup_none_of_absent {V : Type} (entries : List (Key × V)) (query : Key)
    (absent : query ∉ entries.map Prod.fst) : lookup query entries = none := by
  induction entries with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨key, value⟩
    simp only [List.map_cons, List.mem_cons, not_or] at absent
    simp [lookup, Ne.symm absent.1, recursive absent.2]

theorem erase_removed {V : Type} (entries : List (Key × V)) (query : Key)
    (unique : (entries.map Prod.fst).Nodup) : lookup query (erase query entries) = none := by
  induction entries with
  | nil => rfl
  | cons head rest recursive =>
    rcases head with ⟨key, value⟩
    simp only [List.map_cons, List.nodup_cons] at unique
    by_cases same : key = query
    · subst key
      simpa [erase] using lookup_none_of_absent rest query unique.1
    · simp [erase, lookup, same, recursive unique.2]

/-- A total unique-value WP excludes primitive failure and divergence. -/
private theorem exact_of_total {T : Type} (operation : Result T) (value : T)
    (total : operation ⦃ result => result = value ⦄) : operation = .ok value := by
  cases operation using Result.cases with
  | ret returned => exact congrArg Result.ok ((WP.spec_ok returned).mp total)
  | vis effect continuation => exact False.elim ((WP.spec_vis effect continuation).mp total)
  | div => exact False.elim (WP.spec_div.mp total)

private theorem borrow_function :
    @HubDiscoveryCacheModel.core.borrow.Borrow.Blanket.borrow Key =
      (fun key => Result.ok key) := by rfl

private theorem vec_clone_exact {T : Type} (clone : core.clone.Clone T)
    (law : ∀ value, clone.clone value = .ok value) (values : alloc.vec.Vec T) :
    alloc.vec.CloneVec.clone clone values = .ok values := by
  unfold alloc.vec.CloneVec.clone
  have copied := Slice.clone_spec (s := values.slice) (clone := clone.clone)
    (by intro value member; exact law value)
  have exactCopy := exact_of_total _ _ (WP.spec_mono copied (by intro output same; exact same.symm))
  rw [exactCopy]
  simp only [bind_ok, bind_tc_ok]

/-- The read is exact on full cloned items and denied count. The original state
is passed unchanged, rather than summarized by a key/count-only observation. -/
theorem cached_exact {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (law : ∀ value, clone.clone value = .ok value)
    (state : State T) (key : Key) :
    (mcp.discovery.SurfaceState.cached clone eq state key, state) =
      (.ok ((lookup key state.entries).map (fun entry =>
        ({ items := entry.items, denied := entry.denied } : mcp.discovery.AdmittedCatalog T))), state) := by
  congr 1
  unfold mcp.discovery.SurfaceState.cached
  simp only [alloc.collections.btree.map.BTreeMap.get, core.borrow.Borrow.Blanket,
    bind_tc_eq]
  rw [borrow_function]
  rw [map_get_exact]
  cases found : lookup key state.entries <;>
    simp [core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
      found, vec_clone_exact clone law, bind_ok, bind_tc_ok]

/-- Cached-only removal preserves every ticket and retry record. It is explicitly
not a fence against a legitimate in-flight completion. -/
theorem remove_cached_exact {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (state : State T) (key : Key) :
    mcp.discovery.SurfaceState.remove_cached clone eq state key =
      .ok { state with entries := erase key state.entries } := by
  unfold mcp.discovery.SurfaceState.remove_cached
  simp only [alloc.collections.btree.map.BTreeMap.remove, core.borrow.Borrow.Blanket,
    bind_tc_eq]
  rw [borrow_function]
  rw [map_remove_exact]
  simp only [bind_tc_eq, bind_ok]
  rfl

/-- Full key invalidation removes exactly that key from all three maps and
retains generation and every unrelated whole value. -/
theorem discard_key_exact {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (state : State T) (key : Key) :
    mcp.discovery.SurfaceState.discard_key clone eq state key =
      .ok { state with
        entries := erase key state.entries
        in_flight := erase key state.in_flight
        failed := erase key state.failed } := by
  unfold mcp.discovery.SurfaceState.discard_key
  simp only [alloc.collections.btree.map.BTreeMap.remove, core.borrow.Borrow.Blanket,
    bind_tc_eq]
  rw [borrow_function]
  rw [map_remove_exact]
  simp only [bind_tc_eq, bind_ok]
  rw [map_remove_exact]
  simp only [bind_tc_eq, bind_ok]
  rw [map_remove_exact]
  simp only [bind_tc_eq, bind_ok]
  rfl

theorem discard_key_frames {T : Type} (state : State T) (removed query : Key)
    (different : query ≠ removed) :
    lookup query (erase removed state.entries) = lookup query state.entries ∧
    lookup query (erase removed state.in_flight) = lookup query state.in_flight ∧
    lookup query (erase removed state.failed) = lookup query state.failed :=
  ⟨erase_other _ _ _ different, erase_other _ _ _ different, erase_other _ _ _ different⟩

/-- A different optional link makes the complete source keys distinct. -/
theorem different_links_distinct (removed query : Key)
    (different : query.link ≠ removed.link) : query ≠ removed := by
  intro same
  exact different (congrArg (fun key : Key => key.link) same)

/-- Actual key invalidation preserves the other link's complete cache entry,
ticket and retry record, not just an independently defined erase operation. -/
theorem discard_key_link_isolation {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (state : State T) (removed query : Key)
    (different : query.link ≠ removed.link) :
    ∃ final : State T,
      mcp.discovery.SurfaceState.discard_key clone eq state removed = .ok final ∧
      final.generation = state.generation ∧
      lookup query final.entries = lookup query state.entries ∧
      lookup query final.in_flight = lookup query state.in_flight ∧
      lookup query final.failed = lookup query state.failed := by
  refine ⟨{ state with
    entries := erase removed state.entries
    in_flight := erase removed state.in_flight
    failed := erase removed state.failed },
    discard_key_exact clone eq state removed, rfl, ?_⟩
  exact discard_key_frames state removed query (different_links_distinct removed query different)

/-- Absence and every present link are different, including represented empty
strings: no parsing-validity premise or default-empty encoding is used. -/
theorem discard_key_none_some_isolation {T : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (state : State T) (key : Key)
    (link : veoveo_hub_sdk.hub.tool_name.LinkName) :
    ∃ final : State T,
      mcp.discovery.SurfaceState.discard_key clone eq state { key with link := none } =
        .ok final ∧
      final.generation = state.generation ∧
      lookup { key with link := some link } final.entries =
        lookup { key with link := some link } state.entries ∧
      lookup { key with link := some link } final.in_flight =
        lookup { key with link := some link } state.in_flight ∧
      lookup { key with link := some link } final.failed =
        lookup { key with link := some link } state.failed := by
  exact discard_key_link_isolation clone eq state
    { key with link := none } { key with link := some link } (by simp)

end
end CacheKeyFrameProof

#print axioms CacheKeyFrameProof.key_comparison_exact
#print axioms CacheKeyFrameProof.cached_exact
#print axioms CacheKeyFrameProof.remove_cached_exact
#print axioms CacheKeyFrameProof.discard_key_exact
#print axioms CacheKeyFrameProof.discard_key_frames
#print axioms CacheKeyFrameProof.discard_key_link_isolation
#print axioms CacheKeyFrameProof.discard_key_none_some_isolation

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheKeyFrameProof.keyOrder_eq, ``CacheKeyFrameProof.key_comparison_exact,
      ``CacheKeyFrameProof.map_get_exact, ``CacheKeyFrameProof.map_remove_exact,
      ``CacheKeyFrameProof.erase_other, ``CacheKeyFrameProof.erase_removed,
      ``CacheKeyFrameProof.cached_exact, ``CacheKeyFrameProof.remove_cached_exact,
      ``CacheKeyFrameProof.discard_key_exact, ``CacheKeyFrameProof.discard_key_frames,
      ``CacheKeyFrameProof.different_links_distinct, ``CacheKeyFrameProof.discard_key_link_isolation,
      ``CacheKeyFrameProof.discard_key_none_some_isolation] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => allowed.contains dependency) do
      throwError "unexpected cache read/key-frame axiom in {declaration}: {dependencies}"
