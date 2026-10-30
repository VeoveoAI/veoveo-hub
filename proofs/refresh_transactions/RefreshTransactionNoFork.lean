import RefreshTransactionHistory
import RefreshTransactionLocks

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

def generationFor (family : Family) : Outcome → U64
  | .rotated successor => successor.generation
  | _ => family.current_generation

theorem commit_program_generation (db : Database) (request : Request) (family : Family)
    (token : Token) (outcome : Outcome) (present : db.family = some family) :
    (commit db (programFor request family token outcome)).family.map (fun (family : Family) => family.current_generation) =
      some (generationFor family outcome) := by
  cases outcome <;>
    simp [programFor, generationFor, commit, applyWrites, applyWrite, present]
  all_goals split_ifs <;> simp [List.foldl_cons, List.foldl_nil, applyWrite, present]

/-- History is ghost evidence, never a runtime table or commit correctness guard. -/
def HistoryRows (base : Nat) (family : Option Family) (events : List RotationEvent) : Prop :=
  (∀ current, family = some current → current.current_generation.val = base + events.length) ∧
  events.Pairwise (fun left right => left.generation.val < right.generation.val) ∧
  (∀ event ∈ events, event.generation.val ≤ base + events.length)

def HistoryInvariant (base : Nat) (state : State) : Prop :=
  HistoryRows base state.db.family state.rotations

theorem history_append (base : Nat) (family : Option Family) (events : List RotationEvent)
    (event : RotationEvent)
    (ordered : events.Pairwise (fun left right => left.generation.val < right.generation.val))
    (bounded : ∀ old ∈ events, old.generation.val ≤ base + events.length)
    (familyGeneration : ∀ current, family = some current →
      current.current_generation.val = base + events.length + 1)
    (eventGeneration : event.generation.val = base + events.length + 1) :
    HistoryRows base family (events ++ [event]) := by
  refine ⟨?_, ?_, ?_⟩
  · intro current stored
    simpa [Nat.add_assoc] using familyGeneration current stored
  · rw [List.pairwise_append]
    refine ⟨ordered, by simp, ?_⟩
    intro left member right last
    have same : right = event := by simpa using last
    subst right
    have bound := bounded left member
    omega
  · intro candidate member
    rcases List.mem_append.mp member with old | last
    · have bound := bounded candidate old
      simp only [List.length_append, List.length_singleton]
      omega
    · have same : candidate = event := by simpa using last
      subst candidate
      simp only [List.length_append, List.length_singleton]
      omega

theorem initial_history (db : Database) (requests : Id → Request) (base : Nat)
    (generation : ∀ family, db.family = some family → family.current_generation.val = base) :
    HistoryInvariant base (initial db requests) := by
  simpa [HistoryInvariant, HistoryRows, initial] using generation

