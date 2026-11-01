import RegistrationAuditedUpdateRecoveryWitnesses
import RegistrationAuditedUpdateWitnessExecution

/-! Actual model steps plus independent complete recovery outputs. Core refusal
is proved separately: its native SQLx error propagation to fail/confirmed rollback
is a source correspondence boundary, not inferred from a missing append step. -/
namespace RegistrationAuditedUpdate.RecoveryWitnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationAuditedUpdate.Witnesses
open RegistrationTransaction (put)

theorem writing_ordinary (s : World) (actor : Nat) (tx : Tx) (lock : Option Nat)
    (notCommitting : tx.phase ≠ .committing) : Ordinary s.core (writingAt s actor tx lock).core := by
  refine ⟨rfl, ?_⟩
  intro subject previous
  change (put s.core.updates actor tx subject).phase ≠ .committing
  by_cases same : subject = actor
  · simpa [put, same] using notCommitting
  · simpa [put, same] using previous

theorem writing_other (s : World) (actor subject : Nat) (tx : Tx) (lock : Option Nat)
    (other : subject ≠ actor) :
    (writingAt s actor tx lock).core.updates subject = s.core.updates subject := by
  simp only [writingAt, RegistrationUpdate.updateTx, put, if_neg other]

/-- Small attempt-indexed four-step fixture prefix, using the REAL primitive
preflight/deployment/current-row statements. Premises are source inputs/reads,
never a desired final DB, successful whole operation or correct audit batch. -/
theorem ready_execution (s : World) (actor : Nat)
    (method : methods actor = .update)
    (idle : (s.core.updates actor).phase = .idle)
    (original : (s.core.updates actor).request = request)
    (free : s.core.core.lock = none)
    (found : s.core.core.db.registrations (s.core.updates actor).request.client = some first)
    (valid : metadataAccepted (s.core.updates actor).request.metadata s.core.core.db.metadata.2) :
    Execution methods metadataAccepted source encoding s (readyAt s actor) := by
  have one : Step methods metadataAccepted source encoding s (preflightAt s actor) :=
    Step.ordinary s (preflightAt s actor).core
      (RegistrationUpdate.Step.preflight s.core actor first method idle found
        (by rw [original]; rfl) (by rw [original]; decide))
      (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.preflight ≠ .committing; decide))
  have two : Step methods metadataAccepted source encoding (preflightAt s actor) (beginAt s actor) :=
    Step.ordinary (preflightAt s actor) (beginAt s actor).core
      (RegistrationUpdate.Step.beginUpdate (preflightAt s actor).core actor (by
        simp [preflightAt, writingAt, RegistrationUpdate.updateTx, put]))
      (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.begun ≠ .committing; decide))
  have three : Step methods metadataAccepted source encoding (beginAt s actor) (metadataAt s actor) :=
    Step.ordinary (beginAt s actor) (metadataAt s actor).core
      (RegistrationUpdate.Step.metadata (beginAt s actor).core actor
        (Or.inl ⟨rfl, by simp [beginAt, writingAt, RegistrationUpdate.updateTx, put]⟩)
        (by simpa [beginAt, preflightAt, writingAt, RegistrationUpdate.updateTx] using free)
        (by simpa [beginAt, preflightAt, writingAt, RegistrationUpdate.updateTx, put] using valid))
      (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.metadataWritten ≠ .committing; decide))
  have four : Step methods metadataAccepted source encoding (metadataAt s actor) (readyAt s actor) :=
    Step.ordinary (metadataAt s actor) (readyAt s actor).core
      (RegistrationUpdate.Step.matchRow (metadataAt s actor).core actor first
        ⟨rfl, by simp [metadataAt, writingAt, RegistrationUpdate.updateTx, put]⟩
        (by simpa [metadataAt, beginAt, preflightAt, writingAt, RegistrationUpdate.updateTx, put] using found))
      (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.ready ≠ .committing; decide))
  exact .tail (.tail (.tail (.tail (.refl s) one) two) three) four

