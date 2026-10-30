import PublicationProof

/- Next proof increment, deliberately not wired into the first frozen checkpoint.
No scheduler/commit/CAS oracle or new database operation is introduced here. -/
namespace ControlPublication

theorem validBatch_preserves_stored (rows : Id → Option Payload) (batch : List AuditRow)
    (valid : ValidBatch rows batch) (id : Id) (content : Payload)
    (stored : rows id = some content) : appendAudit rows batch id = some content := by
  induction batch generalizing rows with
  | nil => exact stored
  | cons row batch ih =>
    change appendAudit (put rows row.id (some row.content)) batch id = some content
    apply ih _ valid.2
    by_cases same : id = row.id
    · subst id
      rcases valid.1 with absent | identical
      · simp_all
      · simp_all [put]
    · simp [put, same, stored]

theorem validBatch_supplied_present (rows : Id → Option Payload) (batch : List AuditRow)
    (valid : ValidBatch rows batch) (row : AuditRow) (member : row ∈ batch) :
    appendAudit rows batch row.id = some row.content := by
  induction batch generalizing rows with
  | nil => simp at member
  | cons head batch ih =>
    change appendAudit (put rows head.id (some head.content)) batch row.id = some row.content
    rcases List.mem_cons.mp member with rfl | later
    · exact validBatch_preserves_stored _ batch valid.2 row.id row.content (by simp [put])
    · exact ih _ valid.2 later

theorem batch_already_stored_is_idempotent (rows : Id → Option Payload) (batch : List AuditRow)
    (stored : ∀ row ∈ batch, rows row.id = some row.content) : appendAudit rows batch = rows := by
  induction batch with
  | nil => rfl
  | cons row batch ih =>
    have same : put rows row.id (some row.content) = rows := by
      funext id
      by_cases same : id = row.id
      · subst id
        simpa [put] using (stored row (by simp)).symm
      · simp [put, same]
    change appendAudit (put rows row.id (some row.content)) batch = rows
    rw [same]
    exact ih (fun row member => stored row (List.mem_cons_of_mem _ member))

def ValidWrites (db : Database) : List Write → Prop
  | [] => True
  | write :: rest => ValidWrite db write ∧ ValidWrites (applyWrite db write) rest

theorem validWrites_append (db : Database) (left right : List Write) :
    ValidWrites db (left ++ right) ↔
      ValidWrites db left ∧ ValidWrites (applyWrites db left) right := by
  induction left generalizing db with
  | nil => simp [ValidWrites, applyWrites]
  | cons write rest ih =>
    simp only [List.cons_append, ValidWrites, applyWrites, List.foldl_cons]
    rw [ih]
    tauto

/-- Strip the explicitly disjoint environment projections when comparing
publication-owned rows; this is not a snapshot used by primitive commit. -/
def owned (db : Database) : Database :=
  { db with unrelated := fun _ => none, rejectionAudit := fun _ => none }

theorem owned_applyWrite (db : Database) (write : Write) :
    owned (applyWrite db write) = applyWrite (owned db) write := by
  cases write <;> rfl

theorem validWrite_owned (db : Database) (write : Write) :
    ValidWrite (owned db) write ↔ ValidWrite db write := by
  cases write <;> rfl

theorem validWrites_owned (db : Database) (writes : List Write) :
    ValidWrites (owned db) writes ↔ ValidWrites db writes := by
  induction writes generalizing db with
  | nil => rfl
  | cons write rest ih =>
    simp only [ValidWrites, validWrite_owned, ← owned_applyWrite, ih]

@[simp] theorem validWrites_unrelated (db : Database) (writes : List Write)
    (rows : Id → Option Payload) :
    ValidWrites { db with unrelated := rows } writes ↔ ValidWrites db writes := by
  rw [← validWrites_owned, ← validWrites_owned db]
  rfl

@[simp] theorem validWrites_rejection (db : Database) (writes : List Write)
    (rows : Id → Option Payload) :
    ValidWrites { db with rejectionAudit := rows } writes ↔ ValidWrites db writes := by
  rw [← validWrites_owned, ← validWrites_owned db]
  rfl

theorem revision_rows_never_deleted (db : Database) (writes : List Write) (id : Id)
    (present : db.revisions id ≠ none) :
    (applyWrites db writes).revisions id ≠ none := by
  induction writes generalizing db with
  | nil => exact present
  | cons write writes ih =>
    apply ih (applyWrite db write)
    cases write <;> simp_all [applyWrite, put]
    all_goals split_ifs <;> simp_all

