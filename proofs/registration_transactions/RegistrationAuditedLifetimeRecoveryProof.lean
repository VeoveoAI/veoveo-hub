import RegistrationAuditedLifetimeRecoveryWitnesses
import RegistrationAuditedLifetimeWitnessExecution

namespace RegistrationAuditedLifetime.RemovalRecovery
open RegistrationAuditedLifetime
open RegistrationAuditedLifetime.Witnesses
open RegistrationTransaction (put rowMap)

private theorem response_only_ordinary (s : Core) (actor : Nat)
    (reply : RegistrationLifetime.Response) (message : Option String) :
    Ordinary s (RegistrationLifetime.update s actor
      { s.transactions actor with response := reply, error := message }) := by
  refine ⟨rfl, ?_⟩
  intro subject previous
  by_cases same : subject = actor
  · subst subject
    simpa only [RegistrationLifetime.update, put, ite_true] using previous
  · simpa only [RegistrationLifetime.update, put, if_neg same] using previous

private theorem response_phase (s : Core) (actor : Nat)
    (reply : RegistrationLifetime.Response) (message : Option String) :
    ((RegistrationLifetime.update s actor
      { s.transactions actor with response := reply, error := message }).transactions actor).phase =
        (s.transactions actor).phase := by
  simp only [RegistrationLifetime.update, put, ite_true]

private theorem response_commit_database (s : Core) (actor : Nat)
    (reply : RegistrationLifetime.Response) (message : Option String) :
    (RegistrationLifetime.committedState (RegistrationLifetime.update s actor
      { s.transactions actor with response := reply, error := message }) actor).db =
        (RegistrationLifetime.committedState s actor).db := by
  simp only [RegistrationLifetime.committedState, RegistrationLifetime.update, put, ite_true]

theorem write_attempt_ordinary (s : World) (actor : Nat) (tx : Tx) (owner : Option Nat)
    (notCommitting : tx.phase ≠ .committing) : Ordinary s.core (writeAttempt s actor tx owner).core := by
  refine ⟨rfl, ?_⟩
  intro subject previous
  change (put s.core.transactions actor tx subject).phase ≠ .committing
  by_cases same : subject = actor
  · simpa [put, same] using notCommitting
  · simpa [put, same] using previous

