import TaskContinuityProof

open Aeneas Aeneas.Std
open hub_task_proof
open TaskContinuityProof

namespace TaskDispatchProof

/-- The mathematical strict order models production's exact DateTime comparison,
not a truncated timestamp or an already-decided expiry admission. -/
def LawfulOrder {E : Type} [LinearOrder E] (order : core.cmp.PartialOrd E E) : Prop :=
  ∀ left right, order.gt left right = .ok (decide (right < left))

theorem ownership_correct {I L : Type} [DecidableEq I] [DecidableEq L]
    (eqI : core.cmp.PartialEq I I) (lawI : LawfulEq eqI)
    (eqL : core.cmp.PartialEq L L) (lawL : LawfulEq eqL)
    (owner current : decision.Ownership I (Slice L)) :
    decision.owns eqI eqL owner current ⦃ result =>
      result = decide (owner.identity = current.identity ∧
        Contained owner.labels.val current.labels.val) ⦄ := by
  by_cases same : owner.identity = current.identity
  · simpa [decision.owns, lawI.1, same] using
      containment_correct eqL lawL owner.labels current.labels
  · simp [decision.owns, lawI.1, same]

def OwnerAdmitted {B I L S D K T C U : Type}
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U) : Prop :=
  binding = caller.binding ∧ owner.identity = caller.ownership.identity ∧
  Contained owner.labels.val caller.ownership.labels.val ∧
  (caller.delegated = true → caller.capture = some task.capture)

noncomputable def expectedOwner {B I L S D K T C U : Type}
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U) :
    core.result.Result (decision.TaskRequest B (decision.Task S D K T C) U) decision.Refusal := by
  classical
  exact if binding ≠ caller.binding then .Err .Binding
    else if owner.identity ≠ caller.ownership.identity ∨
        ¬ Contained owner.labels.val caller.ownership.labels.val then .Err .Ownership
    else if caller.delegated = true ∧ caller.capture ≠ some task.capture then
      .Err .DelegatedCapture
    else .Ok { binding, task, caller := caller.payload }

/-- Complete typed outcomes, preserving refusal order and every successful field. -/
theorem owner_correct {B I L S D K T C U : Type}
    [DecidableEq B] [DecidableEq I] [DecidableEq L] [DecidableEq C]
    (eqB : core.cmp.PartialEq B B) (lawB : LawfulEq eqB)
    (eqI : core.cmp.PartialEq I I) (lawI : LawfulEq eqI)
    (eqL : core.cmp.PartialEq L L) (lawL : LawfulEq eqL)
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U) :
    decision.admit_owner eqB eqI eqL eqC binding owner task caller ⦃ result =>
      result = expectedOwner binding owner task caller ⦄ := by
  classical
  unfold decision.admit_owner
  rw [lawB.2]
  by_cases sameBinding : binding = caller.binding
  · simp [sameBinding]
    apply WP.spec_bind (ownership_correct eqI lawI eqL lawL owner caller.ownership)
    intro owned ownedEq
    subst owned
    by_cases owns : owner.identity = caller.ownership.identity ∧
        Contained owner.labels.val caller.ownership.labels.val
    · cases delegated : caller.delegated with
      | false => simp [expectedOwner, owns.1, owns.2, delegated]
      | true =>
        cases captured : caller.capture with
        | none => simp [expectedOwner, owns.1, owns.2, delegated, captured]
        | some current =>
          by_cases sameCapture : current = task.capture <;>
            simp [expectedOwner, owns.1, owns.2, delegated, captured, lawC.1, sameCapture]
    · simp [expectedOwner, owns]
  · simp [expectedOwner, sameBinding]

/-- Success iff all admission conditions hold, with the one exact output, rather
than merely proving that any successful result would be safe. -/
theorem owner_success_iff {B I L S D K T C U : Type}
    [DecidableEq B] [DecidableEq I] [DecidableEq L] [DecidableEq C]
    (eqB : core.cmp.PartialEq B B) (lawB : LawfulEq eqB)
    (eqI : core.cmp.PartialEq I I) (lawI : LawfulEq eqI)
    (eqL : core.cmp.PartialEq L L) (lawL : LawfulEq eqL)
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U) :
    decision.admit_owner eqB eqI eqL eqC binding owner task caller ⦃ result =>
      ∀ output, result = .Ok output ↔ OwnerAdmitted binding owner task caller ∧
        output = { binding, task, caller := caller.payload } ⦄ := by
  classical
  apply WP.spec_mono (owner_correct eqB lawB eqI lawI eqL lawL eqC lawC
    binding owner task caller)
  intro result resultEq
  subst result
  unfold expectedOwner OwnerAdmitted
  split_ifs <;> simp_all <;> aesop

