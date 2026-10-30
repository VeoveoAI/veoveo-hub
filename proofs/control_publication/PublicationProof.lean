import PublicationModel
import Lean.Util.CollectAxioms

namespace ControlPublication

@[simp] theorem put_same {α : Type} (rows : Id → α) (id : Id) (row : α) :
    put rows id row id = row := by simp [put]

@[simp] theorem put_other {α : Type} (rows : Id → α) (id other : Id) (row : α)
    (different : other ≠ id) : put rows id row other = rows other := by
  simp [put, different]

theorem applyWrites_append (db : Database) (left right : List Write) :
    applyWrites db (left ++ right) = applyWrites (applyWrites db left) right := by
  exact List.foldl_append

theorem grant_rows_exact (db : Database) (id : Id) (rows : List Payload) :
    applyWrites db (rows.map (.grant id)) =
      { db with grants := put db.grants id (db.grants id ++ rows) } := by
  induction rows generalizing db with
  | nil =>
    apply Database.ext <;> try rfl
    funext key
    by_cases same : key = id <;> simp [applyWrites, put, same]
  | cons row rows ih =>
    change applyWrites (applyWrite db (.grant id row)) (rows.map (.grant id)) = _
    rw [ih]
    ext key <;> by_cases same : key = id <;>
      simp [applyWrite, put, same, List.append_assoc]

theorem membership_rows_exact (db : Database) (id : Id) (rows : List Payload) :
    applyWrites db (rows.map (.membership id)) =
      { db with memberships := put db.memberships id (db.memberships id ++ rows) } := by
  induction rows generalizing db with
  | nil =>
    apply Database.ext <;> try rfl
    funext key
    by_cases same : key = id <;> simp [applyWrites, put, same]
  | cons row rows ih =>
    change applyWrites (applyWrite db (.membership id row)) (rows.map (.membership id)) = _
    rw [ih]
    ext key <;> by_cases same : key = id <;>
      simp [applyWrite, put, same, List.append_assoc]

/-- Each clear/insert loop yields the full supplied ordinal row list, including
empty lists. The program is not allowed to replace this proof by one atomic
"complete context" primitive. -/
theorem context_rows_exact (db : Database) (context : ContextRows) :
    applyWrites db (contextWrites context) = replaceContext db context := by
  simp only [contextWrites, applyWrites_append]
  rw [membership_rows_exact, grant_rows_exact]
  ext key <;> by_cases same : key = context.id <;>
    simp [applyWrites, applyWrite, replaceContext, put, same]

theorem context_list_exact (db : Database) (contexts : List ContextRows) :
    applyWrites db (contexts.flatMap contextWrites) = contexts.foldl replaceContext db := by
  induction contexts generalizing db with
  | nil => rfl
  | cons context contexts ih =>
    simp only [List.flatMap_cons, applyWrites_append, List.foldl_cons]
    rw [context_rows_exact, ih]

theorem context_plan_exact (db : Database) (plan : Plan) :
    applyWrites db plan.contextWrites = expectedContexts db plan := by
  simp only [Plan.contextWrites, applyWrites_append]
  rw [context_list_exact]
  cases empty : plan.contexts.isEmpty <;>
    simp [empty, applyWrites, applyWrite, expectedContexts]

theorem expectedContexts_frame (db : Database) (plan : Plan) :
    (expectedContexts db plan).revisions = db.revisions ∧
    (expectedContexts db plan).objects = db.objects ∧
    (expectedContexts db plan).active = db.active ∧
    (expectedContexts db plan).audit = db.audit ∧
    (expectedContexts db plan).unrelated = db.unrelated ∧
    (expectedContexts db plan).rejectionAudit = db.rejectionAudit := by
  have frame (contexts : List ContextRows) (db : Database) :
      (contexts.foldl replaceContext db).revisions = db.revisions ∧
      (contexts.foldl replaceContext db).objects = db.objects ∧
      (contexts.foldl replaceContext db).active = db.active ∧
      (contexts.foldl replaceContext db).audit = db.audit ∧
      (contexts.foldl replaceContext db).unrelated = db.unrelated ∧
      (contexts.foldl replaceContext db).rejectionAudit = db.rejectionAudit := by
    induction contexts generalizing db with
    | nil => simp
    | cons context contexts ih =>
      simpa only [List.foldl_cons, replaceContext] using ih (replaceContext db context)
  unfold expectedContexts
  split <;> simpa only using frame plan.contexts _

