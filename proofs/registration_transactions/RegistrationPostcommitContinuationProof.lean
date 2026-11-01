import RegistrationPostcommitContinuation
import RegistrationAuditedLifetimeWitnessProof

/-! Source-shaped result/trace theorems. Opaque observations are not a proof of
native reload/cache behavior or ClientInformation construction. Only the final
composition imports the already-checked populated store proof. -/
namespace RegistrationPostcommit
variable {K C S L M U D G E R O : Type}

theorem reload_old_ok (observe : Nat → C → Except E Unit) (position : Nat)
    (rows : List (Ended C S L)) (accepted : AllOldOk observe position rows) :
    reloadOld observe position rows = ⟨oldCalls position rows, .ok ()⟩ := by
  induction rows generalizing position with
  | nil => rfl
  | cons row rows ih =>
    rcases accepted with ⟨head, tail⟩
    simp only [reloadOld, head]
    rw [ih _ tail]
    rfl

theorem reload_old_first_error (observe : Nat → C → Except E Unit) (position : Nat)
    (before : List (Ended C S L)) (failed : Ended C S L) (after : List (Ended C S L)) (error : E)
    (accepted : AllOldOk observe position before)
    (refused : observe (position + before.length) failed.client = .error error) :
    reloadOld observe position (before ++ failed :: after) =
      ⟨oldCalls position before ++
        [.ended failed .replace, .reload (position + before.length) failed.client], .error error⟩ := by
  induction before generalizing position with
  | nil =>
    have failure : observe position failed.client = .error error := by simpa using refused
    simp only [List.nil_append, reloadOld, failure, oldCalls, List.length_nil, Nat.add_zero]
  | cons row rows ih =>
    rcases accepted with ⟨head, tail⟩
    have offset : position + (row :: rows).length = (position + 1) + rows.length := by
      simp only [List.length_cons]
      omega
    have later : observe ((position + 1) + rows.length) failed.client = .error error := by
      simpa only [offset] using refused
    simp only [List.cons_append, reloadOld, head]
    rw [ih _ tail later]
    simp only [oldCalls, List.append_assoc, List.nil_append, List.cons_append, offset]

section Generic
variable (snapshot : K) (ended : List (Ended C S L)) (projection : Projection M S D)
variable (context : Context C L M U) (observe : Observations C D (Context C L M U) E R G O)

theorem old_reload_short_circuit (before : List (Ended C S L)) (failed : Ended C S L)
    (after : List (Ended C S L)) (error : E) (accepted : AllOldOk observe.oldReload 0 before)
    (refused : observe.oldReload before.length failed.client = .error error) :
    finishRegistration snapshot (before ++ failed :: after) projection context observe =
      ⟨some snapshot,
        (oldCalls 0 before ++ [OldCall.ended failed .replace, OldCall.reload before.length failed.client]).map Call.old,
        .error (.internal error)⟩ := by
  have failedRun := reload_old_first_error observe.oldReload 0 before failed after error accepted
    (by simpa using refused)
  simp only [finishRegistration, failedRun, Nat.zero_add]

theorem marker_refusal (error : E) (accepted : AllOldOk observe.oldReload 0 ended)
    (refused : observe.marker (projection.deployment context.metadata) = .confirmedNoWriteRefusal error) :
    finishRegistration snapshot ended projection context observe =
      ⟨some snapshot, (oldCalls 0 ended).map Call.old ++
        [.served (projection.deployment context.metadata)], .error (.internal error)⟩ := by
  simp only [finishRegistration, reload_old_ok observe.oldReload 0 ended accepted, refused]

/-- Only old-telemetry/reload and the marker call were reached. In particular no
new reload, registered log, grant/information attempt or success return occurs. -/
def EarlyOnly : Call C S L M U D G → Prop
  | .old _ | .served _ => True
  | _ => False

private theorem early_trace (before : List (OldCall C S L)) (deployment : D) :
    ∀ call ∈ before.map (Call.old (Metadata := M) (Issued := U) (Deployment := D) (Grant := G)) ++
      [Call.served deployment], EarlyOnly call := by
  intro call member
  rcases List.mem_append.mp member with old | marker
  · rcases List.mem_map.mp old with ⟨old, _, same⟩
    cases same
    trivial
  · have same := List.mem_singleton.mp marker
    cases same
    trivial

theorem marker_refusal_no_later (error : E) (accepted : AllOldOk observe.oldReload 0 ended)
    (refused : observe.marker (projection.deployment context.metadata) = .confirmedNoWriteRefusal error) :
    ∀ call ∈ (finishRegistration snapshot ended projection context observe).calls, EarlyOnly call := by
  rw [marker_refusal snapshot ended projection context observe error accepted refused]
  exact early_trace _ _

