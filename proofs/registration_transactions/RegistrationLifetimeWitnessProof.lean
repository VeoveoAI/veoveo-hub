import RegistrationLifetimeWitnesses

namespace RegistrationLifetime.Witnesses
open RegistrationLifetime
open RegistrationTransaction (put rowMap)

/-- A source-shaped first-registration start; the other transaction may already
have aborted. Only initial state facts are premises, not any desired after-state. -/
theorem before_insert_execution (fixed : Bool) (s : World)
    (database : s.db = seedDB)
    (fresh : s.transactions 0 = { request := .register proposal })
    (instanceFree : s.instanceOwner = none) (deploymentFree : s.deploymentOwner = none) :
    Execution fixed s (beforeInsert fixed s) := by
  have one : Step fixed s (cached s) :=
    Step.preflight s 0 proposal instancePayload (by simp [fresh]) (by simp [fresh])
      (by simp [database, seedDB])
  have two : Step fixed (cached s) (waitingRegister (cached s)) :=
    Step.beginRegister _ 0 proposal rfl (by simp [cached, update, put, fresh])
  have three : Step fixed (waitingRegister (cached s))
      (selectedRegister fixed (waitingRegister (cached s))) := by
    cases fixed with
    | false => exact Step.originalNoSelect _ 0 rfl rfl
    | true =>
      refine Step.acquire _ 0 instancePayload rfl rfl instanceFree ?_
      change s.db.instanceRow = some instancePayload
      rw [database]
      rfl
  let a := selectedRegister fixed (waitingRegister (cached s))
  have aPhase : (a.transactions 0).phase = .held := by
    cases fixed <;> simp [a, selectedRegister, waitingRegister, cached, update, put, fresh, afterLock]
  have aMethod : (a.transactions 0).request = .register proposal := by
    cases fixed <;> simp [a, selectedRegister, waitingRegister, cached, update, put, fresh]
  have four : Step fixed a (preparedRegister a) := Step.prepared _ 0 proposal aPhase aMethod
  have five : Step fixed (preparedRegister a) (upsertedRegister (preparedRegister a)) :=
    Step.upsert _ 0 proposal rfl (by simp [preparedRegister, update, put, aMethod])
      (Or.inl (by cases fixed <;> simpa [preparedRegister, a, selectedRegister,
        waitingRegister, cached, update] using deploymentFree))
  have snapshot : Listing
      (applyWrites (upsertedRegister (preparedRegister a)).db
        ((upsertedRegister (preparedRegister a)).transactions 0).staged) [] := by
    cases fixed <;>
      simp [Listing, applyWrites, applyWrite, rowMap, upsertedRegister, preparedRegister,
        a, selectedRegister, waitingRegister, cached, update, put, fresh, database, seedDB]
  have six : Step fixed (upsertedRegister (preparedRegister a))
      (capturedRows (upsertedRegister (preparedRegister a)) 0 []) := Step.delete _ 0 [] rfl snapshot
  let b := capturedRows (upsertedRegister (preparedRegister a)) 0 []
  have seven : Step fixed b (creatingRegister b) :=
    Step.droppedRegister _ 0 proposal rfl
      (by cases fixed <;> simp [b, capturedRows, upsertedRegister, preparedRegister, a,
        selectedRegister, waitingRegister, cached, update, put, fresh]) rfl
  have valid : ValidWrite (applyWrites (creatingRegister b).db ((creatingRegister b).transactions 0).staged)
      (.registration (.createLogin 20 "candidate-role")) := by
    cases fixed <;>
      simp [ValidWrite, applyWrites, applyWrite, creatingRegister, b, capturedRows, upsertedRegister,
        preparedRegister, a, selectedRegister, waitingRegister, cached, update, put,
        fresh, database, seedDB, unrelatedRoles]
  have eight : Step fixed (creatingRegister b) (createdRegister (creatingRegister b)) :=
    Step.create _ 0 proposal rfl
      (by cases fixed <;> simp [creatingRegister, b, capturedRows, upsertedRegister, preparedRegister,
        a, selectedRegister, waitingRegister, cached, update, put, fresh]) valid
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.tail (Execution.tail (Execution.refl s) one) two) three) four) five) six) seven) eight

