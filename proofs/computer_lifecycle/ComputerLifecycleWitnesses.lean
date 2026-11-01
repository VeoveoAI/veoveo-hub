import ComputerLifecycleSpecification

/-! Executable synthetic inputs and independent full expected values. No generic
correctness/acceptance import,so future semantic diagnostics can reuse actual
executors without inheriting a failed correctness module. -/
namespace ComputerLifecycle.Fixture

def key : Key := ⟨7, 11, 13⟩
def old : Provision := ⟨"v1", "complete-old-template/config/policy/images/secret-references"⟩
def next : Provision := ⟨"v2", "complete-new-template/config/policy/images/secret-references"⟩
def custody : Custody := ⟨101, 102, 103, none, none, none, some 109⟩
def earlier : Notice := ⟨"v0", "v1", "earlier release"⟩
def seed : Row :=
  { tenant := 11, principal := 13, custody := custody, ready := true
    provisioning := old, operation := none, noticeShown := false
    failure := some ⟨"prior failure", -20, "v0"⟩, notice := some earlier
    collectedAt := some (-7), keepOn := true, storageFrame := "created_at=retained;all-unprojected-columns" }
def other : Row :=
  { tenant := 99, principal := 100, custody := ⟨201, 202, 203, none, none, none, some 209⟩
    ready := true, provisioning := old, operation := none, noticeShown := false
    failure := none, notice := none, collectedAt := none, keepOn := false
    storageFrame := "unrelated-complete-stored-frame" }
def base : Table := fun id => if id = 7 then some seed else if id = 8 then some other else none

def claimed := claim base key 70 next 0 (-660)
def context : Context := ⟨key, 70, next, (claimed.rows 7).getD seed, some "new release"⟩
def allGood (_ : Invocation) : Observation Nat := ⟨.ok (), 9⟩
def createError (call : Invocation) : Observation Nat :=
  if call.stage = .createSandbox then ⟨.error "synthetic create refusal", 5⟩ else ⟨.ok (), 4⟩
def succeeded := runWorker claimed.rows context allGood 0 20 .commitAck
def failed := runWorker claimed.rows context createError 0 20 .commitAck

def expectedSuccess : Row :=
  { tenant := 11, principal := 13, custody := custody, ready := true
    provisioning := ⟨"v2", "complete-new-template/config/policy/images/secret-references"⟩
    operation := none, noticeShown := false, failure := none
    notice := some ⟨"v0", "v2", "new release"⟩, collectedAt := none, keepOn := true
    storageFrame := "created_at=retained;all-unprojected-columns" }
def expectedFailed : Row :=
  { tenant := 11, principal := 13, custody := custody, ready := true
    provisioning := ⟨"v1", "complete-old-template/config/policy/images/secret-references"⟩
    operation := none, noticeShown := false
    failure := some ⟨"synthetic create refusal", 20, "v2"⟩
    notice := some ⟨"v0", "v1", "earlier release"⟩, collectedAt := some (-7), keepOn := true
    storageFrame := "created_at=retained;all-unprojected-columns" }
def expectedRows (row : Row) : Table := fun id => if id = 7 then some row else if id = 8 then some other else none

def retryClaim := claim failed.recorded.rows key 71 next 30 (-630)
def retryContext : Context := ⟨key, 71, next, (retryClaim.rows 7).getD seed, some "new release"⟩
def retried := runWorker retryClaim.rows retryContext allGood 5 40 .commitAck

def newerClaim := claim claimed.rows key 71 next 661 1
def newerContext : Context := ⟨key, 71, next, (newerClaim.rows 7).getD seed, some "new release"⟩
def newerDone := runWorker newerClaim.rows newerContext allGood 5 700 .commitAck

/-- Literal B claim output,independent of claim/finish evaluators. -/
def expectedNewerClaim : Row :=
  { tenant := 11, principal := 13, custody := custody, ready := true
    provisioning := ⟨"v1", "complete-old-template/config/policy/images/secret-references"⟩
    operation := some ⟨71, 661, ⟨"v2", "complete-new-template/config/policy/images/secret-references"⟩⟩
    noticeShown := false, failure := none, notice := some ⟨"v0", "v1", "earlier release"⟩
    collectedAt := some (-7), keepOn := true
    storageFrame := "created_at=retained;all-unprojected-columns" }

def expectedFailureAttempts (context : Context) : List Invocation :=
  [⟨context.key, context.operation, context.target, .forSpec⟩,
   ⟨context.key, context.operation, context.target, .remove⟩,
   ⟨context.key, context.operation, context.target, .connect⟩,
   ⟨context.key, context.operation, context.target, .deleteTemplate⟩,
   ⟨context.key, context.operation, context.target, .buildTemplate⟩,
   ⟨context.key, context.operation, context.target, .createTemplate⟩,
   ⟨context.key, context.operation, context.target, .createSandbox⟩]

end ComputerLifecycle.Fixture
