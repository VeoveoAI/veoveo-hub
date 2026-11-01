import RegistrationAuditedLifetimeWitnesses
import Lean.Util.CollectAxioms

/-! Actual populated Step chain and independent complete result. No audited or
lifetime correctness proof is imported: later model mutations cannot be hidden
behind a failed generic correctness prerequisite. -/
namespace RegistrationAuditedLifetime.Witnesses
open RegistrationAuditedLifetime
open RegistrationTransaction (put rowMap)

theorem writing_ordinary (s : World) (tx : Tx) (instanceOwner deploymentOwner : Option Nat)
    (notCommitting : tx.phase ≠ .committing) :
    Ordinary s.core (writing s tx instanceOwner deploymentOwner).core := by
  refine ⟨rfl, ?_⟩
  intro subject previous
  change (put s.core.transactions 0 tx subject).phase ≠ .committing
  by_cases same : subject = 0
  · simpa [put, same] using notCommitting
  · simpa [put, same] using previous

/-- The seed has two distinct clients/logins and all modeled SQL bindings; no
native history producing this seed or native identifier parsing is asserted. -/
theorem seed_valid : RegistrationLifetime.InitialValid seedDB := by
  refine ⟨⟨?_, ?_⟩, ?_⟩
  · intro key row stored
    change (if key = 30 then some oldFirst else if key = 31 then some oldSecond else none) = some row at stored
    split_ifs at stored with first second
    · cases stored
      subst key
      exact ⟨rfl, rfl, rfl⟩
    · cases stored
      subst key
      exact ⟨rfl, rfl, rfl⟩
  · intro first second left right leftStored rightStored sameLogin
    change (if first = 30 then some oldFirst else if first = 31 then some oldSecond else none) = some left at leftStored
    change (if second = 30 then some oldFirst else if second = 31 then some oldSecond else none) = some right at rightStored
    split_ifs at leftStored <;> split_ifs at rightStored <;>
      simp_all [oldFirst, oldSecond]
    all_goals
      subst left
      subst right
      simp at sameLogin
  · simp [RegistrationLifetime.NoOrphanRows, seedDB]

theorem captured_listing : RegistrationLifetime.Listing
    (RegistrationLifetime.applyWrites upserted.core.db (upserted.core.transactions 0).staged)
    [oldFirst, oldSecond] := by
  refine ⟨by decide, ?_⟩
  change seedDB.registrations = rowMap [oldFirst, oldSecond]
  funext key
  simp [seedDB, rowMap, put, oldFirst, oldSecond]

theorem candidate_insert_valid : RegistrationLifetime.ValidWrite
    (RegistrationLifetime.applyWrites created.core.db (created.core.transactions 0).staged)
    (.registration (.insert candidate)) := by
  refine ⟨by decide, by decide, by decide, ?_⟩
  intro key row stored
  have cleared :
      (RegistrationLifetime.applyWrites created.core.db (created.core.transactions 0).staged).registrations =
        fun _ => none := by
    funext key
    by_cases first : key = 30
    · subst key
      rfl
    by_cases second : key = 31
    · subst key
      rfl
    simp [RegistrationLifetime.applyWrites, RegistrationLifetime.applyWrite, created, creating,
      droppedSecond, droppedFirst, captured, upserted, prepared, selected, waiting, cached, start,
      initial, writing, RegistrationLifetime.initial, RegistrationLifetime.update, put, seedDB,
      first, second]
  rw [cleared] at stored
  cases stored

