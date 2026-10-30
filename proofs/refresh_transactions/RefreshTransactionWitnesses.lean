import RefreshTransactionExecution

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/- Concrete finite row fixtures. Family identity/principal/scope metadata are
arbitrary supplied typed values, unchanged by the trace. The nominal envelope
payload is not an interpretation of native crypto or a plaintext token. -/
def key0 : Key := ⟨"00000000-0000-4000-8000-000000000001"⟩
def key1 : Key := ⟨"00000000-0000-4000-8000-000000000002"⟩
def key2 : Key := ⟨"00000000-0000-4000-8000-000000000003"⟩
def digest0 : veoveo_types.digest.Sha256Digest :=
  ⟨"sha256:0000000000000000000000000000000000000000000000000000000000000000"⟩
def digest1 : veoveo_types.digest.Sha256Digest :=
  ⟨"sha256:1111111111111111111111111111111111111111111111111111111111111111"⟩
def digest2 : veoveo_types.digest.Sha256Digest :=
  ⟨"sha256:2222222222222222222222222222222222222222222222222222222222222222"⟩

def fixtureTemplate : Family where
  id := ⟨"00000000-0000-4000-8000-000000000010"⟩
  authorization_server := "refresh-fixture"
  protected_resource := ⟨"https://refresh-fixture.example/resource"⟩
  oauth_client_id := ⟨"refresh-fixture-client"⟩
  work_context := ⟨"refresh-fixture"⟩
  principal := {
    principal := {
      id := ⟨"https://refresh-fixture.example/idp#caller"⟩
      kind := .User
      issuer := ⟨"https://refresh-fixture.example/idp"⟩
      subject := ⟨"caller"⟩
      tenant := none
      groups := ∅
      group_roles := ∅
      roles := ∅
      scopes := ∅
      data_labels := ∅
      assurances := ∅
      authenticated_at := none }
    display_name := "Refresh fixture" }
  scopes := ∅
  current_generation := 0#u64
  issued_at := (0, 0)
  expires_at := (100, 0)
  revoked_at := none
  revocation := none

def witnessFamily (template : Family) : Family :=
  { template with
    current_generation := 0#u64
    issued_at := (0, 0)
    expires_at := (100, 0)
    revoked_at := none
    revocation := none }

def initialToken (family : Family) : Token where
  id := key0
  family := family.id
  digest := digest0
  generation := 0#u64
  issued_at := (0, 0)
  expires_at := (100, 0)
  consumed_at := none
  replacement := none
  replay_detected_at := none
  delivery := none

def successor1 (family : Family) : Token :=
  { initialToken family with
    id := key1
    digest := digest1
    generation := 1#u64
    issued_at := (10, 0)
    delivery := some { envelope := ⟨1⟩, expires_at := (14, 0) } }

def candidate2 (family : Family) : Token :=
  { initialToken family with
    id := key2
    digest := digest2
    generation := 1#u64
    issued_at := (11, 0)
    delivery := some { envelope := ⟨2⟩, expires_at := (15, 0) } }

def witnessDB (template : Family) : Database where
  family := some (witnessFamily template)
  tokens := fun key => if key = key0 then some (initialToken (witnessFamily template)) else none
  audit := fun _ => none
  unrelated := fun _ => none
  rejectionAudit := fun _ => none

def firstRequest (template : Family) : Request where
  operation := .rotate
  digest := digest0
  candidate := successor1 (witnessFamily template)
  now := (10, 0)
  success := ⟨100, 1000⟩
  duplicate := ⟨101, 1010⟩

def retryRequest (template : Family) : Request where
  operation := .rotate
  digest := digest0
  candidate := candidate2 (witnessFamily template)
  now := (11, 0)
  success := ⟨102, 1020⟩
  duplicate := ⟨103, 1030⟩

def witnessRequests (template : Family) : Id → Request :=
  fun id => if id = 0 then firstRequest template else retryRequest template

def rotationWrites (template : Family) : List Write :=
  let successor := successor1 (witnessFamily template)
  [.insertToken successor, .consume key0 successor.id (10, 0),
    .advance successor.generation, .audit [(firstRequest template).success]]