theorem marker_refusal_later_irrelevant
    (other : Observations C D (Context C L M U) E R G O) (error : E)
    (accepted : AllOldOk observe.oldReload 0 ended) (sameOld : other.oldReload = observe.oldReload)
    (first : observe.marker (projection.deployment context.metadata) = .confirmedNoWriteRefusal error)
    (second : other.marker (projection.deployment context.metadata) = .confirmedNoWriteRefusal error) :
    finishRegistration snapshot ended projection context observe =
      finishRegistration snapshot ended projection context other := by
  rw [marker_refusal snapshot ended projection context observe error accepted first,
    marker_refusal snapshot ended projection context other error (by simpa [sameOld] using accepted) second]

/-- An arbitrary marker error is explicitly UNKNOWN, unlike confirmed no-write. -/
theorem marker_general_error (error : E) (accepted : AllOldOk observe.oldReload 0 ended)
    (refused : observe.marker (projection.deployment context.metadata) = .generalOutcome (.error error)) :
    finishRegistration snapshot ended projection context observe =
      ⟨none, (oldCalls 0 ended).map Call.old ++ [.served (projection.deployment context.metadata)],
        .error (.internal error)⟩ := by
  simp only [finishRegistration, reload_old_ok observe.oldReload 0 ended accepted, refused]

theorem new_reload_error (error : E) (accepted : AllOldOk observe.oldReload 0 ended)
    (marked : observe.marker (projection.deployment context.metadata) = .generalOutcome (.ok ()))
    (refused : observe.newReload context.client = .error error) :
    finishRegistration snapshot ended projection context observe =
      ⟨none, (oldCalls 0 ended).map Call.old ++
        [.served (projection.deployment context.metadata), .reloadNew context.client], .error (.internal error)⟩ := by
  simp [finishRegistration, reload_old_ok observe.oldReload 0 ended accepted, marked, afterMarker,
    refused, List.append_assoc]

theorem database_error (error : E) (accepted : AllOldOk observe.oldReload 0 ended)
    (marked : observe.marker (projection.deployment context.metadata) = .generalOutcome (.ok ()))
    (reloaded : observe.newReload context.client = .ok ())
    (refused : observe.database context = .error error) :
    finishRegistration snapshot ended projection context observe =
      ⟨none, (oldCalls 0 ended).map Call.old ++
        [.served (projection.deployment context.metadata), .reloadNew context.client,
          .registeredLog (projection.server context.metadata) context.client context.login, .database context],
        .error (.internal error)⟩ := by
  simp [finishRegistration, reload_old_ok observe.oldReload 0 ended accepted, marked, afterMarker,
    reloaded, refused, List.append_assoc]

/-- Preserve information()'s COMPLETE typed error; Rejected is not Internal. -/
theorem information_error_passthrough (error : ServiceError E R) (grant : G)
    (accepted : AllOldOk observe.oldReload 0 ended)
    (marked : observe.marker (projection.deployment context.metadata) = .generalOutcome (.ok ()))
    (reloaded : observe.newReload context.client = .ok ()) (built : observe.database context = .ok grant)
    (refused : observe.information context grant = .error error) :
    finishRegistration snapshot ended projection context observe =
      ⟨none, (oldCalls 0 ended).map Call.old ++
        [.served (projection.deployment context.metadata), .reloadNew context.client,
          .registeredLog (projection.server context.metadata) context.client context.login,
          .database context, .information context grant], .error error⟩ := by
  simp [finishRegistration, reload_old_ok observe.oldReload 0 ended accepted, marked, afterMarker,
    reloaded, built, refused, List.append_assoc]

theorem success_passthrough (output : O) (grant : G) (accepted : AllOldOk observe.oldReload 0 ended)
    (marked : observe.marker (projection.deployment context.metadata) = .generalOutcome (.ok ()))
    (reloaded : observe.newReload context.client = .ok ()) (built : observe.database context = .ok grant)
    (information : observe.information context grant = .ok output) :
    finishRegistration snapshot ended projection context observe =
      ⟨none, (oldCalls 0 ended).map Call.old ++
        [.served (projection.deployment context.metadata), .reloadNew context.client,
          .registeredLog (projection.server context.metadata) context.client context.login,
          .database context, .information context grant, .returnSuccess], .ok output⟩ := by
  simp [finishRegistration, reload_old_ok observe.oldReload 0 ended accepted, marked, afterMarker,
    reloaded, built, information, List.append_assoc]
end Generic

section Constructive
def exampleContext : Context Nat Nat String String :=
  ⟨10, 20, "synthetic-new-server", "opaque synthetic issuance/configuration inputs"⟩