/-- One actor-indexed source prefix, used for the two distinct actual attempts.
The premises describe the input row/fresh attempt/free lock, never a desired
after-state. Captured rows and complete private writes are constructed below. -/
theorem removal_prefix_execution (s : World) (actor : Nat)
    (database : s.core.db = seedDB)
    (fresh : s.core.transactions actor = { request := .remove })
    (free : s.core.instanceOwner = none) :
    Execution source encoding s (removalPrefix s actor) := by
  have one : Step source encoding s (beginRemoval s actor) :=
    Step.ordinary s (beginRemoval s actor).core
      (RegistrationLifetime.Step.beginRemove s.core actor (by simp [fresh]) (by simp [fresh]))
      (write_attempt_ordinary _ _ _ _ (by
        change RegistrationLifetime.Phase.waiting ≠ .committing
        decide))
  let a := beginRemoval s actor
  have present : a.core.db.instanceRow = some instancePayload := by
    change s.core.db.instanceRow = some instancePayload
    rw [database]
    rfl
  have two : Step source encoding a (selectRemoval a actor) :=
    Step.ordinary a (selectRemoval a actor).core
      (RegistrationLifetime.Step.acquire a.core actor instancePayload rfl
        (by simp [a, beginRemoval, writeAttempt, RegistrationLifetime.update, put]) free present)
      (write_attempt_ordinary _ _ _ _ (by
        simp [selectRemoval, a, beginRemoval, writeAttempt, RegistrationLifetime.update, put,
          fresh, RegistrationLifetime.afterLock]))
  let b := selectRemoval a actor
  have bPhase : (b.core.transactions actor).phase = .deleting := by
    simp [b, selectRemoval, a, beginRemoval, writeAttempt, RegistrationLifetime.update, put,
      fresh, RegistrationLifetime.afterLock]
  have listing : RegistrationLifetime.Listing
      (RegistrationLifetime.applyWrites b.core.db (b.core.transactions actor).staged)
      [oldFirst, oldSecond] := by
    have empty : (b.core.transactions actor).staged = [] := by
      simp [b, selectRemoval, a, beginRemoval, writeAttempt, RegistrationLifetime.update, put, fresh]
    rw [empty]
    change RegistrationLifetime.Listing s.core.db [oldFirst, oldSecond]
    rw [database]
    refine ⟨by decide, ?_⟩
    funext key
    by_cases first : key = 30
    · subst key
      rfl
    by_cases second : key = 31
    · subst key
      rfl
    simp [seedDB, rowMap, put, oldFirst, oldSecond, first, second]
  have three : Step source encoding b (captureRemoval b actor) :=
    Step.ordinary b (captureRemoval b actor).core
      (RegistrationLifetime.Step.delete b.core actor [oldFirst, oldSecond] bPhase listing)
      (write_attempt_ordinary _ _ _ _ (by
        change RegistrationLifetime.Phase.dropping ≠ .committing
        decide))
  let c := captureRemoval b actor
  have four : Step source encoding c (dropRemoval c actor 40 [41]) :=
    Step.ordinary c (dropRemoval c actor 40 [41]).core
      (RegistrationLifetime.Step.drop c.core actor 40 [41]
        (by simp [c, captureRemoval, writeAttempt, RegistrationLifetime.update, put])
        (by simp [c, captureRemoval, writeAttempt, RegistrationLifetime.update, put]))
      (write_attempt_ordinary _ _ _ _ (by
        simp [c, captureRemoval, writeAttempt, RegistrationLifetime.update, put]))
  let d := dropRemoval c actor 40 [41]
  have five : Step source encoding d (dropRemoval d actor 41 []) :=
    Step.ordinary d (dropRemoval d actor 41 []).core
      (RegistrationLifetime.Step.drop d.core actor 41 []
        (by simp [d, dropRemoval, c, captureRemoval, writeAttempt, RegistrationLifetime.update, put])
        (by simp [d, dropRemoval, writeAttempt, RegistrationLifetime.update, put]))
      (write_attempt_ordinary _ _ _ _ (by
        simp [d, dropRemoval, c, captureRemoval, writeAttempt, RegistrationLifetime.update, put]))
  let e := dropRemoval d actor 41 []
  have method : (e.core.transactions actor).request = .remove := by
    simp [e, d, c, b, a, dropRemoval, captureRemoval, selectRemoval, beginRemoval,
      writeAttempt, RegistrationLifetime.update, put, fresh]
  have six : Step source encoding e (eraseRemoval e actor) :=
    Step.ordinary e (eraseRemoval e actor).core
      (RegistrationLifetime.Step.droppedRemove e.core actor
        (by simp [e, d, dropRemoval, c, captureRemoval, writeAttempt, RegistrationLifetime.update, put])
        method (by simp [e, dropRemoval, writeAttempt, RegistrationLifetime.update, put]))
      (write_attempt_ordinary _ _ _ _ (by
        change RegistrationLifetime.Phase.erasing ≠ .committing
        decide))
  let f := eraseRemoval e actor
  have available : f.core.instanceOwner = none ∨ f.core.instanceOwner = some actor := Or.inr rfl
  have stillPresent : f.core.db.instanceRow = some instancePayload := by
    change s.core.db.instanceRow = some instancePayload
    rw [database]
    rfl
  have seven : Step source encoding f (readyRemoval f actor) :=
    Step.ordinary f (readyRemoval f actor).core
      (RegistrationLifetime.Step.erase f.core actor instancePayload
        (by simp [f, eraseRemoval, writeAttempt, RegistrationLifetime.update, put])
        (by simpa [f, eraseRemoval, writeAttempt, RegistrationLifetime.update, put] using method)
        available stillPresent)
      (write_attempt_ordinary _ _ _ _ (by
        change RegistrationLifetime.Phase.ready ≠ .committing
        decide))
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.tail (.refl s) one) two) three) four) five) six) seven

theorem append_at_execution (s : World) (actor : Nat) (current : Cursor String Draft Nat)
    (record : Record Draft Nat) (visibleAfter : Audit)
    (readyPhase : (s.core.transactions actor).phase = .ready)
    (selectedCursor : s.cursors actor = some current)
    (matching : current.remaining.head? = none ∨
      current.remaining.head? = some (recordIntent encoding record))
    (accepted : CoreAppend encoding (overlay encoding s.audit current.acknowledged) record visibleAfter) :
    Execution source encoding s (appendPendingAt s actor record) := by
  cases head : current.remaining with
  | nil =>
    simp only [appendPendingAt, selectedCursor, head]
    exact .refl s
  | cons wanted remaining =>
    have constructed : recordIntent encoding record = wanted := by
      rcases matching with absent | present
      · simp [head] at absent
      · exact (Option.some.inj (by simpa [head] using present)).symm
    have step : Step source encoding s (setCursor s actor (advance current remaining record)) :=
      Step.append s actor current wanted remaining record visibleAfter readyPhase selectedCursor head constructed accepted
    simpa only [appendPendingAt, selectedCursor, head] using Execution.tail (.refl s) step

