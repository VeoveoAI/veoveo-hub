import RefreshTransactionModel
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- The prefix invariant is a property of the Hub program, not a commit guard. -/
def CheckedPhase (phase : Phase) : Prop :=
  phase = .writing ∨ phase = .ready ∨ phase = .committing ∨ phase = .committed

def Aligned (tx : Transaction) : Prop :=
  ((tx.phase = .idle ∨ tx.phase = .waiting ∨ tx.phase = .lockRequested ∨
    tx.phase = .locked ∨ tx.phase = .read) → tx.staged = [] ∧ tx.remaining = []) ∧
  (CheckedPhase tx.phase → tx.staged ++ tx.remaining = tx.program) ∧
  ((tx.phase = .ready ∨ tx.phase = .committing ∨ tx.phase = .committed) → tx.remaining = []) ∧
  (tx.response = .success → tx.phase = .committed ∨ tx.phase = .aborted)

def Alignment (state : State) : Prop := ∀ id, Aligned (state.transactions id)

theorem initial_alignment (db : Database) (requests : Id → Request) : Alignment (initial db requests) := by
  intro id
  simp [Aligned, initial, CheckedPhase]

theorem alignment_step {before after : State} (step : Step before after)
    (aligned : Alignment before) : Alignment after := by
  intro subject
  have atSubject := aligned subject
  clear aligned
  cases step <;>
    simp only [update, put] at * <;>
    try split_ifs at *
  all_goals simp_all [Aligned, CheckedPhase, decisionPhase, List.append_assoc]
  all_goals try (split_ifs <;> simp_all)
  all_goals
    first
    | simpa only [atSubject.2.1, List.append_nil] using atSubject.1
    | simpa only [atSubject.2, List.append_nil] using atSubject.1

theorem execution_alignment {before after : State} (execution : Execution before after)
    (aligned : Alignment before) : Alignment after := by
  induction execution with
  | refl => exact aligned
  | tail execution step ih => exact alignment_step step ih

/-- No arbitrary finite interleaving of maintenance/other publishers/rejection
writes can make this protocol request commit before staging its entire program. -/
theorem committing_program_complete (db : Database) (requests : Id → Request) (state : State)
    (execution : Execution (initial db requests) state) (id : Id)
    (phase : (state.transactions id).phase = .committing) :
    (state.transactions id).staged = (state.transactions id).program := by
  have invariant := execution_alignment execution (initial_alignment db requests) id
  have done := invariant.2.2.1 (Or.inr (Or.inl phase))
  simpa only [done, List.append_nil] using invariant.2.1 (Or.inr (Or.inr (Or.inl phase)))

/-- Exact family result of the actual ordered rotation statements. -/
theorem rotation_family_exact (db : Database) (family : Family) (predecessor : Key)
    (candidate : Token) (now : Timestamp) (audit : AuditRow)
    (stored : db.family = some family) :
    (commit db [.insertToken candidate, .consume predecessor candidate.id now,
      .advance candidate.generation, .audit [audit]]).family =
      some { family with current_generation := candidate.generation } := by
  simp [commit, applyWrites, applyWrite, stored]

/-- A committed generation-only rotation preserves the already-proved session
predicate at the same request/time. Native Serde/caller wiring is still external. -/
theorem rotation_preserves_admission (family : Family) (generation : U64)
    (request : SessionFamilyProof.Request) :
    policy.session.SessionFamilyAuthority.allows
      (RefreshPredicateProof.sessionAuthority { family with current_generation := generation }) request =
    policy.session.SessionFamilyAuthority.allows (RefreshPredicateProof.sessionAuthority family) request :=
  RefreshPredicateProof.rotation_preserves_session_admission family request generation

theorem duplicate_family_unchanged (db : Database) (audit : AuditRow) :
    (commit db [.audit [audit]]).family = db.family := rfl

theorem duplicate_tokens_unchanged (db : Database) (audit : AuditRow) :
    (commit db [.audit [audit]]).tokens = db.tokens := rfl

/-- UPDATE-if-present is essential under maintenance races. -/
theorem consume_absent_does_not_recreate (db : Database) (key replacement : Key) (now : Timestamp)
    (missing : db.tokens key = none) :
    (applyWrite db (.consume key replacement now)).tokens key = none := by
  simp [applyWrite, put, missing]

theorem mark_absent_does_not_recreate (db : Database) (key : Key) (now : Timestamp)
    (missing : db.tokens key = none) :
    (applyWrite db (.markReplay key now)).tokens key = none := by
  simp [applyWrite, put, missing]

/-- First revocation wins, independently for the existing time and reason fields;
wellformed native rows ensure both fields are present together. -/
theorem revocation_preserves_first (db : Database) (family : Family)
    (first later : Timestamp) (firstReason laterReason : Reason)
    (stored : db.family = some family) (time : family.revoked_at = some first)
    (reason : family.revocation = some firstReason) :
    (applyWrite db (.revoke later laterReason)).family = some family := by
  simp only [applyWrite, stored, Option.map_some, coalesce, time, reason]
  rw [← time, ← reason]

end
end RefreshTransaction

#print axioms RefreshTransaction.committing_program_complete
#print axioms RefreshTransaction.rotation_family_exact
#print axioms RefreshTransaction.rotation_preserves_admission
#print axioms RefreshTransaction.revocation_preserves_first

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.committing_program_complete,
      ``RefreshTransaction.rotation_family_exact, ``RefreshTransaction.rotation_preserves_admission,
      ``RefreshTransaction.duplicate_family_unchanged, ``RefreshTransaction.duplicate_tokens_unchanged,
      ``RefreshTransaction.consume_absent_does_not_recreate, ``RefreshTransaction.mark_absent_does_not_recreate,
      ``RefreshTransaction.revocation_preserves_first] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh transaction axiom in {declaration}: {found}"
