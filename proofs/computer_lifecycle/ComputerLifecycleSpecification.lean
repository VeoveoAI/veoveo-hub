import ComputerLifecycleModel

/-! Independent exact durable row and historical call/output contracts. No
postcondition is used as an executor/commit guard. -/
namespace ComputerLifecycle

def ClaimPermitted (row : Row) (key : Key) (cutoff : Int) : Prop :=
  row.tenant = key.tenant ∧ row.principal = key.principal ∧ row.ready = true ∧
    match row.operation with | none => True | some op => op.startedAt < cutoff

def expectedClaim (row : Row) (op : Nat) (target : Provision) (now : Int) : Row :=
  { tenant := row.tenant
    principal := row.principal
    custody := row.custody
    ready := row.ready
    provisioning := row.provisioning
    operation := some ⟨op, now, target⟩
    noticeShown := false
    failure := none
    notice := row.notice
    collectedAt := row.collectedAt
    keepOn := row.keepOn
    storageFrame := row.storageFrame }

def expectedFinish (row : Row) (storedTarget : Provision) (notice : Option Notice) : Row :=
  { tenant := row.tenant
    principal := row.principal
    custody := row.custody
    ready := row.ready
    provisioning := storedTarget
    operation := none
    noticeShown := false
    failure := row.failure
    notice := if row.noticeShown then none else notice
    collectedAt := none
    keepOn := row.keepOn
    storageFrame := row.storageFrame }

def expectedFailure (row : Row) (failure : Failure) : Row :=
  { tenant := row.tenant
    principal := row.principal
    custody := row.custody
    ready := row.ready
    provisioning := row.provisioning
    operation := none
    noticeShown := false
    failure := some failure
    notice := row.notice
    collectedAt := row.collectedAt
    keepOn := row.keepOn
    storageFrame := row.storageFrame }

/-- Full explicit independent historical logical-attempt sequence;not a promise
of remote receipt,continuing readiness or byte-level protobuf construction. -/
def expectedAttempts (context : Context) : List Invocation :=
  [⟨context.key, context.operation, context.target, .forSpec⟩,
   ⟨context.key, context.operation, context.target, .remove⟩,
   ⟨context.key, context.operation, context.target, .connect⟩,
   ⟨context.key, context.operation, context.target, .deleteTemplate⟩,
   ⟨context.key, context.operation, context.target, .buildTemplate⟩,
   ⟨context.key, context.operation, context.target, .createTemplate⟩,
   ⟨context.key, context.operation, context.target, .createSandbox⟩,
   ⟨context.key, context.operation, context.target, .waitReady⟩,
   ⟨context.key, context.operation, context.target, .waitListener⟩]

def Framed (before after : Table) (key : Key) : Prop :=
  ∀ id, id ≠ key.id → after id = before id

/-- Clause for arbitrary actual completion table. Any ownership/op mismatch
preserves the ENTIRE durable table,not just provisioning. Native effects are
intentionally absent from this durable fencing guarantee. -/
def RejectedCompletion (before : Table) (finishResult failResult : Effect) : Prop :=
  finishResult.rows = before ∧ finishResult.matched = false ∧
    failResult.rows = before ∧ failResult.matched = false

end ComputerLifecycle