theorem finish_register_execution (fixed : Bool) (s : World)
    (phase : (s.transactions 0).phase = .inserting)
    (method : (s.transactions 0).request = .register proposal)
    (valid : ValidWrite (applyWrites s.db (s.transactions 0).staged) (.registration (.insert candidate))) :
    Execution fixed s (finishRegister s) := by
  have one : Step fixed s (insertedRegister s) := Step.insert _ 0 proposal phase method valid
  have two : Step fixed (insertedRegister s) (committing (insertedRegister s) 0) := Step.requestCommit _ 0 rfl
  have three : Step fixed (committing (insertedRegister s) 0) (finishRegister s) := Step.serverCommit _ 0 rfl
  exact Execution.tail (Execution.tail (Execution.tail (Execution.refl s) one) two) three

/-- This helper retains original removal's actual late instance row lock and is
also used for the fixed lock order. It does not acquire a deployment lock. -/
theorem empty_removal_ready_execution (fixed : Bool) (s : World)
    (fresh : s.transactions 1 = { request := .remove })
    (present : s.db.instanceRow = some instancePayload)
    (free : s.instanceOwner = none) (snapshot : Listing s.db []) :
    Execution fixed s (emptyRemovalReady fixed s) := by
  have one : Step fixed s (waitingRemove s) := Step.beginRemove _ 1 (by simp [fresh]) (by simp [fresh])
  have two : Step fixed (waitingRemove s) (selectedRemove fixed (waitingRemove s)) := by
    cases fixed with
    | false => exact Step.originalNoSelect _ 1 rfl rfl
    | true => exact Step.acquire _ 1 instancePayload rfl rfl free present
  let a := selectedRemove fixed (waitingRemove s)
  have aPhase : (a.transactions 1).phase = .deleting := by
    cases fixed <;> simp [a, selectedRemove, waitingRemove, update, put, fresh, afterLock]
  have aMethod : (a.transactions 1).request = .remove := by
    cases fixed <;> simp [a, selectedRemove, waitingRemove, update, put, fresh]
  have listing : Listing (applyWrites a.db (a.transactions 1).staged) [] := by
    cases fixed <;> simpa [a, selectedRemove, waitingRemove, update, put, fresh, applyWrites] using snapshot
  have three : Step fixed a (capturedRows a 1 []) := Step.delete _ 1 [] aPhase listing
  let b := capturedRows a 1 []
  have four : Step fixed b (erasingRemove b) :=
    Step.droppedRemove _ 1 rfl (by simp [b, capturedRows, update, put, aMethod]) rfl
  have available : (erasingRemove b).instanceOwner = none ∨ (erasingRemove b).instanceOwner = some 1 := by
    cases fixed with
    | false => exact Or.inl free
    | true => exact Or.inr rfl
  have five : Step fixed (erasingRemove b) (erasedRemove (erasingRemove b)) :=
    Step.erase _ 1 instancePayload rfl
      (by simp [erasingRemove, b, capturedRows, update, put, aMethod]) available
      (by cases fixed <;> simpa [erasingRemove, b, capturedRows, a, selectedRemove,
        waitingRemove, update] using present)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.refl s) one) two) three) four) five

theorem empty_removal_execution (fixed : Bool) (s : World)
    (fresh : s.transactions 1 = { request := .remove })
    (present : s.db.instanceRow = some instancePayload)
    (free : s.instanceOwner = none) (snapshot : Listing s.db []) :
    Execution fixed s (finishEmptyRemoval fixed s) := by
  have ready := empty_removal_ready_execution fixed s fresh present free snapshot
  have commitRequest : Step fixed (emptyRemovalReady fixed s)
      (committing (emptyRemovalReady fixed s) 1) := Step.requestCommit _ 1 rfl
  have decision : Step fixed (committing (emptyRemovalReady fixed s) 1)
      (finishEmptyRemoval fixed s) := Step.serverCommit _ 1 rfl
  exact Execution.tail (Execution.tail ready commitRequest) decision

theorem acknowledge_execution (fixed : Bool) (s : World) (id : Nat)
    (phase : (s.transactions id).phase = .committed)
    (pending : (s.transactions id).response = .pending) :
    Execution fixed s (acknowledged s id) :=
  Execution.tail (Execution.refl s) (Step.acknowledge s id phase pending)

