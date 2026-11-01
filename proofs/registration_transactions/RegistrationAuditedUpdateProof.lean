import RegistrationAuditedUpdateSpecification
import RegistrationAuditedLifetimeProof
import RegistrationUpdateHistory

namespace RegistrationAuditedUpdate
open RegistrationTransaction (put)
open RegistrationAuditedLifetime
variable {C L M B St D T R P F A S J U : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq A]
variable [DecidableEq M] [DecidableEq B] [DecidableEq D]
variable {methods : Nat → RegistrationUpdate.Method}
variable {accepted : RegistrationUpdate.MetadataAccepted M B}
variable {source : Source M S} {encoding : Encoding A S J U}

/-- Core append never replaces a different committed full record. The JSON draft
and stored timestamp are BOTH part of the equality supplied by CoreAppend. -/
theorem core_append_preserves (before after : Ledger A J U) (record : Record J U)
    (append : CoreAppend encoding before record after) (id : A) (previous : Record J U)
    (stored : before id = some previous) : after id = some previous := by
  rcases append with ⟨_, compatible, effect⟩
  rw [effect]
  by_cases same : id = encoding.id record.draft
  · subst id
    have identical : record = previous := by
      rcases compatible with absent | identical
      · rw [stored] at absent
        contradiction
      · exact Option.some.inj (identical.symm.trans stored)
    simp [put, identical]
  · simp [put, same, stored]

/-- Opaque external contributions are constrained to actual finite Core append
semantics, never arbitrary ledger replacement or a desired vocabulary program. -/
theorem batch_preserves (before after : Ledger A J U) (records : List (Record J U))
    (batch : AppendBatch encoding before records after) :
    (∀ id previous, before id = some previous → after id = some previous) ∧
      after = overlay encoding before records := by
  induction batch with
  | nil ledger => exact ⟨fun _ _ stored => stored, rfl⟩
  | @cons before middle after record remaining append rest ih =>
    constructor
    · intro id previous stored
      exact ih.1 id previous (core_append_preserves before middle record append id previous stored)
    · rw [ih.2, append.2.2]
      rfl

theorem overlay_preserves (before : Ledger A J U) (records : List (Record J U))
    (compatible : Compatible encoding before records) (id : A) (previous : Record J U)
    (stored : before id = some previous) :
    overlay encoding before records id = some previous := by
  have sameRecords : ∀ record ∈ records, encoding.id record.draft = id → record = previous := by
    intro record member same
    have matching := compatible record member
    rw [same, stored] at matching
    rcases matching with impossible | identical
    · contradiction
    · exact (Option.some.inj identical).symm
  clear compatible
  revert stored sameRecords
  induction records generalizing before with
  | nil =>
    intro stored _
    exact stored
  | cons record remaining ih =>
    intro stored sameRecords
    change overlay encoding (put before (encoding.id record.draft) (some record)) remaining id = _
    apply ih
    · by_cases same : id = encoding.id record.draft
      · have identical := sameRecords record (by simp) same.symm
        simp [put, same, identical]
      · simp [put, same, stored]
    · intro later member same
      exact sameRecords later (List.mem_cons_of_mem record member) same

/-- Explicit external disjointness protects only ACTIVE private IDs. -/
theorem batch_private_unchanged (s : State C L M B St D T R P F A S J U)
    (records : List (Record J U)) (nextAudit : Ledger A J U)
    (batch : AppendBatch encoding s.audit records nextAudit)
    (disjoint : DisjointPrivate encoding s records)
    (id : Nat) (current : Cursor S J U) (selected : s.cursors id = some current)
    (active : ActivePrivate (s.core.updates id).phase)
    (record : Record J U) (member : record ∈ current.acknowledged) :
    nextAudit (encoding.id record.draft) = s.audit (encoding.id record.draft) := by
  rw [(batch_preserves s.audit nextAudit records batch).2]
  apply RegistrationAuditedLifetime.overlay_unrelated
  intro included
  rcases List.mem_map.mp included with ⟨foreign, inRecords, same⟩
  exact disjoint id current selected active record member foreign inRecords same.symm

theorem data_trans {before middle after : RegistrationUpdate.State C L M B St D T R P F}
    (first : RegistrationUpdate.Execution true methods accepted before middle)
    (second : RegistrationUpdate.Execution true methods accepted middle after) :
    RegistrationUpdate.Execution true methods accepted before after := by
  induction second with
  | refl => exact first
  | tail previous step ih => exact .tail ih step