def exampleProjection : Projection String String Nat := ⟨id, fun _ => 7⟩
def firstOld : Ended Nat String Nat := ⟨30, "synthetic-old-server", 40⟩
def secondOld : Ended Nat String Nat := ⟨31, "synthetic-old-server", 41⟩
def successfulObservations : Observations Nat Nat (Context Nat Nat String String) String String String String :=
  { oldReload := fun _ _ => .ok ()
    marker := fun _ => .generalOutcome (.ok ())
    newReload := fun _ => .ok ()
    database := fun _ => .ok "opaque database-grant observation"
    information := fun _ _ => .ok "opaque successful full-output observation" }
def oldRefusalObservations : Observations Nat Nat (Context Nat Nat String String) String String String String :=
  { oldReload := fun position _ => if position = 0 then .ok () else .error "synthetic second old reload error"
    marker := fun _ => .confirmedNoWriteRefusal "unreachable marker error"
    newReload := fun _ => .error "unreachable new reload error"
    database := fun _ => .error "unreachable grant error"
    information := fun _ _ => .error (.rejected "unreachable information rejection") }

theorem old_refusal_example :
    finishRegistration (42 : Nat) [firstOld, secondOld] exampleProjection exampleContext oldRefusalObservations =
      ⟨some 42,
        [.old (.ended firstOld .replace), .old (.reload 0 30),
          .old (.ended secondOld .replace), .old (.reload 1 31)],
        .error (.internal "synthetic second old reload error")⟩ := by
  exact old_reload_short_circuit 42 exampleProjection exampleContext oldRefusalObservations
    [firstOld] secondOld [] "synthetic second old reload error" ⟨rfl, trivial⟩ rfl

theorem successful_observation_example :
    finishRegistration (42 : Nat) [firstOld, secondOld] exampleProjection exampleContext successfulObservations =
      ⟨none,
        [.old (.ended firstOld .replace), .old (.reload 0 30),
          .old (.ended secondOld .replace), .old (.reload 1 31),
          .served 7, .reloadNew 10, .registeredLog "synthetic-new-server" 10 20,
          .database exampleContext, .information exampleContext "opaque database-grant observation", .returnSuccess],
        .ok "opaque successful full-output observation"⟩ := by
  exact success_passthrough 42 [firstOld, secondOld] exampleProjection exampleContext successfulObservations
    "opaque successful full-output observation" "opaque database-grant observation"
    ⟨rfl, rfl, trivial⟩ rfl rfl rfl rfl
end Constructive

section StoreComposition
open RegistrationAuditedLifetime
open RegistrationAuditedLifetime.Witnesses

def replacementEnded : List (Ended Nat String Nat) :=
  (acknowledged.core.transactions 0).captured.map
    (fun row => ⟨row.client, row.payload.server, row.login⟩)
def replacementContext (issued : U) : Context Nat Nat Metadata U :=
  ⟨proposal.candidate.client, proposal.candidate.login, proposal.candidate.metadata, issued⟩
def replacementProjection (deployment : Metadata → D) : Projection Metadata String D :=
  ⟨Metadata.server, deployment⟩

/-- Real checked store prefix + source-shaped confirmed no-write suffix. Retained
snapshot knowledge has the WHOLE independent DB/ledger, and inner store success
is distinct from the outer Internal result. Native effects/format interpretation
and absence of external durable writers remain the stated source boundary. -/
theorem populated_store_then_marker_refusal (deployment : Metadata → D) (issued : U)
    (observe : Observations Nat D (Context Nat Nat Metadata U) E R G O) (error : E)
    (accepted : AllOldOk observe.oldReload 0 replacementEnded)
    (refused : observe.marker (deployment proposal.candidate.metadata) = .confirmedNoWriteRefusal error) :
    let outcome := finishRegistration acknowledged replacementEnded
      (replacementProjection deployment) (replacementContext issued) observe
    Execution source encoding start acknowledged ∧
      acknowledged.core.db = expectedDB ∧ acknowledged.audit = expectedAudit ∧
      (acknowledged.core.transactions 0).response = .registered ∧
      outcome.result = .error (.internal error) ∧ outcome.knownSnapshot = some acknowledged ∧
      outcome.knownSnapshot.map (fun world => world.core.db) = some expectedDB ∧
      outcome.knownSnapshot.map (fun world => world.audit) = some expectedAudit ∧
      outcome.calls = (oldCalls 0 replacementEnded).map Call.old ++
        [.served (deployment proposal.candidate.metadata)] ∧
      ∀ call ∈ outcome.calls, EarlyOnly call := by
  rcases replacement_complete with ⟨storeRun, database, audit, original, storeReply, rest⟩
  have suffix := marker_refusal acknowledged replacementEnded (replacementProjection deployment)
    (replacementContext issued) observe error accepted refused
  have early := marker_refusal_no_later acknowledged replacementEnded (replacementProjection deployment)
    (replacementContext issued) observe error accepted refused
  dsimp only
  rw [suffix] at early ⊢
  refine ⟨storeRun, database, audit, storeReply, rfl, rfl, ?_, ?_, rfl, early⟩
  · simpa only [Option.map_some] using congrArg some database
  · simpa only [Option.map_some] using congrArg some audit