theorem execution_trans {a b c : World}
    (left : Execution methods metadataAccepted source encoding a b)
    (right : Execution methods metadataAccepted source encoding b c) :
    Execution methods metadataAccepted source encoding a c := by
  induction right with
  | refl => exact left
  | tail previous step ih => exact .tail ih step

theorem first_private_execution : Execution methods metadataAccepted source encoding start auditing0 := by
  have readyPath : Execution methods metadataAccepted source encoding start ready0 :=
    ready_execution start 0 rfl rfl rfl rfl rfl ⟨rfl, rfl⟩
  exact .tail readyPath (Step.beginAudit ready0 0 rfl rfl)

/-- Correct requested instruction and stored-time encoding, with no successful
private append yet. The conflict below is therefore the specific ID/full-record
collision, not an instruction/time refusal or malformed source input. -/
theorem refused_record_valid : auditing0.cursors 0 = some expectedPendingCursor ∧
    expectedPendingCursor.remaining.head? = some (recordIntent encoding refusedRecord) ∧
    recordIntent encoding refusedRecord = ⟨.update, source.server request.metadata⟩ ∧
    refusedRecord.occurredAt = encoding.storedTime refusedRecord.draft ∧
    auditing0.audit 900 = some priorRecord := by
  exact ⟨rfl, rfl, rfl, by decide, rfl⟩

theorem conflicting_append_refused (next : Audit) :
    ¬CoreAppend encoding
      (overlay encoding auditing0.audit expectedPendingCursor.acknowledged) refusedRecord next := by
  have different : priorRecord ≠ refusedRecord := by
    intro equal
    have activity := congrArg (fun record : Record Draft Nat => record.draft.intent.activity) equal
    change Activity.register = Activity.update at activity
    cases activity
  rintro ⟨_, compatible, _⟩
  change priorAudit 900 = none ∨ priorAudit 900 = some refusedRecord at compatible
  rcases compatible with absent | identical
  · change some priorRecord = none at absent
    cases absent
  · change some priorRecord = some refusedRecord at identical
    exact different (Option.some.inj identical)

/-- Native refused-call→fail is explicit correspondence. fail publishes the
.error tag and retains the lock; only serverAbort below confirms rollback. -/
theorem failure_step : Step methods metadataAccepted source encoding auditing0 failed :=
  Step.ordinary auditing0 failed.core
    (RegistrationUpdate.Step.fail auditing0.core 0 rfl
      (by change RegistrationUpdate.Phase.ready ≠ .committed; decide)
      (by change RegistrationUpdate.Phase.ready ≠ .committing; decide)
      (by change RegistrationUpdate.Phase.ready ≠ .aborted; decide))
    (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.rollbackPending ≠ .committing; decide))

theorem abort_step : Step methods metadataAccepted source encoding failed aborted :=
  Step.ordinary failed aborted.core
    (RegistrationUpdate.Step.serverAbort failed.core 0 (Or.inr rfl))
    (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.aborted ≠ .committing; decide))

theorem failure_execution : Execution methods metadataAccepted source encoding start failed :=
  .tail first_private_execution failure_step

theorem abort_execution : Execution methods metadataAccepted source encoding start aborted :=
  .tail failure_execution abort_step

theorem error_then_abort : (failed.core.updates 0).phase = .rollbackPending ∧
    (failed.core.updates 0).response = .error ∧ failed.core.core.lock = some 0 ∧
    failed.core.core.db = seedDB ∧ failed.audit = priorAudit ∧
    (aborted.core.updates 0).phase = .aborted ∧ (aborted.core.updates 0).response = .error ∧
    aborted.core.core.lock = none := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem failure_other_updates (id : Nat) (other : id ≠ 0) :
    failed.core.updates id = start.core.updates id ∧ aborted.core.updates id = start.core.updates id := by
  have preserved : failed.core.updates id = start.core.updates id := by
    calc
      failed.core.updates id = auditing0.core.updates id := writing_other _ _ _ _ _ other
      _ = ready0.core.updates id := rfl
      _ = (metadataAt start 0).core.updates id := writing_other _ _ _ _ _ other
      _ = (beginAt start 0).core.updates id := writing_other _ _ _ _ _ other
      _ = (preflightAt start 0).core.updates id := writing_other _ _ _ _ _ other
      _ = start.core.updates id := writing_other _ _ _ _ _ other
  exact ⟨preserved, (writing_other _ _ _ _ _ other).trans preserved⟩

