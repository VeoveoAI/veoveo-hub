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

private theorem finishNoop (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F)
    (phase : (before.updates 0).phase = .preflight)
    (pending : (before.updates 0).response = .pending) (free : before.core.lock = none)
    (absent : before.core.db.registrations (before.updates 0).request.client = none)
    (valid : accepted (before.updates 0).request.metadata before.core.db.metadata.2) :
    ∃ result, RegistrationUpdate.Execution true methods accepted before result ∧
      result.core.db = before.core.db ∧ result.events = before.events ∧
      result.core.transactions = before.core.transactions ∧ result.core.lock = none ∧ (result.updates 0).phase = .aborted ∧
      (result.updates 0).response = .success := by
  let begun := updateTx before 0 { before.updates 0 with phase := .begun }
  have starts : RegistrationUpdate.Step true methods accepted before begun :=
    .beginUpdate before 0 phase
  let metadataWritten :=
    { (updateTx begun 0 { begun.updates 0 with
        phase := .metadataWritten
        staged := (begun.updates 0).staged ++ [.deploymentMetadata (begun.updates 0).request.metadata] }) with
      core := { begun.core with lock := some 0 } }
  have metadata : RegistrationUpdate.Step true methods accepted begun metadataWritten :=
    .metadata begun 0 (Or.inl ⟨rfl, by simp [begun, updateTx, put]⟩) free valid
  let missing := updateTx metadataWritten 0
    { metadataWritten.updates 0 with phase := .rollbackPending, outcome := .noop }
  have missed : RegistrationUpdate.Step true methods accepted metadataWritten missing :=
    .missRow metadataWritten 0 (Or.inl ⟨rfl, by simp [metadataWritten, updateTx, put]⟩) absent
  let rolledBack :=
    { (updateTx missing 0 { missing.updates 0 with phase := .aborted }) with
      core := { missing.core with lock := release missing.core.lock 0 } }
  have rolled : RegistrationUpdate.Step true methods accepted missing rolledBack :=
    .rollbackNoop missing 0 (by simp [missing, updateTx, put]) (by simp [missing, updateTx, put])
  let result := updateTx rolledBack 0 { rolledBack.updates 0 with response := .success }
  have acknowledged : RegistrationUpdate.Step true methods accepted rolledBack result :=
    .acknowledgeNoop rolledBack 0 (by simp [rolledBack, updateTx, put])
      (by simp [rolledBack, missing, updateTx, put])
      (by simpa [rolledBack, missing, metadataWritten, begun, updateTx, put] using pending)
  have execution : RegistrationUpdate.Execution true methods accepted before result :=
    .tail (.tail (.tail (.tail (.tail (.refl before) starts) metadata) missed) rolled) acknowledged
  refine ⟨result, execution, rfl, rfl, rfl, ?_, ?_, ?_⟩
  · simp [result, rolledBack, missing, metadataWritten, updateTx, release, put]
  · simp [result, rolledBack, updateTx, put]
  · simp [result, updateTx, put]

private theorem finishOldMissing (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F)
    (phase : (before.updates 0).phase = .preflight)
    (pending : (before.updates 0).response = .pending) (empty : (before.updates 0).staged = [])
    (free : before.core.lock = none)
    (absent : before.core.db.registrations (before.updates 0).request.client = none)
    (valid : accepted (before.updates 0).request.metadata before.core.db.metadata.2) :
    ∃ result, RegistrationUpdate.Execution false methods accepted before result ∧
      result.core.db = { before.core.db with
        metadata := ((before.updates 0).request.metadata, before.core.db.metadata.2) } ∧
      result.events = before.events ++ [.update 0 [.deploymentMetadata (before.updates 0).request.metadata]] ∧
      result.core.transactions = before.core.transactions ∧ result.core.lock = none ∧ (result.updates 0).phase = .committed ∧
      (result.updates 0).outcome = .noop ∧ (result.updates 0).response = .success := by
  let begun := updateTx before 0 { before.updates 0 with phase := .begun }
  have starts : RegistrationUpdate.Step false methods accepted before begun :=
    .beginUpdate before 0 phase
  let missing := updateTx begun 0 { begun.updates 0 with phase := .registrationWritten, outcome := .noop }
  have missed : RegistrationUpdate.Step false methods accepted begun missing :=
    .missRow begun 0 (Or.inr ⟨rfl, by simp [begun, updateTx, put]⟩) absent
  let metadataWritten :=
    { (updateTx missing 0 { missing.updates 0 with
        phase := .ready
        staged := (missing.updates 0).staged ++ [.deploymentMetadata (missing.updates 0).request.metadata] }) with
      core := { missing.core with lock := some 0 } }
  have metadata : RegistrationUpdate.Step false methods accepted missing metadataWritten :=
    .metadata missing 0 (Or.inr ⟨rfl, by simp [missing, updateTx, put]⟩) free valid
  let committing := updateTx metadataWritten 0 { metadataWritten.updates 0 with phase := .committing }
  have commitRequested : RegistrationUpdate.Step false methods accepted metadataWritten committing :=
    .requestCommit metadataWritten 0 (by simp [metadataWritten, updateTx, put])
  let committed := RegistrationUpdate.committedState committing 0
  have committedStep : RegistrationUpdate.Step false methods accepted committing committed :=
    .serverCommit committing 0 (by simp [committing, updateTx, put])
  let result := updateTx committed 0 { committed.updates 0 with response := .success }
  have acknowledged : RegistrationUpdate.Step false methods accepted committed result :=
    .acknowledgeUpdate committed 0 (by simp [committed, RegistrationUpdate.committedState, updateTx, put])
      (by simpa [committed, RegistrationUpdate.committedState, committing, metadataWritten,
        missing, begun, updateTx, put] using pending)
  have execution : RegistrationUpdate.Execution false methods accepted before result :=
    .tail (.tail (.tail (.tail (.tail (.tail (.refl before) starts) missed) metadata)
      commitRequested) committedStep) acknowledged
  have staged : (committing.updates 0).staged = [.deploymentMetadata (before.updates 0).request.metadata] := by
    simp [committing, metadataWritten, missing, begun, updateTx, put, empty]
  refine ⟨result, execution, ?_, ?_, rfl, ?_, ?_, ?_, ?_⟩
  · change RegistrationUpdate.commit before.core.db (committing.updates 0).staged = _
    rw [staged]
    rfl
  · change before.events ++ [.update 0 (committing.updates 0).staged] = _
    rw [staged]
  · simp [result, committed, RegistrationUpdate.committedState, updateTx, put,
      release, committing, metadataWritten]
  · simp [result, committed, RegistrationUpdate.committedState, updateTx, put]
  · simp [result, committed, RegistrationUpdate.committedState, committing, metadataWritten,
      missing, updateTx, put]
  · simp [result, updateTx, put]