/-- Closed synthetic observations. All later work would succeed if reached, so
ignoring the marker refusal cannot accidentally satisfy the expected Internal. -/
def closedMarkerObservations :
    Observations Nat Nat (Context Nat Nat Metadata String) String String String String :=
  { oldReload := fun _ _ => .ok ()
    marker := fun _ => .confirmedNoWriteRefusal "synthetic confirmed no-write served refusal"
    newReload := fun _ => .ok ()
    database := fun _ => .ok "opaque successful database-grant observation"
    information := fun _ _ => .ok "opaque successful full-output observation" }
def closedMarkerOutcome :=
  finishRegistration acknowledged replacementEnded
    (replacementProjection (fun _ => (7 : Nat)))
    (replacementContext "opaque synthetic issuance/configuration inputs") closedMarkerObservations

/-- Actual checked populated store execution followed by a CLOSED refusal path,
not only a conditional observation theorem. Deployment projection/issued values
are synthetic; native format and no-external-writer interpretation stay explicit. -/
theorem closed_store_marker_refusal :
    Execution source encoding start acknowledged ∧
      acknowledged.core.db = expectedDB ∧ acknowledged.audit = expectedAudit ∧
      (acknowledged.core.transactions 0).response = .registered ∧
      closedMarkerOutcome.result = .error (.internal "synthetic confirmed no-write served refusal") ∧
      closedMarkerOutcome.knownSnapshot = some acknowledged ∧
      closedMarkerOutcome.knownSnapshot.map (fun world => world.core.db) = some expectedDB ∧
      closedMarkerOutcome.knownSnapshot.map (fun world => world.audit) = some expectedAudit ∧
      closedMarkerOutcome.calls =
        [.old (.ended ⟨30, oldServer, 40⟩ .replace), .old (.reload 0 30),
          .old (.ended ⟨31, oldServer, 41⟩ .replace), .old (.reload 1 31), .served 7] ∧
      ∀ call ∈ closedMarkerOutcome.calls, EarlyOnly call := by
  have captured : replacementEnded = [⟨30, oldServer, 40⟩, ⟨31, oldServer, 41⟩] := rfl
  have accepted : AllOldOk closedMarkerObservations.oldReload 0 replacementEnded := by
    rw [captured]
    exact ⟨rfl, rfl, trivial⟩
  have complete := populated_store_then_marker_refusal (fun _ => (7 : Nat))
    "opaque synthetic issuance/configuration inputs" closedMarkerObservations
    "synthetic confirmed no-write served refusal" accepted rfl
  rcases complete with ⟨run, database, audit, storedReply, error, known, knownDB, knownAudit, calls, early⟩
  change closedMarkerOutcome.calls = (oldCalls 0 replacementEnded).map Call.old ++ [.served 7] at calls
  refine ⟨run, database, audit, storedReply, error, known, knownDB, knownAudit, ?_, early⟩
  simpa only [captured, oldCalls, List.map_cons, List.map_nil, List.append_nil,
    List.nil_append, List.cons_append, Nat.zero_add, replacementProjection, replacementContext] using calls
end StoreComposition

#print axioms reload_old_first_error
#print axioms old_reload_short_circuit
#print axioms marker_refusal
#print axioms marker_refusal_no_later
#print axioms information_error_passthrough
#print axioms successful_observation_example
#print axioms populated_store_then_marker_refusal
#print axioms closed_store_marker_refusal

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``reload_old_ok, ``reload_old_first_error, ``old_reload_short_circuit,
      ``marker_refusal, ``EarlyOnly, ``marker_refusal_no_later, ``marker_refusal_later_irrelevant,
      ``marker_general_error, ``new_reload_error, ``database_error, ``information_error_passthrough,
      ``success_passthrough, ``old_refusal_example, ``successful_observation_example,
      ``replacementEnded, ``replacementContext, ``replacementProjection, ``populated_store_then_marker_refusal,
      ``closed_store_marker_refusal] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected registration postcommit continuation axiom in {declaration}: {unexpected}"

end RegistrationPostcommit