theorem step_erasure {before after : State C L M B St D T R P F A S J U}
    (step : Step methods accepted source encoding before after) :
    RegistrationUpdate.Execution true methods accepted before.core after.core := by
  cases step
  case ordinary s next base notCommit => exact .tail (.refl _) base
  case beginAudit => exact .refl _
  case append => exact .refl _
  case requestCommit => exact .tail (.refl _) (.requestCommit _ _ (by assumption))
  case serverCommit => exact .tail (.refl _) (.serverCommit _ _ (by assumption))
  case externalRegisterCommit s next id writes records nextAudit base event appends disjoint =>
    exact .tail (.refl _) base
  case foreignAppend => exact .refl _

theorem execution_erasure {before after : State C L M B St D T R P F A S J U}
    (execution : Execution methods accepted source encoding before after) :
    RegistrationUpdate.Execution true methods accepted before.core after.core := by
  induction execution with
  | refl => exact .refl _
  | tail previous step ih => exact data_trans ih (step_erasure step)

theorem progress_setCursor (s : State C L M B St D T R P F A S J U)
    (id : Nat) (current : Cursor S J U) (previous : Progress encoding s)
    (complete : current.acknowledged.map (recordIntent encoding) ++ current.remaining = current.program) :
    Progress encoding (setCursor s id current) := by
  intro subject observed selected
  by_cases same : subject = id
  · subst subject
    simp only [setCursor, put, if_pos rfl] at selected
    cases selected
    exact complete
  · exact previous subject observed (by simpa [setCursor, put, same] using selected)

theorem progress_step {before after : State C L M B St D T R P F A S J U}
    (step : Step methods accepted source encoding before after) (previous : Progress encoding before) :
    Progress encoding after := by
  cases step
  case beginAudit s id ready unused =>
    apply progress_setCursor _ _ _ previous
    simp [cursor]
  case append s id current wanted remaining record visibleAfter ready selected head constructed append =>
    apply progress_setCursor _ _ _ previous
    have complete := previous id current selected
    simpa [advance, List.map_append, List.append_assoc, constructed, head] using complete
  all_goals exact previous

theorem execution_progress {before after : State C L M B St D T R P F A S J U}
    (execution : Execution methods accepted source encoding before after) (previous : Progress encoding before) :
    Progress encoding after := by
  induction execution with
  | refl => exact previous
  | tail previous step ih => exact progress_step step ih

theorem binding_core (s : State C L M B St D T R P F A S J U)
    (next : RegistrationUpdate.State C L M B St D T R P F)
    (base : RegistrationUpdate.Step true methods accepted s.core next)
    (previous : SourceBinding source s) : SourceBinding source { s with core := next } := by
  intro id current selected
  have original := RegistrationUpdate.update_request_step methods accepted base id
  simpa only [original] using previous id current selected

theorem binding_setCursor (s : State C L M B St D T R P F A S J U)
    (id : Nat) (current : Cursor S J U) (previous : SourceBinding source s)
    (bound : current.program = sourceProgram source (s.core.updates id).request) :
    SourceBinding source (setCursor s id current) := by
  intro subject observed selected
  by_cases same : subject = id
  · subst subject
    simp only [setCursor, put, if_pos rfl] at selected
    cases selected
    exact bound
  · exact previous subject observed (by simpa [setCursor, put, same] using selected)

theorem binding_step {before after : State C L M B St D T R P F A S J U}
    (step : Step methods accepted source encoding before after) (previous : SourceBinding source before) :
    SourceBinding source after := by
  cases step
  case ordinary next base notCommit => exact binding_core before next base previous
  case beginAudit id ready unused => exact binding_setCursor before id _ previous rfl
  case append id current wanted remaining record visibleAfter ready selected head constructed append =>
    exact binding_setCursor before id _ previous (previous id current selected)
  case requestCommit =>
    exact binding_core (methods := methods) (accepted := accepted) _ _
      (RegistrationUpdate.Step.requestCommit (methods := methods) (accepted := accepted) _ _ (by assumption)) previous
  case serverCommit =>
    exact binding_core (methods := methods) (accepted := accepted) _ _
      (RegistrationUpdate.Step.serverCommit (methods := methods) (accepted := accepted) _ _ (by assumption)) previous
  case externalRegisterCommit next id writes records nextAudit base event appends disjoint =>
    exact binding_core before next base previous
  case foreignAppend => exact previous