private theorem execution_trans {fixed : Bool} {accepted : MetadataAccepted M B}
    {a b c : State Nat Nat M B S D T R P F}
    (first : RegistrationUpdate.Execution fixed methods accepted a b)
    (second : RegistrationUpdate.Execution fixed methods accepted b c) :
    RegistrationUpdate.Execution fixed methods accepted a c := by
  induction second with
  | refl => exact first
  | tail previous step ih => exact RegistrationUpdate.Execution.tail ih step

private theorem registrationMethod (id : Nat) (actor : (some 1 : Option Nat) = some id) :
    methods id = .register := by
  have same := Option.some.inj actor
  subst id
  simp [methods]

/-- Proof-only wrapper for ONE real fixed registration constructor, with its
Prop label and all original statement guards. No operation effect is assumed. -/
private theorem registerPlain {fixed : Bool} (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F) (nextCore : RegisterState Nat Nat M B S D T R P F)
    (actual : RegistrationTransaction.Step true before.core nextCore)
    (label : RegisterAction actual (some 1) []) :
    RegistrationUpdate.Step fixed methods accepted before { before with core := nextCore } := by
  simpa only [List.append_nil] using
    (RegistrationUpdate.Step.register before nextCore actual (some 1) [] label registrationMethod)

/- Private proof-state chunks for the shared ordinary replacement prefix. These
are concrete constructor-shaped endpoints, not an assumed registration reducer. -/
private def replacementRead (core : RegisterState Nat Nat M B S D T R P F) :=
  let begun := RegistrationTransaction.update core 1 { core.transactions 1 with phase := .waiting }
  let locked := { (RegistrationTransaction.update begun 1
    { begun.transactions 1 with phase := .locked }) with lock := some 1 }
  RegistrationTransaction.update locked 1
    { locked.transactions 1 with phase := .read, observed := some locked.db.metadata }

private def replacementDeleting (core : RegisterState Nat Nat M B S D T R P F) :=
  let replacing := RegistrationTransaction.update core 1
    { core.transactions 1 with phase := .upserting, replacing := true }
  { (RegistrationTransaction.update replacing 1 { replacing.transactions 1 with
      phase := .deleting
      staged := (replacing.transactions 1).staged ++
        [.deployment (replacing.transactions 1).request.candidate.metadata] }) with lock := some 1 }

private def replacementCreating (core : RegisterState Nat Nat M B S D T R P F)
    (victims : List (Row Nat Nat M B S D T R)) :=
  let deleted := RegistrationTransaction.update core 1 { core.transactions 1 with
    phase := .dropping
    captured := victims
    drops := victims.map (·.login)
    staged := (core.transactions 1).staged ++ [.deleteRows (victims.map (·.client))] }
  let dropOne := RegistrationTransaction.update deleted 1 { deleted.transactions 1 with
    drops := [11]
    staged := (deleted.transactions 1).staged ++ [.dropLogin 10] }
  let dropTwo := RegistrationTransaction.update dropOne 1 { dropOne.transactions 1 with
    drops := []
    staged := (dropOne.transactions 1).staged ++ [.dropLogin 11] }
  RegistrationTransaction.update dropTwo 1 { dropTwo.transactions 1 with phase := .creating }

private def replacementCommitting (core : RegisterState Nat Nat M B S D T R P F) :=
  let created := RegistrationTransaction.update core 1 { core.transactions 1 with
    phase := .inserting
    staged := (core.transactions 1).staged ++
      [.createLogin (core.transactions 1).request.candidate.login (core.transactions 1).request.role] }
  let inserted := RegistrationTransaction.update created 1 { created.transactions 1 with
    phase := .ready
    staged := (created.transactions 1).staged ++ [.insert (created.transactions 1).request.candidate] }
  RegistrationTransaction.update inserted 1 { inserted.transactions 1 with phase := .committing }

