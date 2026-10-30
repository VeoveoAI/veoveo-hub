import RegistrationUpdateHistory

namespace RegistrationUpdate.Witnesses
open RegistrationTransaction RegistrationUpdate
variable {M B S D T R P F : Type}
variable [DecidableEq M] [DecidableEq B] [DecidableEq D]

private def first (metadata : M) (binding : B) (payload : Payload S D T R) : Row Nat Nat M B S D T R :=
  { client := 0, login := 10, metadata := (metadata, binding), payload }

private def second (metadata : M) (binding : B) (payload : Payload S D T R) : Row Nat Nat M B S D T R :=
  { client := 1, login := 11, metadata := (metadata, binding), payload }

private def seed (metadata : M) (binding : B) (firstPayload secondPayload : Payload S D T R)
    (firstRole secondRole unrelated : P) (frame : F) : DB Nat Nat M B S D T R P F :=
  { metadata := (metadata, binding)
    registrations := fun key =>
      if key = 0 then some (first metadata binding firstPayload)
      else if key = 1 then some (second metadata binding secondPayload) else none
    logins := fun key =>
      if key = 10 then some firstRole else if key = 11 then some secondRole
      else if key = 99 then some unrelated else none
    frame }

private def methods (id : Nat) : Method := if id = 0 then .update else .register

private def request (metadata : M) (statement : S) (digest : D) (now : T) :
    RegistrationUpdate.Request Nat M S D T :=
  { client := 0, metadata, statement, presentedDigest := digest, now }

/-- The concrete row domain and roles are satisfiable; no hidden map-key or
unique-login oracle is assumed. Metadata/crypto validity remains admitted input. -/
theorem seed_wellFormed (metadata : M) (binding : B) (firstPayload secondPayload : Payload S D T R)
    (firstRole secondRole unrelated : P) (frame : F) :
    WellFormed (seed metadata binding firstPayload secondPayload firstRole secondRole unrelated frame) := by
  constructor
  · intro key row found
    simp only [seed] at found
    split_ifs at found <;> simp_all [first, second, seed]
    all_goals subst row; simp
  · intro leftKey rightKey left right leftFound rightFound same
    simp only [seed] at leftFound rightFound
    split_ifs at leftFound <;> split_ifs at rightFound <;> simp_all [first, second]
    all_goals subst left; subst right; simp at same