theorem fixture_shape (template : Family) :
    CandidateProjection (witnessFamily template) (firstRequest template).candidate ∧
    ReplacementShape (witnessFamily template) (initialToken (witnessFamily template))
      (firstRequest template).candidate := by
  have arithmetic : U64.checked_add 0#u64 1#u64 = some 1#u64 := by rfl
  simp [CandidateProjection, ReplacementShape, firstRequest, successor1, initialToken,
    witnessFamily, Before, SessionFamilyProof.Before, arithmetic]

theorem fixture_initial_choice (template : Family) :
    choose (witnessDB template) (firstRequest template) (witnessFamily template)
      (initialToken (witnessFamily template)) =
      (.rotated (successor1 (witnessFamily template)), rotationWrites template) := by
  simp [choose, firstRequest, successor1, initialToken, witnessFamily,
    rotationWrites, Before, SessionFamilyProof.Before]

/-- Begin and snapshot the lock query's family target before waiting. -/
def beginLookup (state : State) (id : Id) (token : Token) : State :=
  let begun := update state id { state.transactions id with phase := .waiting }
  update begun id { begun.transactions id with phase := .lockRequested, lookupFamily := some token.family }

theorem beginLookup_execution (state : State) (id : Id) (token : Token)
    (phase : (state.transactions id).phase = .idle)
    (stored : state.db.tokens token.id = some token)
    (digest : token.digest = (state.transactions id).request.digest) :
    Execution state (beginLookup state id token) := by
  let begun := update state id { state.transactions id with phase := .waiting }
  have first : Step state begun := Step.begin state id phase
  have second : Step begun (beginLookup state id token) :=
    Step.queryFamily begun id token (by simp [begun, update, put]) stored
      (by simpa [begun, update, put] using digest)
  exact Execution.tail (Execution.tail (Execution.refl state) first) second

/-- This witness state follows the three actual constructors after lock wait;
its outcome/program are computed by choose, never supplied as a success oracle. -/
def prepared (state : State) (id : Id) (family : Family) (token : Token) : State :=
  let locked : State := { update state id
    { state.transactions id with phase := .locked, familyRead := some family } with familyLock := some id }
  let read := update locked id { locked.transactions id with phase := .read, tokenRead := some token }
  update read id { read.transactions id with
    phase := decisionPhase (choose read.db (read.transactions id).request family token).2
    outcome := (choose read.db (read.transactions id).request family token).1
    program := (choose read.db (read.transactions id).request family token).2
    remaining := (choose read.db (read.transactions id).request family token).2 }

theorem prepared_execution (state : State) (id : Id) (family : Family) (token : Token)
    (phase : (state.transactions id).phase = .lockRequested) (free : state.familyLock = none)
    (target : (state.transactions id).lookupFamily = some family.id)
    (familyRow : state.db.family = some family) (tokenRow : state.db.tokens token.id = some token)
    (digest : token.digest = (state.transactions id).request.digest)
    (operation : (state.transactions id).request.operation = .rotate)
    (projection : CandidateProjection family (state.transactions id).request.candidate)
    (shape : ReplacementShape family token (state.transactions id).request.candidate) :
    Execution state (prepared state id family token) := by
  let locked : State := { update state id
    { state.transactions id with phase := .locked, familyRead := some family } with familyLock := some id }
  let read := update locked id { locked.transactions id with phase := .read, tokenRead := some token }
  have lockStep : Step state locked := Step.lock state id family phase free target familyRow
  have readStep : Step locked read := Step.readToken locked id token
    (by simp [locked, update, put]) tokenRow
    (by simpa [locked, update, put] using digest)
  have decideStep : Step read (prepared state id family token) := Step.decide read id family token
    (by simp [read, update, put])
    (by simpa [read, locked, update, put] using operation)
    (by simp [read, locked, update, put]) (by simp [read, update, put])
    (by simpa [read, locked, update, put] using projection)
    (by simpa [read, locked, update, put] using shape)
  exact Execution.tail (Execution.tail (Execution.tail (Execution.refl state) lockStep) readStep) decideStep