theorem capture_matches_exact {C D : Type} [DecidableEq C] [DecidableEq D]
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (retained actual : C) (deployment : D) (selected : Option D) :
    decision.capture_matches eqC eqD retained actual deployment selected =
      .ok (decide (retained = actual ∧ selected = some deployment)) := by
  by_cases same : retained = actual <;> cases selected <;>
    simp [decision.capture_matches, core.cmp.impls.PartialEqShared.eq,
      lawC.1, lawD.1, same]

theorem capture_exact {B S D K T C U : Type} [DecidableEq C] [DecidableEq D]
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (request : decision.TaskRequest B (decision.Task S D K T C) U)
    (actual : C) (selected : Option D) :
    decision.capture eqD eqC request actual selected = .ok
      (if request.task.capture = actual ∧ selected = some request.task.deployment
        then .Ok request else .Err .Capture) := by
  unfold decision.capture
  rw [capture_matches_exact eqC lawC eqD lawD]
  by_cases agrees : request.task.capture = actual ∧ selected = some request.task.deployment <;>
    simp [agrees]

theorem capture_success_iff {B S D K T C U : Type} [DecidableEq C] [DecidableEq D]
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (request : decision.TaskRequest B (decision.Task S D K T C) U)
    (actual : C) (selected : Option D) :
    decision.capture eqD eqC request actual selected ⦃ result =>
      ∀ output, result = .Ok output ↔ request.task.capture = actual ∧
        selected = some request.task.deployment ∧ output = request ⦄ := by
  rw [capture_exact eqC lawC eqD lawD]
  split_ifs <;> simp_all [eq_comm]

theorem retry_exact {K C E R : Type} [DecidableEq K] [LinearOrder E]
    (eqK : core.cmp.PartialEq K K) (lawK : LawfulEq eqK)
    (order : core.cmp.PartialOrd E E) (lawOrder : LawfulOrder order)
    (retained : decision.Retained K C E R) (proposed : K) (now : E) :
    decision.reuse eqK order retained proposed now = .ok
      (if retained.key = proposed ∧ now < retained.expires then .Ok retained
        else .Err .Retry) := by
  unfold LawfulOrder at lawOrder
  by_cases same : retained.key = proposed <;> by_cases live : now < retained.expires <;>
    simp [decision.reuse, lawK.1, lawOrder, same, live]

theorem retry_success_iff {K C E R : Type} [DecidableEq K] [LinearOrder E]
    (eqK : core.cmp.PartialEq K K) (lawK : LawfulEq eqK)
    (order : core.cmp.PartialOrd E E) (lawOrder : LawfulOrder order)
    (retained : decision.Retained K C E R) (proposed : K) (now : E) :
    decision.reuse eqK order retained proposed now ⦃ result =>
      ∀ output, result = .Ok output ↔ retained.key = proposed ∧
        now < retained.expires ∧ output = retained ⦄ := by
  rw [retry_exact eqK lawK order lawOrder]
  split_ifs <;> simp_all [eq_comm]

theorem retry_at_expiry_is_refused {K C E R : Type} [DecidableEq K] [LinearOrder E]
    (eqK : core.cmp.PartialEq K K) (lawK : LawfulEq eqK)
    (order : core.cmp.PartialOrd E E) (lawOrder : LawfulOrder order)
    (retained : decision.Retained K C E R) :
    decision.reuse eqK order retained retained.key retained.expires = .ok (.Err .Retry) := by
  simp [retry_exact eqK lawK order lawOrder]

theorem recipient_exact {S D R : Type} [DecidableEq S] [DecidableEq D] [DecidableEq R]
    (eqS : core.cmp.PartialEq S S) (lawS : LawfulEq eqS)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (eqR : core.cmp.PartialEq R R) (lawR : LawfulEq eqR)
    (expected presented : recipient.Recipient S D R) :
    recipient.bind eqS eqD eqR expected presented = .ok
      (if expected.server ≠ presented.server ∨ expected.resource ≠ presented.resource
        then .Err .Audience
        else if expected.deployment ≠ presented.deployment then .Err .Deployment
        else .Ok presented) := by
  by_cases server : expected.server = presented.server <;>
    by_cases resource : expected.resource = presented.resource <;>
    by_cases deployment : expected.deployment = presented.deployment <;>
    simp [recipient.bind, lawS.2, lawD.2, lawR.2, server, resource, deployment]