/-- The actual existing primitive constraints permit this orphan. In particular,
INSERT sees its own private deployment upsert; it never checks instance presence. -/
theorem original_orphan_execution : Execution false start originalFinal := by
  have prefixRun := before_insert_execution false start rfl rfl rfl rfl
  have removeRun := empty_removal_execution false (beforeInsert false start) rfl rfl rfl
    (by exact ⟨by simp, rfl⟩)
  have valid : ValidWrite (applyWrites originalRemoval.db (originalRemoval.transactions 0).staged)
      (.registration (.insert candidate)) := by
    simp [ValidWrite, originalRemoval, finishEmptyRemoval, emptyRemovalReady, erasedRemove,
      erasingRemove, capturedRows, selectedRemove, waitingRemove, beforeInsert, createdRegister,
      creatingRegister, upsertedRegister, preparedRegister, selectedRegister, waitingRegister,
      cached, start, initial, requests, seedDB, candidate, proposal, metadata, payload,
      committedState, committing, applyWrites, applyWrite, upsert, update, put, unrelatedRoles]
  have finishRun := finish_register_execution false originalRemoval rfl rfl valid
  have acknowledgeRemove := acknowledge_execution false (finishRegister originalRemoval) 1 rfl rfl
  have acknowledgeRegister := acknowledge_execution false (acknowledged (finishRegister originalRemoval) 1) 0 rfl rfl
  exact execution_trans (execution_trans (execution_trans (execution_trans prefixRun removeRun)
    finishRun) acknowledgeRemove) acknowledgeRegister

/-- Independent full original result, not an invocation of sequential replacement. -/
theorem original_orphan_output : OriginalOrphan := by
  refine ⟨?_, rfl, rfl, rfl, rfl, rfl⟩
  rfl

theorem first_registration_execution (fixed : Bool) (s : World)
    (database : s.db = seedDB)
    (fresh : s.transactions 0 = { request := .register proposal })
    (instanceFree : s.instanceOwner = none) (deploymentFree : s.deploymentOwner = none) :
    Execution fixed s (finishRegister (beforeInsert fixed s)) := by
  have prefixRun := before_insert_execution fixed s database fresh instanceFree deploymentFree
  have method : ((beforeInsert fixed s).transactions 0).request = .register proposal := by
    cases fixed <;> simp [beforeInsert, createdRegister, creatingRegister, capturedRows, upsertedRegister,
      preparedRegister, selectedRegister, waitingRegister, cached, update, put, fresh]
  have valid : ValidWrite (applyWrites (beforeInsert fixed s).db ((beforeInsert fixed s).transactions 0).staged)
      (.registration (.insert candidate)) := by
    cases fixed <;>
      simp [ValidWrite, beforeInsert, createdRegister, creatingRegister, capturedRows, upsertedRegister,
        preparedRegister, selectedRegister, waitingRegister, cached, update, put, fresh, database,
        seedDB, applyWrites, applyWrite, upsert, candidate, proposal, unrelatedRoles]
  exact execution_trans prefixRun (finish_register_execution fixed _ rfl method valid)

theorem one_removal_execution (s : World)
    (fresh : s.transactions 1 = { request := .remove })
    (present : s.db.instanceRow = some instancePayload)
    (free : s.instanceOwner = none) (snapshot : Listing s.db [candidate]) :
    Execution true s (finishOneRemoval s) := by
  have one : Step true s (waitingRemove s) := Step.beginRemove _ 1 (by simp [fresh]) (by simp [fresh])
  have two : Step true (waitingRemove s) (selectedRemove true (waitingRemove s)) :=
    Step.acquire _ 1 instancePayload rfl rfl free present
  let a := selectedRemove true (waitingRemove s)
  have aMethod : (a.transactions 1).request = .remove := by
    simp [a, selectedRemove, waitingRemove, update, put, fresh]
  have listing : Listing (applyWrites a.db (a.transactions 1).staged) [candidate] := by
    simpa [a, selectedRemove, waitingRemove, update, put, fresh, applyWrites] using snapshot
  have three : Step true a (capturedRows a 1 [candidate]) := Step.delete _ 1 [candidate]
    (by simp [a, selectedRemove, waitingRemove, update, put, fresh, afterLock]) listing
  let b := capturedRows a 1 [candidate]
  have four : Step true b (droppedOne b 1 20) := Step.drop _ 1 20 [] rfl rfl
  let c := droppedOne b 1 20
  have five : Step true c (erasingRemove c) := Step.droppedRemove _ 1 rfl
    (by simp [c, droppedOne, b, capturedRows, update, put, aMethod]) rfl
  have six : Step true (erasingRemove c) (erasedRemove (erasingRemove c)) :=
    Step.erase _ 1 instancePayload rfl
      (by simp [erasingRemove, c, droppedOne, b, capturedRows, update, put, aMethod]) (Or.inr rfl) present
  have seven : Step true (erasedRemove (erasingRemove c))
      (committing (erasedRemove (erasingRemove c)) 1) := Step.requestCommit _ 1 rfl
  have eight : Step true (committing (erasedRemove (erasingRemove c)) 1) (finishOneRemoval s) :=
    Step.serverCommit _ 1 rfl
  exact Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.tail (Execution.tail (Execution.refl s) one) two) three) four) five) six) seven) eight