/-- The complete data prefix constructs the source's real captured rows, drops
both logins, creates the candidate and stages INSERT before audit begins. -/
theorem replacement_ready_execution : Execution source encoding start ready := by
  have one : Step source encoding start cached :=
    Step.ordinary start cached.core
      (RegistrationLifetime.Step.preflight start.core 0 proposal instancePayload rfl rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have two : Step source encoding cached waiting :=
    Step.ordinary cached waiting.core
      (RegistrationLifetime.Step.beginRegister cached.core 0 proposal rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have three : Step source encoding waiting selected :=
    Step.ordinary waiting selected.core
      (RegistrationLifetime.Step.acquire waiting.core 0 instancePayload rfl rfl rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have four : Step source encoding selected prepared :=
    Step.ordinary selected prepared.core
      (RegistrationLifetime.Step.prepared selected.core 0 proposal rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have five : Step source encoding prepared upserted :=
    Step.ordinary prepared upserted.core
      (RegistrationLifetime.Step.upsert prepared.core 0 proposal rfl rfl (Or.inl rfl))
      (writing_ordinary _ _ _ _ (by decide))
  have six : Step source encoding upserted captured :=
    Step.ordinary upserted captured.core
      (RegistrationLifetime.Step.delete upserted.core 0 [oldFirst, oldSecond] rfl captured_listing)
      (writing_ordinary _ _ _ _ (by decide))
  have seven : Step source encoding captured droppedFirst :=
    Step.ordinary captured droppedFirst.core
      (RegistrationLifetime.Step.drop captured.core 0 40 [41] rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have eight : Step source encoding droppedFirst droppedSecond :=
    Step.ordinary droppedFirst droppedSecond.core
      (RegistrationLifetime.Step.drop droppedFirst.core 0 41 [] rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have nine : Step source encoding droppedSecond creating :=
    Step.ordinary droppedSecond creating.core
      (RegistrationLifetime.Step.droppedRegister droppedSecond.core 0 proposal rfl rfl rfl)
      (writing_ordinary _ _ _ _ (by decide))
  have ten : Step source encoding creating created :=
    Step.ordinary creating created.core
      (RegistrationLifetime.Step.create creating.core 0 proposal rfl rfl (by
        change (RegistrationLifetime.applyWrites creating.core.db
          (creating.core.transactions 0).staged).logins 20 = none
        rfl))
      (writing_ordinary _ _ _ _ (by decide))
  have eleven : Step source encoding created ready :=
    Step.ordinary created ready.core
      (RegistrationLifetime.Step.insert created.core 0 proposal rfl rfl candidate_insert_valid)
      (writing_ordinary _ _ _ _ (by decide))
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
      (Execution.tail (.refl start) one) two) three) four) five) six) seven) eight) nine) ten) eleven

/-- The bounded fixture helper has an actual legal path in BOTH cases. An empty
cursor performs no append; a nonempty one must match its real selected head and
successfully append the complete record. This lemma supplies no desired output. -/
theorem append_pending_execution (s : World) (current : Cursor String Draft Nat)
    (record : Record Draft Nat) (visibleAfter : Audit)
    (readyPhase : (s.core.transactions 0).phase = .ready)
    (selectedCursor : s.cursors 0 = some current)
    (matching : current.remaining.head? = none ∨
      current.remaining.head? = some (recordIntent encoding record))
    (accepted : CoreAppend encoding (overlay encoding s.audit current.acknowledged) record visibleAfter) :
    Execution source encoding s (appendPending s record) := by
  cases head : current.remaining with
  | nil =>
    simp only [appendPending, selectedCursor, head]
    exact .refl s
  | cons wanted remaining =>
    have constructed : recordIntent encoding record = wanted := by
      rcases matching with absent | present
      · simp [head] at absent
      · have same : wanted = recordIntent encoding record :=
          Option.some.inj (by simpa [head] using present)
        exact same.symm
    have step : Step source encoding s (setCursor s 0 (advance current remaining record)) :=
      Step.append s 0 current wanted remaining record visibleAfter readyPhase selectedCursor head constructed accepted
    simpa only [appendPending, selectedCursor, head] using Execution.tail (.refl s) step

/-- Baseline has three real one-record appends. The candidate append uses server
B while both ended-record appends use A. No total-list limit is assumed. -/
theorem replacement_audit_execution : Execution source encoding ready appendedRegister := by
  have begunStep : Step source encoding ready begun := Step.beginAudit ready 0 rfl rfl
  have first : Execution source encoding begun appendedFirst :=
    append_pending_execution begun cursor0 replaceFirst (put priorAudit 100 (some replaceFirst))
      rfl rfl (by decide) ⟨by decide, Or.inl rfl, rfl⟩
  have second : Execution source encoding appendedFirst appendedSecond :=
    append_pending_execution appendedFirst cursor1 replaceSecond
      (put (overlay encoding priorAudit [replaceFirst]) 101 (some replaceSecond))
      rfl rfl (by decide) ⟨by decide, Or.inl rfl, rfl⟩
  have last : Execution source encoding appendedSecond appendedRegister :=
    append_pending_execution appendedSecond cursor2 registerRecord
      (put (overlay encoding priorAudit [replaceFirst, replaceSecond]) 102 (some registerRecord))
      rfl rfl (by decide) ⟨by decide, Or.inl rfl, rfl⟩
  have trans {a b c : World} (left : Execution source encoding a b)
      (right : Execution source encoding b c) : Execution source encoding a c := by
    induction right with
    | refl => exact left
    | tail previous step ih => exact Execution.tail ih step
  exact trans (trans (trans (Execution.tail (.refl ready) begunStep) first) second) last