theorem execution_binding {before after : State C L M B St D T R P F A S J U}
    (execution : Execution methods accepted source encoding before after) (previous : SourceBinding source before) :
    SourceBinding source after := by
  induction execution with
  | refl => exact previous
  | tail previous step ih => exact binding_step step ih

/-- A cursor can start only on the matched path; subsequent failures may retain
its ghost records, but cannot turn the attempt into an affected0 no-op. -/
def SettledCursor (tx : RegistrationUpdate.Transaction C L M B St D T R) : Prop :=
  (tx.phase = .ready ∨ tx.phase = .committing ∨ tx.phase = .committed ∨
    tx.phase = .rollbackPending ∨ tx.phase = .aborted) ∧ tx.outcome ≠ .noop

def CursorSettled (s : State C L M B St D T R P F A S J U) : Prop :=
  ∀ id current, s.cursors id = some current → SettledCursor (s.core.updates id)

theorem settled_data_step {before after : RegistrationUpdate.State C L M B St D T R P F}
    (base : RegistrationUpdate.Step true methods accepted before after) (id : Nat)
    (previous : SettledCursor (before.updates id)) : SettledCursor (after.updates id) := by
  cases base <;>
    simp only [RegistrationUpdate.updateTx, RegistrationUpdate.committedState, put] <;>
    try split_ifs
  all_goals simp_all [SettledCursor]

theorem settled_core (s : State C L M B St D T R P F A S J U)
    (next : RegistrationUpdate.State C L M B St D T R P F)
    (base : RegistrationUpdate.Step true methods accepted s.core next)
    (previous : CursorSettled s) : CursorSettled { s with core := next } := by
  intro id current selected
  exact settled_data_step base id (previous id current selected)

theorem settled_setCursor (s : State C L M B St D T R P F A S J U)
    (id : Nat) (current : Cursor S J U) (previous : CursorSettled s)
    (settled : SettledCursor (s.core.updates id)) : CursorSettled (setCursor s id current) := by
  intro subject observed selected
  by_cases same : subject = id
  · subst subject
    simp only [setCursor, put, if_pos rfl] at selected
    cases selected
    exact settled
  · exact previous subject observed (by simpa [setCursor, put, same] using selected)

theorem settled_step {before after : State C L M B St D T R P F A S J U}
    (step : Step methods accepted source encoding before after)
    (previous : CursorSettled before) (alignment : RegistrationUpdate.Alignment before.core) :
    CursorSettled after := by
  cases step
  case ordinary next base notCommit => exact settled_core before next base previous
  case beginAudit id ready unused =>
    have complete : RegistrationUpdate.Complete (before.core.updates id) := by
      simpa only [RegistrationUpdate.Aligned, ready] using alignment id
    rcases complete with ⟨row, matched, updated, writes⟩
    apply settled_setCursor before id _ previous
    simp [SettledCursor, ready, updated]
  case append id current wanted remaining record visibleAfter ready selected head constructed append =>
    exact settled_setCursor before id _ previous (previous id current selected)
  case requestCommit =>
    exact settled_core (methods := methods) (accepted := accepted) _ _
      (RegistrationUpdate.Step.requestCommit (methods := methods) (accepted := accepted) _ _ (by assumption)) previous
  case serverCommit =>
    exact settled_core (methods := methods) (accepted := accepted) _ _
      (RegistrationUpdate.Step.serverCommit (methods := methods) (accepted := accepted) _ _ (by assumption)) previous
  case externalRegisterCommit next id writes records nextAudit base event appends disjoint =>
    exact settled_core before next base previous
  case foreignAppend => exact previous

theorem reached_cursor_settled (db : RegistrationUpdate.DB C L M B St D T R P F)
    registrations updates (audit : Ledger A J U) (s : State C L M B St D T R P F A S J U)
    (execution : Execution methods accepted source encoding (initial db registrations updates audit) s) :
    CursorSettled s := by
  induction execution with
  | refl =>
    intro id current impossible
    simp [initial] at impossible
  | tail earlier step ih =>
    exact settled_step step ih (RegistrationUpdate.reached_alignment methods accepted db registrations updates
      _ (execution_erasure earlier))

/-- This is the additional WHOLE-PATH no-own-audit claim, unlike per-step
rollback preservation. Neither cursor nor successful own append ever existed. -/
theorem reached_noop_no_cursor (db : RegistrationUpdate.DB C L M B St D T R P F)
    registrations updates (audit : Ledger A J U) (s : State C L M B St D T R P F A S J U)
    (execution : Execution methods accepted source encoding (initial db registrations updates audit) s)
    (id : Nat) (noop : (s.core.updates id).outcome = .noop) : s.cursors id = none := by
  cases selected : s.cursors id with
  | none => rfl
  | some current =>
    have settled := reached_cursor_settled db registrations updates audit s execution id current selected
    exact False.elim (settled.2 noop)

