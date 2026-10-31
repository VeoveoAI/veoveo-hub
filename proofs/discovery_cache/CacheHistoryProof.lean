import CacheStateInvariantProof

open Aeneas Aeneas.Std HubDiscoveryCacheModel
open CacheKeyFrameProof CacheStartSpecification CacheStartProof
open CacheCompleteSpecification CacheCompleteProof CacheMapInvariantProof CacheStateInvariantProof

namespace CacheHistoryProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Observations needed to state finite accepted-ticket nonreuse. Full callback,
item/denied and state observations remain in the actual Step equations below. -/
inductive Event where
  | read
  | start (accepted : Option Fetch)
  | complete
  | remove_cached
  | discard_key

/-- A step is an actual extracted method's normal-return equation, never a
copied executor or an assumed state invariant. Reads explicitly stutter; failure,
divergence and native post-panic states are outside this normal-return relation. -/
inductive Step {T : Type} : State T → Event → State T → Prop where
  | read {state : State T} (clone : core.clone.Clone T) (eq : core.cmp.PartialEq T T)
      (key : Key) (observed : Option (mcp.discovery.AdmittedCatalog T))
      (normal : mcp.discovery.SurfaceState.cached clone eq state key = .ok observed) :
      Step state .read state
  | start {before after : State T} {N R : Type} (clone : core.clone.Clone T)
      (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
      (fresh : core.ops.function.FnOnce R Unit Ticket) (key : Key) (coalesce : Bool)
      (now : N) (ticket : R) (accepted : Option Fetch)
      (normal : mcp.discovery.SurfaceState.start clone eq clock fresh before key coalesce now ticket =
        .ok (accepted, after)) : Step before (.start accepted) after
  | complete {before after : State T} {N : Type} (clone : core.clone.Clone T)
      (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
      (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N) (changed : Bool)
      (normal : mcp.discovery.SurfaceState.complete clone eq clock before fetch items now =
        .ok (changed, after)) : Step before .complete after
  | remove_cached {before after : State T} (clone : core.clone.Clone T)
      (eq : core.cmp.PartialEq T T) (key : Key)
      (normal : mcp.discovery.SurfaceState.remove_cached clone eq before key = .ok after) :
      Step before .remove_cached after
  | discard_key {before after : State T} (clone : core.clone.Clone T)
      (eq : core.cmp.PartialEq T T) (key : Key)
      (normal : mcp.discovery.SurfaceState.discard_key clone eq before key = .ok after) :
      Step before .discard_key after

/-- Finite sequences of actual supported operation observations. This relation
contains no StateOk premises and includes read observations as stuttering steps. -/
inductive History {T : Type} : State T → List Event → State T → Prop where
  | nil (state : State T) : History state [] state
  | cons {before middle after : State T} {event : Event} {events : List Event}
      (step : Step before event middle) (rest : History middle events after) :
      History before (event :: events) after

/-- Concatenation retains each original actual-operation equation. -/
theorem history_append {T : Type} {before middle after : State T} {past suffix : List Event}
    (first : History before past middle) (second : History middle suffix after) :
    History before (past ++ suffix) after := by
  induction first with
  | nil => exact second
  | cons step rest recursive => exact .cons step (recursive second)

theorem step_preserves {T : Type} {before after : State T} {event : Event}
    (step : Step before event after) (valid : StateOk before) :
    StateOk after ∧ before.generation.val ≤ after.generation.val := by
  cases step with
  | read => exact ⟨valid, Nat.le_refl _⟩
  | start clone eq clock fresh key coalesce now ticket accepted normal =>
    exact start_preserves clone eq clock fresh _ key coalesce now ticket accepted _ valid normal
  | complete clone eq clock fetch items now changed normal =>
    obtain ⟨afterValid, same⟩ := complete_preserves clone eq clock _ fetch items now changed _ valid normal
    exact ⟨afterValid, by rw [same]⟩
  | remove_cached clone eq key normal =>
    obtain ⟨afterValid, same⟩ := remove_cached_preserves clone eq _ key _ valid normal
    exact ⟨afterValid, by rw [same]⟩
  | discard_key clone eq key normal =>
    obtain ⟨afterValid, same⟩ := discard_key_preserves clone eq _ key _ valid normal
    exact ⟨afterValid, by rw [same]⟩

theorem history_preserves {T : Type} {before after : State T} {events : List Event}
    (history : History before events after) (valid : StateOk before) :
    StateOk after ∧ before.generation.val ≤ after.generation.val := by
  induction history with
  | nil => exact ⟨valid, Nat.le_refl _⟩
  | cons step rest recursive =>
    obtain ⟨middleValid, firstGeneration⟩ := step_preserves step valid
    obtain ⟨afterValid, laterGeneration⟩ := recursive middleValid
    exact ⟨afterValid, firstGeneration.trans laterGeneration⟩

/-- Every supported normal-return history from empty derives all map capacities,
strict ordering, unique keys and stored-key generation coherence. -/
theorem reachable_ok {T : Type} (generation : U64) {after : State T} {events : List Event}
    (history : History (empty T generation) events after) : StateOk after :=
  (history_preserves history (empty_ok T generation)).1

/-- Once generation has advanced past a fetch, any arbitrary later supported
normal-return suffix keeps its entire completion stale. No ticket premise. -/
theorem generation_fence {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (generation : U64) {advanced final : State T} {past suffix : List Event}
    (reached : History (empty T generation) past advanced)
    (later : History advanced suffix final) (fetch : Fetch) (items : Option (alloc.vec.Vec T))
    (now : N) (older : fetch.key.catalog_generation.val < advanced.generation.val) :
    mcp.discovery.SurfaceState.complete clone eq clock final fetch items now = .ok (false, final) := by
  obtain ⟨finalValid, monotone⟩ := history_preserves later (reachable_ok generation reached)
  apply generation_stale_complete clone eq clock final fetch items now finalValid
  omega

/-- Only actually accepted same-key start outputs in this finite suffix must
avoid the old ticket. Rejected starts, unused callback observations, other keys,
clock results and all other operations are unrestricted. -/
def Allows (key : Key) (old : Ticket) : Event → Prop
  | .start (some accepted) => accepted.key = key → accepted.request ≠ old
  | _ => True

def Nonreuse (key : Key) (old : Ticket) (events : List Event) : Prop :=
  ∀ event ∈ events, Allows key old event

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

private theorem erase_excludes (rows : List (Key × Ticket)) (removed query : Key) (old : Ticket)
    (unique : Unique rows) (excluded : lookup query rows ≠ some old) :
    lookup query (erase removed rows) ≠ some old := by
  by_cases same : removed = query
  · subst removed
    rw [erase_removed rows query unique]
    simp
  · rw [erase_other rows removed query (Ne.symm same)]
    exact excluded

private theorem prepared_excludes {T : Type} (state : State T) (key query : Key) (old : Ticket)
    (excluded : lookup query state.in_flight ≠ some old) :
    lookup query (prepared state key).in_flight ≠ some old := by
  unfold prepared
  split
  · simp [lookup]
  · exact excluded

/-- The accepted-output event, not any prospective ticket observation, supplies
precisely the scoped nonreuse needed for a same-key replacement. -/
theorem start_excludes {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (before after : State T) (key query : Key)
    (coalesce : Bool) (now : N) (ticket : R) (accepted : Option Fetch) (old : Ticket)
    (valid : StateOk before) (excluded : lookup query before.in_flight ≠ some old)
    (safe : Allows query old (.start accepted))
    (normal : mcp.discovery.SurfaceState.start clone eq clock fresh before key coalesce now ticket =
      .ok (accepted, after)) : lookup query after.in_flight ≠ some old := by
  rw [start_exact clone eq clock fresh before key coalesce now ticket
    (Or.inr (map_physical valid.2.1))] at normal
  unfold startOutcome at normal
  split at normal
  · have same := returned_state normal
    subst after
    exact excluded
  · have preparedExcluded := prepared_excludes before key query old excluded
    obtain ⟨blocked, _, resumed⟩ := bind_normal normal
    cases blocked with
    | true =>
      simp only [↓reduceIte] at resumed
      have same := returned_state resumed
      subst after
      exact preparedExcluded
    | false =>
      simp only [Bool.false_eq_true, ↓reduceIte] at resumed
      split at resumed
      · have same := returned_state resumed
        subst after
        exact preparedExcluded
      · split at resumed
        · have same := returned_state resumed
          subst after
          exact preparedExcluded
        · obtain ⟨request, _, stored⟩ := bind_normal resumed
          have same := returned_state stored
          have output : some ({ key, request, denied := 0#u32 } : Fetch) = accepted :=
            congrArg Prod.fst (Result.ok_injective stored)
          subst after
          rw [← output] at safe
          by_cases sameKey : query = key
          · subst query
            rw [put_reads_inserted]
            have different : request ≠ old := safe rfl
            simpa using different
          · rw [put_frames _ _ _ _ sameKey]
            exact preparedExcluded

/-- A normal completion can remove a ticket, never create a new one. Strict
key uniqueness is derived from StateOk before erasing, not assumed afterwards. -/
theorem complete_excludes {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (before after : State T) (fetch : Fetch) (items : Option (alloc.vec.Vec T)) (now : N)
    (changed : Bool) (query : Key) (old : Ticket) (valid : StateOk before)
    (excluded : lookup query before.in_flight ≠ some old)
    (normal : mcp.discovery.SurfaceState.complete clone eq clock before fetch items now =
      .ok (changed, after)) : lookup query after.in_flight ≠ some old := by
  have physical : CacheCompleteSpecification.Physical before fetch items := by
    apply Or.inr
    cases items with
    | none => exact map_physical valid.2.2
    | some _ => exact map_physical valid.1
  rw [complete_exact clone eq clock before fetch items now physical] at normal
  unfold completeOutcome at normal
  split at normal
  · have same := returned_state normal
    subst after
    exact excluded
  · cases items with
    | none =>
      obtain ⟨sampled, _, observed⟩ := bind_normal normal
      obtain ⟨deadline, _, stored⟩ := bind_normal observed
      have same := returned_state stored
      subst after
      exact erase_excludes _ fetch.key query old (map_unique valid.2.1) excluded
    | some values =>
      obtain ⟨different, _, stored⟩ := bind_normal normal
      have same := returned_state stored
      subst after
      exact erase_excludes _ fetch.key query old (map_unique valid.2.1) excluded

theorem step_excludes {T : Type} {before after : State T} {event : Event}
    (step : Step before event after) (valid : StateOk before) (key : Key) (old : Ticket)
    (excluded : lookup key before.in_flight ≠ some old) (safe : Allows key old event) :
    lookup key after.in_flight ≠ some old := by
  cases step with
  | read => exact excluded
  | start clone eq clock fresh requested coalesce now ticket accepted normal =>
    exact start_excludes clone eq clock fresh _ _ requested key coalesce now ticket accepted old
      valid excluded safe normal
  | complete clone eq clock fetch items now changed normal =>
    exact complete_excludes clone eq clock _ _ fetch items now changed key old valid excluded normal
  | remove_cached clone eq removed normal =>
    rw [remove_cached_exact] at normal
    have same := Result.ok_injective normal
    subst after
    exact excluded
  | discard_key clone eq removed normal =>
    rw [discard_key_exact] at normal
    have same := Result.ok_injective normal
    subst after
    exact erase_excludes _ removed key old (map_unique valid.2.1) excluded

theorem history_excludes {T : Type} {before after : State T} {events : List Event}
    (history : History before events after) (valid : StateOk before) (key : Key) (old : Ticket)
    (excluded : lookup key before.in_flight ≠ some old) (safe : Nonreuse key old events) :
    lookup key after.in_flight ≠ some old := by
  induction history with
  | nil => exact excluded
  | @cons before middle after event tail step rest recursive =>
    have headSafe := safe _ (List.mem_cons_self)
    have tailSafe : Nonreuse key old tail := by
      intro event member
      exact safe event (List.mem_cons_of_mem _ member)
    exact recursive (step_preserves step valid).1
      (step_excludes step valid key old excluded headSafe) tailSafe

/-- Actual key invalidation followed by arbitrary supported operations, including
accepted replacements with different tickets, fences the old whole completion.
The final ticket mismatch is derived, never supplied as a premise. -/
theorem discard_suffix_fence {T N : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (generation : U64) {before invalidated final : State T} {past suffix : List Event}
    (reached : History (empty T generation) past before) (fetch : Fetch)
    (discarded : mcp.discovery.SurfaceState.discard_key clone eq before fetch.key = .ok invalidated)
    (later : History invalidated suffix final) (nonreuse : Nonreuse fetch.key fetch.request suffix)
    (items : Option (alloc.vec.Vec T)) (now : N) :
    mcp.discovery.SurfaceState.complete clone eq clock final fetch items now = .ok (false, final) := by
  have beforeValid := reachable_ok generation reached
  have invalidatedValid := (discard_key_preserves clone eq before fetch.key invalidated beforeValid discarded).1
  have absent : lookup fetch.key invalidated.in_flight = none := by
    rw [discard_key_exact] at discarded
    have same := Result.ok_injective discarded
    subst invalidated
    exact erase_removed _ _ (map_unique beforeValid.2.1)
  apply stale_complete
  apply history_excludes later invalidatedValid fetch.key fetch.request
  · rw [absent]
    simp
  · exact nonreuse

/-- A constructible accepted history: from an empty equal-generation state,
noncoalesced start returns the exact denied-zero fetch and stores its actual
observed ticket. There are no clock/retry/pending constraints. -/
theorem accepted_start_history {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (key : Key) (now : N) (ticket : R)
    (request : Ticket) (observed : fresh.call_once ticket () = .ok request) :
    let initial := empty T key.catalog_generation
    let accepted : Fetch := { key, request, denied := 0#u32 }
    let final : State T := { initial with in_flight := [(key, request)] }
    History initial [.start (some accepted)] final ∧ StateOk final ∧
      lookup key final.in_flight = some request := by
  dsimp only
  have accepted := (uncoalesced_accepted clone eq clock fresh
    (empty T key.catalog_generation) key now ticket rfl
    (map_physical (empty_ok T key.catalog_generation).2.1) (by simp [empty]) request observed).1
  have exactStart : mcp.discovery.SurfaceState.start clone eq clock fresh
      (empty T key.catalog_generation) key false now ticket =
      .ok (some { key, request, denied := 0#u32 },
        { (empty T key.catalog_generation) with in_flight := [(key, request)] }) := by
    simpa [empty, put] using accepted
  have history : History (empty T key.catalog_generation)
      [.start (some { key, request, denied := 0#u32 })]
      { (empty T key.catalog_generation) with in_flight := [(key, request)] } :=
    .cons (.start clone eq clock fresh key false now ticket _ exactStart) (.nil _)
  have finalValid := (start_preserves clone eq clock fresh
    (empty T key.catalog_generation) key false now ticket _ _
    (empty_ok T key.catalog_generation) exactStart).1
  exact ⟨history, finalValid, by simp [lookup]⟩

/-- A genuinely advancing accepted step after any reachable past, resetting
all previous maps. Its supplied clock remains unrestricted and unused. -/
theorem accepted_advance_history {T N R : Type} (clone : core.clone.Clone T)
    (eq : core.cmp.PartialEq T T) (clock : core.ops.function.FnOnce N Unit Instant)
    (fresh : core.ops.function.FnOnce R Unit Ticket) (generation : U64)
    {before : State T} {past : List Event} (reached : History (empty T generation) past before)
    (key : Key) (now : N) (ticket : R) (request : Ticket)
    (newer : key.catalog_generation.val > before.generation.val)
    (observed : fresh.call_once ticket () = .ok request) :
    let final : State T :=
      { generation := key.catalog_generation
        entries := []
        in_flight := [(key, request)]
        failed := [] }
    Step before (.start (some { key, request, denied := 0#u32 })) final ∧
      History (empty T generation)
        (past ++ [.start (some { key, request, denied := 0#u32 })]) final ∧
      StateOk final ∧ before.generation.val < final.generation.val := by
  dsimp only
  have normal := newer_generation clone eq clock fresh before key true now ticket newer
  rw [observed] at normal
  simp only [bind_tc_eq, bind_ok] at normal
  have step : Step before (.start (some { key, request, denied := 0#u32 }))
      { generation := key.catalog_generation, entries := [], in_flight := [(key, request)], failed := [] } :=
    .start clone eq clock fresh key true now ticket _ normal
  have extended := history_append reached (.cons step (.nil _))
  exact ⟨step, extended, by simp [StateOk, MapOk, Ordered, KeysSatisfy], newer⟩

end
end CacheHistoryProof

#print axioms CacheHistoryProof.step_preserves
#print axioms CacheHistoryProof.reachable_ok
#print axioms CacheHistoryProof.generation_fence
#print axioms CacheHistoryProof.history_excludes
#print axioms CacheHistoryProof.discard_suffix_fence
#print axioms CacheHistoryProof.accepted_start_history
#print axioms CacheHistoryProof.accepted_advance_history

run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``CacheHistoryProof.start_excludes] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => standard.contains dependency) do
      throwError "unexpected cache-history start axiom in {declaration}: {dependencies}"
  -- History/Step carry the complete-method equation in their types, so even
  -- start-only constructibility lemmas inherit its existing addition boundary.
  let native := standard.push ``HubDiscoveryCacheModel.tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add
  for declaration in #[``CacheHistoryProof.history_append, ``CacheHistoryProof.accepted_start_history,
      ``CacheHistoryProof.accepted_advance_history,
      ``CacheHistoryProof.step_preserves, ``CacheHistoryProof.history_preserves,
      ``CacheHistoryProof.reachable_ok, ``CacheHistoryProof.generation_fence,
      ``CacheHistoryProof.complete_excludes, ``CacheHistoryProof.step_excludes,
      ``CacheHistoryProof.history_excludes, ``CacheHistoryProof.discard_suffix_fence] do
    let dependencies ← Lean.collectAxioms declaration
    unless dependencies.all (fun dependency => native.contains dependency) do
      throwError "unexpected actual-cache history axiom in {declaration}: {dependencies}"
