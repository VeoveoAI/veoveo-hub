import RefreshTransactionProof

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

def programFor (request : Request) (family : Family) (token : Token) : Outcome → List Write
  | .rotated successor =>
      [.insertToken successor, .consume token.id successor.id request.now,
        .advance successor.generation, .audit [request.success]]
  | .duplicate _ => [.audit [request.duplicate]]
  | .replay => [.revoke request.now .TokenReplay, .markReplay token.id request.now]
  | .revoked _ => if family.revoked_at.isSome then [] else [.revoke request.now .ClientRevocation]
  | .invalid => []

theorem choose_program (db : Database) (request : Request) (family : Family) (token : Token) :
    (choose db request family token).2 = programFor request family token (choose db request family token).1 := by
  cases found : Successor db token <;>
    simp only [choose, found] <;> split_ifs <;> rfl

theorem choose_rotation_checked (db : Database) (request : Request) (family : Family)
    (token successor : Token) (shape : ReplacementShape family token request.candidate)
    (choice : (choose db request family token).1 = .rotated successor) :
    successor = request.candidate ∧ token.generation = family.current_generation ∧
      some successor.generation = U64.checked_add token.generation 1#u64 := by
  cases found : Successor db token <;>
    simp only [choose, found] at choice <;>
    split_ifs at choice <;> simp_all [ReplacementShape]
  all_goals exact UScalar.eq_of_val_eq (by tauto)

theorem revocation_program (request : Request) (binding : Binding) (family : Family) (token : Token) :
    (chooseRevocation request binding family token).2 =
      programFor request family token (chooseRevocation request binding family token).1 := by
  unfold chooseRevocation
  split_ifs <;> simp_all [programFor]

theorem revocation_not_rotation (request : Request) (binding : Binding) (family : Family)
    (token successor : Token) :
    (chooseRevocation request binding family token).1 ≠ .rotated successor := by
  unfold chooseRevocation
  split_ifs <;> intro impossible <;> cases impossible

/-- The stored program is tied to raw source branch observations; no assumed
successful result or pre-authorized replacement is used. -/
def Chosen (tx : Transaction) : Prop :=
  ∃ family token, tx.familyRead = some family ∧ tx.tokenRead = some token ∧
    tx.program = programFor tx.request family token tx.outcome ∧
    (∀ successor, tx.outcome = .rotated successor →
      tx.request.operation = .rotate ∧ successor = tx.request.candidate ∧
      token.generation = family.current_generation ∧
      some successor.generation = U64.checked_add token.generation 1#u64)

def Recorded (state : State) : Prop :=
  ∀ id, CheckedPhase (state.transactions id).phase → Chosen (state.transactions id)

theorem initial_recorded (db : Database) (requests : Id → Request) : Recorded (initial db requests) := by
  intro id checked
  simp [initial, CheckedPhase] at checked

theorem recorded_step {before after : State} (step : Step before after)
    (recorded : Recorded before) : Recorded after := by
  intro subject checked
  have prior := recorded subject
  cases step
  case decide id family token phase operation familyRead tokenRead projection shape =>
    by_cases same : subject = id
    · subst subject
      simp only [update, put]
      change Chosen _
      refine ⟨family, token, familyRead, tokenRead, choose_program _ _ _ _, ?_⟩
      intro successor choice
      exact ⟨operation, choose_rotation_checked _ _ _ _ _ shape choice⟩
    · have wasChecked : CheckedPhase (before.transactions subject).phase := by
        simpa [update, put, same] using checked
      simpa [update, put, same] using prior wasChecked
  case decideRevocation id family token binding phase operation familyRead tokenRead =>
    by_cases same : subject = id
    · subst subject
      simp only [update, put]
      change Chosen _
      refine ⟨family, token, familyRead, tokenRead, revocation_program _ _ _ _, ?_⟩
      intro successor impossible
      exact False.elim (revocation_not_rotation _ _ _ _ _ impossible)
    · have wasChecked : CheckedPhase (before.transactions subject).phase := by
        simpa [update, put, same] using checked
      simpa [update, put, same] using prior wasChecked
  all_goals clear recorded
  all_goals
    simp only [update, put] at * <;> try split_ifs at *
  all_goals simp_all [CheckedPhase, Chosen]

theorem execution_recorded {before after : State} (execution : Execution before after)
    (recorded : Recorded before) : Recorded after := by
  induction execution with
  | refl => exact recorded
  | tail execution step ih => exact recorded_step step ih

/-- Reachability now supplies both completeness and the chosen ordered program,
stronger than the given-list field-effect helper in the first checkpoint. -/
theorem reachable_rotation_program (db : Database) (requests : Id → Request) (state : State)
    (execution : Execution (initial db requests) state) (id : Id) (successor : Token)
    (phase : (state.transactions id).phase = .committing)
    (outcome : (state.transactions id).outcome = .rotated successor) :
    ∃ family token, (state.transactions id).familyRead = some family ∧
      (state.transactions id).tokenRead = some token ∧
      token.generation = family.current_generation ∧
      some successor.generation = U64.checked_add token.generation 1#u64 ∧
      (state.transactions id).staged =
        [.insertToken successor, .consume token.id successor.id (state.transactions id).request.now,
          .advance successor.generation, .audit [(state.transactions id).request.success]] := by
  have chosen := execution_recorded execution (initial_recorded db requests) id
    (Or.inr (Or.inr (Or.inl phase)))
  rcases chosen with ⟨family, token, familyRead, tokenRead, program, checked⟩
  have rotation := checked successor outcome
  refine ⟨family, token, familyRead, tokenRead, rotation.2.2.1, rotation.2.2.2, ?_⟩
  rw [committing_program_complete db requests state execution id phase, program, outcome]
  rfl

end
end RefreshTransaction

#print axioms RefreshTransaction.reachable_rotation_program
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.choose_program,
      ``RefreshTransaction.choose_rotation_checked, ``RefreshTransaction.revocation_program,
      ``RefreshTransaction.execution_recorded, ``RefreshTransaction.reachable_rotation_program] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh history axiom in {declaration}: {found}"
