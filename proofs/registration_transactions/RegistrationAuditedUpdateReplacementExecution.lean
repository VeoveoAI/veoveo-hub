import RegistrationAuditedUpdateReplacementWitnesses
import RegistrationAuditedUpdateWitnessExecution

/-! Constructive old mixed register steps and real stale UPDATE rollback.
No generic audit/UPDATE correctness or output acceptance import is needed. -/
namespace RegistrationAuditedUpdate.ReplacementWitnesses
open RegistrationAuditedUpdate
open RegistrationAuditedLifetime
open RegistrationAuditedUpdate.Witnesses
open RegistrationTransaction (put rowMap)

/-- The same admitted deployment/slug,binding and selected metadata domain.
Already-held namespace pin is an explicit native interpretation,not stored here. -/
theorem input_valid : RegistrationTransaction.WellFormed seedDB ∧
    registerRequest.kind = .ordinary ∧ registerRequest.candidate.metadata.1.server = first.metadata.1.server ∧
    registerRequest.candidate.metadata.2 = seedDB.metadata.2 ∧
    first.payload.tokenDigest = request.presentedDigest ∧ first.metadata.1 ≠ request.metadata := by
  exact ⟨RegistrationAuditedUpdate.Witnesses.seed_wellFormed, rfl, rfl, rfl, rfl, by decide⟩

theorem update_ordinary (s : World) (tx : Tx) (lock : Option Nat)
    (notCommitting : tx.phase ≠ .committing) : Ordinary s.core (updateAt s tx lock).core := by
  refine ⟨rfl, ?_⟩
  intro subject previous
  change (put s.core.updates 0 tx subject).phase ≠ .committing
  by_cases same : subject = 0
  · simpa [put, same] using notCommitting
  · simpa [put, same] using previous

/-- Lift actual old constructors with their indexed actual event label. This
adds no fabricated correct replacement or audit program premise. -/
theorem register_ordinary (s : World) (next : RegCore)
    (actual : RegistrationTransaction.Step true s.core.core next)
    (label : RegistrationUpdate.RegisterAction actual (some 1) []) :
    Step methods metadataAccepted source encoding s { s with core := { s.core with core := next } } := by
  have method : ∀ id : Nat, some 1 = some id → methods id = .register := by
    intro id same
    have equal := Option.some.inj same
    subst id
    rfl
  have base := RegistrationUpdate.Step.register (fixed := true) (methods := methods)
    (accepted := metadataAccepted) s.core next actual (some 1) [] label method
  apply Step.ordinary s _
  · simpa only [List.append_nil] using base
  · exact ⟨rfl, fun _ previous => previous⟩

theorem stale_preflight : Step methods metadataAccepted source encoding start stale :=
  Step.ordinary start stale.core
    (RegistrationUpdate.Step.preflight start.core 0 first rfl rfl rfl rfl (by decide))
    (update_ordinary _ _ _ (by change RegistrationUpdate.Phase.preflight ≠ .committing; decide))

theorem captured_listing : RegistrationTransaction.Listing
    (RegistrationTransaction.applyWrites upserted.core.core.db (upserted.core.core.transactions 1).staged)
    [first, second] := by
  refine ⟨by decide, ?_⟩
  change seedDB.registrations = rowMap [first, second]
  funext key
  simp [seedDB, rowMap, put, first, second]

theorem candidate_insert_valid : RegistrationTransaction.ValidWrite
    (RegistrationTransaction.applyWrites created.core.core.db (created.core.core.transactions 1).staged)
    (.insert candidate) := by
  refine ⟨rfl, rfl, ?_⟩
  have cleared :
      (RegistrationTransaction.applyWrites created.core.core.db
        (created.core.core.transactions 1).staged).registrations = (fun _ => none) := by
    change (fun key => if key ∈ [30, 31] then none else seedDB.registrations key) = (fun _ => none)
    funext key
    by_cases firstKey : key = 30
    · subst key
      rfl
    by_cases secondKey : key = 31
    · subst key
      rfl
    simp [seedDB, firstKey, secondKey]
  intro key row stored
  rw [cleared] at stored
  cases stored