/-- No publication statement, and hence no primitive commit, can replace an
unrelated row or independently appended rejection audit. This is a delta model,
not a commit of an old whole-database snapshot. -/
theorem commit_frame (db : Database) (staged : List Write) :
    (commit db staged).unrelated = db.unrelated ∧
    (commit db staged).rejectionAudit = db.rejectionAudit := by
  induction staged generalizing db with
  | nil => exact ⟨rfl, rfl⟩
  | cons write staged ih =>
    have frame := ih (applyWrite db write)
    cases write <;> simpa only [commit, applyWrites, List.foldl_cons, applyWrite] using frame

/-- Complete row after-image, derived by expanding the ordered SQL effects and
proving the context clear/insert loops above. Commit itself accepts partial logs. -/
theorem publication_commit_exact (db : Database) (plan : Plan) :
    commit db plan.writes = expectedPublication db plan := by
  simp only [commit, Plan.writes, applyWrites_append]
  rw [context_plan_exact]
  have auditFrame := (expectedContexts_frame
    { db with
      revisions := put db.revisions plan.revision (some plan.document)
      objects := put db.objects plan.revision plan.objects } plan).2.2.2.1
  simp only [applyWrites, List.foldl_cons, List.foldl_nil, applyWrite,
    expectedPublication, auditFrame]

/-- The exact fields that the application's program counter has checked. No
such invariant is imposed on the database's unrestricted commit primitive. -/
def Aligned (transaction : Transaction) : Prop :=
  ((transaction.phase = .idle ∨ transaction.phase = .begun ∨ transaction.phase = .locked) →
    transaction.staged = [] ∧ transaction.remaining = []) ∧
  (CheckedPhase transaction.phase →
    transaction.staged ++ transaction.remaining = transaction.plan.writes) ∧
  ((transaction.phase = .ready ∨ transaction.phase = .committing) → transaction.remaining = []) ∧
  (transaction.response = .success → transaction.phase = .committed)

def Alignment (state : State) : Prop := ∀ id, Aligned (state.transactions id)

theorem initial_alignment (db : Database) (plans : Id → Plan) :
    Alignment (initial db plans) := by
  intro id
  simp [Aligned, initial, CheckedPhase]

theorem alignment_step {before after : State} (step : Step before after)
    (aligned : Alignment before) : Alignment after := by
  intro subject
  have atSubject := aligned subject
  clear aligned
  cases step <;>
    simp only [updateTransaction, put] at * <;>
    try split_ifs at *
  all_goals
    simp_all [Aligned, CheckedPhase, PrecommitPhase,
      List.append_assoc]
  all_goals
    first
    | simpa only [atSubject.2.1, List.append_nil] using atSubject.1
    | simpa only [atSubject.2, List.append_nil] using atSubject.1

theorem execution_alignment {before after : State} (execution : Execution before after)
    (aligned : Alignment before) : Alignment after := by
  induction execution with
  | refl => exact aligned
  | tail execution step ih => exact alignment_step step ih

