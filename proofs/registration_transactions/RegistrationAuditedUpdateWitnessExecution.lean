import RegistrationAuditedUpdateWitnesses
import Lean.Util.CollectAxioms

/-! Actual populated steps and complete DB effect, independently importable from
acceptance. No RegistrationAuditedUpdateProof or output-assertion import. -/
namespace RegistrationAuditedUpdate.Witnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationTransaction (put)

theorem seed_wellFormed : RegistrationTransaction.WellFormed seedDB := by
  constructor
  · intro key row stored
    change (if key = 30 then some first else if key = 31 then some second else none) = some row at stored
    split_ifs at stored with firstKey secondKey
    · cases stored
      subst key
      exact ⟨rfl, rfl⟩
    · cases stored
      subst key
      exact ⟨rfl, rfl⟩
  · intro firstKey secondKey left right leftStored rightStored sameLogin
    change (if firstKey = 30 then some first else if firstKey = 31 then some second else none) = some left at leftStored
    change (if secondKey = 30 then some first else if secondKey = 31 then some second else none) = some right at rightStored
    split_ifs at leftStored <;> split_ifs at rightStored <;>
      simp_all [first, second]
    all_goals
      subst left
      subst right
      simp at sameLogin

/-- Explicit selected native admission interpretation. This does not establish
synthetic token parsing/signatures or a native history creating the seed. -/
theorem input_admitted : metadataAccepted request.metadata seedDB.metadata.2 ∧
    first.metadata.1.server = request.metadata.server ∧
    first.payload.tokenDigest = request.presentedDigest ∧
    first.metadata.1 ≠ request.metadata ∧ seedDB.registrations request.client = some first := by
  refine ⟨⟨rfl, rfl⟩, rfl, rfl, by decide, rfl⟩

theorem writing_ordinary (s : World) (tx : Tx) (lock : Option Nat)
    (notCommitting : tx.phase ≠ .committing) :
    Ordinary s.core (writing s tx lock).core := by
  refine ⟨rfl, ?_⟩
  intro subject previous
  change (put s.core.updates 0 tx subject).phase ≠ .committing
  by_cases same : subject = 0
  · simpa [put, same] using notCommitting
  · simpa [put, same] using previous

theorem update_ready_execution : Execution methods metadataAccepted source encoding start ready := by
  have one : Step methods metadataAccepted source encoding start preflight :=
    Step.ordinary start preflight.core
      (RegistrationUpdate.Step.preflight start.core 0 first rfl rfl rfl rfl (by decide))
      (writing_ordinary _ _ _ (by decide))
  have two : Step methods metadataAccepted source encoding preflight begun :=
    Step.ordinary preflight begun.core (RegistrationUpdate.Step.beginUpdate preflight.core 0 rfl)
      (writing_ordinary _ _ _ (by decide))
  have three : Step methods metadataAccepted source encoding begun metadataWritten :=
    Step.ordinary begun metadataWritten.core
      (RegistrationUpdate.Step.metadata begun.core 0 (Or.inl ⟨rfl, rfl⟩) rfl ⟨rfl, rfl⟩)
      (writing_ordinary _ _ _ (by decide))
  have four : Step methods metadataAccepted source encoding metadataWritten ready :=
    Step.ordinary metadataWritten ready.core
      (RegistrationUpdate.Step.matchRow metadataWritten.core 0 first ⟨rfl, rfl⟩ rfl)
      (writing_ordinary _ _ _ (by decide))
  exact .tail (.tail (.tail (.tail (.refl start) one) two) three) four

/-- A selected empty cursor performs no append; a nonempty cursor must execute
its actual next instruction through CoreAppend. No desired result is a premise. -/
theorem append_pending_execution (s : World) (current : Cursor String Draft Nat)
    (record : Record Draft Nat) (visibleAfter : Audit)
    (readyPhase : (s.core.updates 0).phase = .ready)
    (selected : s.cursors 0 = some current)
    (matching : current.remaining.head? = none ∨
      current.remaining.head? = some (recordIntent encoding record))
    (accepted : CoreAppend encoding (overlay encoding s.audit current.acknowledged) record visibleAfter) :
    Execution methods metadataAccepted source encoding s (appendPending s record) := by
  cases head : current.remaining with
  | nil =>
    simp only [appendPending, selected, head]
    exact .refl s
  | cons wanted remaining =>
    have constructed : recordIntent encoding record = wanted := by
      rcases matching with absent | present
      · simp [head] at absent
      · exact (Option.some.inj (by simpa [head] using present)).symm
    have step : Step methods metadataAccepted source encoding s
        (RegistrationAuditedUpdate.setCursor s 0 (advance current remaining record)) :=
      Step.append s 0 current wanted remaining record visibleAfter readyPhase selected head constructed accepted
    simpa only [appendPending, selected, head] using Execution.tail (.refl s) step