/-- Twelve actual register steps from stale preflight to real commit-pending.
One valid DELETE Listing order is chosen explicitly. -/
theorem replacement_pending_execution : Execution methods metadataAccepted source encoding stale committing := by
  have one : Step methods metadataAccepted source encoding stale waiting :=
    register_ordinary stale waiting.core.core
      (RegistrationTransaction.Step.begin stale.core.core 1 rfl)
      (RegistrationUpdate.RegisterAction.begin stale.core.core 1 rfl)
  have two : Step methods metadataAccepted source encoding waiting selected :=
    register_ordinary waiting selected.core.core
      (RegistrationTransaction.Step.acquire waiting.core.core 1 rfl rfl (Or.inl rfl))
      (RegistrationUpdate.RegisterAction.acquire waiting.core.core 1 rfl rfl (Or.inl rfl))
  have three : Step methods metadataAccepted source encoding selected observed :=
    register_ordinary selected observed.core.core
      (RegistrationTransaction.Step.read selected.core.core 1 rfl rfl)
      (RegistrationUpdate.RegisterAction.read selected.core.core 1 rfl rfl)
  have four : Step methods metadataAccepted source encoding observed replacing :=
    register_ordinary observed replacing.core.core
      (RegistrationTransaction.Step.replace observed.core.core 1 rfl (by decide))
      (RegistrationUpdate.RegisterAction.replace observed.core.core 1 rfl (by decide))
  have five : Step methods metadataAccepted source encoding replacing upserted :=
    register_ordinary replacing upserted.core.core
      (RegistrationTransaction.Step.upsert replacing.core.core 1 rfl (Or.inr rfl))
      (RegistrationUpdate.RegisterAction.upsert replacing.core.core 1 rfl (Or.inr rfl))
  have six : Step methods metadataAccepted source encoding upserted captured :=
    register_ordinary upserted captured.core.core
      (RegistrationTransaction.Step.delete upserted.core.core 1 [first, second] rfl captured_listing)
      (RegistrationUpdate.RegisterAction.delete upserted.core.core 1 [first, second] rfl captured_listing)
  have seven : Step methods metadataAccepted source encoding captured droppedFirst :=
    register_ordinary captured droppedFirst.core.core
      (RegistrationTransaction.Step.drop captured.core.core 1 40 [41] rfl rfl)
      (RegistrationUpdate.RegisterAction.drop captured.core.core 1 40 [41] rfl rfl)
  have eight : Step methods metadataAccepted source encoding droppedFirst droppedSecond :=
    register_ordinary droppedFirst droppedSecond.core.core
      (RegistrationTransaction.Step.drop droppedFirst.core.core 1 41 [] rfl rfl)
      (RegistrationUpdate.RegisterAction.drop droppedFirst.core.core 1 41 [] rfl rfl)
  have nine : Step methods metadataAccepted source encoding droppedSecond creating :=
    register_ordinary droppedSecond creating.core.core
      (RegistrationTransaction.Step.dropped droppedSecond.core.core 1 rfl rfl)
      (RegistrationUpdate.RegisterAction.dropped droppedSecond.core.core 1 rfl rfl)
  have ten : Step methods metadataAccepted source encoding creating created :=
    register_ordinary creating created.core.core
      (RegistrationTransaction.Step.create creating.core.core 1 rfl rfl)
      (RegistrationUpdate.RegisterAction.create creating.core.core 1 rfl rfl)
  have eleven : Step methods metadataAccepted source encoding created ready :=
    register_ordinary created ready.core.core
      (RegistrationTransaction.Step.insert created.core.core 1 rfl candidate_insert_valid)
      (RegistrationUpdate.RegisterAction.insert created.core.core 1 rfl candidate_insert_valid)
  have twelve : Step methods metadataAccepted source encoding ready committing :=
    register_ordinary ready committing.core.core
      (RegistrationTransaction.Step.requestCommit ready.core.core 1 rfl)
      (RegistrationUpdate.RegisterAction.requestCommit ready.core.core 1 rfl)
  exact (.tail (.tail (.tail (.tail (.tail (.tail (.tail (.tail (.tail (.tail (.tail (.tail (.refl stale) one) two) three) four) five) six) seven) eight) nine) ten) eleven) twelve)

/-- Explicit finite Core append sequence:two ended-row SERVER Replace records
and the candidate's Register. No ClaimNamespace in this already-held namespace
interpretation. This does not certify all external registration contributions. -/
theorem external_batch_valid : AppendBatch encoding committing.audit externalRecords externalAudit := by
  change AppendBatch encoding priorAudit [replaceFirst, replaceSecond, registerRecord] externalAudit
  exact .cons ⟨by decide, Or.inl rfl, rfl⟩
    (.cons ⟨by decide, Or.inl rfl, rfl⟩ (.cons ⟨by decide, Or.inl rfl, rfl⟩ (.nil _)))

theorem external_batch_source : externalRecords.map (recordIntent encoding) =
    [⟨.replace, first.payload.stable.server⟩, ⟨.replace, second.payload.stable.server⟩,
      ⟨.register, candidate.metadata.1.server⟩] ∧
    (externalRecords.map (recordIntent encoding)).count ⟨.replace, server⟩ = 2 ∧
    (externalRecords.map (recordIntent encoding)).count ⟨.register, server⟩ = 1 ∧
    (externalRecords.map (recordIntent encoding)).count ⟨.claimNamespace, server⟩ = 0 := by
  exact ⟨rfl, by decide, by decide, by decide⟩

theorem external_batch_fresh : NewIdentities encoding committing.audit externalRecords := by
  refine ⟨by decide, ?_⟩
  intro record member
  simp only [externalRecords, List.mem_cons, List.mem_singleton] at member
  rcases member with one | two | three
  · subst record
    rfl
  · subst record
    rfl
  · rcases three with equal | impossible
    · subst record
      rfl
    · cases impossible