/-- Arbitrary unrelated actions and publisher interleavings cannot cause a
protocol commit to skip any staged statement. Database atomicity alone would
not give this theorem. -/
theorem committed_stages_complete (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (ready : (state.transactions id).phase = .committing) :
    (state.transactions id).staged = (state.transactions id).plan.writes := by
  have aligned := execution_alignment execution (initial_alignment db plans) id
  have done := aligned.2.2.1 (Or.inr ready)
  simpa only [done, List.append_nil] using aligned.2.1 (Or.inr (Or.inr ready))

theorem acknowledged_commit (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (success : (state.transactions id).response = .success) :
    (state.transactions id).phase = .committed := by
  exact (execution_alignment execution (initial_alignment db plans) id).2.2.2 success

/-- The lock protects the checked active value, not ordinary multi-query readers. -/
def Locked (state : State) : Prop :=
  ∀ id, ActivePhase (state.transactions id).phase →
    state.lock = some id ∧ (state.transactions id).observed = state.db.active

def Compared (state : State) : Prop :=
  ∀ id, CheckedPhase (state.transactions id).phase →
    (state.transactions id).observed = (state.transactions id).plan.expected

theorem initial_locked (db : Database) (plans : Id → Plan) : Locked (initial db plans) := by
  intro id active
  simp [initial, ActivePhase] at active

theorem initial_compared (db : Database) (plans : Id → Plan) : Compared (initial db plans) := by
  intro id checked
  simp [initial, CheckedPhase] at checked

theorem locked_step {before after : State} (step : Step before after)
    (locked : Locked before) : Locked after := by
  intro subject active
  have atSubject := locked subject
  cases step
  case serverCommit id phase =>
    by_cases same : subject = id
    · simp [updateTransaction, put, same, ActivePhase] at active
    · have wasActive : ActivePhase (before.transactions subject).phase := by
        simpa [updateTransaction, put, same] using active
      have subjectOwner := (locked subject wasActive).1
      have committingOwner := (locked id (Or.inr (Or.inr (Or.inr phase)))).1
      have equal : subject = id := Option.some.inj (subjectOwner.symm.trans committingOwner)
      exact False.elim (same equal)
  all_goals clear locked
  all_goals
    simp only [updateTransaction, put] at * <;>
    try split_ifs at *
  all_goals
    simp_all [ActivePhase, PrecommitPhase, release]

theorem compared_step {before after : State} (step : Step before after)
    (compared : Compared before) : Compared after := by
  intro subject checked
  have atSubject := compared subject
  clear compared
  cases step <;>
    simp only [updateTransaction, put] at * <;>
    try split_ifs at *
  all_goals simp_all [CheckedPhase, PrecommitPhase]

theorem execution_locked {before after : State} (execution : Execution before after)
    (locked : Locked before) : Locked after := by
  induction execution with
  | refl => exact locked
  | tail execution step ih => exact locked_step step ih

theorem execution_compared {before after : State} (execution : Execution before after)
    (compared : Compared before) : Compared after := by
  induction execution with
  | refl => exact compared
  | tail execution step ih => exact compared_step step ih

theorem committed_predecessor_checked (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (ready : (state.transactions id).phase = .committing) :
    state.lock = some id ∧ state.db.active = (state.transactions id).plan.expected := by
  have locked := execution_locked execution (initial_locked db plans) id
    (Or.inr (Or.inr (Or.inr ready)))
  have compared := execution_compared execution (initial_compared db plans) id
    (Or.inr (Or.inr ready))
  exact ⟨locked.1, locked.2.symm.trans compared⟩

theorem protocol_commit_complete (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (ready : (state.transactions id).phase = .committing) :
    commit state.db (state.transactions id).staged =
      expectedPublication state.db (state.transactions id).plan := by
  rw [committed_stages_complete db plans state execution id ready, publication_commit_exact]

/-- Server rollback discards the local delta without changing committed rows.
Other transactions may already have committed; this is not a global trace-wide
unchanged-state assertion. The abort request itself retains the lock. -/
theorem server_abort_no_contribution (state : State) (id : Id) :
    ({ updateTransaction state id
        { state.transactions id with phase := .aborted, staged := [], remaining := [] } with
        lock := release state.lock id } : State).db = state.db := rfl

#print axioms context_rows_exact
#print axioms context_plan_exact
#print axioms commit_frame
#print axioms publication_commit_exact
#print axioms committed_stages_complete
#print axioms acknowledged_commit
#print axioms committed_predecessor_checked
#print axioms protocol_commit_complete
#print axioms server_abort_no_contribution

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``context_rows_exact, ``context_plan_exact, ``commit_frame, ``publication_commit_exact,
      ``committed_stages_complete, ``acknowledged_commit, ``committed_predecessor_checked,
      ``protocol_commit_complete, ``server_abort_no_contribution] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication proof axiom in {declaration}: {found}"

end ControlPublication
