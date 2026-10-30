import RegistrationUpdateSpecification

namespace RegistrationUpdate
open RegistrationTransaction
variable {C L M B S D T R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M] [DecidableEq B] [DecidableEq D]

/-- Whole row equality, not an incomplete field selection. -/
theorem patched_exact (row : Row C L M B S D T R) (request : Request C M S D T) :
    patched row request = expectedRow row request := by
  rfl

/-- Independently specified effect of the TWO actual staged SQL assignments. -/
theorem matched_program_exact (db : DB C L M B S D T R P F)
    (request : Request C M S D T) (row : Row C L M B S D T R) :
    commit db [.deploymentMetadata request.metadata,
      .matchedRow request.client (patched row request)] = expectedMatched db request row := by
  rfl

theorem matched_sequential_exact (db : DB C L M B S D T R P F)
    (request : Request C M S D T) (row : Row C L M B S D T R)
    (found : db.registrations request.client = some row) :
    commit db [.deploymentMetadata request.metadata,
      .matchedRow request.client (patched row request)] = sequentialUpdate db request := by
  rw [sequentialUpdate, found]
  exact matched_program_exact db request row

/-- Native client PK correspondence follows from the initial/current row domain,
not from a new check in the SQL or a fabricated client ID on a stale snapshot. -/
theorem matched_key (db : DB C L M B S D T R P F)
    (request : Request C M S D T) (row : Row C L M B S D T R)
    (wellFormed : WellFormed db) (found : db.registrations request.client = some row) :
    (patched row request).client = request.client := by
  exact (wellFormed.1 request.client row found).1

theorem matched_fields (db : DB C L M B S D T R P F)
    (request : Request C M S D T) (row : Row C L M B S D T R) :
    let after := expectedMatched db request row
    after.metadata = (request.metadata, db.metadata.2) ∧
    after.registrations request.client = some (expectedRow row request) ∧
    (∀ other, other ≠ request.client → after.registrations other = db.registrations other) ∧
    after.logins = db.logins ∧ after.frame = db.frame ∧
    (expectedRow row request).client = row.client ∧
    (expectedRow row request).login = row.login ∧
    (expectedRow row request).metadata.2 = row.metadata.2 ∧
    (expectedRow row request).payload.tokenDigest = row.payload.tokenDigest ∧
    (expectedRow row request).payload.issuedAt = row.payload.issuedAt ∧
    (expectedRow row request).payload.stable = row.payload.stable := by
  simp [expectedMatched, expectedRow]
  intro other different same
  exact (different same).elim

/-- Exact preservation at the SERVER ROLLBACK transition. No claim is made that
other writers cannot commit between this step and the later Ok acknowledgement. -/
theorem rollback_noop_exact (methods : Nat → Method) (accepted : MetadataAccepted M B)
    (state : State C L M B S D T R P F) (id : Nat)
    (phase : (state.updates id).phase = .rollbackPending)
    (outcome : (state.updates id).outcome = .noop) :
    ∃ after, Step true methods accepted state after ∧
      after.core.db = state.core.db ∧ after.events = state.events ∧
      after.core.commits = state.core.commits ∧
      (after.updates id).phase = .aborted ∧ (after.updates id).outcome = .noop := by
  refine ⟨_, Step.rollbackNoop state id phase outcome, rfl, rfl, rfl, ?_⟩
  simp [updateTx, put, outcome]

#print axioms patched_exact
#print axioms matched_program_exact
#print axioms matched_sequential_exact
#print axioms matched_key
#print axioms matched_fields
#print axioms rollback_noop_exact

run_cmd do
  for declaration in [``patched_exact, ``matched_program_exact, ``matched_sequential_exact,
      ``matched_key, ``matched_fields, ``rollback_noop_exact] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration update effect axiom: {name}"

end RegistrationUpdate