private theorem replacement_read_steps {fixed : Bool} (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F)
    (idle : (before.core.transactions 1).phase = .idle) (free : before.core.lock = none)
    (ordinary : (before.core.transactions 1).request.kind = .ordinary) :
    RegistrationUpdate.Execution fixed methods accepted before
      { before with core := replacementRead before.core } := by
  let begun := RegistrationTransaction.update before.core 1
    { before.core.transactions 1 with phase := .waiting }
  let a : State Nat Nat M B S D T R P F := { before with core := begun }
  have beginStep : RegistrationUpdate.Step fixed methods accepted before a :=
    registerPlain accepted before begun (RegistrationTransaction.Step.begin before.core 1 idle)
      (RegisterAction.begin before.core 1 idle)
  let locked := { (RegistrationTransaction.update begun 1
    { begun.transactions 1 with phase := .locked }) with lock := some 1 }
  let b : State Nat Nat M B S D T R P F := { a with core := locked }
  have waiting : (begun.transactions 1).phase = .waiting := by simp [begun, RegistrationTransaction.update, put]
  have acquireStep : RegistrationUpdate.Step fixed methods accepted a b :=
    registerPlain accepted a locked (RegistrationTransaction.Step.acquire begun 1 waiting free (Or.inl rfl))
      (RegisterAction.acquire begun 1 waiting free (Or.inl rfl))
  let read := RegistrationTransaction.update locked 1
    { locked.transactions 1 with phase := .read, observed := some locked.db.metadata }
  have phase : (locked.transactions 1).phase = .locked := by simp [locked, RegistrationTransaction.update, put]
  have kind : (locked.transactions 1).request.kind = .ordinary := by
    simpa [locked, begun, RegistrationTransaction.update, put] using ordinary
  have readStep : RegistrationUpdate.Step fixed methods accepted b { b with core := read } :=
    registerPlain accepted b read (RegistrationTransaction.Step.read locked 1 phase kind)
      (RegisterAction.read locked 1 phase kind)
  exact .tail (.tail (.tail (.refl before) beginStep) acquireStep) readStep


private theorem replacement_decide_steps {fixed : Bool} (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F)
    (phase : (before.core.transactions 1).phase = .read)
    (different : (before.core.transactions 1).observed ≠ some (before.core.transactions 1).request.candidate.metadata)
    (locked : before.core.lock = some 1) :
    RegistrationUpdate.Execution fixed methods accepted before
      { before with core := replacementDeleting before.core } := by
  let replacing := RegistrationTransaction.update before.core 1
    { before.core.transactions 1 with phase := .upserting, replacing := true }
  let a : State Nat Nat M B S D T R P F := { before with core := replacing }
  have choose : RegistrationUpdate.Step fixed methods accepted before a :=
    registerPlain accepted before replacing (RegistrationTransaction.Step.replace before.core 1 phase different)
      (RegisterAction.replace before.core 1 phase different)
  let deleting := { (RegistrationTransaction.update replacing 1 { replacing.transactions 1 with
    phase := .deleting
    staged := (replacing.transactions 1).staged ++
      [.deployment (replacing.transactions 1).request.candidate.metadata] }) with lock := some 1 }
  have upserting : (replacing.transactions 1).phase = .upserting := by
    simp [replacing, RegistrationTransaction.update, put]
  have available : replacing.lock = none ∨ replacing.lock = some 1 := Or.inr locked
  have upsert : RegistrationUpdate.Step fixed methods accepted a { a with core := deleting } :=
    registerPlain accepted a deleting (RegistrationTransaction.Step.upsert replacing 1 upserting available)
      (RegisterAction.upsert replacing 1 upserting available)
  exact .tail (.tail (.refl before) choose) upsert

private theorem replacement_delete_steps {fixed : Bool} (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F) (victims : List (Row Nat Nat M B S D T R))
    (phase : (before.core.transactions 1).phase = .deleting)
    (snapshot : Listing (applyWrites before.core.db (before.core.transactions 1).staged) victims)
    (logins : victims.map (·.login) = [10, 11]) :
    RegistrationUpdate.Execution fixed methods accepted before
      { before with core := replacementCreating before.core victims } := by
  let deleted := RegistrationTransaction.update before.core 1 { before.core.transactions 1 with
    phase := .dropping
    captured := victims
    drops := victims.map (·.login)
    staged := (before.core.transactions 1).staged ++ [.deleteRows (victims.map (·.client))] }
  let a : State Nat Nat M B S D T R P F := { before with core := deleted }
  have deletedStep : RegistrationUpdate.Step fixed methods accepted before a :=
    registerPlain accepted before deleted (RegistrationTransaction.Step.delete before.core 1 victims phase snapshot)
      (RegisterAction.delete before.core 1 victims phase snapshot)
  let dropOne := RegistrationTransaction.update deleted 1 { deleted.transactions 1 with
    drops := [11], staged := (deleted.transactions 1).staged ++ [.dropLogin 10] }
  let b : State Nat Nat M B S D T R P F := { a with core := dropOne }
  have dropping : (deleted.transactions 1).phase = .dropping := by simp [deleted, RegistrationTransaction.update, put]
  have next : (deleted.transactions 1).drops = 10 :: [11] := by
    simpa [deleted, RegistrationTransaction.update, put] using logins
  have firstDrop : RegistrationUpdate.Step fixed methods accepted a b :=
    registerPlain accepted a dropOne (RegistrationTransaction.Step.drop deleted 1 10 [11] dropping next)
      (RegisterAction.drop deleted 1 10 [11] dropping next)
  let dropTwo := RegistrationTransaction.update dropOne 1 { dropOne.transactions 1 with
    drops := [], staged := (dropOne.transactions 1).staged ++ [.dropLogin 11] }
  let c : State Nat Nat M B S D T R P F := { b with core := dropTwo }
  have droppingOne : (dropOne.transactions 1).phase = .dropping := by
    simp [dropOne, RegistrationTransaction.update, put, dropping]
  have nextOne : (dropOne.transactions 1).drops = 11 :: [] := by simp [dropOne, RegistrationTransaction.update, put]
  have secondDrop : RegistrationUpdate.Step fixed methods accepted b c :=
    registerPlain accepted b dropTwo (RegistrationTransaction.Step.drop dropOne 1 11 [] droppingOne nextOne)
      (RegisterAction.drop dropOne 1 11 [] droppingOne nextOne)
  let creating := RegistrationTransaction.update dropTwo 1 { dropTwo.transactions 1 with phase := .creating }
  have droppingTwo : (dropTwo.transactions 1).phase = .dropping := by
    simp [dropTwo, RegistrationTransaction.update, put, droppingOne]
  have done : (dropTwo.transactions 1).drops = [] := by simp [dropTwo, RegistrationTransaction.update, put]
  have finish : RegistrationUpdate.Step fixed methods accepted c { c with core := creating } :=
    registerPlain accepted c creating (RegistrationTransaction.Step.dropped dropTwo 1 droppingTwo done)
      (RegisterAction.dropped dropTwo 1 droppingTwo done)
  exact .tail (.tail (.tail (.tail (.refl before) deletedStep) firstDrop) secondDrop) finish