theorem execution_trans {first middle last : World}
    (left : Execution methods metadataAccepted source encoding first middle)
    (right : Execution methods metadataAccepted source encoding middle last) :
    Execution methods metadataAccepted source encoding first last := by
  induction right with
  | refl => exact left
  | tail previous step ih => exact .tail ih step


theorem update_audit_execution : Execution methods metadataAccepted source encoding ready appended := by
  have beginStep : Step methods metadataAccepted source encoding ready auditBegun :=
    Step.beginAudit ready 0 rfl rfl
  have appendStep : Execution methods metadataAccepted source encoding auditBegun appended :=
    append_pending_execution auditBegun cursor0 updateRecord (put priorAudit 100 (some updateRecord))
      rfl rfl (by decide) ⟨by decide, Or.inl rfl, rfl⟩
  exact execution_trans (.tail (.refl ready) beginStep) appendStep

/-- Freshness is for the actual acknowledged subset, not a hardcoded required
Update record. This remains a valid path premise if sourceProgram later omits it. -/
theorem acknowledged_fresh : NewIdentities encoding committing.audit finalCursor.acknowledged := by
  have known : finalCursor.acknowledged ⊆ [updateRecord] := by
    simp [finalCursor, appended, appendPending, auditBegun, RegistrationAuditedUpdate.setCursor,
      RegistrationAuditedUpdate.sourceProgram, cursor, advance, put]
  constructor
  · have actual : finalCursor.acknowledged = [updateRecord] ∨ finalCursor.acknowledged = [] := by
      simp [finalCursor, appended, appendPending, auditBegun, RegistrationAuditedUpdate.setCursor,
        RegistrationAuditedUpdate.sourceProgram, cursor, advance, put]
    rcases actual with one | empty
    · simp [one]
    · simp [empty]
  · intro record member
    have same := List.mem_singleton.mp (known member)
    subst record
    rfl

theorem update_commit_execution : Execution methods metadataAccepted source encoding appended acknowledged := by
  have enter : Step methods metadataAccepted source encoding appended committing :=
    Step.requestCommit appended 0 finalCursor rfl rfl rfl
  have compatible : Compatible encoding committing.audit finalCursor.acknowledged := by
    intro record member
    exact Or.inl (acknowledged_fresh.2 record member)
  have commit : Step methods metadataAccepted source encoding committing committed :=
    Step.serverCommit committing 0 finalCursor rfl rfl rfl compatible
  have acknowledge : Step methods metadataAccepted source encoding committed acknowledged :=
    Step.ordinary committed acknowledged.core
      (RegistrationUpdate.Step.acknowledgeUpdate committed.core 0 rfl rfl)
      (writing_ordinary _ _ _ (by decide))
  exact .tail (.tail (.tail (.refl appended) enter) commit) acknowledge


theorem update_execution : Execution methods metadataAccepted source encoding start acknowledged :=
  execution_trans (execution_trans update_ready_execution update_audit_execution) update_commit_execution

theorem update_database : acknowledged.core.core.db = expectedDB := by
  change RegistrationUpdate.commit seedDB
    [.deploymentMetadata nextMetadata, .matchedRow 30 (RegistrationUpdate.patched first request)] = expectedDB
  apply RegistrationTransaction.Database.ext
  · rfl
  · funext key
    by_cases current : key = 30
    · subst key
      rfl
    by_cases other : key = 31
    · subst key
      rfl
    simp [RegistrationUpdate.commit, RegistrationUpdate.applyWrite, RegistrationUpdate.patched,
      seedDB, expectedDB, request, put, current, other]
  · rfl
  · rfl

#print axioms seed_wellFormed
#print axioms input_admitted
#print axioms update_execution
#print axioms update_database
#print axioms acknowledged_fresh

run_cmd do
  for declaration in [``seed_wellFormed, ``input_admitted, ``writing_ordinary,
      ``update_ready_execution, ``append_pending_execution, ``update_audit_execution,
      ``acknowledged_fresh, ``update_commit_execution, ``execution_trans,
      ``update_execution, ``update_database] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unexpected populated UPDATE execution axiom in {declaration}: {name}"

end RegistrationAuditedUpdate.Witnesses