theorem records_new : NewIdentities encoding priorAudit cursor3.acknowledged := by
  refine ⟨by decide, ?_⟩
  intro record member
  have known : record ∈ [replaceFirst, replaceSecond, registerRecord] := by
    simp [cursor3, appendedRegister, appendedSecond, appendedFirst, appendPending, begun,
      setCursor, cursor, advance, sourceProgram, source, ready, created, creating,
      droppedSecond, droppedFirst, captured, upserted, prepared, selected, waiting, cached,
      writing, start, initial, RegistrationLifetime.initial, RegistrationLifetime.update,
      put, requests, proposal, oldFirst, oldSecond, candidate] at member
    simp only [List.mem_cons]
    aesop
  simp [List.mem_cons] at known
  rcases known with rfl | rfl | rfl <;> rfl

theorem replacement_commit_execution : Execution source encoding appendedRegister acknowledged := by
  have enter : Step source encoding appendedRegister committing :=
    Step.requestCommit appendedRegister 0 cursor3 rfl rfl rfl
  have compatible : Compatible encoding committing.audit cursor3.acknowledged := by
    intro record member
    exact Or.inl (records_new.2 record member)
  have commit : Step source encoding committing committed :=
    Step.serverCommit committing 0 cursor3 rfl rfl rfl compatible
  have acknowledge : Step source encoding committed acknowledged := by
    apply Step.ordinary committed acknowledged.core
      (RegistrationLifetime.Step.acknowledge committed.core 0 rfl rfl)
    change Ordinary committed.core (writing committed
      { committed.core.transactions 0 with response := .registered } none none).core
    exact writing_ordinary _ _ _ _ (by decide)
  exact Execution.tail (Execution.tail (Execution.tail (.refl appendedRegister) enter) commit) acknowledge

theorem execution_trans {first middle last : World}
    (left : Execution source encoding first middle) (right : Execution source encoding middle last) :
    Execution source encoding first last := by
  induction right with
  | refl => exact left
  | tail previous step ih => exact Execution.tail ih step

theorem replacement_execution : Execution source encoding start acknowledged :=
  execution_trans (execution_trans replacement_ready_execution replacement_audit_execution)
    replacement_commit_execution

theorem replacement_database : acknowledged.core.db = expectedDB := by
  change RegistrationLifetime.applyWrites seedDB
    [.upsertDeployment newMetadata "unused-first-deployment-value", .registration (.deleteRows [30, 31]),
      .registration (.dropLogin 40), .registration (.dropLogin 41),
      .registration (.createLogin 20 "new-registration-role"), .registration (.insert candidate)] = expectedDB
  apply RegistrationLifetime.Database.ext
  · rfl
  · rfl
  · funext key
    by_cases newKey : key = 10
    · subst key
      rfl
    by_cases oldKey : key = 30
    · subst key
      rfl
    by_cases secondKey : key = 31
    · subst key
      rfl
    simp [RegistrationLifetime.applyWrites, RegistrationLifetime.applyWrite, seedDB, expectedDB,
      put, candidate, newKey, oldKey, secondKey]
  · funext login
    by_cases newLogin : login = 20
    · subst login
      rfl
    by_cases oldLogin : login = 40
    · subst login
      rfl
    by_cases secondLogin : login = 41
    · subst login
      rfl
    simp [RegistrationLifetime.applyWrites, RegistrationLifetime.applyWrite, seedDB, expectedDB,
      put, newLogin, oldLogin, secondLogin]
  · rfl

theorem replacement_audit : acknowledged.audit = expectedAudit := by
  change overlay encoding priorAudit [replaceFirst, replaceSecond, registerRecord] = expectedAudit
  funext id
  by_cases first : id = 100
  · subst id
    rfl
  by_cases second : id = 101
  · subst id
    rfl
  by_cases last : id = 102
  · subst id
    rfl
  simp [overlay, encoding, replaceFirst, replaceSecond, registerRecord, priorAudit, expectedAudit,
    put, first, second, last]

/-- Independent complete output plus explicit execution, preventing a disconnected
endpoint computation or unconditional refusal from qualifying this witness. -/
theorem replacement_complete : Execution source encoding start acknowledged ∧ ReplacementAccepted := by
  refine ⟨replacement_execution, replacement_database, replacement_audit,
    rfl, rfl, rfl, rfl, rfl, rfl, rfl, ?_, ?_, ?_⟩
  all_goals decide

#print axioms seed_valid
#print axioms replacement_ready_execution
#print axioms replacement_audit_execution
#print axioms replacement_execution
#print axioms replacement_database
#print axioms replacement_audit
#print axioms replacement_complete

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``writing_ordinary, ``seed_valid, ``captured_listing,
      ``candidate_insert_valid, ``replacement_ready_execution, ``append_pending_execution,
      ``replacement_audit_execution,
      ``records_new, ``replacement_commit_execution, ``execution_trans, ``replacement_execution,
      ``replacement_database, ``replacement_audit, ``replacement_complete] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected audited replacement witness axiom in {declaration}: {unexpected}"

end RegistrationAuditedLifetime.Witnesses