def StagedValid (state : State) : Prop :=
  ∀ id, CheckedPhase (state.transactions id).phase →
    ValidWrites state.db (state.transactions id).staged

theorem initial_staged_valid (db : Database) (plans : Id → Plan) :
    StagedValid (initial db plans) := by
  intro id checked
  simp [initial, CheckedPhase] at checked

theorem staged_valid_step {before after : State} (step : Step before after)
    (aligned : Alignment before) (locked : Locked before) (staged : StagedValid before) :
    StagedValid after := by
  intro subject checked
  have alignedSubject := aligned subject
  have validSubject := staged subject
  cases step
  case serverCommit id phase =>
    by_cases same : subject = id
    · simp [updateTransaction, put, same, CheckedPhase] at checked
    · have previous : CheckedPhase (before.transactions subject).phase := by
        simpa [updateTransaction, put, same] using checked
      have subjectOwner := (locked subject (Or.inr previous)).1
      have committingOwner := (locked id (Or.inr (Or.inr (Or.inr phase)))).1
      have equal : subject = id := Option.some.inj (subjectOwner.symm.trans committingOwner)
      exact False.elim (same equal)
  all_goals clear aligned locked staged
  all_goals
    simp only [updateTransaction, put] at * <;>
    try split_ifs at *
  all_goals
    simp_all [StagedValid, Alignment, Aligned, Locked, ActivePhase, CheckedPhase,
      PrecommitPhase, ValidWrites, validWrites_append]
  all_goals aesop

theorem execution_staged_valid (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) : StagedValid state := by
  induction execution with
  | refl => exact initial_staged_valid db plans
  | @tail middle last previous step ih =>
    exact staged_valid_step step
      (execution_alignment previous (initial_alignment db plans))
      (execution_locked previous (initial_locked db plans)) ih

theorem complete_log_checks_revision_key (db : Database) (plan : Plan)
    (valid : ValidWrites db plan.writes) : db.revisions plan.revision = none := by
  exact valid.1

/-- Each committed event must be reflected in the publisher's terminal state.
Transactions are not reused: a new attempt receives a fresh model publisher ID.
The revision itself is *not* assumed fresh; its PK is checked by ValidWrite. -/
def CommitTerminal (state : State) : Prop :=
  ∀ event ∈ state.commits, (state.transactions event.publisher).phase = .committed

theorem commit_terminal_step {before after : State} (step : Step before after)
    (terminal : CommitTerminal before) : CommitTerminal after := by
  intro event member
  have eventTerminal := terminal event
  cases step <;>
    simp only [updateTransaction, put] at * <;>
    try split_ifs at *
  all_goals
    simp_all [CommitTerminal, List.mem_append, ActivePhase, PrecommitPhase]
  all_goals aesop

theorem execution_commit_terminal (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) : CommitTerminal state := by
  induction execution with
  | refl => simp [CommitTerminal, initial]
  | tail previous step ih => exact commit_terminal_step step ih