/-- A legitimate changed-metadata store update reaches a successful commit and
acknowledgement. It changes exactly one current row and deployment metadata,
leaving the second replica, binding, all roles, digest/issuance and stable fields
as given by expectedMatched/expectedRow. Full metadata/payload types are opaque. -/
theorem normal_update_succeeds (old next : M) (binding : B) (different : old ≠ next)
    (firstPayload secondPayload : Payload S D T R) (statement : S) (now : T)
    (firstRole secondRole unrelated : P) (frame : F)
    (accepted : MetadataAccepted M B) (valid : accepted next binding) :
    let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
    let req := request next statement firstPayload.tokenDigest now
    let registrations := fun _ : Nat =>
      ({ candidate := first old binding firstPayload, role := firstRole, kind := .ordinary } :
        RegistrationTransaction.Request Nat Nat (M × B) (Payload S D T R) P)
    let updates := fun _ : Nat => req
    WellFormed db ∧ ∃ result,
      RegistrationUpdate.Execution true methods accepted (RegistrationUpdate.initial db registrations updates) result ∧
      result.core.db = expectedMatched db req (first old binding firstPayload) ∧
      result.core.lock = none ∧ (result.updates 0).phase = .committed ∧
      (result.updates 0).response = .success ∧
      result.events = [.update 0 [.deploymentMetadata next,
        .matchedRow 0 (patched (first old binding firstPayload) req)]] := by
  dsimp only
  let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
  let req := request next statement firstPayload.tokenDigest now
  let registrations := fun _ : Nat =>
    ({ candidate := first old binding firstPayload, role := firstRole, kind := .ordinary } :
      RegistrationTransaction.Request Nat Nat (M × B) (Payload S D T R) P)
  let updates := fun _ : Nat => req
  let start := RegistrationUpdate.initial db registrations updates
  let preflight := updateTx start 0
    { start.updates 0 with phase := .preflight, authenticated := some (first old binding firstPayload) }
  have preflightStep : RegistrationUpdate.Step true methods accepted start preflight :=
    RegistrationUpdate.Step.preflight start 0 (first old binding firstPayload)
      (by simp [methods]) rfl rfl rfl different
  let begun := updateTx preflight 0 { preflight.updates 0 with phase := .begun }
  have beginStep : RegistrationUpdate.Step true methods accepted preflight begun :=
    RegistrationUpdate.Step.beginUpdate preflight 0 (by simp [preflight, updateTx, put])
  let metadataWritten :=
    { (updateTx begun 0 { begun.updates 0 with
        phase := .metadataWritten
        staged := (begun.updates 0).staged ++ [.deploymentMetadata (begun.updates 0).request.metadata] }) with
      core := { begun.core with lock := some 0 } }
  have metadataStep : RegistrationUpdate.Step true methods accepted begun metadataWritten :=
    RegistrationUpdate.Step.metadata begun 0 (Or.inl ⟨rfl, by simp [begun, updateTx, put]⟩) rfl valid
  let matched := updateTx metadataWritten 0 { metadataWritten.updates 0 with
    phase := .ready
    matched := some (first old binding firstPayload)
    outcome := .updated
    staged := (metadataWritten.updates 0).staged ++
      [.matchedRow (metadataWritten.updates 0).request.client
        (patched (first old binding firstPayload) (metadataWritten.updates 0).request)] }
  have matchStep : RegistrationUpdate.Step true methods accepted metadataWritten matched :=
    RegistrationUpdate.Step.matchRow metadataWritten 0 (first old binding firstPayload)
      ⟨rfl, by simp [metadataWritten, updateTx, put]⟩ rfl
  let committing := updateTx matched 0 { matched.updates 0 with phase := .committing }
  have requestStep : RegistrationUpdate.Step true methods accepted matched committing :=
    RegistrationUpdate.Step.requestCommit matched 0 (by simp [matched, updateTx, put])
  let committed := RegistrationUpdate.committedState committing 0
  have commitStep : RegistrationUpdate.Step true methods accepted committing committed :=
    RegistrationUpdate.Step.serverCommit committing 0 (by simp [committing, updateTx, put])
  let result := updateTx committed 0 { committed.updates 0 with response := .success }
  have acknowledge : RegistrationUpdate.Step true methods accepted committed result :=
    RegistrationUpdate.Step.acknowledgeUpdate committed 0
      (by simp [committed, RegistrationUpdate.committedState, updateTx, put]) rfl
  have execution : RegistrationUpdate.Execution true methods accepted start result :=
    .tail (.tail (.tail (.tail (.tail (.tail (.tail (.refl start) preflightStep) beginStep)
      metadataStep) matchStep) requestStep) commitStep) acknowledge
  refine ⟨seed_wellFormed old binding firstPayload secondPayload firstRole secondRole unrelated frame,
    result, execution, ?_, ?_, ?_, ?_, ?_⟩
  · change RegistrationUpdate.commit db (committing.updates 0).staged = _
    have staged : (committing.updates 0).staged = [.deploymentMetadata next,
        .matchedRow 0 (patched (first old binding firstPayload) req)] := by
      simp [committing, matched, metadataWritten, begun, preflight, start,
        RegistrationUpdate.initial, updateTx, put, updates, req, request]
    rw [staged]
    exact matched_program_exact db req (first old binding firstPayload)
  · simp [result, committed, RegistrationUpdate.committedState, updateTx, release, put, committing,
      matched, metadataWritten]
  · simp [result, committed, RegistrationUpdate.committedState, updateTx, put]
  · simp [result, updateTx, put]
  · simp [result, committed, RegistrationUpdate.committedState, committing, matched, metadataWritten,
      begun, preflight, start, RegistrationUpdate.initial, updateTx, put, updates, req, request]

#print axioms seed_wellFormed
#print axioms normal_update_succeeds
run_cmd do
  for declaration in [``seed_wellFormed, ``normal_update_succeeds] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration update witness axiom: {name}"

end RegistrationUpdate.Witnesses