theorem fixed_accepted_then_removed_execution : Execution true start fixedFinal := by
  have registration := first_registration_execution true start rfl rfl rfl rfl
  have listing : Listing fixedRegistration.db [candidate] := by exact ⟨by simp, rfl⟩
  have removal := one_removal_execution fixedRegistration rfl rfl rfl listing
  have acknowledgeRemove := acknowledge_execution true (finishOneRemoval fixedRegistration) 1 rfl rfl
  have acknowledgeRegister := acknowledge_execution true (acknowledged (finishOneRemoval fixedRegistration) 1) 0 rfl rfl
  exact execution_trans (execution_trans (execution_trans registration removal) acknowledgeRemove) acknowledgeRegister

theorem fixed_accepted_then_removed_output : FixedAcceptedThenRemoved := by
  refine ⟨rfl, ?_, rfl, rfl, rfl, rfl, rfl⟩
  apply Database.ext <;> try rfl
  · funext key
    by_cases same : key = 10 <;>
      simp [fixedFinal, fixedRegistration, finishOneRemoval, oneRemovalReady, erasedRemove, erasingRemove,
        droppedOne, capturedRows, selectedRemove, waitingRemove, finishRegister, insertedRegister,
        beforeInsert, createdRegister, creatingRegister, upsertedRegister, preparedRegister,
        selectedRegister, waitingRegister, cached, acknowledged, committedState, committing,
        applyWrites, applyWrite, start, initial, requests, seedDB, candidate, proposal, update, put,
        admittedThenRemovedDB, same]
  · funext login
    by_cases candidateLogin : login = 20 <;> by_cases unrelatedLogin : login = 99 <;>
      simp [fixedFinal, fixedRegistration, finishOneRemoval, oneRemovalReady, erasedRemove, erasingRemove,
        droppedOne, capturedRows, selectedRemove, waitingRemove, finishRegister, insertedRegister,
        beforeInsert, createdRegister, creatingRegister, upsertedRegister, preparedRegister,
        selectedRegister, waitingRegister, cached, acknowledged, committedState, committing,
        applyWrites, applyWrite, start, initial, requests, seedDB, candidate, proposal, update, put,
        admittedThenRemovedDB, unrelatedRoles, candidateLogin, unrelatedLogin]

theorem cached_authority_removal_first_execution : Execution true start absentFinal := by
  have one : Step true start (cached start) := Step.preflight _ 0 proposal instancePayload rfl rfl rfl
  have two : Step true (cached start) cachedBeforeRemoval := Step.beginRegister _ 0 proposal rfl rfl
  have removal := empty_removal_execution true cachedBeforeRemoval rfl rfl rfl (by exact ⟨by simp, rfl⟩)
  have ack := acknowledge_execution true removedBeforeAdmission 1 rfl rfl
  have missing : Step true (acknowledged removedBeforeAdmission 1)
      (absentRegister (acknowledged removedBeforeAdmission 1)) := Step.missing _ 0 rfl rfl rfl rfl
  have absent : Step true (absentRegister (acknowledged removedBeforeAdmission 1)) absentFinal :=
    Step.returnAbsent _ 0 rfl
  exact Execution.tail (Execution.tail (execution_trans
    (execution_trans (Execution.tail (Execution.tail (Execution.refl start) one) two) removal) ack) missing) absent