theorem initial_row_only (template : Family) (key : Key) (token : Token)
    (stored : (witnessDB template).tokens key = some token) :
    key = key0 ∧ token = initialToken (witnessFamily template) := by
  by_cases same : key = key0
  · refine ⟨same, ?_⟩
    have row : initialToken (witnessFamily template) = token := by
      simpa [witnessDB, same] using stored
    exact row.symm
  · simp [witnessDB, same] at stored

theorem initial_constraints_satisfiable (template : Family) : WellFormed (witnessDB template) := by
  refine ⟨?_, ?_, ?_⟩
  · intro family stored
    have same : witnessFamily template = family := by simpa [witnessDB] using stored
    subst family
    simp [witnessFamily, StoredGeneration, Before, SessionFamilyProof.Before]
  · intro key token stored
    rcases initial_row_only template key token stored with ⟨rfl, rfl⟩
    simp [initialToken, witnessDB, StoredGeneration, Before, SessionFamilyProof.Before]
  · intro left right a b rowA rowB different
    have leftKey := (initial_row_only template left a rowA).1
    have rightKey := (initial_row_only template right b rowB).1
    exact False.elim (different (leftKey.trans rightKey.symm))

theorem rotation_constraints_satisfiable (template : Family) :
    ValidWrites (witnessDB template) (rotationWrites template) := by
  let family := witnessFamily template
  let candidate := successor1 family
  have insertValid : ValidWrite (witnessDB template) (.insertToken candidate) := by
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
    · simp [witnessDB, candidate, successor1, key0, key1]
    · norm_num [StoredGeneration, candidate, successor1]
    · simp [candidate, successor1, initialToken, Before, SessionFamilyProof.Before]
    · exact ⟨family, rfl, rfl⟩
    · intro key old stored
      have same := (initial_row_only template key old stored).2
      subst old
      simp [initialToken, candidate, successor1, digest0, digest1]
    · intro key old stored sameFamily
      have same := (initial_row_only template key old stored).2
      subst old
      simp [initialToken, candidate, successor1]
  let inserted := applyWrite (witnessDB template) (.insertToken candidate)
  let consumed := applyWrite inserted (.consume key0 candidate.id (10, 0))
  let advanced := applyWrite consumed (.advance candidate.generation)
  change ValidWrite (witnessDB template) (.insertToken candidate) ∧
    ValidWrite inserted (.consume key0 candidate.id (10, 0)) ∧
    ValidWrite consumed (.advance candidate.generation) ∧
    ValidWrite advanced (.audit [(firstRequest template).success]) ∧ True
  refine ⟨insertValid, ?_, ?_, ?_, trivial⟩
  · exact Or.inr ⟨candidate, by simp [inserted, applyWrite, put]⟩
  · norm_num [ValidWrite, StoredGeneration, candidate, successor1]
  · change (none = none ∨ none = some 1000) ∧ True
    exact ⟨Or.inl rfl, trivial⟩

def consumed0 (family : Family) : Token :=
  { initialToken family with consumed_at := some (10, 0), replacement := some key1 }

def rotatedDB (template : Family) : Database := commit (witnessDB template) (rotationWrites template)