theorem recipient_success_iff {S D R : Type} [DecidableEq S] [DecidableEq D] [DecidableEq R]
    (eqS : core.cmp.PartialEq S S) (lawS : LawfulEq eqS)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (eqR : core.cmp.PartialEq R R) (lawR : LawfulEq eqR)
    (expected presented : recipient.Recipient S D R) :
    recipient.bind eqS eqD eqR expected presented ⦃ result =>
      ∀ output, result = .Ok output ↔ expected.server = presented.server ∧
        expected.resource = presented.resource ∧ expected.deployment = presented.deployment ∧
        output = presented ⦄ := by
  by_cases server : expected.server = presented.server <;>
    by_cases resource : expected.resource = presented.resource <;>
    by_cases deployment : expected.deployment = presented.deployment <;>
    simp [recipient_exact eqS lawS eqD lawD eqR lawR, server, resource, deployment, eq_comm]

abbrev DispatchOutcome (B S D K T C U R : Type) :=
  Sum decision.Refusal (Sum recipient.Mismatch
    (decision.TaskRequest B (decision.Task S D K T C) U × recipient.Recipient S D R))

def presentedRecipient {B S D K T C U R : Type}
    (request : decision.TaskRequest B (decision.Task S D K T C) U) (resource : R) :
    recipient.Recipient S D R :=
  { server := request.task.server, deployment := request.task.deployment, resource }

/-- A proof trace composed ONLY from the three generated production functions.
It is not a replacement for the asynchronous Hub path. In production, authoritative
lookup, exposure and current policy intervene before the route is returned; the
selected registration supplies `resource`, and crypto/transport are outside this trace. -/
def dispatch {B I L S D K T C U R : Type}
    (eqB : core.cmp.PartialEq B B) (eqI : core.cmp.PartialEq I I)
    (eqL : core.cmp.PartialEq L L) (eqC : core.cmp.PartialEq C C)
    (eqD : core.cmp.PartialEq D D) (eqS : core.cmp.PartialEq S S)
    (eqR : core.cmp.PartialEq R R)
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U)
    (actual : C) (selected : Option D)
    (expected : recipient.Recipient S D R) (resource : R) :
    Result (DispatchOutcome B S D K T C U R) := do
  let owned ← decision.admit_owner eqB eqI eqL eqC binding owner task caller
  match owned with
  | .Err refusal => .ok (.inl refusal)
  | .Ok request =>
    let captured ← decision.capture eqD eqC request actual selected
    match captured with
    | .Err refusal => .ok (.inl refusal)
    | .Ok request =>
      let recipient ← recipient.bind eqS eqD eqR expected (presentedRecipient request resource)
      match recipient with
      | .Err mismatch => .ok (.inr (.inl mismatch))
      | .Ok recipient => .ok (.inr (.inr (request, recipient)))

noncomputable def expectedDispatch {B I L S D K T C U R : Type}
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U)
    (actual : C) (selected : Option D)
    (expected : recipient.Recipient S D R) (resource : R) :
    DispatchOutcome B S D K T C U R := by
  classical
  exact match expectedOwner binding owner task caller with
  | .Err refusal => .inl refusal
  | .Ok request =>
    if request.task.capture ≠ actual ∨ selected ≠ some request.task.deployment then .inl .Capture
    else if expected.server ≠ request.task.server ∨ expected.resource ≠ resource then
      .inr (.inl .Audience)
    else if expected.deployment ≠ request.task.deployment then .inr (.inl .Deployment)
    else .inr (.inr (request, presentedRecipient request resource))

