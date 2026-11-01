import RegistrationAuditedLifetimeSpecification
import RegistrationLifetimeHistoryProof

namespace RegistrationAuditedLifetime
open RegistrationTransaction (put)
variable {C L M R P I D F A S J T : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq A]

theorem core_append_identical (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (record : Record J T) (time : record.occurredAt = encoding.storedTime record.draft)
    (present : ledger (encoding.id record.draft) = some record) :
    CoreAppend encoding ledger record ledger := by
  refine ⟨time, Or.inr present, ?_⟩
  funext id
  by_cases same : id = encoding.id record.draft
  · subst id
    simp [put, present]
  · simp [put, same]

theorem core_append_conflict_refused (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (record : Record J T) (conflict : Conflict encoding ledger record) (after : Ledger A J T) :
    ¬CoreAppend encoding ledger record after := by
  rintro ⟨_, accepted, _⟩
  rcases conflict with ⟨previous, stored, different⟩
  rcases accepted with absent | identical
  · rw [stored] at absent
    contradiction
  · have equal : previous = record := Option.some.inj (stored.symm.trans identical)
    exact different equal

theorem overlay_append (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (records : List (Record J T)) (record : Record J T) :
    overlay encoding ledger (records ++ [record]) =
      put (overlay encoding ledger records) (encoding.id record.draft) (some record) := by
  simp [overlay, List.foldl_append]

theorem overlay_unrelated (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (records : List (Record J T)) (id : A)
    (unrelated : id ∉ records.map (fun record => encoding.id record.draft)) :
    overlay encoding ledger records id = ledger id := by
  induction records generalizing ledger with
  | nil => rfl
  | cons record remaining ih =>
    have absent := List.ne_and_not_mem_of_not_mem_cons unrelated
    change overlay encoding (put ledger (encoding.id record.draft) (some record)) remaining id = ledger id
    rw [ih _ absent.2]
    simp [put, absent.1]

theorem overlay_stores_records (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (records : List (Record J T))
    (distinct : (records.map (fun record => encoding.id record.draft)).Nodup) :
    ∀ record ∈ records, overlay encoding ledger records (encoding.id record.draft) = some record := by
  induction records generalizing ledger with
  | nil => simp
  | cons first remaining ih =>
    have distinctParts := List.nodup_cons.mp distinct
    intro record member
    rcases List.mem_cons.mp member with same | later
    · subst record
      change overlay encoding (put ledger (encoding.id first.draft) (some first)) remaining
        (encoding.id first.draft) = some first
      rw [overlay_unrelated encoding _ remaining _ distinctParts.1]
      simp [put]
    · exact ih _ distinctParts.2 record later

theorem full_record_extension (encoding : Encoding A S J T) (ledger : Ledger A J T)
    (records : List (Record J T)) (fresh : NewIdentities encoding ledger records) :
    FullRecordExtension encoding ledger (overlay encoding ledger records) records :=
  ⟨overlay_stores_records encoding ledger records fresh.1, overlay_unrelated encoding ledger records⟩

section Bag
variable [DecidableEq S]

theorem count_intents (activity : Activity) (servers : List S) (wanted : Intent S) :
    (servers.map (fun server => Intent.mk activity server)).count wanted =
      if wanted.activity = activity then servers.count wanted.server else 0 := by
  cases wanted with
  | mk wantedActivity wantedServer =>
    by_cases same : wantedActivity = activity
    · subst wantedActivity
      induction servers with
      | nil => simp
      | cons server remaining ih => simp [List.count_cons, Intent.mk.injEq, ih]
    · induction servers with
      | nil => simp
      | cons server remaining ih => simp [ih, same, Ne.symm same]

theorem ended_intent_count (source : Source M R S) (activity : Activity)
    (ended : List (RegistrationTransaction.Registration C L M R)) (wanted : Intent S) :
    (ended.map (fun row => Intent.mk activity (source.endedServer row.payload))).count wanted =
      if wanted.activity = activity then
        (ended.map (fun row => source.endedServer row.payload)).count wanted.server else 0 := by
  simpa only [List.map_map, Function.comp_def] using
    count_intents activity (ended.map (fun row => source.endedServer row.payload)) wanted

/-- The independently specified bag retains repeated server targets and uses the
captured old server for Replace/Deregister, even when the candidate changes slug. -/
theorem source_program_bag (source : Source M R S)
    (request : RegistrationLifetime.Request C L M R P D)
    (ended : List (RegistrationTransaction.Registration C L M R)) (wanted : Intent S) :
    (sourceProgram source request ended).count wanted = expectedBag source request ended wanted := by
  cases request with
  | register proposal =>
    rw [sourceProgram, List.count_append, ended_intent_count]
    cases wanted with
    | mk activity server =>
      cases activity <;> simp [expectedBag, List.count_singleton, Intent.mk.injEq]
  | remove =>
    rw [sourceProgram, ended_intent_count]
    rfl

end Bag

theorem initial_progress (db : RegistrationLifetime.Database C L M R P I D F) requests
    (audit : Ledger A J T) (encoding : Encoding A S J T) :
    Progress encoding (initial db requests audit) := by
  intro id current selected
  simp [initial] at selected

theorem progress_setCursor (encoding : Encoding A S J T)
    (s : State C L M R P I D F A S J T) (id : Nat) (current : Cursor S J T)
    (previous : Progress encoding s)
    (complete : current.acknowledged.map (recordIntent encoding) ++ current.remaining = current.program) :
    Progress encoding (setCursor s id current) := by
  intro subject observed selected
  by_cases same : subject = id
  · subst subject
    simp only [setCursor, put, if_pos rfl] at selected
    cases selected
    exact complete
  · exact previous subject observed (by simpa [setCursor, put, same] using selected)

/-- Each acknowledged append moves exactly one instruction. No bound on the
ended-registration list or the number of successful appends is used. -/
theorem progress_step {source : Source M R S} {encoding : Encoding A S J T}
    {before after : State C L M R P I D F A S J T}
    (step : Step source encoding before after) (previous : Progress encoding before) :
    Progress encoding after := by
  cases step
  case ordinary => exact previous
  case beginAudit s id ready unused =>
    apply progress_setCursor _ _ _ _ previous
    simp [cursor]
  case append s id current wanted remaining record visibleAfter ready selected head constructed accepted =>
    apply progress_setCursor _ _ _ _ previous
    have complete := previous id current selected
    simpa [advance, List.map_append, List.append_assoc, constructed, head] using complete
  case requestCommit => exact previous
  case serverCommit => exact previous
  case foreignAppend => exact previous

theorem execution_progress {source : Source M R S} {encoding : Encoding A S J T}
    {before after : State C L M R P I D F A S J T}
    (execution : Execution source encoding before after) (previous : Progress encoding before) :
    Progress encoding after := by
  induction execution with
  | refl => exact previous
  | tail earlier step ih => exact progress_step step ih

/-- The source request/captured rows cannot change once audit begins. This is a
property of the reused data transitions, not an output supplied to commit. -/
def Settled (phase : RegistrationLifetime.Phase) : Prop :=
  phase = .ready ∨ phase = .committing ∨ phase = .committed ∨
    phase = .abortPending ∨ phase = .aborted

theorem settled_step {before after : RegistrationLifetime.State C L M R P I D F}
    (step : RegistrationLifetime.Step true before after) (id : Nat)
    (settled : Settled (before.transactions id).phase) :
    Settled (after.transactions id).phase ∧
      (after.transactions id).request = (before.transactions id).request ∧
      (after.transactions id).captured = (before.transactions id).captured := by
  cases step <;>
    simp only [RegistrationLifetime.update, RegistrationLifetime.committedState, put] <;>
    try split_ifs
  all_goals simp_all [Settled]

def SourceBinding (source : Source M R S) (s : State C L M R P I D F A S J T) : Prop :=
  ∀ id current, s.cursors id = some current →
    Settled (s.core.transactions id).phase ∧
      current.program = sourceProgram source (s.core.transactions id).request (s.core.transactions id).captured

theorem binding_core_step {source : Source M R S}
    {s : State C L M R P I D F A S J T} {next}
    (step : RegistrationLifetime.Step true s.core next) (previous : SourceBinding source s) :
    SourceBinding source { s with core := next } := by
  intro id current selected
  have bound := previous id current selected
  have stable := settled_step step id bound.1
  refine ⟨stable.1, ?_⟩
  simpa only [stable.2.1, stable.2.2] using bound.2

theorem binding_setCursor {source : Source M R S}
    (s : State C L M R P I D F A S J T) (id : Nat) (current : Cursor S J T)
    (previous : SourceBinding source s)
    (bound : Settled (s.core.transactions id).phase ∧
      current.program = sourceProgram source (s.core.transactions id).request (s.core.transactions id).captured) :
    SourceBinding source (setCursor s id current) := by
  intro subject observed selected
  by_cases same : subject = id
  · subst subject
    simp only [setCursor, put, if_pos rfl] at selected
    cases selected
    exact bound
  · exact previous subject observed (by simpa [setCursor, put, same] using selected)

theorem binding_step {source : Source M R S} {encoding : Encoding A S J T}
    {before after : State C L M R P I D F A S J T}
    (step : Step source encoding before after) (previous : SourceBinding source before) :
    SourceBinding source after := by
  cases step
  case ordinary s next base notCommit => exact binding_core_step base previous
  case beginAudit s id ready unused =>
    apply binding_setCursor _ _ _ previous
    exact ⟨Or.inl ready, rfl⟩
  case append s id current wanted remaining record visibleAfter ready selected head constructed accepted =>
    apply binding_setCursor _ _ _ previous
    exact previous id current selected
  case requestCommit =>
    exact binding_core_step (RegistrationLifetime.Step.requestCommit _ _ (by assumption)) previous
  case serverCommit =>
    exact binding_core_step (RegistrationLifetime.Step.serverCommit _ _ (by assumption)) previous
  case foreignAppend => exact previous

theorem initial_binding (source : Source M R S)
    (db : RegistrationLifetime.Database C L M R P I D F) requests (audit : Ledger A J T) :
    SourceBinding source (initial db requests audit) := by
  intro id current selected
  simp [initial] at selected

theorem execution_binding {source : Source M R S} {encoding : Encoding A S J T}
    {before after : State C L M R P I D F A S J T}
    (execution : Execution source encoding before after) (previous : SourceBinding source before) :
    SourceBinding source after := by
  induction execution with
  | refl => exact previous
  | tail earlier step ih => exact binding_step step ih

theorem lifetime_trans {before middle after : RegistrationLifetime.State C L M R P I D F}
    (first : RegistrationLifetime.Execution true before middle)
    (second : RegistrationLifetime.Execution true middle after) :
    RegistrationLifetime.Execution true before after := by
  induction second with
  | refl => exact first
  | tail earlier step ih => exact RegistrationLifetime.Execution.tail ih step

/-- Only stuttering audit operations are erased; real data staging/abort/reply
and both sides of the server decision remain actual old protocol operations. -/
theorem step_erasure {source : Source M R S} {encoding : Encoding A S J T}
    {before after : State C L M R P I D F A S J T} (step : Step source encoding before after) :
    RegistrationLifetime.Execution true before.core after.core := by
  cases step
  case ordinary s next base notCommit =>
    exact RegistrationLifetime.Execution.tail (.refl _) base
  case beginAudit => exact .refl _
  case append => exact .refl _
  case requestCommit =>
    exact RegistrationLifetime.Execution.tail (.refl _)
      (RegistrationLifetime.Step.requestCommit _ _ (by assumption))
  case serverCommit =>
    exact RegistrationLifetime.Execution.tail (.refl _)
      (RegistrationLifetime.Step.serverCommit _ _ (by assumption))
  case foreignAppend => exact .refl _

theorem execution_erasure {source : Source M R S} {encoding : Encoding A S J T}
    {before after : State C L M R P I D F A S J T} (execution : Execution source encoding before after) :
    RegistrationLifetime.Execution true before.core after.core := by
  induction execution with
  | refl => exact .refl _
  | tail earlier step ih => exact lifetime_trans ih (step_erasure step)

/-- Completion follows from all actual append steps plus exhaustion, and the
immutable source binding is derived from the reached lifetime execution. -/
theorem reached_program_complete (source : Source M R S) (encoding : Encoding A S J T)
    (db : RegistrationLifetime.Database C L M R P I D F) requests (audit : Ledger A J T)
    (s : State C L M R P I D F A S J T)
    (execution : Execution source encoding (initial db requests audit) s)
    (id : Nat) (current : Cursor S J T) (selected : s.cursors id = some current)
    (exhausted : current.remaining = []) :
    current.acknowledged.map (recordIntent encoding) =
      sourceProgram source (s.core.transactions id).request (s.core.transactions id).captured := by
  have complete := execution_progress execution (initial_progress db requests audit encoding) id current selected
  have bound := execution_binding execution (initial_binding source db requests audit) id current selected
  simpa only [exhausted, List.append_nil, bound.2] using complete

/-- Real audit/data commit: the entire independent stored operation follows the
old checked protocol; the appended intent list follows actual successful audit
steps. Prior/unrelated full records are preserved pointwise, without ordering
claims about audit sequence numbers or concurrent commits. -/
theorem reached_atomic_store_cut (source : Source M R S) (encoding : Encoding A S J T)
    (db : RegistrationLifetime.Database C L M R P I D F) requests (audit : Ledger A J T)
    (s : State C L M R P I D F A S J T)
    (execution : Execution source encoding (initial db requests audit) s)
    (id : Nat) (current : Cursor S J T) (selected : s.cursors id = some current)
    (phase : (s.core.transactions id).phase = .committing) (exhausted : current.remaining = []) :
    (RegistrationLifetime.committedState s.core id).db =
        RegistrationLifetime.sequential s.core.db (requests id) ∧
      current.acknowledged.map (recordIntent encoding) =
        sourceProgram source (requests id) (s.core.transactions id).captured ∧
      ∀ recordId, recordId ∉ current.acknowledged.map (fun record => encoding.id record.draft) →
        overlay encoding s.audit current.acknowledged recordId = s.audit recordId := by
  have erased := execution_erasure execution
  have original := RegistrationLifetime.execution_original_requests erased
    (RegistrationLifetime.initial_original_requests db requests) id
  refine ⟨RegistrationLifetime.reached_commit_effect db requests s.core erased id phase, ?_, ?_⟩
  · simpa only [original] using reached_program_complete source encoding db requests audit s execution
      id current selected exhausted
  · exact overlay_unrelated encoding s.audit current.acknowledged

/-- A reached transaction with exhausted cursor can take the actual combined
server decision and satisfies the independent complete STORE contract. Fresh
record identity is scoped to this attempt's explicit records/committed ledger. -/
theorem reached_store_decision [DecidableEq S]
    (source : Source M R S) (encoding : Encoding A S J T)
    (db : RegistrationLifetime.Database C L M R P I D F) requests (audit : Ledger A J T)
    (s : State C L M R P I D F A S J T)
    (execution : Execution source encoding (initial db requests audit) s)
    (id : Nat) (current : Cursor S J T) (selected : s.cursors id = some current)
    (phase : (s.core.transactions id).phase = .committing) (exhausted : current.remaining = [])
    (fresh : NewIdentities encoding s.audit current.acknowledged) :
    ∃ after, Step source encoding s after ∧
      StoreDecision source encoding s after (requests id) (s.core.transactions id).captured
        current.acknowledged := by
  have compatible : Compatible encoding s.audit current.acknowledged := by
    intro record member
    exact Or.inl (fresh.2 record member)
  refine ⟨{ s with
    core := RegistrationLifetime.committedState s.core id
    audit := overlay encoding s.audit current.acknowledged },
    Step.serverCommit s id current phase selected exhausted compatible, ?_⟩
  have cut := reached_atomic_store_cut source encoding db requests audit s execution id current
    selected phase exhausted
  refine ⟨cut.1, ?_, full_record_extension encoding s.audit current.acknowledged fresh⟩
  intro intent
  rw [cut.2.1]
  exact source_program_bag source (requests id) (s.core.transactions id).captured intent

/-- Abort after earlier private appends has no data/audit commit event. This does
not deny unrelated foreign records, or reinterpret an error reply as an abort. -/
theorem confirmed_abort_has_no_data_event (source : Source M R S) (encoding : Encoding A S J T)
    (db : RegistrationLifetime.Database C L M R P I D F) requests (audit : Ledger A J T)
    (s : State C L M R P I D F A S J T)
    (execution : Execution source encoding (initial db requests audit) s) (id : Nat)
    (aborted : (s.core.transactions id).phase = .aborted)
    (writes : List (RegistrationLifetime.Write C L M R P D)) :
    (id, writes) ∉ s.core.commits :=
  RegistrationLifetime.confirmed_abort_has_no_event db requests s.core
    (execution_erasure execution) id aborted writes

/-- Confirmed rollback leaves BOTH committed stores unchanged, even with an
arbitrary nonempty private append history. The ghost cursor is retained; it is
not a live native identity reservation after this server decision. -/
theorem abort_discards_private (source : Source M R S) (encoding : Encoding A S J T)
    (s : State C L M R P I D F A S J T) (id : Nat)
    (phase : (s.core.transactions id).phase = .abortPending ∨
      (s.core.transactions id).phase = .committing) :
    ∃ after, Step source encoding s after ∧
      after.core.db = s.core.db ∧ after.audit = s.audit ∧
      after.cursors = s.cursors ∧ (after.core.transactions id).phase = .aborted := by
  let next : RegistrationLifetime.State C L M R P I D F :=
    { RegistrationLifetime.update s.core id { s.core.transactions id with phase := .aborted } with
      instanceOwner := RegistrationLifetime.release s.core.instanceOwner id
      deploymentOwner := RegistrationLifetime.release s.core.deploymentOwner id }
  have permitted : Ordinary s.core next := by
    refine ⟨rfl, ?_⟩
    intro subject notCommitting
    simp only [next, RegistrationLifetime.update, put]
    split_ifs <;> simp_all
  refine ⟨{ s with core := next },
    Step.ordinary s next (RegistrationLifetime.Step.serverAbort s.core id phase) permitted,
    rfl, rfl, rfl, ?_⟩
  simp [next, RegistrationLifetime.update, put]

/-- Aborted records remain ghost history, not live reservations. In particular a
foreign append need not be disjoint from this retired cursor's IDs. -/
theorem aborted_not_active (phase : RegistrationLifetime.Phase) (aborted : phase = .aborted) :
    ¬ActivePrivate phase := by
  simp [ActivePrivate, aborted]

#print axioms core_append_identical
#print axioms core_append_conflict_refused
#print axioms execution_progress
#print axioms execution_binding
#print axioms execution_erasure
#print axioms reached_program_complete
#print axioms reached_atomic_store_cut
#print axioms reached_store_decision
#print axioms confirmed_abort_has_no_data_event
#print axioms abort_discards_private
#print axioms aborted_not_active

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``core_append_identical, ``core_append_conflict_refused,
      ``overlay_append, ``overlay_unrelated, ``overlay_stores_records, ``full_record_extension,
      ``count_intents, ``ended_intent_count, ``source_program_bag,
      ``initial_progress, ``progress_setCursor,
      ``progress_step, ``execution_progress, ``Settled, ``settled_step, ``SourceBinding,
      ``binding_core_step, ``binding_setCursor, ``binding_step, ``initial_binding,
      ``execution_binding, ``lifetime_trans, ``step_erasure, ``execution_erasure,
      ``reached_program_complete, ``reached_atomic_store_cut, ``reached_store_decision,
      ``confirmed_abort_has_no_data_event, ``abort_discards_private, ``aborted_not_active] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected audited lifetime axiom in {declaration}: {unexpected}"

end RegistrationAuditedLifetime