theorem partial_execution : Execution source encoding start partialRemoval := by
  have readyRun := removal_prefix_execution start 1 rfl rfl rfl
  have begin : Step source encoding firstReady firstBegun := Step.beginAudit firstReady 1 rfl rfl
  have appendRun : Execution source encoding firstBegun partialRemoval :=
    append_at_execution firstBegun 1 (cursorAt firstBegun 1) firstDeregister
      (put priorAudit 100 (some firstDeregister)) rfl rfl (by decide)
      ⟨by decide, Or.inl rfl, rfl⟩
  exact execution_trans (Execution.tail readyRun begin) appendRun

/-- The refused record is the actual next instruction and has the correct stored
time. Refusal follows from full committed identity content, not a wrong head/time. -/
theorem conflicting_second_append_refused :
    (cursorAt partialRemoval 1).remaining = [⟨.deregister, oldServer⟩] ∧
    recordIntent encoding conflictingDeregister = ⟨.deregister, oldServer⟩ ∧
    conflictingDeregister.occurredAt = encoding.storedTime conflictingDeregister.draft ∧
    priorRecord.draft ≠ conflictingDeregister.draft ∧
    priorRecord.occurredAt ≠ conflictingDeregister.occurredAt ∧
    Conflict encoding (overlay encoding partialRemoval.audit (cursorAt partialRemoval 1).acknowledged) conflictingDeregister ∧
    ∀ after, ¬CoreAppend encoding
      (overlay encoding partialRemoval.audit (cursorAt partialRemoval 1).acknowledged) conflictingDeregister after := by
  have different : priorRecord.occurredAt ≠ conflictingDeregister.occurredAt := by decide
  have present :
      overlay encoding partialRemoval.audit (cursorAt partialRemoval 1).acknowledged
        (encoding.id conflictingDeregister.draft) = some priorRecord := rfl
  refine ⟨rfl, rfl, by decide, by decide, different, ?_, ?_⟩
  · refine ⟨priorRecord, present, ?_⟩
    intro equal
    exact different (congrArg Record.occurredAt equal)
  · intro after accepted
    rcases accepted.2.1 with absent | identical
    · rw [present] at absent
      contradiction
    · have equal := Option.some.inj (present.symm.trans identical)
      exact different (congrArg Record.occurredAt equal)

/-- `fail` records the observed native error. The derived append refusal above
does not automatically manufacture this source observation or SQL rollback. -/
theorem abort_execution : Execution source encoding start aborted := by
  have failure : Step source encoding partialRemoval refused :=
    Step.ordinary partialRemoval refused.core
      (RegistrationLifetime.Step.fail partialRemoval.core 1 refusalMessage (by decide) (by decide) (by decide) rfl)
      (write_attempt_ordinary _ _ _ _ (by decide))
  have rollback : Step source encoding refused aborted := by
    apply Step.ordinary refused aborted.core
      (RegistrationLifetime.Step.serverAbort refused.core 1 (Or.inl rfl))
    change Ordinary refused.core
      (writeAttempt refused 1 { refused.core.transactions 1 with phase := .aborted } none).core
    exact write_attempt_ordinary _ _ _ _ (by decide)
  exact Execution.tail (Execution.tail partial_execution failure) rollback

theorem confirmed_rollback_output : ConfirmedRollback := by
  repeat' constructor
  all_goals rfl

theorem aborted_not_live : ¬ActivePrivate (aborted.core.transactions 1).phase := by
  simp [ActivePrivate, aborted, RegistrationLifetime.update, put]

theorem rollback_complete : Execution source encoding start aborted ∧ ConfirmedRollback ∧
    ¬ActivePrivate (aborted.core.transactions 1).phase :=
  ⟨abort_execution, confirmed_rollback_output, aborted_not_live⟩