theorem history_step {before after : State} (step : Step before after) (base : Nat)
    (aligned : Alignment before) (recorded : Recorded before) (locked : Locked before)
    (history : HistoryInvariant base before) : HistoryInvariant base after := by
  rcases history with ⟨familyBefore, ordered, bounded⟩
  cases step
  case serverCommit id phase =>
    have chosen := recorded id (Or.inr (Or.inr (Or.inl phase)))
    rcases chosen with ⟨family, token, familyRead, tokenRead, program, checked⟩
    have present : before.db.family = some family :=
      ((locked id (by simp [OwnsFamily, phase])).2).symm.trans familyRead
    have alignedId := aligned id
    have done := alignedId.2.2.1 (Or.inr (Or.inl phase))
    have complete : (before.transactions id).staged = (before.transactions id).program := by
      simpa only [done, List.append_nil] using alignedId.2.1 (Or.inr (Or.inr (Or.inl phase)))
    have projected := commit_program_generation before.db (before.transactions id).request family token
      (before.transactions id).outcome present
    rw [← program, ← complete] at projected
    have oldGeneration := familyBefore family present
    cases outcome : (before.transactions id).outcome with
    | rotated successor =>
      have facts := checked successor outcome
      have next : successor.generation.val = token.generation.val + 1 :=
        RefreshPredicateProof.next_generation_arithmetic token successor facts.2.2.2
      have newGeneration : successor.generation.val = base + before.rotations.length + 1 := by
        rw [next, facts.2.2.1, oldGeneration]
      simp only [generationFor, outcome] at projected
      simp only [HistoryInvariant, tokenRead]
      change HistoryRows base (commit before.db (before.transactions id).staged).family
        (before.rotations ++ [⟨id, token.id, successor.id, successor.generation⟩])
      apply history_append base _ _ _ ordered bounded _ newGeneration
      intro current stored
      rw [stored] at projected
      have same : current.current_generation = successor.generation := by
        simpa only [Option.map_some, Option.some.injEq] using projected
      rw [same, newGeneration]
    | duplicate successor =>
      simp only [generationFor, outcome] at projected
      simp only [HistoryInvariant, tokenRead]
      change HistoryRows base (commit before.db (before.transactions id).staged).family before.rotations
      refine ⟨?_, ordered, bounded⟩
      intro current stored
      rw [stored] at projected
      have same : current.current_generation = family.current_generation := by
        simpa only [Option.map_some, Option.some.injEq] using projected
      rw [same, oldGeneration]
    | replay =>
      simp only [generationFor, outcome] at projected
      simp only [HistoryInvariant, tokenRead]
      change HistoryRows base (commit before.db (before.transactions id).staged).family before.rotations
      refine ⟨?_, ordered, bounded⟩
      intro current stored
      rw [stored] at projected
      have same : current.current_generation = family.current_generation := by
        simpa only [Option.map_some, Option.some.injEq] using projected
      rw [same, oldGeneration]
    | revoked result =>
      simp only [generationFor, outcome] at projected
      simp only [HistoryInvariant, tokenRead]
      change HistoryRows base (commit before.db (before.transactions id).staged).family before.rotations
      refine ⟨?_, ordered, bounded⟩
      intro current stored
      rw [stored] at projected
      have same : current.current_generation = family.current_generation := by
        simpa only [Option.map_some, Option.some.injEq] using projected
      rw [same, oldGeneration]
    | invalid =>
      simp only [generationFor, outcome] at projected
      simp only [HistoryInvariant, tokenRead]
      change HistoryRows base (commit before.db (before.transactions id).staged).family before.rotations
      refine ⟨?_, ordered, bounded⟩
      intro current stored
      rw [stored] at projected
      have same : current.current_generation = family.current_generation := by
        simpa only [Option.map_some, Option.some.injEq] using projected
      rw [same, oldGeneration]
  case pruneFamily family now present expired free tokensFree =>
    exact ⟨by simp, ordered, bounded⟩
  all_goals exact ⟨familyBefore, ordered, bounded⟩

theorem execution_history (db : Database) (requests : Id → Request) (base : Nat) (state : State)
    (generation : ∀ family, db.family = some family → family.current_generation.val = base)
    (execution : Execution (initial db requests) state) : HistoryInvariant base state := by
  induction execution with
  | refl => exact initial_history db requests base generation
  | tail execution step ih =>
    exact history_step step base
      (execution_alignment execution (initial_alignment db requests))
      (execution_recorded execution (initial_recorded db requests))
      (execution_locked execution (initial_locked db requests)) ih

/-- Distinct committed rotations have strictly increasing generations within this
retained family incarnation. UUID reuse cannot invalidate this statement. -/
theorem rotations_strictly_increase (db : Database) (requests : Id → Request) (base : Nat) (state : State)
    (generation : ∀ family, db.family = some family → family.current_generation.val = base)
    (execution : Execution (initial db requests) state) :
    state.rotations.Pairwise (fun left right => left.generation.val < right.generation.val) :=
  (execution_history db requests base state generation execution).2.1

end
end RefreshTransaction

#print axioms RefreshTransaction.execution_history
#print axioms RefreshTransaction.rotations_strictly_increase
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.execution_history,
      ``RefreshTransaction.rotations_strictly_increase] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh no-fork axiom in {declaration}: {found}"