theorem rotation_full_rows (template : Family) :
    (rotatedDB template).family = some { witnessFamily template with current_generation := 1#u64 } ∧
    (rotatedDB template).tokens =
      (fun key => if key = key1 then some (successor1 (witnessFamily template))
        else if key = key0 then some (consumed0 (witnessFamily template)) else none) ∧
    (rotatedDB template).audit = (fun id => if id = 100 then some 1000 else none) := by
  refine ⟨?_, ?_, ?_⟩
  · simp [rotatedDB, rotationWrites, commit, applyWrites, applyWrite, witnessDB, successor1]
  · funext key
    by_cases first : key = key0
    · subst key
      simp [rotatedDB, rotationWrites, commit, applyWrites, applyWrite, put, successor1,
        witnessDB, initialToken, consumed0, key0, key1]
    · by_cases next : key = key1
      · subst key
        simp [rotatedDB, rotationWrites, commit, applyWrites, applyWrite, put, successor1,
          witnessDB, initialToken, consumed0, key0, key1]
      · simp [rotatedDB, rotationWrites, commit, applyWrites, applyWrite, put, successor1,
          witnessDB, initialToken, consumed0, first, next]
  · funext id
    simp [rotatedDB, rotationWrites, commit, applyWrites, applyWrite, appendAudit,
      firstRequest, witnessDB, put]

theorem retry_shape (template : Family) :
    CandidateProjection { witnessFamily template with current_generation := 1#u64 }
      (retryRequest template).candidate ∧
    ReplacementShape { witnessFamily template with current_generation := 1#u64 }
      (consumed0 (witnessFamily template)) (retryRequest template).candidate := by
  have arithmetic : U64.checked_add 0#u64 1#u64 = some 1#u64 := by rfl
  simp [CandidateProjection, ReplacementShape, retryRequest, candidate2, consumed0, initialToken,
    witnessFamily, Before, SessionFamilyProof.Before, arithmetic]

theorem retry_same_successor_choice (template : Family) :
    choose (rotatedDB template) (retryRequest template)
      { witnessFamily template with current_generation := 1#u64 } (consumed0 (witnessFamily template)) =
      (.duplicate (successor1 (witnessFamily template)), [.audit [(retryRequest template).duplicate]]) := by
  have arithmetic : U64.checked_add 0#u64 1#u64 = some 1#u64 := by rfl
  have rows := rotation_full_rows template
  simp [choose, Successor, rows.2.1, retryRequest, consumed0, initialToken, successor1,
    witnessFamily, CanDeliver, RefreshPredicateProof.Deliverable, RefreshPredicateProof.NextGeneration,
    Before, SessionFamilyProof.Before, arithmetic, key0, key1]


theorem retry_constraints_satisfiable (template : Family) :
    ValidWrites (rotatedDB template) [.audit [(retryRequest template).duplicate]] := by
  have rows := (rotation_full_rows template).2.2
  change (((rotatedDB template).audit 103 = none ∨ (rotatedDB template).audit 103 = some 1030) ∧ True) ∧ True
  simp [rows]

def waitingState (template : Family) : State :=
  let family := witnessFamily template
  let token := initialToken family
  let start := initial (witnessDB template) (witnessRequests template)
  let queried := beginLookup start 0 token
  let firstReady := prepared queried 0 family token
  beginLookup firstReady 1 token

theorem waiting_execution (template : Family) :
    Execution (initial (witnessDB template) (witnessRequests template)) (waitingState template) := by
  let family := witnessFamily template
  let token := initialToken family
  let start := initial (witnessDB template) (witnessRequests template)
  let queried := beginLookup start 0 token
  let firstReady := prepared queried 0 family token
  have first := beginLookup_execution start 0 token rfl
    (by simp [start, initial, witnessDB, token, initialToken, family])
    (by simp [start, initial, witnessRequests, firstRequest, token, initialToken, family])
  have second := prepared_execution queried 0 family token
    (by simp [queried, beginLookup, update, put]) rfl
    (by simp [queried, beginLookup, update, put, token, initialToken, family]) rfl
    (by simp [queried, beginLookup, update, start, initial, witnessDB, token, initialToken, family])
    (by simp [queried, beginLookup, update, put, start, initial, witnessRequests, firstRequest, token, initialToken, family])
    (by simp [queried, beginLookup, update, put, start, initial, witnessRequests, firstRequest])
    (by simpa [queried, beginLookup, update, put, start, initial, witnessRequests, family]
      using (fixture_shape template).1)
    (by simpa [queried, beginLookup, update, put, start, initial, witnessRequests, family, token]
      using (fixture_shape template).2)
  have third := beginLookup_execution firstReady 1 token
    (by simp [firstReady, prepared, queried, beginLookup, update, put, start, initial])
    (by simp [firstReady, prepared, queried, beginLookup, update, start, initial, witnessDB, token, initialToken, family])
    (by simp [firstReady, prepared, queried, beginLookup, update, put, start, initial,
      witnessRequests, retryRequest, token, initialToken, family])
  exact execution_trans (execution_trans first second) third

theorem waiting_properties (template : Family) :
    (waitingState template).db = witnessDB template ∧
    (waitingState template).familyLock = some 0 ∧
    (waitingState template).transactions 0 = {
      request := firstRequest template
      phase := .writing
      lookupFamily := some (witnessFamily template).id
      familyRead := some (witnessFamily template)
      tokenRead := some (initialToken (witnessFamily template))
      outcome := .rotated (successor1 (witnessFamily template))
      program := rotationWrites template
      staged := []
      remaining := rotationWrites template
      response := .pending } ∧
    (waitingState template).transactions 1 = {
      request := retryRequest template
      phase := .lockRequested
      lookupFamily := some (witnessFamily template).id } ∧
    (∀ key, (waitingState template).tokenLocks key = none) ∧
    (∀ key, (waitingState template).auditLocks key = none) := by
  have tokenFamily : (initialToken (witnessFamily template)).family = (witnessFamily template).id := rfl
  simp [waitingState, beginLookup, prepared, update, put, initial, witnessRequests,
    fixture_initial_choice, tokenFamily, decisionPhase, rotationWrites]

/-- A legitimate interleaving: the retry's lock query has started while the
first attempt owns the family lock; after that first commit, both calls succeed
with the same full stored successor, despite distinct proposed candidates. -/
theorem concurrent_rotation_and_same_successor (template : Family) :
    WellFormed (witnessDB template) ∧
    ∃ waiting result,
      Execution (initial (witnessDB template) (witnessRequests template)) waiting ∧
      waiting.familyLock = some 0 ∧ (waiting.transactions 1).phase = .lockRequested ∧
      Execution waiting result ∧
      (result.transactions 0).response = .success ∧ (result.transactions 1).response = .success ∧
      (result.transactions 0).outcome = .rotated (successor1 (witnessFamily template)) ∧
      (result.transactions 1).outcome = .duplicate (successor1 (witnessFamily template)) ∧
      result.db = commit (rotatedDB template) [.audit [(retryRequest template).duplicate]] := by
  refine ⟨initial_constraints_satisfiable template, ?_⟩
  let waiting := waitingState template
  rcases waiting_properties template with ⟨rows, owner, firstTx, retryTx, tokensFree, auditFree⟩
  have first := finish_writing waiting 0 (rotationWrites template)
    (by rw [firstTx]) (by rw [firstTx]) (by rw [firstTx])
    (by simpa [waiting, rows, firstTx, applyWrites] using rotation_constraints_satisfiable template)
    ⟨fun key => Or.inl (tokensFree key), fun key => Or.inl (auditFree key)⟩ owner
  rcases first with ⟨rotated, firstRun, firstRows, firstOutcome, firstResponse, lockFree, tokenLocks, auditLocks, other⟩
  have rotatedRows : rotated.db = rotatedDB template := by
    simpa [waiting, rows, firstTx, rotatedDB] using firstRows
  have afterRetry : rotated.transactions 1 = {
      request := retryRequest template
      phase := .lockRequested
      lookupFamily := some (witnessFamily template).id } :=
    (other 1 Nat.one_ne_zero).trans retryTx
  let family := { witnessFamily template with current_generation := 1#u64 }
  let token := consumed0 (witnessFamily template)
  let secondReady := prepared rotated 1 family token
  have secondPrepare := prepared_execution rotated 1 family token
    (by rw [afterRetry]) lockFree (by rw [afterRetry])
    (by rw [rotatedRows]; exact (rotation_full_rows template).1)
    (by rw [rotatedRows, (rotation_full_rows template).2.1]
        simp [token, consumed0, initialToken, key0, key1])
    (by simp [afterRetry, token, consumed0, initialToken, retryRequest])
    (by simp [afterRetry, retryRequest])
    (by rw [afterRetry]; exact (retry_shape template).1)
    (by rw [afterRetry]; exact (retry_shape template).2)
  have secondRows : secondReady.db = rotatedDB template := by
    simpa [secondReady, prepared, update] using rotatedRows
  have secondTx : secondReady.transactions 1 = {
      request := retryRequest template
      phase := .writing
      lookupFamily := some (witnessFamily template).id
      familyRead := some family
      tokenRead := some token
      outcome := .duplicate (successor1 (witnessFamily template))
      program := [.audit [(retryRequest template).duplicate]]
      remaining := [.audit [(retryRequest template).duplicate]] } := by
    simp [secondReady, prepared, update, put, afterRetry, rotatedRows, family, token,
      retry_same_successor_choice, decisionPhase]
  have second := finish_writing secondReady 1 [.audit [(retryRequest template).duplicate]]
    (by rw [secondTx]) (by rw [secondTx]) (by rw [secondTx])
    (by simpa [secondRows, secondTx, applyWrites] using retry_constraints_satisfiable template)
    ⟨fun key => Or.inl (by simpa [secondReady, prepared, update] using tokenLocks key),
      fun key => Or.inl (by simpa [secondReady, prepared, update] using auditLocks key)⟩
    (by simp [secondReady, prepared, update])
  rcases second with ⟨result, secondRun, deliveredRows, deliveredOutcome, deliveredResponse,
    resultLock, resultTokenLocks, resultAuditLocks, secondOther⟩
  have preserveFirst : result.transactions 0 = rotated.transactions 0 := by
    have otherReady : secondReady.transactions 0 = rotated.transactions 0 := by
      simp [secondReady, prepared, update, put]
    exact (secondOther 0 Nat.zero_ne_one).trans otherReady
  refine ⟨waiting, result, waiting_execution template, owner, ?_, ?_, ?_, deliveredResponse, ?_, ?_, ?_⟩
  · rw [retryTx]
  · exact execution_trans firstRun (execution_trans secondPrepare secondRun)
  · rw [preserveFirst]
    exact firstResponse
  · rw [preserveFirst, firstOutcome, firstTx]
  · rw [deliveredOutcome, secondTx]
  · simpa [secondRows, secondTx] using deliveredRows

/-- Full independent output maps: only the committed successor, retained family
metadata with generation one, and the first-success/second-duplicate audit IDs. -/
theorem delivered_full_rows (template : Family) :
    let db := commit (rotatedDB template) [.audit [(retryRequest template).duplicate]]
    db.family = some { witnessFamily template with current_generation := 1#u64 } ∧
    db.tokens = (fun key => if key = key1 then some (successor1 (witnessFamily template))
      else if key = key0 then some (consumed0 (witnessFamily template)) else none) ∧
    db.audit = (fun id => if id = 103 then some 1030 else if id = 100 then some 1000 else none) := by
  have previous := rotation_full_rows template
  refine ⟨previous.1, previous.2.1, ?_⟩
  funext id
  simp [commit, applyWrites, applyWrite, appendAudit, previous.2.2, retryRequest, put]

/-- Fully closed typed data instantiate the concurrent witness. UUID/digest/
envelope operations and native store decoding are not proved by these values. -/
theorem concrete_concurrent_success :
    ∃ waiting result,
      Execution (initial (witnessDB fixtureTemplate) (witnessRequests fixtureTemplate)) waiting ∧
      waiting.familyLock = some 0 ∧ (waiting.transactions 1).phase = .lockRequested ∧
      Execution waiting result ∧
      (result.transactions 0).response = .success ∧ (result.transactions 1).response = .success ∧
      (result.transactions 0).outcome = .rotated (successor1 (witnessFamily fixtureTemplate)) ∧
      (result.transactions 1).outcome = .duplicate (successor1 (witnessFamily fixtureTemplate)) ∧
      result.db = commit (rotatedDB fixtureTemplate) [.audit [(retryRequest fixtureTemplate).duplicate]] :=
  (concurrent_rotation_and_same_successor fixtureTemplate).2

end
end RefreshTransaction

#print axioms RefreshTransaction.initial_constraints_satisfiable
#print axioms RefreshTransaction.rotation_constraints_satisfiable
#print axioms RefreshTransaction.rotation_full_rows
#print axioms RefreshTransaction.retry_same_successor_choice
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.initial_constraints_satisfiable,
      ``RefreshTransaction.rotation_constraints_satisfiable, ``RefreshTransaction.rotation_full_rows,
      ``RefreshTransaction.retry_shape, ``RefreshTransaction.retry_same_successor_choice,
      ``RefreshTransaction.retry_constraints_satisfiable, ``RefreshTransaction.concurrent_rotation_and_same_successor,
      ``RefreshTransaction.delivered_full_rows, ``RefreshTransaction.concrete_concurrent_success] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh witness axiom in {declaration}: {found}"