theorem retry_pending_execution : Execution source encoding start retryCommitting := by
  have readyRun := removal_prefix_execution aborted 2 rfl rfl rfl
  have begin : Step source encoding retryReady retryBegun := Step.beginAudit retryReady 2 rfl rfl
  have first : Execution source encoding retryBegun retryOnce :=
    append_at_execution retryBegun 2 (cursorAt retryBegun 2) retryDeregisterFirst
      (put priorAudit 200 (some retryDeregisterFirst)) rfl rfl (by decide)
      ⟨by decide, Or.inl rfl, rfl⟩
  have second : Execution source encoding retryOnce retryTwice :=
    append_at_execution retryOnce 2 (cursorAt retryOnce 2) retryDeregisterSecond
      (put (overlay encoding priorAudit [retryDeregisterFirst]) 201 (some retryDeregisterSecond))
      rfl rfl (by decide) ⟨by decide, Or.inl rfl, rfl⟩
  have enter : Step source encoding retryTwice retryCommitting :=
    Step.requestCommit retryTwice 2 (cursorAt retryTwice 2) rfl rfl rfl
  exact Execution.tail (execution_trans (execution_trans
    (Execution.tail (execution_trans abort_execution readyRun) begin) first) second) enter

theorem retry_records_compatible : Compatible encoding retryCommitting.audit
    (cursorAt retryCommitting 2).acknowledged := by
  intro record member
  change record ∈ [retryDeregisterFirst, retryDeregisterSecond] at member
  simp [List.mem_cons] at member
  rcases member with rfl | rfl <;> exact Or.inl rfl

private theorem retry_phase : (retryCommitting.core.transactions 2).phase = .committing := by
  simp only [retryCommitting, RegistrationLifetime.update, put, ite_true]

private theorem retry_cursor_selected :
    retryCommitting.cursors 2 = some (cursorAt retryCommitting 2) := by
  rfl

private theorem retry_cursor_exhausted : (cursorAt retryCommitting 2).remaining = [] := by
  rfl

private theorem retry_pending_response : (retryCommitting.core.transactions 2).response = .pending := by
  rfl

set_option maxHeartbeats 800000 in
theorem retry_execution : Execution source encoding start retryAcknowledged := by
  have commit : Step source encoding retryCommitting retryCommitted :=
    Step.serverCommit retryCommitting 2 (cursorAt retryCommitting 2)
      retry_phase retry_cursor_selected retry_cursor_exhausted retry_records_compatible
  have committedPhase : (retryCommitted.core.transactions 2).phase = .committed := by
    simp only [retryCommitted, RegistrationLifetime.committedState, RegistrationLifetime.update, put, ite_true]
  have pending : (retryCommitted.core.transactions 2).response = .pending := by
    simpa only [retryCommitted, RegistrationLifetime.committedState, RegistrationLifetime.update,
      put, ite_true] using retry_pending_response
  have acknowledge : Step source encoding retryCommitted retryAcknowledged :=
    Step.ordinary retryCommitted retryAcknowledged.core
      (RegistrationLifetime.Step.acknowledge retryCommitted.core 2 committedPhase pending)
      (response_only_ordinary retryCommitted.core 2 .removed (retryCommitted.core.transactions 2).error)
  exact Execution.tail (Execution.tail retry_pending_execution commit) acknowledge

theorem removed_database : retryAcknowledged.core.db = expectedRemovedDB := by
  change RegistrationLifetime.applyWrites seedDB removalWrites = expectedRemovedDB
  apply RegistrationLifetime.Database.ext
  · rfl
  · rfl
  · funext key
    by_cases first : key = 30
    · subst key
      rfl
    by_cases second : key = 31
    · subst key
      rfl
    simp [RegistrationLifetime.applyWrites, RegistrationLifetime.applyWrite, removalWrites,
      seedDB, expectedRemovedDB, first, second]
  · funext login
    by_cases first : login = 40
    · subst login
      rfl
    by_cases second : login = 41
    · subst login
      rfl
    simp [RegistrationLifetime.applyWrites, RegistrationLifetime.applyWrite, removalWrites,
      seedDB, expectedRemovedDB, put, first, second]
  · rfl

theorem removed_audit : retryAcknowledged.audit = expectedRemovedAudit := by
  change overlay encoding priorAudit [retryDeregisterFirst, retryDeregisterSecond] = expectedRemovedAudit
  funext id
  by_cases first : id = 200
  · subst id
    rfl
  by_cases second : id = 201
  · subst id
    rfl
  simp [overlay, encoding, retryDeregisterFirst, retryDeregisterSecond, priorAudit,
    expectedRemovedAudit, put, first, second]

