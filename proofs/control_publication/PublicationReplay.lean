import PublicationHistory

/- Ghost replay of the committed event list in the mathematical model only.
There is no corresponding runtime journal or second publication implementation. -/
namespace ControlPublication

theorem plans_preserved_step {before after : State} (step : Step before after) (subject : Id) :
    (after.transactions subject).plan = (before.transactions subject).plan := by
  cases step <;>
    simp only [updateTransaction, put] <;>
    try split_ifs
  all_goals simp_all

theorem plans_preserved (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (subject : Id) :
    (state.transactions subject).plan = plans subject := by
  induction execution with
  | refl => rfl
  | tail previous step ih => exact (plans_preserved_step step subject).trans ih

theorem owned_commit (db : Database) (writes : List Write) :
    owned (commit db writes) = commit (owned db) writes := by
  induction writes generalizing db with
  | nil => rfl
  | cons write writes ih =>
    simpa only [commit, applyWrites, List.foldl_cons, owned_applyWrite] using ih (applyWrite db write)

def replay (db : Database) (plans : Id → Plan) (events : List CommitEvent) : Database :=
  events.foldl (fun current event => commit current (plans event.publisher).writes) (owned db)

theorem owned_replay_step (db : Database) (plans : Id → Plan) (before after : State)
    (previous : Execution (initial db plans) before) (step : Step before after)
    (replayed : owned before.db = replay db plans before.commits) :
    owned after.db = replay db plans after.commits := by
  cases step
  case serverCommit id phase =>
    have complete := committed_stages_complete db plans before previous id phase
    have plan := plans_preserved db plans before previous id
    change owned (commit before.db (before.transactions id).staged) =
      replay db plans (before.commits ++ [CommitEvent.mk id
        (before.transactions id).plan.expected (before.transactions id).plan.revision])
    simp only [replay, List.foldl_append, List.foldl_cons, List.foldl_nil]
    rw [owned_commit, complete, plan]
    exact congrArg (fun current => commit current (plans id).writes) replayed
  all_goals simpa only [owned, updateTransaction] using replayed

/-- Every tracked committed row is explained by the ordered committed publisher
plans. Unrelated/rejection projections are erased only for this ghost equality;
the actual primitive commit retains them, as commit_frame proves. -/
theorem reachable_owned_replay (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) :
    owned state.db = replay db plans state.commits := by
  induction execution with
  | refl => rfl
  | @tail middle last previous step ih => exact owned_replay_step db plans middle last previous step ih

/-- An aborted attempt contributes no committed publication write or successful
batch: the whole tracked projection is replayable using events that all exclude
that attempt. Another publisher may commit, and pre-existing identical audit rows
or independent rejection audits may remain. No global unchanged-DB claim is made. -/
theorem aborted_publication_has_no_committed_contribution
    (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (aborted : (state.transactions id).phase = .aborted) :
    ∃ events, owned state.db = replay db plans events ∧
      ∀ event ∈ events, event.publisher ≠ id := by
  exact ⟨state.commits, reachable_owned_replay db plans state execution,
    aborted_attempt_has_no_commit db plans state execution id aborted⟩

#print axioms plans_preserved
#print axioms reachable_owned_replay
#print axioms aborted_publication_has_no_committed_contribution

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``plans_preserved, ``reachable_owned_replay,
      ``aborted_publication_has_no_committed_contribution] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication replay axiom in {declaration}: {found}"

end ControlPublication