/-- Reachable abort is absent from the actual commit-event history. This goes
beyond the local serverAbort database-projection rfl helper. It permits other
publishers to commit and independent endpoint rejection audits to exist. -/
theorem aborted_attempt_has_no_commit (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (aborted : (state.transactions id).phase = .aborted) :
    ∀ event ∈ state.commits, event.publisher ≠ id := by
  intro event member same
  have committed := execution_commit_terminal db plans state execution event member
  rw [same, aborted] at committed
  cases committed

theorem protocol_commit_revision_is_fresh (db : Database) (plans : Id → Plan) (state : State)
    (execution : Execution (initial db plans) state) (id : Id)
    (ready : (state.transactions id).phase = .committing) :
    state.db.revisions (state.transactions id).plan.revision = none := by
  have valid := execution_staged_valid db plans state execution id (Or.inr (Or.inr ready))
  rw [committed_stages_complete db plans state execution id ready] at valid
  exact complete_log_checks_revision_key state.db (state.transactions id).plan valid

theorem complete_commit_revision_rows (db : Database) (plan : Plan) :
    (commit db plan.writes).revisions = put db.revisions plan.revision (some plan.document) := by
  rw [publication_commit_exact]
  exact (expectedContexts_frame
    { db with
      revisions := put db.revisions plan.revision (some plan.document)
      objects := put db.objects plan.revision plan.objects } plan).1

theorem complete_commit_active (db : Database) (plan : Plan) :
    (commit db plan.writes).active = some plan.revision := by
  rw [publication_commit_exact]
  rfl

def RetainedHistory (state : State) : Prop :=
  (∀ id, state.db.active = some id → state.db.revisions id ≠ none) ∧
  (state.commits.map CommitEvent.predecessor).Nodup ∧
  (∀ event ∈ state.commits,
    state.db.active ≠ event.predecessor ∧
    ∀ id, event.predecessor = some id → state.db.revisions id ≠ none)

theorem initial_retained_history (db : Database) (plans : Id → Plan)
    (activeFK : ∀ id, db.active = some id → db.revisions id ≠ none) :
    RetainedHistory (initial db plans) := by
  exact ⟨activeFK, by simp [initial], by simp [initial]⟩

theorem nodup_append_fresh {α : Type} (rows : List α) (row : α)
    (unique : rows.Nodup) (fresh : row ∉ rows) : (rows ++ [row]).Nodup := by
  induction rows with
  | nil => simp
  | cons head rows ih =>
    simp only [List.nodup_cons] at unique
    simp only [List.mem_cons, not_or] at fresh
    simp only [List.cons_append, List.nodup_cons, List.mem_append, List.mem_singleton, not_or]
    exact ⟨⟨unique.1, Ne.symm fresh.1⟩, ih unique.2 fresh.2⟩

def committedState (state : State) (id : Id) : State :=
  { updateTransaction state id { state.transactions id with phase := .committed } with
    db := commit state.db (state.transactions id).staged
    lock := release state.lock id
    commits := state.commits ++ [⟨id, (state.transactions id).plan.expected,
      (state.transactions id).plan.revision⟩] }

theorem retained_history_commit (state : State) (id : Id) (history : RetainedHistory state)
    (complete : (state.transactions id).staged = (state.transactions id).plan.writes)
    (checked : state.db.active = (state.transactions id).plan.expected)
    (fresh : state.db.revisions (state.transactions id).plan.revision = none) :
    RetainedHistory (committedState state id) := by
  let plan := (state.transactions id).plan
  have revisions : (committedState state id).db.revisions =
      put state.db.revisions plan.revision (some plan.document) := by
    simpa only [committedState, complete, plan] using complete_commit_revision_rows state.db plan
  have active : (committedState state id).db.active = some plan.revision := by
    simpa only [committedState, complete, plan] using complete_commit_active state.db plan
  have retains (key : Id) (present : state.db.revisions key ≠ none) :
      (committedState state id).db.revisions key ≠ none := by
    rw [revisions]
    by_cases same : key = plan.revision <;> simp_all [put]
  have noParent (event : CommitEvent) (member : event ∈ state.commits) :
      some plan.revision ≠ event.predecessor := by
    intro same
    exact (history.2.2 event member).2 plan.revision same.symm fresh
  have newParent : plan.expected ∉ state.commits.map CommitEvent.predecessor := by
    rintro member
    rcases List.mem_map.mp member with ⟨event, member, same⟩
    exact (history.2.2 event member).1 (checked.trans same.symm)
  have distinct : some plan.revision ≠ plan.expected := by
    intro same
    exact history.1 plan.revision (checked.trans same.symm) fresh
  refine ⟨?_, ?_, ?_⟩
  · intro key hit
    rw [active] at hit
    have same := Option.some.inj hit
    rw [← same, revisions]
    simp [put]
  · change ((state.commits ++ [CommitEvent.mk id plan.expected plan.revision]).map CommitEvent.predecessor).Nodup
    simpa only [List.map_append, List.map_cons, List.map_nil] using
      nodup_append_fresh (state.commits.map CommitEvent.predecessor) plan.expected history.2.1 newParent
  · intro event member
    change event ∈ state.commits ++ [CommitEvent.mk id plan.expected plan.revision] at member
    rcases List.mem_append.mp member with old | added
    · refine ⟨?_, ?_⟩
      · rw [active]
        exact noParent event old
      · intro key parent
        exact retains key ((history.2.2 event old).2 key parent)
    · simp only [List.mem_singleton] at added
      subst event
      refine ⟨?_, ?_⟩
      · rw [active]
        exact distinct
      · intro key parent
        exact retains key (history.1 key (checked.trans parent))

theorem retained_history_step {before after : State} (step : Step before after)
    (aligned : Alignment before) (locked : Locked before) (compared : Compared before)
    (valid : StagedValid before) (history : RetainedHistory before) : RetainedHistory after := by
  cases step
  case serverCommit id phase =>
    have checkedPhase : CheckedPhase (before.transactions id).phase := Or.inr (Or.inr phase)
    have complete : (before.transactions id).staged = (before.transactions id).plan.writes := by
      have log := (aligned id).2.1 checkedPhase
      simpa only [(aligned id).2.2.1 (Or.inr phase), List.append_nil] using log
    have owner := locked id (Or.inr (Or.inr (Or.inr phase)))
    have checked := owner.2.symm.trans (compared id checkedPhase)
    have validLog := valid id checkedPhase
    rw [complete] at validLog
    exact retained_history_commit before id history complete checked
      (complete_log_checks_revision_key before.db _ validLog)
  all_goals simpa only [RetainedHistory, updateTransaction] using history

theorem execution_retained_history (db : Database) (plans : Id → Plan) (state : State)
    (activeFK : ∀ id, db.active = some id → db.revisions id ≠ none)
    (execution : Execution (initial db plans) state) : RetainedHistory state := by
  induction execution with
  | refl => exact initial_retained_history db plans activeFK
  | @tail middle last previous step ih =>
    exact retained_history_step step
      (execution_alignment previous (initial_alignment db plans))
      (execution_locked previous (initial_locked db plans))
      (execution_compared previous (initial_compared db plans))
      (execution_staged_valid db plans middle previous) ih

/-- A list whose mapped predecessor IDs are distinct has a unique event for
each predecessor, even when arbitrary many publishers are scheduled. -/
theorem same_predecessor_event_unique (events : List CommitEvent)
    (unique : (events.map CommitEvent.predecessor).Nodup)
    (left right : CommitEvent) (leftIn : left ∈ events) (rightIn : right ∈ events)
    (same : left.predecessor = right.predecessor) : left = right := by
  induction events with
  | nil => simp at leftIn
  | cons head tail ih =>
    simp only [List.map_cons, List.nodup_cons] at unique
    simp only [List.mem_cons] at leftIn rightIn
    rcases leftIn with rfl | leftIn
    · rcases rightIn with rfl | rightIn
      · rfl
      · exact False.elim (unique.1 (by
          rw [List.mem_map]
          exact ⟨right, rightIn, same.symm⟩))
    · rcases rightIn with rfl | rightIn
      · exact False.elim (unique.1 (by
          rw [List.mem_map]
          exact ⟨left, leftIn, same⟩))
      · exact ih unique.2 leftIn rightIn

/-- Final public single-winner statement will obtain RetainedHistory from the
initial state and Step preservation, rather than accept it as a caller oracle.
This helper only records the finite-list consequence needed by that induction. -/
theorem history_single_winner (state : State) (history : RetainedHistory state)
    (left right : CommitEvent) (leftIn : left ∈ state.commits) (rightIn : right ∈ state.commits)
    (same : left.predecessor = right.predecessor) :
    left.publisher = right.publisher ∧ left.revision = right.revision := by
  have equal := same_predecessor_event_unique state.commits history.2.1 left right leftIn rightIn same
  cases equal
  exact ⟨rfl, rfl⟩

/-- Public single-winner theorem: no assumed overall publication oracle or
Nodup premise. It derives retained history from actual model transitions, the
locked CAS and the revision-insert primary-key check. -/
theorem same_base_single_winner (db : Database) (plans : Id → Plan) (state : State)
    (activeFK : ∀ id, db.active = some id → db.revisions id ≠ none)
    (execution : Execution (initial db plans) state)
    (left right : CommitEvent) (leftIn : left ∈ state.commits) (rightIn : right ∈ state.commits)
    (same : left.predecessor = right.predecessor) :
    left.publisher = right.publisher ∧ left.revision = right.revision := by
  exact history_single_winner state (execution_retained_history db plans state activeFK execution)
    left right leftIn rightIn same

#print axioms validBatch_preserves_stored
#print axioms validBatch_supplied_present
#print axioms batch_already_stored_is_idempotent
#print axioms aborted_attempt_has_no_commit
#print axioms protocol_commit_revision_is_fresh
#print axioms execution_retained_history
#print axioms same_base_single_winner

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``validBatch_preserves_stored, ``validBatch_supplied_present,
      ``batch_already_stored_is_idempotent, ``aborted_attempt_has_no_commit,
      ``protocol_commit_revision_is_fresh, ``execution_retained_history,
      ``same_base_single_winner] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun name => !allowed.contains name)).isEmpty do
      throwError "unreviewed publication history axiom in {declaration}: {found}"

end ControlPublication