theorem external_batch_disjoint : DisjointPrivate encoding committing externalRecords := by
  intro id current selected
  change none = some current at selected
  cases selected

theorem replacement_commit : Step methods metadataAccepted source encoding committing registered := by
  have method : ∀ id : Nat, some 1 = some id → methods id = .register := by
    intro id same
    have equal := Option.some.inj same
    subst id
    rfl
  have base : RegistrationUpdate.Step true methods metadataAccepted committing.core registered.core :=
    RegistrationUpdate.Step.register committing.core
      (RegistrationTransaction.committedState committing.core.core 1)
      (RegistrationTransaction.Step.serverCommit committing.core.core 1 rfl) (some 1)
      [.register 1 (committing.core.core.transactions 1).staged]
      (RegistrationUpdate.RegisterAction.serverCommit committing.core.core 1 rfl) method
  exact Step.externalRegisterCommit committing registered.core 1 (committing.core.core.transactions 1).staged
    externalRecords externalAudit base rfl external_batch_valid external_batch_disjoint

theorem replacement_ack : Step methods metadataAccepted source encoding registered registerAcknowledged :=
  register_ordinary registered registerAcknowledged.core.core
    (RegistrationTransaction.Step.acknowledge registered.core.core 1 rfl rfl)
    (RegistrationUpdate.RegisterAction.acknowledge registered.core.core 1 rfl rfl)

theorem replacement_execution : Execution methods metadataAccepted source encoding start registerAcknowledged :=
  let before := Execution.tail (.refl start) stale_preflight
  let complete := RegistrationAuditedUpdate.Witnesses.execution_trans before replacement_pending_execution
  .tail (.tail complete replacement_commit) replacement_ack

/-- Actual current client is missing after completed replacement. The deployment
write remains private until EXPLICIT rollback; no Update audit begins. -/
theorem noop_execution : Execution methods metadataAccepted source encoding registerAcknowledged acknowledged := by
  have one : Step methods metadataAccepted source encoding registerAcknowledged updateBegun :=
    Step.ordinary registerAcknowledged updateBegun.core
      (RegistrationUpdate.Step.beginUpdate registerAcknowledged.core 0 rfl)
      (update_ordinary _ _ _ (by change RegistrationUpdate.Phase.begun ≠ .committing; decide))
  have two : Step methods metadataAccepted source encoding updateBegun metadataWritten :=
    Step.ordinary updateBegun metadataWritten.core
      (RegistrationUpdate.Step.metadata updateBegun.core 0 (Or.inl ⟨rfl, rfl⟩) rfl ⟨rfl, rfl⟩)
      (update_ordinary _ _ _ (by change RegistrationUpdate.Phase.metadataWritten ≠ .committing; decide))
  have three : Step methods metadataAccepted source encoding metadataWritten missing :=
    Step.ordinary metadataWritten missing.core
      (RegistrationUpdate.Step.missRow metadataWritten.core 0 (Or.inl ⟨rfl, rfl⟩) rfl)
      (update_ordinary _ _ _ (by change RegistrationUpdate.Phase.rollbackPending ≠ .committing; decide))
  have four : Step methods metadataAccepted source encoding missing rolledBack :=
    Step.ordinary missing rolledBack.core (RegistrationUpdate.Step.rollbackNoop missing.core 0 rfl rfl)
      (update_ordinary _ _ _ (by change RegistrationUpdate.Phase.aborted ≠ .committing; decide))
  have five : Step methods metadataAccepted source encoding rolledBack acknowledged :=
    Step.ordinary rolledBack acknowledged.core
      (RegistrationUpdate.Step.acknowledgeNoop rolledBack.core 0 rfl rfl rfl)
      (update_ordinary _ _ _ (by change RegistrationUpdate.Phase.aborted ≠ .committing; decide))
  exact .tail (.tail (.tail (.tail (.tail (.refl registerAcknowledged) one) two) three) four) five

theorem composed_execution : Execution methods metadataAccepted source encoding start acknowledged :=
  RegistrationAuditedUpdate.Witnesses.execution_trans replacement_execution noop_execution

#print axioms input_valid
#print axioms replacement_pending_execution
#print axioms external_batch_valid
#print axioms external_batch_source
#print axioms external_batch_disjoint
#print axioms composed_execution

run_cmd do
  for declaration in [``input_valid, ``update_ordinary, ``register_ordinary, ``stale_preflight,
      ``captured_listing, ``candidate_insert_valid, ``replacement_pending_execution,
      ``external_batch_valid, ``external_batch_source, ``external_batch_fresh,
      ``external_batch_disjoint, ``replacement_commit, ``replacement_ack,
      ``replacement_execution, ``noop_execution, ``composed_execution] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unexpected replacement/noop execution axiom in {declaration}: {name}"

end RegistrationAuditedUpdate.ReplacementWitnesses