private theorem replacement_insert_steps {fixed : Bool} (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F)
    (phase : (before.core.transactions 1).phase = .creating)
    (validCreate : ValidWrite (applyWrites before.core.db (before.core.transactions 1).staged)
      (.createLogin (before.core.transactions 1).request.candidate.login (before.core.transactions 1).request.role))
    (validInsert : ValidWrite (applyWrites before.core.db ((before.core.transactions 1).staged ++
      [.createLogin (before.core.transactions 1).request.candidate.login (before.core.transactions 1).request.role]))
      (.insert (before.core.transactions 1).request.candidate)) :
    RegistrationUpdate.Execution fixed methods accepted before
      { before with core := replacementCommitting before.core } := by
  let created := RegistrationTransaction.update before.core 1 { before.core.transactions 1 with
    phase := .inserting
    staged := (before.core.transactions 1).staged ++
      [.createLogin (before.core.transactions 1).request.candidate.login (before.core.transactions 1).request.role] }
  let a : State Nat Nat M B S D T R P F := { before with core := created }
  have createStep : RegistrationUpdate.Step fixed methods accepted before a :=
    registerPlain accepted before created (RegistrationTransaction.Step.create before.core 1 phase validCreate)
      (RegisterAction.create before.core 1 phase validCreate)
  let inserted := RegistrationTransaction.update created 1 { created.transactions 1 with
    phase := .ready
    staged := (created.transactions 1).staged ++ [.insert (created.transactions 1).request.candidate] }
  let b : State Nat Nat M B S D T R P F := { a with core := inserted }
  have inserting : (created.transactions 1).phase = .inserting := by simp [created, RegistrationTransaction.update, put]
  have valid : ValidWrite (applyWrites created.db (created.transactions 1).staged)
      (.insert (created.transactions 1).request.candidate) := by
    simpa [created, RegistrationTransaction.update, put] using validInsert
  have insertStep : RegistrationUpdate.Step fixed methods accepted a b :=
    registerPlain accepted a inserted (RegistrationTransaction.Step.insert created 1 inserting valid)
      (RegisterAction.insert created 1 inserting valid)
  let committing := RegistrationTransaction.update inserted 1 { inserted.transactions 1 with phase := .committing }
  have ready : (inserted.transactions 1).phase = .ready := by simp [inserted, RegistrationTransaction.update, put]
  have requestStep : RegistrationUpdate.Step fixed methods accepted b { b with core := committing } :=
    registerPlain accepted b committing (RegistrationTransaction.Step.requestCommit inserted 1 ready)
      (RegisterAction.requestCommit inserted 1 ready)
  exact .tail (.tail (.tail (.refl before) createStep) insertStep) requestStep

private def replacementFinished (before : State Nat Nat M B S D T R P F) : State Nat Nat M B S D T R P F :=
  let committed := RegistrationTransaction.committedState before.core 1
  { before with
    core := RegistrationTransaction.update committed 1 { committed.transactions 1 with response := .success }
    events := before.events ++ [.register 1 (before.core.transactions 1).staged] }

/- Shallow projections are proved on an ABSTRACT endpoint before substituting
large fixture states. They do not normalize a staged program or assume its effect. -/
private theorem finished_db (state : State Nat Nat M B S D T R P F) :
    (replacementFinished state).core.db =
      RegistrationTransaction.commit state.core.db (state.core.transactions 1).staged := rfl

private theorem finished_updates (state : State Nat Nat M B S D T R P F) :
    (replacementFinished state).updates = state.updates := rfl

private theorem finished_response (state : State Nat Nat M B S D T R P F) :
    ((replacementFinished state).core.transactions 1).response = .success := by
  simp [replacementFinished, RegistrationTransaction.update, put]

private theorem finished_lock (state : State Nat Nat M B S D T R P F) :
    (replacementFinished state).core.lock = release state.core.lock 1 := rfl

private theorem finished_events (state : State Nat Nat M B S D T R P F) :
    (replacementFinished state).events = state.events ++ [.register 1 (state.core.transactions 1).staged] := rfl

private theorem replacement_finish_steps {fixed : Bool} (accepted : MetadataAccepted M B)
    (before : State Nat Nat M B S D T R P F)
    (phase : (before.core.transactions 1).phase = .committing)
    (pending : (before.core.transactions 1).response = .pending) :
    RegistrationUpdate.Execution fixed methods accepted before (replacementFinished before) := by
  let committed := RegistrationTransaction.committedState before.core 1
  let a : State Nat Nat M B S D T R P F :=
    { before with core := committed, events := before.events ++ [.register 1 (before.core.transactions 1).staged] }
  have commitStep : RegistrationUpdate.Step fixed methods accepted before a :=
    RegistrationUpdate.Step.register before committed (RegistrationTransaction.Step.serverCommit before.core 1 phase)
      (some 1) [.register 1 (before.core.transactions 1).staged]
      (RegisterAction.serverCommit before.core 1 phase) registrationMethod
  let result := RegistrationTransaction.update committed 1 { committed.transactions 1 with response := .success }
  have committedPhase : (committed.transactions 1).phase = .committed := by
    simp [committed, RegistrationTransaction.committedState, RegistrationTransaction.update, put]
  have stillPending : (committed.transactions 1).response = .pending := by
    simpa [committed, RegistrationTransaction.committedState, RegistrationTransaction.update, put] using pending
  have acknowledge : RegistrationUpdate.Step fixed methods accepted a { a with core := result } :=
    registerPlain accepted a result (RegistrationTransaction.Step.acknowledge committed 1 committedPhase stillPending)
      (RegisterAction.acknowledge committed 1 committedPhase stillPending)
  exact .tail (.tail (.refl before) commitStep) acknowledge


