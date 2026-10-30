import RefreshTransactionProof

open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

/-- Erase only explicitly disjoint other-family and endpoint audit projections. -/
def owned (db : Database) : Database :=
  { db with unrelated := fun _ => none, rejectionAudit := fun _ => none }

theorem owned_write (db : Database) (write : Write) :
    owned (applyWrite db write) = applyWrite (owned db) write := by
  cases write <;> rfl

theorem owned_commit (db : Database) (writes : List Write) :
    owned (commit db writes) = commit (owned db) writes := by
  induction writes generalizing db with
  | nil => rfl
  | cons head rest ih =>
    change owned (commit (applyWrite db head) rest) = commit (applyWrite (owned db) head) rest
    rw [ih, owned_write]

/-- A mathematical execution trace, not runtime history machinery. Maintenance
remains visible; failed/aborted attempt cannot hide its effects in an oracle. -/
inductive ReplayOp where
  | transaction (owner : Id) (writes : List Write)
  | clear (key : Key)
  | prune (key : Key)
  | pruneFamily

def applyOp (db : Database) : ReplayOp → Database
  | .transaction _ writes => commit db writes
  | .clear key =>
    { db with
      tokens := put db.tokens key ((db.tokens key).map fun token => { token with delivery := none }) }
  | .prune key => { db with tokens := deleteToken db.tokens key }
  | .pruneFamily => { db with family := none, tokens := fun _ => none }

def replay (db : Database) (ops : List ReplayOp) : Database := ops.foldl applyOp db

def NotBy (subject : Id) : ReplayOp → Prop
  | .transaction owner _ => owner ≠ subject
  | _ => True

theorem owned_op (db : Database) (op : ReplayOp) :
    owned (applyOp db op) = applyOp (owned db) op := by
  cases op with
  | transaction owner writes => exact owned_commit db writes
  | clear key => rfl
  | prune key => rfl
  | pruneFamily => rfl

theorem replay_append (db : Database) (left right : List ReplayOp) :
    replay db (left ++ right) = replay (replay db left) right := by
  simp [replay, List.foldl_append]

/-- An attempt is single-shot in this execution model. Committed never becomes
aborted, even when its caller receives an error/lost acknowledgement. -/
theorem committed_step {before after : State} (step : Step before after) (subject : Id)
    (committed : (before.transactions subject).phase = .committed) :
    (after.transactions subject).phase = .committed := by
  cases step <;>
    simp only [update, put] at * <;>
    try split_ifs at *
  all_goals simp_all

/-- Proof-only spelling of the original serverCommit constructor's target. -/
def committedState (state : State) (id : Id) : State :=
  { update state id { state.transactions id with phase := .committed } with
    db := commit state.db (state.transactions id).staged
    familyLock := if state.familyLock = some id then none else state.familyLock
    tokenLocks := release state.tokenLocks id
    auditLocks := release state.auditLocks id
    rotations := match (state.transactions id).outcome, (state.transactions id).tokenRead with
      | .rotated successor, some predecessor => state.rotations ++ [⟨id, predecessor.id, successor.id, successor.generation⟩]
      | _, _ => state.rotations }

/-- A silent step cannot hide an idempotent/no-row-change server commit. -/
def NoServerCommit (before after : State) : Prop :=
  ∀ id, (before.transactions id).phase = .committing →
    (after.transactions id).phase ≠ .committed

/-- Each nonempty label is one original Step constructor, including its actual
constraints and exact target state. Silent labels wrap an actual control/framed
step and must preserve owned rows without a commit phase transition. -/
inductive Contribution : State → State → List ReplayOp → Prop where
  | transaction (state : State) (id : Id) (phase : (state.transactions id).phase = .committing) :
      Contribution state (committedState state id) [.transaction id (state.transactions id).staged]
  | clear (state : State) (key : Key) (token : Token) (envelope : Envelope) (now : Timestamp)
      (present : state.db.tokens key = some token) (has : token.delivery = some envelope)
      (expired : ¬ Before now envelope.expires_at) (free : state.tokenLocks key = none) :
      Contribution state
        { state with db := { state.db with tokens := put state.db.tokens key (some { token with delivery := none }) } }
        [.clear key]
  | prune (state : State) (key : Key) (token : Token) (now : Timestamp)
      (present : state.db.tokens key = some token) (expired : ¬ Before now token.expires_at)
      (free : state.tokenLocks key = none)
      (referencesFree : ∀ other row, state.db.tokens other = some row → row.replacement = some key → state.tokenLocks other = none) :
      Contribution state { state with db := { state.db with tokens := deleteToken state.db.tokens key } } [.prune key]
  | pruneFamily (state : State) (family : Family) (now : Timestamp)
      (present : state.db.family = some family) (expired : ¬ Before now family.expires_at)
      (free : state.familyLock = none) (tokensFree : ∀ key, state.tokenLocks key = none) :
      Contribution state { state with db := { state.db with family := none, tokens := fun _ => none } } [.pruneFamily]
  | silent {before after : State} (actual : Step before after)
      (notCommit : NoServerCommit before after) (same : owned after.db = owned before.db) :
      Contribution before after []

inductive TracedExecution : State → State → List ReplayOp → Prop where
  | refl (state : State) : TracedExecution state state []
  | tail {first middle last : State} {earlier later : List ReplayOp} :
      TracedExecution first middle earlier → Contribution middle last later →
      TracedExecution first last (earlier ++ later)