/-- Append completion is DERIVED from reached cursor progress and immutable
request binding. Neither premise is a desired-output commit guard. -/
theorem reached_singleton_program (db : RegistrationUpdate.DB C L M B St D T R P F)
    registrations updates (audit : Ledger A J U) (s : State C L M B St D T R P F A S J U)
    (execution : Execution methods accepted source encoding (initial db registrations updates audit) s)
    (id : Nat) (current : Cursor S J U) (selected : s.cursors id = some current)
    (exhausted : current.remaining = []) :
    ∃ record, current.acknowledged = [record] ∧
      recordIntent encoding record = ⟨.update, source.server (updates id).metadata⟩ := by
  have initialProgress : Progress encoding (initial db registrations updates audit) := by
    intro subject observed impossible
    simp [initial] at impossible
  have initialBinding : SourceBinding source (initial db registrations updates audit) := by
    intro subject observed impossible
    simp [initial] at impossible
  have progress := execution_progress execution initialProgress id current selected
  have bound := execution_binding execution initialBinding id current selected
  have original := (RegistrationUpdate.reached_requests methods accepted db registrations updates s.core
    (execution_erasure execution) id).2
  have mapped : current.acknowledged.map (recordIntent encoding) =
      [⟨.update, source.server (updates id).metadata⟩] := by
    simpa only [exhausted, List.append_nil, bound, sourceProgram, original] using progress
  cases records : current.acknowledged with
  | nil => simp [records] at mapped
  | cons first rest =>
    cases rest with
    | nil => exact ⟨first, rfl, by simpa [records] using mapped⟩
    | cons second tail => simp [records] at mapped

/-- Whole data and complete record publication at the actual commit. This reuses
mixed data/lock proofs; it makes no claim about external registration vocabulary. -/
theorem reached_update_decision (db : RegistrationUpdate.DB C L M B St D T R P F)
    registrations updates (audit : Ledger A J U) (s : State C L M B St D T R P F A S J U)
    (execution : Execution methods accepted source encoding (initial db registrations updates audit) s)
    (id : Nat) (current : Cursor S J U) (selected : s.cursors id = some current)
    (phase : (s.core.updates id).phase = .committing) (exhausted : current.remaining = [])
    (fresh : NewIdentities encoding s.audit current.acknowledged) :
    ∃ after, Step methods accepted source encoding s after ∧
      UpdateDecision source encoding s after (updates id) current.acknowledged := by
  have compatible : Compatible encoding s.audit current.acknowledged := by
    intro record member
    exact Or.inl (fresh.2 record member)
  let after : State C L M B St D T R P F A S J U :=
    { s with
      core := RegistrationUpdate.committedState s.core id
      audit := overlay encoding s.audit current.acknowledged }
  refine ⟨after, Step.serverCommit s id current phase selected exhausted compatible, ?_⟩
  have erased := execution_erasure execution
  have original := (RegistrationUpdate.reached_requests methods accepted db registrations updates s.core erased id).2
  refine ⟨?_, reached_singleton_program db registrations updates audit s execution id current selected exhausted,
    RegistrationAuditedLifetime.full_record_extension encoding s.audit current.acknowledged fresh, ?_⟩
  · change RegistrationUpdate.commit s.core.core.db (s.core.updates id).staged = _
    rw [RegistrationUpdate.committing_sequential_update methods accepted db registrations updates s.core erased id phase,
      original]
  · intro recordId record stored
    exact overlay_preserves s.audit current.acknowledged compatible recordId record stored

/-- Reuse the checked mixed commit-order data history, retaining its explicit
owned-frame projection. Audit correctness is scoped to UPDATE cuts above. -/
theorem reached_data_history (db : RegistrationUpdate.DB C L M B St D T R P F)
    registrations updates (audit : Ledger A J U) (s : State C L M B St D T R P F A S J U)
    (execution : Execution methods accepted source encoding (initial db registrations updates audit) s) :
    RegistrationUpdate.SerialHistory registrations updates db s.core.events
      (RegistrationTransaction.owned s.core.core.db db.frame) :=
  RegistrationUpdate.execution_serial methods accepted db registrations updates s.core (execution_erasure execution)