theorem retry_output : RemovalRetried := by
  refine ⟨removed_database, removed_audit, rfl, rfl, rfl, rfl, rfl, rfl, rfl,
    rfl, rfl, rfl, rfl, rfl, rfl, rfl, ?_⟩
  decide

theorem removal_recovery_complete : Execution source encoding start retryAcknowledged ∧
    ConfirmedRollback ∧ RemovalRetried :=
  ⟨retry_execution, confirmed_rollback_output, retry_output⟩

set_option maxHeartbeats 800000 in
theorem lost_reply_execution : Execution source encoding start replyLostCommitted := by
  have lost : Step source encoding retryCommitting replyLost :=
    Step.ordinary retryCommitting replyLost.core
      (RegistrationLifetime.Step.deliveryError retryCommitting.core 2 lostReplyMessage
        (Or.inl retry_phase) retry_pending_response)
      (response_only_ordinary retryCommitting.core 2 .error (some lostReplyMessage))
  have phase : (replyLost.core.transactions 2).phase = .committing := by
    change ((RegistrationLifetime.update retryCommitting.core 2
      { retryCommitting.core.transactions 2 with response := .error, error := some lostReplyMessage }).transactions 2).phase = .committing
    exact (response_phase retryCommitting.core 2 .error (some lostReplyMessage)).trans retry_phase
  have selected : replyLost.cursors 2 = some (cursorAt replyLost 2) := by
    simpa only [replyLost, cursorAt] using retry_cursor_selected
  have exhausted : (cursorAt replyLost 2).remaining = [] := by
    simpa only [replyLost, cursorAt] using retry_cursor_exhausted
  have compatible : Compatible encoding replyLost.audit (cursorAt replyLost 2).acknowledged := by
    simpa only [replyLost, cursorAt] using retry_records_compatible
  have commit : Step source encoding replyLost replyLostCommitted :=
    Step.serverCommit replyLost 2 (cursorAt replyLost 2) phase selected exhausted compatible
  exact Execution.tail (Execution.tail retry_pending_execution lost) commit

set_option maxHeartbeats 800000 in
theorem lost_reply_output : ErrorReplyCommitted := by
  have databaseSame : replyLostCommitted.core.db = retryAcknowledged.core.db := by
    change (RegistrationLifetime.committedState (RegistrationLifetime.update retryCommitting.core 2
      { retryCommitting.core.transactions 2 with response := .error, error := some lostReplyMessage }) 2).db =
        (RegistrationLifetime.committedState retryCommitting.core 2).db
    exact response_commit_database retryCommitting.core 2 .error (some lostReplyMessage)
  have auditSame : replyLostCommitted.audit = retryAcknowledged.audit := by
    simp only [replyLostCommitted, retryAcknowledged, retryCommitted, replyLost, cursorAt]
  refine ⟨databaseSame.trans removed_database, auditSame.trans removed_audit,
    rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem error_does_not_imply_rollback : Execution source encoding start replyLostCommitted ∧
    ErrorReplyCommitted ∧ replyLostCommitted.core.db ≠ seedDB ∧ replyLostCommitted.audit ≠ priorAudit := by
  refine ⟨lost_reply_execution, lost_reply_output, ?_, ?_⟩
  · intro equal
    have instanceEqual := congrArg RegistrationLifetime.Database.instanceRow equal
    change none = some instancePayload at instanceEqual
    cases instanceEqual
  · intro equal
    have recordEqual := congrFun equal 200
    change some retryDeregisterFirst = none at recordEqual
    cases recordEqual

#print axioms partial_execution
#print axioms conflicting_second_append_refused
#print axioms rollback_complete
#print axioms retry_execution
#print axioms removal_recovery_complete
#print axioms lost_reply_execution
#print axioms error_does_not_imply_rollback

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``write_attempt_ordinary, ``removal_prefix_execution, ``append_at_execution,
      ``partial_execution, ``conflicting_second_append_refused, ``abort_execution,
      ``confirmed_rollback_output, ``aborted_not_live, ``rollback_complete,
      ``retry_pending_execution, ``retry_records_compatible, ``retry_execution,
      ``removed_database, ``removed_audit, ``retry_output, ``removal_recovery_complete,
      ``lost_reply_execution, ``lost_reply_output, ``error_does_not_imply_rollback] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected audited removal recovery axiom in {declaration}: {unexpected}"

end RegistrationAuditedLifetime.RemovalRecovery
