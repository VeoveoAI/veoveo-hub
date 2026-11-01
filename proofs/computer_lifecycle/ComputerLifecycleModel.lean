import Mathlib.Data.List.Basic
import Mathlib.Tactic

/-! Manual source-shaped protocol;all external results are observations. -/
namespace ComputerLifecycle

structure Key where
  id : Nat
  tenant : Nat
  principal : Nat
  deriving DecidableEq, Repr

/-- Complete admitted snapshot;version hashing/parsing is an interpretation. -/
structure Provision where
  version : String
  complete : String
  deriving DecidableEq, Repr
structure Failure where
  cause : String
  atTime : Int
  version : String
  deriving DecidableEq, Repr
structure Notice where
  fromVersion : String
  toVersion : String
  whatsNew : String
  deriving DecidableEq, Repr
structure Operation where
  id : Nat
  startedAt : Int
  target : Provision
  deriving DecidableEq, Repr
structure Custody where
  workspaceRequest : Nat
  modelProviderRequest : Nat
  hubProviderRequest : Nat
  handoff : Option Nat
  configureRequest : Option Nat
  rotateRequest : Option Nat
  family : Option Nat
  deriving DecidableEq, Repr
structure Row where
  tenant : Nat
  principal : Nat
  custody : Custody
  ready : Bool
  provisioning : Provision
  operation : Option Operation
  noticeShown : Bool
  failure : Option Failure
  notice : Option Notice
  collectedAt : Option Int
  keepOn : Bool
  storageFrame : String
  deriving DecidableEq, Repr
abbrev Table := Nat → Option Row

def put (rows : Table) (id : Nat) (row : Option Row) : Table :=
  fun other => if other = id then row else rows other

def owned (row : Row) (key : Key) : Bool :=
  row.tenant == key.tenant && row.principal == key.principal

def running (row : Row) (now : Int) : Option Operation :=
  row.operation.filter (fun op => decide (op.startedAt > now - 660))

def claimAllowed (row : Row) (key : Key) (cutoff : Int) : Bool :=
  owned row key && row.ready && match row.operation with
    | none => true
    | some op => decide (op.startedAt < cutoff)

structure Effect where
  rows : Table
  matched : Bool

def claim (rows : Table) (key : Key) (op : Nat) (target : Provision) (now cutoff : Int) : Effect :=
  match rows key.id with
  | none => ⟨rows, false⟩
  | some row => if claimAllowed row key cutoff then
      ⟨put rows key.id (some { row with
        operation := some ⟨op, now, target⟩
        noticeShown := false
        failure := none }), true⟩
    else ⟨rows, false⟩

def completionAllowed (row : Row) (key : Key) (op : Nat) : Bool :=
  owned row key && row.operation.any (fun current => current.id == op)

def finish (rows : Table) (key : Key) (op : Nat) (notice : Option Notice) : Effect :=
  match rows key.id with
  | none => ⟨rows, false⟩
  | some row => match row.operation with
    | none => ⟨rows, false⟩
    | some current => if owned row key && current.id == op then
        ⟨put rows key.id (some { row with
          provisioning := current.target
          notice := if row.noticeShown then none else notice
          operation := none
          noticeShown := false
          collectedAt := none }), true⟩
      else ⟨rows, false⟩

def fail (rows : Table) (key : Key) (op : Nat) (failure : Failure) : Effect :=
  match rows key.id with
  | none => ⟨rows, false⟩
  | some row => if completionAllowed row key op then
      ⟨put rows key.id (some { row with
        failure := some failure
        operation := none
        noticeShown := false }), true⟩
    else ⟨rows, false⟩

def showNotice (rows : Table) (key : Key) (op : Nat) : Effect :=
  match rows key.id with
  | none => ⟨rows, false⟩
  | some row => if completionAllowed row key op then
      ⟨put rows key.id (some { row with noticeShown := true }), true⟩
    else ⟨rows, false⟩