/-- Every composed outcome is specified, including all ordered refusal classes;
the expected recipient is arbitrary, not assumed equal to the emitted recipient. -/
theorem dispatch_correct {B I L S D K T C U R : Type}
    [DecidableEq B] [DecidableEq I] [DecidableEq L] [DecidableEq C]
    [DecidableEq D] [DecidableEq S] [DecidableEq R]
    (eqB : core.cmp.PartialEq B B) (lawB : LawfulEq eqB)
    (eqI : core.cmp.PartialEq I I) (lawI : LawfulEq eqI)
    (eqL : core.cmp.PartialEq L L) (lawL : LawfulEq eqL)
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (eqS : core.cmp.PartialEq S S) (lawS : LawfulEq eqS)
    (eqR : core.cmp.PartialEq R R) (lawR : LawfulEq eqR)
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U)
    (actual : C) (selected : Option D)
    (expected : recipient.Recipient S D R) (resource : R) :
    dispatch eqB eqI eqL eqC eqD eqS eqR binding owner task caller
      actual selected expected resource ⦃ result =>
      result = expectedDispatch binding owner task caller actual selected expected resource ⦄ := by
  classical
  unfold dispatch
  apply WP.spec_bind (owner_correct eqB lawB eqI lawI eqL lawL eqC lawC
    binding owner task caller)
  intro owned ownedEq
  cases owned with
  | Err refusal => simp [expectedDispatch, ← ownedEq]
  | Ok request =>
    dsimp only
    rw [capture_exact eqC lawC eqD lawD]
    by_cases captureAgrees : request.task.capture = actual ∧ selected = some request.task.deployment
    · simp [captureAgrees]
      rw [recipient_exact eqS lawS eqD lawD eqR lawR]
      by_cases server : expected.server = request.task.server <;>
        by_cases resourceMatches : expected.resource = resource <;>
        by_cases deployment : expected.deployment = request.task.deployment <;>
        simp [expectedDispatch, ← ownedEq, captureAgrees, server, resourceMatches, deployment,
          presentedRecipient]
    · simp [expectedDispatch, ← ownedEq, captureAgrees]

/-- Completeness and safety of this composed extracted-function trace: a unique
successful output exists exactly for the full owner/capture/recipient contract.
All canonical/source/server/deployment/capture/current-caller fields are retained. -/
theorem dispatch_success_iff {B I L S D K T C U R : Type}
    [DecidableEq B] [DecidableEq I] [DecidableEq L] [DecidableEq C]
    [DecidableEq D] [DecidableEq S] [DecidableEq R]
    (eqB : core.cmp.PartialEq B B) (lawB : LawfulEq eqB)
    (eqI : core.cmp.PartialEq I I) (lawI : LawfulEq eqI)
    (eqL : core.cmp.PartialEq L L) (lawL : LawfulEq eqL)
    (eqC : core.cmp.PartialEq C C) (lawC : LawfulEq eqC)
    (eqD : core.cmp.PartialEq D D) (lawD : LawfulEq eqD)
    (eqS : core.cmp.PartialEq S S) (lawS : LawfulEq eqS)
    (eqR : core.cmp.PartialEq R R) (lawR : LawfulEq eqR)
    (binding : B) (owner : decision.Ownership I (Slice L))
    (task : decision.Task S D K T C) (caller : decision.Caller B I (Slice L) C U)
    (actual : C) (selected : Option D)
    (expected : recipient.Recipient S D R) (resource : R) :
    dispatch eqB eqI eqL eqC eqD eqS eqR binding owner task caller
      actual selected expected resource ⦃ result =>
      ∀ request recipient, result = .inr (.inr (request, recipient)) ↔
        OwnerAdmitted binding owner task caller ∧ task.capture = actual ∧
        selected = some task.deployment ∧ expected.server = task.server ∧
        expected.resource = resource ∧ expected.deployment = task.deployment ∧
        request = { binding, task, caller := caller.payload } ∧
        recipient = presentedRecipient request resource ⦄ := by
  classical
  apply WP.spec_mono (dispatch_correct eqB lawB eqI lawI eqL lawL eqC lawC
    eqD lawD eqS lawS eqR lawR binding owner task caller actual selected expected resource)
  intro result resultEq
  subst result
  unfold expectedDispatch expectedOwner OwnerAdmitted presentedRecipient
  split_ifs <;> simp_all <;> aesop

#print axioms owner_correct
#print axioms owner_success_iff
#print axioms capture_exact
#print axioms capture_success_iff
#print axioms retry_exact
#print axioms retry_success_iff
#print axioms retry_at_expiry_is_refused
#print axioms recipient_exact
#print axioms recipient_success_iff
#print axioms dispatch_correct
#print axioms dispatch_success_iff

end TaskDispatchProof