theorem cached_authority_removal_first_output : CachedAuthorityCannotAdmit := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem rollback_then_accepted_execution : Execution true start rollbackFinal := by
  have ready := empty_removal_ready_execution true start rfl rfl rfl (by exact ⟨by simp, rfl⟩)
  have fail : Step true (emptyRemovalReady true start) (failedRemove (emptyRemovalReady true start)) :=
    Step.fail _ 1 "synthetic server refusal before commit" (by decide) (by decide) (by decide) rfl
  have abort : Step true (failedRemove (emptyRemovalReady true start)) removalAborted :=
    Step.serverAbort _ 1 (Or.inl rfl)
  have registration := first_registration_execution true removalAborted rfl rfl rfl rfl
  have ack := acknowledge_execution true (finishRegister (beforeInsert true removalAborted)) 0 rfl rfl
  exact execution_trans (execution_trans (Execution.tail (Execution.tail ready fail) abort) registration) ack

theorem rollback_then_accepted_output : RollbackThenAccepted := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem committed_lost_reply_execution : Execution true start lostReplyFinal := by
  have committed := first_registration_execution true start rfl rfl rfl rfl
  have lost : Step true fixedRegistration lostReplyFinal :=
    Step.deliveryError _ 0 "synthetic lost commit response" (Or.inr rfl) rfl
  exact Execution.tail committed lost

theorem committed_lost_reply_output : LostReplyStillCommitted := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- Exported scenarios combine constructive reachability and independent outputs. -/
theorem original_complete : Execution false start originalFinal ∧ OriginalOrphan :=
  ⟨original_orphan_execution, original_orphan_output⟩
theorem fixed_complete : Execution true start fixedFinal ∧ FixedAcceptedThenRemoved :=
  ⟨fixed_accepted_then_removed_execution, fixed_accepted_then_removed_output⟩
theorem absent_complete : Execution true start absentFinal ∧ CachedAuthorityCannotAdmit :=
  ⟨cached_authority_removal_first_execution, cached_authority_removal_first_output⟩
theorem rollback_complete : Execution true start rollbackFinal ∧ RollbackThenAccepted :=
  ⟨rollback_then_accepted_execution, rollback_then_accepted_output⟩
theorem lost_reply_complete : Execution true start lostReplyFinal ∧ LostReplyStillCommitted :=
  ⟨committed_lost_reply_execution, committed_lost_reply_output⟩

theorem replacement_seed_valid : InitialValid replacementSeed := by
  have key_is_thirty {key : Nat} {row : Row}
      (stored : replacementSeed.registrations key = some row) : key = 30 := by
    by_contra different
    simp [replacementSeed, put, different] at stored
  constructor
  · constructor
    · intro key row stored
      have same := key_is_thirty stored
      subst key
      have equal : priorRow = row := Option.some.inj (by simpa [replacementSeed, put] using stored)
      cases equal
      exact ⟨rfl, rfl, rfl⟩
    · intro first second left right leftStored rightStored _
      exact (key_is_thirty leftStored).trans (key_is_thirty rightStored).symm
  · intro absent
    simp [replacementSeed] at absent

theorem replacement_execution : Execution true replacementStart replacementFinal := by
  have one : Step true replacementStart (cached replacementStart) := Step.preflight _ 0 proposal instancePayload rfl rfl rfl
  have two : Step true (cached replacementStart) (waitingRegister (cached replacementStart)) :=
    Step.beginRegister _ 0 proposal rfl rfl
  let a := selectedRegister true (waitingRegister (cached replacementStart))
  have three : Step true (waitingRegister (cached replacementStart)) a := Step.acquire _ 0 instancePayload rfl rfl rfl rfl
  have four : Step true a (preparedRegister a) := Step.prepared _ 0 proposal rfl rfl
  let b := upsertedRegister (preparedRegister a)
  have five : Step true (preparedRegister a) b := Step.upsert _ 0 proposal rfl rfl (Or.inl rfl)
  have snapshot : Listing (applyWrites b.db (b.transactions 0).staged) [priorRow] := by exact ⟨by simp, rfl⟩
  have six : Step true b (capturedRows b 0 [priorRow]) := Step.delete _ 0 [priorRow] rfl snapshot
  let c := capturedRows b 0 [priorRow]
  have seven : Step true c (droppedOne c 0 40) := Step.drop _ 0 40 [] rfl rfl
  let d := droppedOne c 0 40
  have eight : Step true d (creatingRegister d) := Step.droppedRegister _ 0 proposal rfl rfl rfl
  have validCreate : ValidWrite (applyWrites (creatingRegister d).db ((creatingRegister d).transactions 0).staged)
      (.registration (.createLogin 20 "candidate-role")) := by
    simp [ValidWrite, creatingRegister, d, droppedOne, c, capturedRows, b, upsertedRegister,
      preparedRegister, a, selectedRegister, waitingRegister, cached, replacementStart, initial,
      requests, replacementSeed, priorRow, proposal, candidate, update, put, applyWrites,
      applyWrite, unrelatedRoles]
  have nine : Step true (creatingRegister d) beforeReplacement := Step.create _ 0 proposal rfl rfl validCreate
  have validInsert : ValidWrite (applyWrites beforeReplacement.db (beforeReplacement.transactions 0).staged)
      (.registration (.insert candidate)) := by
    simp [ValidWrite, beforeReplacement, createdRegister, creatingRegister, droppedOne, capturedRows,
      upsertedRegister, preparedRegister, selectedRegister, waitingRegister, cached, replacementStart,
      initial, requests, replacementSeed, priorRow, proposal, candidate, update, put, applyWrites,
      applyWrite, upsert, unrelatedRoles]
  have finishRun := finish_register_execution true beforeReplacement rfl rfl validInsert
  have ack := acknowledge_execution true (finishRegister beforeReplacement) 0 rfl rfl
  have prefixRun := Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.tail
    (Execution.tail (Execution.tail (Execution.tail (Execution.tail (Execution.refl replacementStart)
      one) two) three) four) five) six) seven) eight) nine
  exact execution_trans (execution_trans prefixRun finishRun) ack