theorem contribution_step {before after : State} {ops : List ReplayOp}
    (contribution : Contribution before after ops) : Step before after := by
  cases contribution with
  | transaction id phase => exact Step.serverCommit _ id phase
  | clear key token envelope now present has expired free =>
    exact Step.clearEnvelope _ key token envelope now present has expired free
  | prune key token now present expired free referencesFree =>
    exact Step.pruneToken _ key token now present expired free referencesFree
  | pruneFamily family now present expired free tokensFree =>
    exact Step.pruneFamily _ family now present expired free tokensFree
  | silent actual notCommit same => exact actual

theorem contribution_rows {before after : State} {ops : List ReplayOp}
    (contribution : Contribution before after ops) :
    owned after.db = replay (owned before.db) ops := by
  cases contribution with
  | transaction id phase => exact owned_commit _ _
  | clear key token envelope now present has expired free =>
    simp [replay, applyOp, owned, put, present]
  | prune key token now present expired free referencesFree => rfl
  | pruneFamily family now present expired free tokensFree => rfl
  | silent actual notCommit same => exact same

theorem step_has_contribution {before after : State} (step : Step before after) :
    ∃ ops, Contribution before after ops := by
  have actual := step
  cases step
  case serverCommit id phase => exact ⟨_, Contribution.transaction _ id phase⟩
  case clearEnvelope key token envelope now present has expired free =>
    exact ⟨_, Contribution.clear _ key token envelope now present has expired free⟩
  case pruneToken key token now present expired free referencesFree =>
    exact ⟨_, Contribution.prune _ key token now present expired free referencesFree⟩
  case pruneFamily family now present expired free tokensFree =>
    exact ⟨_, Contribution.pruneFamily _ family now present expired free tokensFree⟩
  all_goals
    refine ⟨[], Contribution.silent actual ?_ rfl⟩
    intro subject was now
    simp only [update, put] at * <;> try split_ifs at *
    all_goals simp_all

theorem execution_has_trace {first last : State} (execution : Execution first last) :
    ∃ ops, TracedExecution first last ops := by
  induction execution with
  | refl => exact ⟨[], TracedExecution.refl _⟩
  | tail previous step ih =>
    rcases ih with ⟨earlier, trace⟩
    rcases step_has_contribution step with ⟨later, contribution⟩
    exact ⟨earlier ++ later, TracedExecution.tail trace contribution⟩

theorem traced_execution {first last : State} {ops : List ReplayOp}
    (trace : TracedExecution first last ops) : Execution first last := by
  induction trace with
  | refl => exact Execution.refl _
  | tail earlier contribution ih => exact Execution.tail ih (contribution_step contribution)

theorem traced_replay {first last : State} {ops : List ReplayOp}
    (trace : TracedExecution first last ops) : owned last.db = replay (owned first.db) ops := by
  induction trace with
  | refl => rfl
  | tail earlier contribution ih =>
    rw [replay_append, ← ih]
    exact contribution_rows contribution

theorem contribution_excludes {before after : State} {ops : List ReplayOp}
    (contribution : Contribution before after ops) (subject : Id)
    (notCommitted : (after.transactions subject).phase ≠ .committed) :
    ∀ op ∈ ops, NotBy subject op := by
  cases contribution with
  | transaction id phase =>
    have distinct : id ≠ subject := by
      intro same
      subst subject
      apply notCommitted
      simp [committedState, update, put]
    simpa [NotBy] using distinct
  | clear key token envelope now present has expired free => simp [NotBy]
  | prune key token now present expired free referencesFree => simp [NotBy]
  | pruneFamily family now present expired free tokensFree => simp [NotBy]
  | silent actual notCommit same => simp

/-- Exclusion holds for every trace tied to the actual execution, not merely an
invented list of deltas that happens to produce an equal projected database. -/
theorem traced_exclusion {first last : State} {ops : List ReplayOp}
    (trace : TracedExecution first last ops) :
    ∀ subject, (last.transactions subject).phase ≠ .committed →
      ∀ op ∈ ops, NotBy subject op := by
  induction trace with
  | refl => simp
  | tail earlier contribution ih =>
    intro subject notCommitted op member
    have priorNot : _ := fun committed =>
      notCommitted (committed_step (contribution_step contribution) subject committed)
    rcases List.mem_append.mp member with old | recent
    · exact ih subject priorNot op old
    · exact contribution_excludes contribution subject notCommitted op recent

/-- Every concrete execution has a matching contribution trace. A final aborted
attempt contributes no server-commit delta to that trace, while other commits,
maintenance, pre-existing identical audits and framed endpoint audit remain. -/
theorem aborted_attempt_excluded {first last : State} (execution : Execution first last)
    (subject : Id) (aborted : (last.transactions subject).phase = .aborted) :
    ∃ ops, TracedExecution first last ops ∧ owned last.db = replay (owned first.db) ops ∧
      ∀ op ∈ ops, NotBy subject op := by
  rcases execution_has_trace execution with ⟨ops, trace⟩
  exact ⟨ops, trace, traced_replay trace, traced_exclusion trace subject (by simp [aborted])⟩

end
end RefreshTransaction

#print axioms RefreshTransaction.aborted_attempt_excluded
run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``RefreshTransaction.committed_step,
      ``RefreshTransaction.execution_has_trace, ``RefreshTransaction.traced_replay,
      ``RefreshTransaction.traced_exclusion, ``RefreshTransaction.aborted_attempt_excluded] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed refresh replay axiom in {declaration}: {found}"