theorem failure_outputs : FailureOutputs failed .rollbackPending (some 0) ∧
    FailureOutputs aborted .aborted none := by
  refine ⟨⟨rfl, rfl, rfl, ?_, rfl, rfl, rfl, rfl⟩,
    ⟨rfl, rfl, rfl, ?_, rfl, rfl, rfl, rfl⟩⟩
  · funext id
    by_cases actor : id = 0
    · subst id
      rfl
    simp only [expectedFailureUpdates, put, if_neg actor]
    exact (failure_other_updates id actor).1
  · funext id
    by_cases actor : id = 0
    · subst id
      rfl
    simp only [expectedFailureUpdates, put, if_neg actor]
    exact (failure_other_updates id actor).2

theorem aborted_cursor_retired : aborted.cursors 0 = some expectedPendingCursor ∧
    ¬RegistrationAuditedUpdate.ActivePrivate (aborted.core.updates 0).phase := by
  refine ⟨rfl, ?_⟩
  change ¬RegistrationAuditedUpdate.ActivePrivate .aborted
  simp [RegistrationAuditedUpdate.ActivePrivate]

theorem retry_execution : Execution methods metadataAccepted source encoding aborted retryAcknowledged := by
  have readyPath : Execution methods metadataAccepted source encoding aborted retryReady :=
    ready_execution aborted 1 rfl rfl rfl rfl rfl ⟨rfl, rfl⟩
  have beginAudit : Step methods metadataAccepted source encoding retryReady retryAuditing :=
    Step.beginAudit retryReady 1 rfl rfl
  have append : Step methods metadataAccepted source encoding retryAuditing retryAppended :=
    Step.append retryAuditing 1 retryCursor0 ⟨.update, server⟩ [] updateRecord
      (put priorAudit 100 (some updateRecord)) rfl rfl rfl rfl
      ⟨by decide, Or.inl rfl, rfl⟩
  have enter : Step methods metadataAccepted source encoding retryAppended retryCommitting :=
    Step.requestCommit retryAppended 1 retryCursor1 rfl rfl rfl
  have compatible : Compatible encoding retryCommitting.audit retryCursor1.acknowledged := by
    intro record member
    change record ∈ [updateRecord] at member
    have same := List.mem_singleton.mp member
    subst record
    exact Or.inl rfl
  have commit : Step methods metadataAccepted source encoding retryCommitting retryCommitted :=
    Step.serverCommit retryCommitting 1 retryCursor1 rfl rfl rfl compatible
  have acknowledge : Step methods metadataAccepted source encoding retryCommitted retryAcknowledged :=
    Step.ordinary retryCommitted retryAcknowledged.core
      (RegistrationUpdate.Step.acknowledgeUpdate retryCommitted.core 1 rfl rfl)
      (writing_ordinary _ _ _ _ (by change RegistrationUpdate.Phase.committed ≠ .committing; decide))
  exact .tail (.tail (.tail (.tail (.tail readyPath beginAudit) append) enter) commit) acknowledge

theorem retry_fresh : NewIdentities encoding retryCommitting.audit retryCursor1.acknowledged := by
  refine ⟨by decide, ?_⟩
  intro record member
  change record ∈ [updateRecord] at member
  have same := List.mem_singleton.mp member
  subst record
  rfl

theorem retry_database : retryAcknowledged.core.core.db = expectedDB := by
  change RegistrationUpdate.commit seedDB
    [.deploymentMetadata nextMetadata, .matchedRow 30 (RegistrationUpdate.patched first request)] = expectedDB
  exact RegistrationAuditedUpdate.Witnesses.update_database