theorem replacement_output : ReplacementAccepted := by
  refine ⟨?_, rfl, rfl, rfl, rfl, rfl⟩
  apply Database.ext <;> try rfl
  · funext key
    by_cases newKey : key = 10 <;> by_cases oldKey : key = 30 <;>
      simp [replacementFinal, finishRegister, insertedRegister, beforeReplacement, createdRegister,
        creatingRegister, droppedOne, capturedRows, upsertedRegister, preparedRegister,
        selectedRegister, waitingRegister, cached, acknowledged, committing, committedState,
        replacementStart, initial, requests, replacementSeed, priorRow, candidate, proposal,
        applyWrites, applyWrite, update, put, expectedReplacement, newKey, oldKey]
  · funext login
    by_cases newLogin : login = 20 <;> by_cases oldLogin : login = 40 <;> by_cases unrelatedLogin : login = 99 <;>
      simp [replacementFinal, finishRegister, insertedRegister, beforeReplacement, createdRegister,
        creatingRegister, droppedOne, capturedRows, upsertedRegister, preparedRegister,
        selectedRegister, waitingRegister, cached, acknowledged, committing, committedState,
        replacementStart, initial, requests, replacementSeed, priorRow, candidate, proposal,
        applyWrites, applyWrite, update, put, expectedReplacement, unrelatedRoles,
        newLogin, oldLogin, unrelatedLogin]

theorem replacement_complete : Execution true replacementStart replacementFinal ∧ ReplacementAccepted :=
  ⟨replacement_execution, replacement_output⟩

#print axioms original_complete
#print axioms fixed_complete
#print axioms absent_complete
#print axioms rollback_complete
#print axioms lost_reply_complete
#print axioms replacement_complete

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``seed_valid, ``execution_trans, ``before_insert_execution,
      ``finish_register_execution, ``empty_removal_ready_execution, ``empty_removal_execution,
      ``acknowledge_execution, ``original_orphan_execution, ``original_orphan_output,
      ``first_registration_execution, ``one_removal_execution,
      ``fixed_accepted_then_removed_execution, ``fixed_accepted_then_removed_output,
      ``cached_authority_removal_first_execution, ``cached_authority_removal_first_output,
      ``rollback_then_accepted_execution, ``rollback_then_accepted_output,
      ``committed_lost_reply_execution, ``committed_lost_reply_output,
      ``original_complete, ``fixed_complete, ``absent_complete, ``rollback_complete,
      ``lost_reply_complete, ``replacement_seed_valid, ``replacement_execution,
      ``replacement_output, ``replacement_complete,
      ``OriginalOrphan, ``FixedAcceptedThenRemoved, ``CachedAuthorityCannotAdmit,
      ``RollbackThenAccepted, ``LostReplyStillCommitted, ``ReplacementAccepted] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected registration lifetime witness axiom in {declaration}: {unexpected}"

end RegistrationLifetime.Witnesses