/-- An acknowledged SQL statement committed;error delivery can follow either
commit or abort. No branch is guarded by a desired row property. -/
inductive Delivery where
  | commitAck
  | commitError (message : String)
  | abortError (message : String)
  deriving DecidableEq, Repr
structure Recorded where
  rows : Table
  response : Except String Bool

def deliver (before : Table) (effect : Effect) : Delivery → Recorded
  | .commitAck => ⟨effect.rows, .ok effect.matched⟩
  | .commitError message => ⟨effect.rows, .error message⟩
  | .abortError message => ⟨before, .error message⟩

inductive Stage where
  | forSpec | remove | connect | deleteTemplate | buildTemplate | createTemplate
  | createSandbox | waitReady | waitListener
  deriving DecidableEq, Repr
structure Context where
  key : Key
  operation : Nat
  target : Provision
  previous : Row
  /-- Observed whats_new result;None is its native default-on-error path. -/
  whatsNew : Option String
  deriving DecidableEq, Repr
structure Invocation where
  key : Key
  operation : Nat
  target : Provision
  stage : Stage
  deriving DecidableEq, Repr

def invoke (context : Context) (stage : Stage) : Invocation :=
  ⟨context.key, context.operation, context.target, stage⟩
def program : List Stage := [.forSpec, .remove, .connect, .deleteTemplate, .buildTemplate,
  .createTemplate, .createSandbox, .waitReady, .waitListener]

structure Observation (Native : Type) where
  result : Except String Unit
  afterNative : Native
structure NativeOutcome (Native : Type) where
  result : Except String Unit
  native : Native
  attempts : List Invocation

def runStages {N : Type} (context : Context) (observe : Invocation → Observation N)
    (native : N) : List Stage → NativeOutcome N
  | [] => ⟨.ok (), native, []⟩
  | stage :: remaining =>
      let call := invoke context stage
      let observed := observe call
      match observed.result with
      | .error cause => ⟨.error cause, observed.afterNative, [call]⟩
      | .ok () =>
          let next := runStages context observe observed.afterNative remaining
          ⟨next.result, next.native, call :: next.attempts⟩

def updateNotice (context : Context) : Option Notice :=
  let fromVersion := (context.previous.notice.map (·.fromVersion)).getD context.previous.provisioning.version
  if fromVersion = context.target.version then none else
    some ⟨fromVersion, context.target.version, context.whatsNew.getD ""⟩

structure WorkerOutcome (Native : Type) where
  native : NativeOutcome Native
  recorded : Recorded
  loggedRecordingError : Option String

/-- rows is the table at COMPLETION SQL linearization,not a claim that it stayed
unchanged during native effects. This first adapter covers stage-return results;
the outer600s scheduler interruption is a separate explicit boundary. -/
def runWorker {N : Type} (rows : Table) (context : Context)
    (observe : Invocation → Observation N) (native : N) (endedAt : Int) (delivery : Delivery) : WorkerOutcome N :=
  let result := runStages context observe native program
  let effect := match result.result with
    | .ok () => finish rows context.key context.operation (updateNotice context)
    | .error cause => fail rows context.key context.operation ⟨cause, endedAt, context.target.version⟩
  let recorded := deliver rows effect delivery
  ⟨result, recorded, match recorded.response with | .ok _ => none | .error cause => some cause⟩

/-- Begin answers Started only after acknowledged claim and successful reread.
Read/parsing errors can leave a committed claim with no launched worker. -/
inductive Launch where
  | started (context : Context)
  | running
  | error (cause : String)
  deriving DecidableEq, Repr

def launch (recorded : Recorded) (key : Key) (op : Nat) (target : Provision)
    (whatsNew : Option String) (reread : Except String (Option Row)) : Launch :=
  match recorded.response with
  | .error cause => .error cause
  | .ok false => .running
  | .ok true => match reread with
    | .error cause => .error cause
    | .ok none => .error "the Computer no longer exists"
    | .ok (some previous) => .started ⟨key, op, target, previous, whatsNew⟩

end ComputerLifecycle