/-- Current missing-row conclusion is DERIVED before the no-op rollback, not
supplied as authority to omit an audit record. -/
theorem reached_noop_absence (db : RegistrationUpdate.DB C L M B St D T R P F)
    registrations updates (audit : Ledger A J U) (s : State C L M B St D T R P F A S J U)
    (execution : Execution methods accepted source encoding (initial db registrations updates audit) s)
    (id : Nat) (phase : (s.core.updates id).phase = .rollbackPending)
    (noop : (s.core.updates id).outcome = .noop) :
    s.core.core.db.registrations (updates id).client = none ∧
      RegistrationUpdate.sequentialUpdate s.core.core.db (updates id) = s.core.core.db := by
  have original := (RegistrationUpdate.reached_requests methods accepted db registrations updates s.core
    (execution_erasure execution) id).2
  simpa only [original] using RegistrationUpdate.pending_noop_absence methods accepted db registrations updates
    s.core (execution_erasure execution) id phase noop

theorem noop_discards (s : State C L M B St D T R P F A S J U) (id : Nat)
    (phase : (s.core.updates id).phase = .rollbackPending)
    (noop : (s.core.updates id).outcome = .noop) :
    ∃ after, Step methods accepted source encoding s after ∧ Discarded s after ∧
      (after.core.updates id).phase = .aborted := by
  let next : RegistrationUpdate.State C L M B St D T R P F :=
    { (RegistrationUpdate.updateTx s.core id { s.core.updates id with phase := .aborted }) with
      core := { s.core.core with lock := RegistrationTransaction.release s.core.core.lock id } }
  have ordinary : Ordinary s.core next := by
    refine ⟨rfl, ?_⟩
    intro subject notCommitting
    simp only [next, RegistrationUpdate.updateTx, put]
    split_ifs <;> simp_all
  refine ⟨{ s with core := next }, Step.ordinary s next (.rollbackNoop s.core id phase noop) ordinary,
    ⟨rfl, rfl, rfl, rfl⟩, ?_⟩
  simp [next, RegistrationUpdate.updateTx, put]

theorem abort_discards (s : State C L M B St D T R P F A S J U) (id : Nat)
    (phase : (s.core.updates id).phase = .committing ∨
      (s.core.updates id).phase = .rollbackPending) :
    ∃ after, Step methods accepted source encoding s after ∧ Discarded s after ∧
      (after.core.updates id).phase = .aborted := by
  let next : RegistrationUpdate.State C L M B St D T R P F :=
    { (RegistrationUpdate.updateTx s.core id { s.core.updates id with phase := .aborted }) with
      core := { s.core.core with lock := RegistrationTransaction.release s.core.core.lock id } }
  have ordinary : Ordinary s.core next := by
    refine ⟨rfl, ?_⟩
    intro subject notCommitting
    simp only [next, RegistrationUpdate.updateTx, put]
    split_ifs <;> simp_all
  refine ⟨{ s with core := next }, Step.ordinary s next (.serverAbort s.core id phase) ordinary,
    ⟨rfl, rfl, rfl, rfl⟩, ?_⟩
  simp [next, RegistrationUpdate.updateTx, put]

theorem aborted_not_active (phase : RegistrationUpdate.Phase) (aborted : phase = .aborted) :
    ¬ActivePrivate phase := by
  simp [ActivePrivate, aborted]

#print axioms batch_preserves
#print axioms batch_private_unchanged
#print axioms execution_erasure
#print axioms reached_singleton_program
#print axioms reached_update_decision
#print axioms reached_data_history
#print axioms reached_noop_no_cursor
#print axioms reached_noop_absence
#print axioms noop_discards
#print axioms abort_discards

run_cmd do
  for declaration in [``core_append_preserves, ``batch_preserves, ``overlay_preserves,
      ``batch_private_unchanged, ``data_trans, ``step_erasure, ``execution_erasure,
      ``progress_setCursor, ``progress_step, ``execution_progress,
      ``binding_core, ``binding_setCursor, ``binding_step, ``execution_binding,
      ``settled_data_step, ``settled_core, ``settled_setCursor, ``settled_step,
      ``reached_cursor_settled, ``reached_noop_no_cursor,
      ``reached_singleton_program, ``reached_update_decision, ``reached_data_history,
      ``reached_noop_absence, ``noop_discards, ``abort_discards, ``aborted_not_active] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed audited UPDATE axiom: {name}"

end RegistrationAuditedUpdate