theorem retry_outputs : RetryOutputs := by
  refine ⟨retry_database, rfl, rfl, ?_, rfl, rfl, rfl, ?_⟩
  · funext id
    by_cases secondActor : id = 1
    · subst id
      rfl
    by_cases firstActor : id = 0
    · subst id
      rfl
    simp only [expectedRetryUpdates, if_neg secondActor, if_neg firstActor]
    calc
      retryAcknowledged.core.updates id = retryCommitted.core.updates id :=
        writing_other _ _ _ _ _ secondActor
      _ = retryCommitting.core.updates id := by
        simp only [retryCommitted, RegistrationUpdate.committedState,
          RegistrationUpdate.updateTx, put, if_neg secondActor]
      _ = retryAppended.core.updates id := writing_other _ _ _ _ _ secondActor
      _ = retryReady.core.updates id := rfl
      _ = (metadataAt aborted 1).core.updates id := writing_other _ _ _ _ _ secondActor
      _ = (beginAt aborted 1).core.updates id := writing_other _ _ _ _ _ secondActor
      _ = (preflightAt aborted 1).core.updates id := writing_other _ _ _ _ _ secondActor
      _ = aborted.core.updates id := writing_other _ _ _ _ _ secondActor
      _ = start.core.updates id := (failure_other_updates id firstActor).2
  · funext id
    by_cases secondActor : id = 1
    · subst id
      rfl
    by_cases firstActor : id = 0
    · subst id
      rfl
    simp only [expectedRetryCursors, if_neg secondActor, if_neg firstActor]
    calc
      retryAcknowledged.cursors id = retryAppended.cursors id := rfl
      _ = retryAuditing.cursors id := by
        simp only [retryAppended, RegistrationAuditedUpdate.setCursor, put, if_neg secondActor]
      _ = retryReady.cursors id := by
        simp only [retryAuditing, RegistrationAuditedUpdate.setCursor, put, if_neg secondActor]
      _ = aborted.cursors id := rfl
      _ = none := by
        change expectedFailureCursors id = none
        simp only [expectedFailureCursors, put, if_neg firstActor]

/-- Both concrete actual attempts and the COMPLETE independent outputs. An error
is not used as evidence of rollback; the explicit abort Step is required. -/
theorem recovery_complete : RegistrationTransaction.WellFormed seedDB ∧
    Execution methods metadataAccepted source encoding start failed ∧
    Execution methods metadataAccepted source encoding start aborted ∧
    FailureOutputs failed .rollbackPending (some 0) ∧ FailureOutputs aborted .aborted none ∧
    (∀ next, ¬CoreAppend encoding
      (overlay encoding auditing0.audit expectedPendingCursor.acknowledged) refusedRecord next) ∧
    Execution methods metadataAccepted source encoding aborted retryAcknowledged ∧
    Execution methods metadataAccepted source encoding start retryAcknowledged ∧ RetryOutputs := by
  exact ⟨RegistrationAuditedUpdate.Witnesses.seed_wellFormed, failure_execution, abort_execution,
    failure_outputs.1, failure_outputs.2, conflicting_append_refused, retry_execution,
    execution_trans abort_execution retry_execution, retry_outputs⟩

#print axioms first_private_execution
#print axioms refused_record_valid
#print axioms conflicting_append_refused
#print axioms error_then_abort
#print axioms failure_outputs
#print axioms aborted_cursor_retired
#print axioms retry_execution
#print axioms retry_outputs
#print axioms recovery_complete

run_cmd do
  for declaration in [``writing_ordinary, ``writing_other, ``ready_execution, ``execution_trans,
      ``first_private_execution, ``refused_record_valid, ``conflicting_append_refused,
      ``failure_step, ``abort_step, ``failure_execution, ``abort_execution, ``error_then_abort,
      ``failure_other_updates, ``failure_outputs, ``aborted_cursor_retired,
      ``retry_execution, ``retry_fresh, ``retry_database, ``retry_outputs, ``recovery_complete] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unexpected audited UPDATE recovery axiom in {declaration}: {name}"

end RegistrationAuditedUpdate.RecoveryWitnesses