private def winner (metadata : M) (binding : B) (payload : Payload S D T R) : Row Nat Nat M B S D T R :=
  { client := 2, login := 12, metadata := (metadata, binding), payload }

private def replacementRequest (metadata : M) (binding : B) (payload : Payload S D T R) (role : P) :
    RegistrationTransaction.Request Nat Nat (M × B) (Payload S D T R) P :=
  { candidate := winner metadata binding payload, role, kind := .ordinary }

private theorem seed_listing (metadata : M) (binding : B) (firstPayload secondPayload : Payload S D T R)
    (firstRole secondRole unrelated : P) (frame : F) :
    Listing (seed metadata binding firstPayload secondPayload firstRole secondRole unrelated frame)
      [first metadata binding firstPayload, second metadata binding secondPayload] := by
  constructor
  · simp [first, second]
  · funext key
    by_cases zero : key = 0 <;> by_cases one : key = 1 <;>
      simp [seed, rowMap, first, second, put, zero, one]

/- Concrete endpoint projections exceed the default kernel reduction budget.
Keep this finite allowance local to the shared-prefix proof. -/
set_option maxHeartbeats 800000 in
/-- Actual ordinary replacement of the two-row fixture, valid under EITHER
update-source order. Both executions use the SAME fixed registration protocol. -/
private theorem replacement_seed_steps (accepted : MetadataAccepted M B)
    (old next : M) (binding : B) (different : old ≠ next)
    (firstPayload secondPayload winnerPayload : Payload S D T R)
    (firstRole secondRole winnerRole unrelated : P) (frame : F)
    (before : State Nat Nat M B S D T R P F)
    (database : before.core.db = seed old binding firstPayload secondPayload firstRole secondRole unrelated frame)
    (free : before.core.lock = none)
    (transaction : before.core.transactions 1 = { request := replacementRequest next binding winnerPayload winnerRole }) :
    let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
    let reg := replacementRequest next binding winnerPayload winnerRole
    let victims := [first old binding firstPayload, second old binding secondPayload]
    ∃ after, (∀ fixed : Bool, RegistrationUpdate.Execution fixed methods accepted before after) ∧
      after.core.db = expectedReplacement db reg ∧ after.updates = before.updates ∧
      (after.core.transactions 1).response = .success ∧ after.core.lock = none ∧
      after.events = before.events ++ [.register 1 (replaceWrites reg victims)] := by
  dsimp only
  let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
  let reg := replacementRequest next binding winnerPayload winnerRole
  let victims := [first old binding firstPayload, second old binding secondPayload]
  let read : State Nat Nat M B S D T R P F := { before with core := replacementRead before.core }
  have readSteps (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted before read :=
    replacement_read_steps accepted before (by simp [transaction]) free (by simp [transaction, replacementRequest])
  have readDB : read.core.db = db := database
  have readLock : read.core.lock = some 1 := rfl
  have readTx : read.core.transactions 1 =
      { request := reg, phase := .read, observed := some (old, binding) } := by
    simp [read, replacementRead, RegistrationTransaction.update, put, transaction, reg, database, seed]
  let deleting : State Nat Nat M B S D T R P F := { read with core := replacementDeleting read.core }
  have selected : (read.core.transactions 1).observed ≠ some (read.core.transactions 1).request.candidate.metadata := by
    rw [readTx]
    simp [reg, replacementRequest, winner, different]
  have decideSteps (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted read deleting :=
    replacement_decide_steps accepted read (by simp [readTx]) selected readLock
  have deleteDB : deleting.core.db = db := readDB
  have deleteTx : deleting.core.transactions 1 =
      { request := reg
        phase := .deleting
        observed := some (old, binding)
        replacing := true
        staged := [.deployment (next, binding)] } := by
    simp [deleting, replacementDeleting, RegistrationTransaction.update, put, readTx,
      reg, replacementRequest, winner]
  have snapshot : Listing (applyWrites deleting.core.db (deleting.core.transactions 1).staged) victims := by
    rw [deleteDB, deleteTx]
    simpa [applyWrites, RegistrationTransaction.applyWrite, Listing, db, victims] using
      seed_listing old binding firstPayload secondPayload firstRole secondRole unrelated frame
  let creating : State Nat Nat M B S D T R P F := { deleting with core := replacementCreating deleting.core victims }
  have deleteSteps (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted deleting creating :=
    replacement_delete_steps accepted deleting victims (by simp [deleteTx]) snapshot
      (by simp [victims, first, second])
  have createDB : creating.core.db = db := deleteDB
  have createTx : creating.core.transactions 1 =
      { request := reg
        phase := .creating
        observed := some (old, binding)
        replacing := true
        captured := victims
        staged := [.deployment (next, binding), .deleteRows [0, 1], .dropLogin 10, .dropLogin 11] } := by
    simp [creating, replacementCreating, RegistrationTransaction.update, put, deleteTx, victims, first, second]
  have validCreate : ValidWrite (applyWrites creating.core.db (creating.core.transactions 1).staged)
      (.createLogin (creating.core.transactions 1).request.candidate.login (creating.core.transactions 1).request.role) := by
    rw [createDB, createTx]
    simp [reg, replacementRequest, winner, ValidWrite, applyWrites, RegistrationTransaction.applyWrite, put, db, seed]
  have validInsert : ValidWrite (applyWrites creating.core.db ((creating.core.transactions 1).staged ++
      [.createLogin (creating.core.transactions 1).request.candidate.login (creating.core.transactions 1).request.role]))
      (.insert (creating.core.transactions 1).request.candidate) := by
    rw [createDB, createTx]
    simp [reg, replacementRequest, winner, ValidWrite, applyWrites, RegistrationTransaction.applyWrite, put, db, seed]
    intro key stored notZero notOne found
    simp [notZero, notOne] at found
  let committing : State Nat Nat M B S D T R P F := { creating with core := replacementCommitting creating.core }
  have insertSteps (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted creating committing :=
    replacement_insert_steps accepted creating (by simp [createTx]) validCreate validInsert
  have committingTx : committing.core.transactions 1 =
      { request := reg
        phase := .committing
        observed := some (old, binding)
        replacing := true
        captured := victims
        staged := replaceWrites reg victims } := by
    simp [committing, replacementCommitting, RegistrationTransaction.update, put, createTx,
      replaceWrites, admitWrites, reg, replacementRequest, winner, victims, first, second]
  have finish (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted committing (replacementFinished committing) :=
    replacement_finish_steps accepted committing (by simp [committingTx]) (by simp [committingTx])
  have execution (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted before (replacementFinished committing) :=
    execution_trans (execution_trans (execution_trans (execution_trans (readSteps fixed) (decideSteps fixed))
      (deleteSteps fixed)) (insertSteps fixed)) (finish fixed)
  have finalDB := finished_db committing
  rw [committingTx] at finalDB
  have committingDB : committing.core.db = db := createDB
  rw [committingDB] at finalDB
  have exactDB : (replacementFinished committing).core.db = expectedReplacement db reg :=
    finalDB.trans (replacement_exact db reg victims
      (seed_listing old binding firstPayload secondPayload firstRole secondRole unrelated frame))
  have exactUpdates : (replacementFinished committing).updates = before.updates := finished_updates committing
  have finalLock := finished_lock committing
  have lock : committing.core.lock = some 1 := rfl
  rw [lock] at finalLock
  simp only [release, ite_true] at finalLock
  have finalEvents := finished_events committing
  rw [committingTx] at finalEvents
  have exactEvents : (replacementFinished committing).events =
      before.events ++ [.register 1 (replaceWrites reg victims)] := finalEvents
  exact ⟨replacementFinished committing, execution, exactDB, exactUpdates,
    finished_response committing, finalLock, exactEvents⟩


private def capturedSeed (old replacement stale : M) (binding : B)
    (firstPayload secondPayload winnerPayload : Payload S D T R) (statement : S) (now : T)
    (firstRole secondRole winnerRole unrelated : P) (frame : F) : State Nat Nat M B S D T R P F :=
  let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
  let reg := replacementRequest replacement binding winnerPayload winnerRole
  let req := request stale statement firstPayload.tokenDigest now
  let initial := RegistrationUpdate.initial db (fun _ => reg) (fun _ => req)
  updateTx initial 0 { initial.updates 0 with
    phase := .preflight
    authenticated := some (first old binding firstPayload) }

/-- Concrete preflight-gap success: authenticate/read the old client, run a FULL
ordinary changed-metadata replacement, then the fixed stale store update rolls
back and returns Ok. Exact replacement state survives this chosen isolated
suffix. The TWO-row model seed differs from the one-seed native regression;
opaque payloads/roles/binding are preserved, with no HTTP/crypto refinement claim. -/
theorem stale_update_after_replacement_is_noop
    (old replacement stale : M) (binding : B) (replacementDifferent : old ≠ replacement)
    (updateDifferent : old ≠ stale)
    (firstPayload secondPayload winnerPayload : Payload S D T R) (statement : S) (now : T)
    (firstRole secondRole winnerRole unrelated : P) (frame : F)
    (accepted : MetadataAccepted M B) (valid : accepted stale binding) :
    let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
    let reg := replacementRequest replacement binding winnerPayload winnerRole
    let req := request stale statement firstPayload.tokenDigest now
    let victims := [first old binding firstPayload, second old binding secondPayload]
    let initial := RegistrationUpdate.initial db (fun _ => reg) (fun _ => req)
    let captured := capturedSeed old replacement stale binding firstPayload secondPayload winnerPayload
      statement now firstRole secondRole winnerRole unrelated frame
    WellFormed db ∧ RegistrationUpdate.Execution true methods accepted initial captured ∧
      (captured.updates 0).authenticated = some (first old binding firstPayload) ∧
      (first old binding firstPayload).payload.tokenDigest = req.presentedDigest ∧
      ∃ replaced, (∀ fixed : Bool, RegistrationUpdate.Execution fixed methods accepted captured replaced) ∧
      replaced.core.db = expectedReplacement db reg ∧
      (replaced.core.transactions 1).response = .success ∧
      replaced.core.db.registrations 0 = none ∧
      ∃ result, RegistrationUpdate.Execution true methods accepted replaced result ∧
        result.core.db = expectedReplacement db reg ∧ result.core.lock = none ∧
        (result.core.transactions 1).response = .success ∧
        (result.updates 0).phase = .aborted ∧ (result.updates 0).response = .success ∧
        result.events = [.register 1 (replaceWrites reg victims)] ∧
        ∀ writes, Event.update 0 writes ∉ result.events := by
  dsimp only
  let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
  let reg := replacementRequest replacement binding winnerPayload winnerRole
  let req := request stale statement firstPayload.tokenDigest now
  let victims := [first old binding firstPayload, second old binding secondPayload]
  let initial := RegistrationUpdate.initial db (fun _ => reg) (fun _ => req)
  let captured := capturedSeed old replacement stale binding firstPayload secondPayload winnerPayload
    statement now firstRole secondRole winnerRole unrelated frame
  have preflight : RegistrationUpdate.Step true methods accepted initial captured :=
    .preflight initial 0 (first old binding firstPayload) (by simp [methods]) rfl rfl rfl updateDifferent
  obtain ⟨replaced, registrationRun, fields⟩ := replacement_seed_steps accepted old replacement binding replacementDifferent
    firstPayload secondPayload winnerPayload firstRole secondRole winnerRole unrelated frame captured rfl rfl rfl
  change replaced.core.db = expectedReplacement db reg ∧ replaced.updates = captured.updates ∧
    (replaced.core.transactions 1).response = .success ∧ replaced.core.lock = none ∧
    replaced.events = captured.events ++ [.register 1 (replaceWrites reg victims)] at fields
  have client : (replaced.updates 0).request.client = 0 := by
    rw [fields.2.1]
    rfl
  have absent : replaced.core.db.registrations (replaced.updates 0).request.client = none := by
    rw [fields.1, fields.2.1]
    simp [captured, capturedSeed, updateTx, put, RegistrationUpdate.initial,
      request, expectedReplacement, reg, replacementRequest, winner]
  have pending : (replaced.updates 0).response = .pending := by
    rw [fields.2.1]
    simp [captured, capturedSeed, updateTx, put, RegistrationUpdate.initial]
  have metadataValid : accepted (replaced.updates 0).request.metadata replaced.core.db.metadata.2 := by
    rw [fields.1, fields.2.1]
    simpa [captured, capturedSeed, updateTx, put, RegistrationUpdate.initial, expectedReplacement,
      request, reg, replacementRequest, winner] using valid
  obtain ⟨result, suffix, sameDB, sameEvents, sameTransactions, free, aborted, success⟩ :=
    finishNoop accepted replaced (by rw [fields.2.1]; simp [captured, capturedSeed, updateTx, put])
      pending fields.2.2.2.1 absent metadataValid
  refine ⟨seed_wellFormed old binding firstPayload secondPayload firstRole secondRole unrelated frame,
    .tail (.refl initial) preflight, rfl, rfl, replaced, registrationRun, fields.1, fields.2.2.1, ?_,
    result, suffix, sameDB.trans fields.1, free, ?_, aborted, success, ?_, ?_⟩
  · simpa only [client] using absent
  · rw [sameTransactions]
    exact fields.2.2.1
  · rw [sameEvents, fields.2.2.2.2]
    rfl
  · intro writes
    rw [sameEvents, fields.2.2.2.2]
    simp [captured, capturedSeed, updateTx, RegistrationUpdate.initial]

#print axioms stale_update_after_replacement_is_noop
run_cmd do
  let axioms ← Lean.collectAxioms ``stale_update_after_replacement_is_noop
  for name in axioms do
    unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
      throwError "unreviewed stale update no-op witness axiom: {name}"


private noncomputable def sequentialEvent
    (registrations : Nat → RegistrationTransaction.Request Nat Nat (M × B) (Payload S D T R) P)
    (updates : Nat → RegistrationUpdate.Request Nat M S D T)
    (db : DB Nat Nat M B S D T R P F) : Event Nat Nat M B S D T R P → DB Nat Nat M B S D T R P F
  | .register id _ => RegistrationTransaction.sequential db (registrations id)
  | .update id _ => sequentialUpdate db (updates id)

private theorem serial_event_fold
    (registrations : Nat → RegistrationTransaction.Request Nat Nat (M × B) (Payload S D T R) P)
    (updates : Nat → RegistrationUpdate.Request Nat M S D T)
    {first last : DB Nat Nat M B S D T R P F} {events : List (Event Nat Nat M B S D T R P)}
    (history : RegistrationUpdate.SerialHistory registrations updates first events last) :
    events.foldl (sequentialEvent registrations updates) first = last := by
  induction history with
  | nil => rfl
  | register previous id writes ih =>
    simp only [List.foldl_append, List.foldl_cons, List.foldl_nil, sequentialEvent, ih]
  | update previous id writes ih =>
    simp only [List.foldl_append, List.foldl_cons, List.foldl_nil, sequentialEvent, ih]

/-- Paired source control: ONE reached ordinary-replacement endpoint is used by
both update orders. Fixed source rolls back; former update-only order commits
stale deployment metadata after affecting zero client rows. Registration itself
uses the FIXED lock/read/compare protocol throughout, never the older unlocked
register fault. Full DB maps remain explicit, and actual event order fails the
same independent mixed serial contract. This is a design-model counterexample,
linked separately to the measured native one-seed regression. -/
theorem old_update_order_is_not_serial
    (old replacement stale : M) (binding : B)
    (replacementDifferent : old ≠ replacement) (updateDifferent : old ≠ stale)
    (wrongMetadata : replacement ≠ stale)
    (firstPayload secondPayload winnerPayload : Payload S D T R) (statement : S) (now : T)
    (firstRole secondRole winnerRole unrelated : P) (frame : F)
    (accepted : MetadataAccepted M B) (valid : accepted stale binding) :
    let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
    let reg := replacementRequest replacement binding winnerPayload winnerRole
    let req := request stale statement firstPayload.tokenDigest now
    let victims := [first old binding firstPayload, second old binding secondPayload]
    let registrations := fun _ : Nat => reg
    let updates := fun _ : Nat => req
    let initial := RegistrationUpdate.initial db registrations updates
    let captured := capturedSeed old replacement stale binding firstPayload secondPayload winnerPayload
      statement now firstRole secondRole winnerRole unrelated frame
    WellFormed db ∧
      (∀ fixed : Bool, RegistrationUpdate.Execution fixed methods accepted initial captured) ∧
      ∃ replaced, (∀ fixed : Bool, RegistrationUpdate.Execution fixed methods accepted captured replaced) ∧
        replaced.core.db = expectedReplacement db reg ∧
        (replaced.core.transactions 1).response = .success ∧
        ∃ fixedResult oldResult,
          RegistrationUpdate.Execution true methods accepted replaced fixedResult ∧
          RegistrationUpdate.Execution false methods accepted replaced oldResult ∧
          fixedResult.core.db = expectedReplacement db reg ∧
          (fixedResult.updates 0).phase = .aborted ∧ (fixedResult.updates 0).response = .success ∧
          oldResult.core.db = { expectedReplacement db reg with metadata := (stale, binding) } ∧
          (oldResult.core.transactions 1).response = .success ∧ oldResult.core.lock = none ∧
          (oldResult.updates 0).phase = .committed ∧ (oldResult.updates 0).response = .success ∧
          oldResult.events = [.register 1 (replaceWrites reg victims), .update 0 [.deploymentMetadata stale]] ∧
          ¬RegistrationUpdate.SerialHistory registrations updates db oldResult.events (owned oldResult.core.db db.frame) := by
  dsimp only
  let db := seed old binding firstPayload secondPayload firstRole secondRole unrelated frame
  let reg := replacementRequest replacement binding winnerPayload winnerRole
  let req := request stale statement firstPayload.tokenDigest now
  let victims := [first old binding firstPayload, second old binding secondPayload]
  let registrations := fun _ : Nat => reg
  let updates := fun _ : Nat => req
  let initial := RegistrationUpdate.initial db registrations updates
  let captured := capturedSeed old replacement stale binding firstPayload secondPayload winnerPayload
    statement now firstRole secondRole winnerRole unrelated frame
  have preflight (fixed : Bool) : RegistrationUpdate.Execution fixed methods accepted initial captured :=
    .tail (.refl initial) (RegistrationUpdate.Step.preflight initial 0 (first old binding firstPayload)
      (by simp [methods]) rfl rfl rfl updateDifferent)
  obtain ⟨replaced, registrationRun, fields⟩ := replacement_seed_steps accepted old replacement binding replacementDifferent
    firstPayload secondPayload winnerPayload firstRole secondRole winnerRole unrelated frame captured rfl rfl rfl
  change replaced.core.db = expectedReplacement db reg ∧ replaced.updates = captured.updates ∧
    (replaced.core.transactions 1).response = .success ∧ replaced.core.lock = none ∧
    replaced.events = captured.events ++ [.register 1 (replaceWrites reg victims)] at fields
  have client : (replaced.updates 0).request.client = 0 := by rw [fields.2.1]; rfl
  have metadata : (replaced.updates 0).request.metadata = stale := by rw [fields.2.1]; rfl
  have pending : (replaced.updates 0).response = .pending := by rw [fields.2.1]; rfl
  have empty : (replaced.updates 0).staged = [] := by rw [fields.2.1]; rfl
  have phase : (replaced.updates 0).phase = .preflight := by rw [fields.2.1]; rfl
  have absent : replaced.core.db.registrations (replaced.updates 0).request.client = none := by
    rw [client, fields.1]
    simp [expectedReplacement, reg, replacementRequest, winner, put]
  have acceptedMetadata : accepted (replaced.updates 0).request.metadata replaced.core.db.metadata.2 := by
    rw [metadata, fields.1]
    exact valid
  obtain ⟨fixedResult, fixedRun, fixedDB, fixedEvents, fixedTx, fixedFree, fixedAborted, fixedSuccess⟩ :=
    finishNoop accepted replaced phase pending fields.2.2.2.1 absent acceptedMetadata
  obtain ⟨oldResult, oldRun, oldDB, oldEvents, oldTx, oldFree, oldCommitted, oldOutcome, oldSuccess⟩ :=
    finishOldMissing accepted replaced phase pending empty fields.2.2.2.1 absent acceptedMetadata
  have wrongDB : oldResult.core.db = { expectedReplacement db reg with metadata := (stale, binding) } := by
    rw [oldDB, fields.1, metadata]
    rfl
  have actualEvents : oldResult.events =
      [.register 1 (replaceWrites reg victims), .update 0 [.deploymentMetadata stale]] := by
    rw [oldEvents, fields.2.2.2.2, metadata]
    rfl
  have notSerial : ¬RegistrationUpdate.SerialHistory registrations updates db oldResult.events
      (owned oldResult.core.db db.frame) := by
    intro history
    have folded := serial_event_fold registrations updates history
    rw [actualEvents] at folded
    have compared := congrArg (fun value : DB Nat Nat M B S D T R P F => value.metadata.1) folded
    have serialMetadata :
        (([Event.register 1 (replaceWrites reg victims), Event.update 0 [.deploymentMetadata stale]]).foldl
          (sequentialEvent registrations updates) db).metadata.1 = replacement := by
      simp [sequentialEvent, registrations, updates, RegistrationTransaction.sequential,
        sequentialUpdate, expectedReplacement, expectedReplica, db, seed, reg,
        replacementRequest, winner, req, request, put, replacementDifferent]
    rw [serialMetadata, wrongDB] at compared
    exact wrongMetadata compared
  refine ⟨seed_wellFormed old binding firstPayload secondPayload firstRole secondRole unrelated frame,
    preflight, replaced, registrationRun, fields.1, fields.2.2.1,
    fixedResult, oldResult, fixedRun, oldRun, fixedDB.trans fields.1,
    fixedAborted, fixedSuccess, wrongDB, ?_, oldFree, oldCommitted, oldSuccess, actualEvents, notSerial⟩
  rw [oldTx]
  exact fields.2.2.1

#print axioms old_update_order_is_not_serial
run_cmd do
  let axioms ← Lean.collectAxioms ``old_update_order_is_not_serial
  for name in axioms do
    unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
      throwError "unreviewed old update-order counterexample axiom: {name}"

#print axioms finishNoop
#print axioms finishOldMissing

#print axioms seed_wellFormed
#print axioms normal_update_succeeds
run_cmd do
  for declaration in [``seed_wellFormed, ``normal_update_succeeds] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration update witness axiom: {name}"

end RegistrationUpdate.Witnesses
