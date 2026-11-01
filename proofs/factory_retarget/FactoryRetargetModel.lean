import Mathlib.Tactic

/-! Manual source-correspondence model of Factory::apply_to, not extracted Rust.
Normal-return, serialized existing-runner operations only. Native/SQL contracts
are interpretation boundaries, not axioms added to Lean. -/
namespace FactoryRetarget

abbrev Deployment := Fin (2 ^ 128)
abbrev Timestamp := Int

inductive Stage | database | build | capacity | create | restart | delete
  deriving DecidableEq, Repr

structure Error where
  code : String
  message : String
  deriving DecidableEq, Repr

/-- All 21 columns of the current hub.instance schema. Text retains full values;
UUIDs retain all 128 bits; timestamps are PostgreSQL microsecond observations. -/
structure Row where
  deployment : Deployment
  tokenDigest : String
  createdAt : Timestamp
  database : String
  branch : String
  repository : String
  servedAt : Option Timestamp
  pusher : Option String
  revision : Option String
  failedStage : Option Stage
  failedMessage : Option String
  warning : Option String
  clientDigest : Option String
  ungrantedProviders : List String
  link : Option String
  linkedRepository : Option String
  trackedBranch : Option String
  trackedName : Option String
  linkedPusher : Option String
  linkedRevision : Option String
  ownTree : Option String
  deriving DecidableEq, Repr

structure Target where
  repository : String
  trackedBranch : String
  trackedName : String
  pusher : String
  revision : String
  deriving DecidableEq, Repr

structure Request where
  target : Target
  revision : String
  pusher : String
  ownTree : String
  ownEntries : List String
  deriving DecidableEq, Repr

structure Client where
  deployment : Deployment
  pusher : Option String
  repository : String
  linkedRepository : Option String
  linkedPusher : Option String
  deriving DecidableEq, Repr

def client (r : Row) : Client :=
  ⟨r.deployment, r.pusher, r.repository, r.linkedRepository, r.linkedPusher⟩

/-- tracking.sql filters repository; linked_pushed adds the tracked-ID filter. -/
def selectedByPush (r : Row) (repository branch : String) : Prop :=
  r.linkedRepository = some repository ∧ r.trackedBranch = some branch

def changed (r : Row) (q : Request) : Bool :=
  decide (r.linkedRepository ≠ some q.target.repository ∨
    r.trackedBranch ≠ some q.target.trackedBranch)

/-- Actual immutable identity within one existing-runner apply/retry history. -/
def identity (r : Row) :=
  (r.deployment, r.database, r.repository, r.branch, r.link, r.createdAt)

def Runner (r : Row) : Prop :=
  r.link.isSome ∧ r.linkedRepository.isSome ∧ r.trackedBranch.isSome ∧ r.trackedName.isSome

/-- Structured native URL observation. set_path replaces only path; rendering and
percent-encoding correspondence remain standard URL boundaries. -/
structure Url where
  scheme : String
  authority : String
  path : String
  query : Option String
  fragment : Option String
  deriving DecidableEq, Repr

def Url.setPath (u : Url) (path : String) : Url := { u with path := path }
def Url.render (u : Url) : String :=
  u.scheme ++ "://" ++ u.authority ++ u.path ++
    (u.query.map ("?" ++ ·)).getD "" ++ (u.fragment.map ("#" ++ ·)).getD ""

/-- Complete external protobuf policy/shape values are retained as a parameter.
Provider outputs are observations of the actual active-catalog helper, not a proof
of that helper; requested/grant validation is an explicit correspondence boundary. -/
structure Prepared (P : Type) where
  image : String
  policy : P
  resources : Option P
  driver : Option P
  providers : List String
  ungranted : List String
  loreBase : Url
  workspace : String
  hubIssuer : String
  port : Fin (2 ^ 16)
  deploymentText : Deployment → String
  runnerScript : String
  registrationTokenRaw : String
  registrationTokenDigest : String
  clientSecretRaw : String
  clientSecretDigest : String
  /-- Exact remaining protobuf defaults, interpreted at the native boundary. -/
  templateDefaults : P
  specDefaults : P
  requestDefaults : P

/-- The actual source takes the last 17 ASCII bytes of canonical UUID text.
The native UUID text/ASCII correspondence is stated separately from this model. -/
def sandboxName (r : Row) (p : Prepared P) : String :=
  let text := (p.deploymentText r.deployment).toList
  String.ofList (text.drop (text.length - 17))

structure Template (P : Type) where
  image : String
  resources : Option P
  driver : Option P
  remainder : P
  deriving DecidableEq

structure Configuration (P : Type) where
  environment : List (String × String)
  template : Option (Template P)
  policy : Option P
  providers : List String
  command : List String
  restartAlways : Bool
  remainder : P
  deriving DecidableEq

structure CreateRequest (P : Type) where
  name : String
  workspace : String
  spec : Configuration P
  remainder : P
  deriving DecidableEq

inductive Phase | provisioning | starting | ready | stopping | stopped | completed | error | deleting | unknown
  deriving DecidableEq, Repr

structure Sandbox (P : Type) where
  spec : Option (Configuration P)
  phase : Phase
  restartCount : Nat
  remainder : P
  deriving DecidableEq

structure RefreshMaterial where
  workspace : String
  provider : String
  credentialKey : String
  clientId : String
  clientSecretRaw : String
  strategy : String
  secretMaterialKeys : List String
  deriving DecidableEq, Repr

def refreshMaterial (r : Row) (p : Prepared P) : RefreshMaterial :=
  ⟨p.workspace, sandboxName r p ++ "-lore", "LORE_ACCESS_TOKEN",
    p.deploymentText r.deployment, p.clientSecretRaw,
    "oauth2-client-credentials", ["client_secret"]⟩

structure Providers where
  registration : Option String
  lorePresent : Bool
  configured : Bool
  refreshMaterial : Option RefreshMaterial
  credential : Option Client
  deriving DecidableEq, Repr

structure State (P : Type) where
  rows : Deployment → Option Row
  sandboxes : Deployment → Option (Sandbox P)
  providers : Deployment → Providers
  /-- Successful RPC submissions and explicitly observed error-path submissions;
      never proof of remote receipt, execution or readiness. -/
  createRequests : Deployment → List (CreateRequest P)

/-- Manual Source::command correspondence; script is the source's fixed command
text and syncs branch@LATEST, never a measured checkout of q.revision. -/
def command (r : Row) (q : Request) (p : Prepared P) : List String :=
  ["/bin/sh", "-c", p.runnerScript, "runner",
    (p.loreBase.setPath ("/" ++ r.repository)).render, r.branch,
    (p.loreBase.setPath ("/" ++ q.target.repository)).render,
    q.target.trackedBranch] ++ q.ownEntries

def configuration (r : Row) (q : Request) (p : Prepared P) : Configuration P :=
  ⟨[("VEOVEO_DEPLOYMENT_ID", p.deploymentText r.deployment),
    ("LORE_IDENTITY", p.deploymentText r.deployment),
    ("VEOVEO_PUBLIC_BASE_URL", "http://" ++ sandboxName r p ++ ".internal:" ++ toString p.port.val),
    ("VEOVEO_PORT", toString p.port.val), ("VEOVEO_HUB_ISSUER", p.hubIssuer)],
   some ⟨p.image, p.resources, p.driver, p.templateDefaults⟩, some p.policy,
   [sandboxName r p ++ "-registration", sandboxName r p ++ "-lore"] ++ p.providers,
   command r q p, true, p.specDefaults⟩

def createRequest (r : Row) (q : Request) (p : Prepared P) : CreateRequest P :=
  ⟨sandboxName r p, p.workspace, configuration r q p, p.requestDefaults⟩

def besideImage [DecidableEq P] (old expected : Configuration P) : Bool :=
  match old.template, expected.template with
  | some a, some b => decide (a.resources = b.resources ∧ a.driver = b.driver ∧
      old.policy = expected.policy ∧ old.providers.Perm expected.providers ∧
      old.command = expected.command)
  | _, _ => false

def runs [DecidableEq P] (old expected : Configuration P) : Bool :=
  besideImage old expected &&
    match old.template, expected.template with
    | some a, some b => decide (a.image = b.image)
    | _, _ => false

inductive Choice | keep | restart | replace | create
  deriving DecidableEq, Repr

def choice [DecidableEq P] (r : Row) (q : Request) (p : Prepared P)
    (current : Option (Sandbox P)) : Choice :=
  match current with
  | none => .create
  | some old =>
      match old.spec with
      | none => .replace
      | some oldSpec =>
          if !changed r q && decide (r.ownTree = some q.ownTree ∧
              r.linkedRevision = some q.target.revision) &&
              besideImage oldSpec (configuration r q p) then .keep
          else if !changed r q && runs oldSpec (configuration r q p)
            then .restart else .replace

inductive Sql
  | ungranted (values : List String) | push (pusher : String) | unserved
  | target (value : Target) | token (digest : String) | client (digest : String)
  | linkedApplied (revision : String) | ownTree (digest : String) | applied (revision : String)
  | failed (stage : Stage) (message : String) | warning (message : Option String)
  deriving DecidableEq, Repr

/-- UPDATE semantics include the actual link IS NOT NULL filter. A missing row
is handled by Option.map: zero affected rows, not a fabricated updated row. -/
def updateRow (sql : Sql) (r : Row) : Row :=
  match sql with
  | .ungranted xs => { r with ungrantedProviders := xs }
  | .push p => { r with pusher := some p }
  | .unserved => { r with servedAt := none }
  | .target t => if r.link.isSome then
      { r with
        linkedRepository := some t.repository,
        trackedBranch := some t.trackedBranch,
        trackedName := some t.trackedName,
        linkedPusher := some t.pusher } else r
  | .token d => { r with tokenDigest := d }
  | .client d => { r with clientDigest := some d }
  | .linkedApplied v => if r.link.isSome then { r with linkedRevision := some v } else r
  | .ownTree v => if r.link.isSome then { r with ownTree := some v } else r
  | .applied v => { r with revision := some v, failedStage := none, failedMessage := none }
  | .failed s m => { r with failedStage := some s, failedMessage := some m }
  | .warning m => { r with warning := m }

def update (f : α → α) (key : Deployment) (values : Deployment → α) : Deployment → α :=
  fun query => if query = key then f (values query) else values query

def sqlEffect (d : Deployment) (sql : Sql) (s : State P) : State P :=
  { s with rows := update (Option.map (updateRow sql)) d s.rows }

inductive Rpc
  | ensureDatabase | build | policies | template | requestedProviders | readSandbox
  | runnerApplied | countPushed | deleteSandbox | deleteRegistrationProvider | createRegistrationProvider
  | deleteLoreProvider | createLoreProvider | configureRefresh | refresh | createSandbox
  | stopSandbox | startSandbox
  deriving DecidableEq, Repr

inductive Action | sql (statement : Sql) | rpc (operation : Rpc) | abort (error : Error)
  deriving DecidableEq, Repr

structure Step where
  action : Action
  recorded : Option Stage
  deriving DecidableEq, Repr

structure Admission where
  configured : Bool
  count : Nat
  limit : Nat
  enforce : Bool
  /-- Native formatting of the count/cap warning, not a new rejection policy. -/
  message : String
  deriving DecidableEq, Repr

structure Observations (P : Type) where
  prepared : Prepared P
  installationDatabase : Bool
  admission : Admission
  /-- The first encountered failing operation suffices for every terminating
      failure prefix; later operations are not observed. None permits success. -/
  fault : Option Action
  error : Error
  /-- SQL Err can follow an atomic committed write with a lost reply. -/
  sqlCommittedOnError : Bool
  failureLogCommitted : Bool
  failureLogAcknowledged : Bool
  /-- Actual successful readSandbox observation, after preparation. It need not
      equal any phase/spec observed before the image build. -/
  readNative : Option (Sandbox P)
  failedNative : Option (Sandbox P)
  failedProviders : Providers
  /-- Source raw_grpc_fresh may fail before submission; an Err alone does not
      establish this observation. Unknown delivery is not claimed as submitted. -/
  createSubmittedOnError : Bool
  /-- RPC acknowledgement does not determine a subsequent observed phase/spec. -/
  createdNative : Option (Sandbox P)
  stoppedNative : Option (Sandbox P)
  startedNative : Option (Sandbox P)

abbrev direct (a : Action) : Step := ⟨a, none⟩
abbrev recorded (stage : Stage) (a : Action) : Step := ⟨a, some stage⟩

def creation (p : Prepared P) : List Step :=
  [recorded .create (.sql (.token p.registrationTokenDigest)),
   recorded .create (.rpc .deleteRegistrationProvider),
   recorded .create (.rpc .createRegistrationProvider),
   recorded .create (.sql (.client p.clientSecretDigest)),
   recorded .create (.rpc .deleteLoreProvider), recorded .create (.rpc .createLoreProvider),
   recorded .create (.rpc .configureRefresh), recorded .create (.rpc .refresh),
   recorded .create (.rpc .createSandbox)]

def admission (a : Admission) : List Step :=
  if !a.configured then [] else
    [direct (.rpc .countPushed)] ++
    if a.count > a.limit ∧ a.enforce then
      [direct (.sql (.failed .capacity a.message)),
       direct (.abort ⟨"capacity", a.message⟩)]
    else [direct (.sql (.warning (if a.count > a.limit then some a.message else none)))]

/-- Whole source protocol, including unrecorded SQL errors. The original row and
native observation determine the path; no concurrent removal/registration/native
configuration mutation is part of these sequential histories. -/
def program [DecidableEq P] (r : Row) (q : Request) (o : Observations P)
    (native : Option (Sandbox P)) : List Step :=
  (if o.installationDatabase then [] else [recorded .database (.rpc .ensureDatabase)]) ++
  [recorded .build (.rpc .build), recorded .create (.rpc .policies),
   recorded .create (.rpc .template), recorded .create (.rpc .requestedProviders),
   direct (.sql (.ungranted o.prepared.ungranted)), direct (.sql (.push q.pusher))] ++
  (if changed r q then [] else [direct (.sql (.target q.target))]) ++
  [recorded .create (.rpc .readSandbox), direct (.rpc .runnerApplied)] ++
  (match choice r q o.prepared native with
   | .keep => []
   | .restart =>
       [direct (.sql .unserved), recorded .restart (.rpc .refresh)] ++
       if (match native with
          | none => false
          | some s => decide (s.phase ≠ .provisioning ∧
              (s.phase ≠ .starting ∨ s.restartCount > 0))) = true then
         [recorded .restart (.rpc .stopSandbox), recorded .restart (.rpc .startSandbox)] else []
   | .replace =>
       [direct (.sql .unserved), recorded .create (.rpc .deleteSandbox)] ++
       (if changed r q then [recorded .create (.sql (.target q.target))] else []) ++
       creation o.prepared
   | .create =>
       [direct (.sql .unserved)] ++ admission o.admission ++
       (if changed r q then [recorded .create (.sql (.target q.target))] else []) ++
       creation o.prepared) ++
  [direct (.sql (.linkedApplied q.target.revision)), direct (.sql (.ownTree q.ownTree)),
   direct (.sql (.applied q.revision))]

def providerEffect (d : Deployment) (f : Providers → Providers) (s : State P) : State P :=
  { s with providers := update f d s.providers }

def rpcEffect (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (rpc : Rpc) (s : State P) : State P :=
  match rpc with
  | .readSandbox => { s with sandboxes := update (fun _ => o.readNative) d s.sandboxes }
  | .deleteSandbox => { s with sandboxes := update (fun _ => none) d s.sandboxes }
  | .createSandbox => { s with
      sandboxes := update (fun _ => o.createdNative) d s.sandboxes,
      createRequests := update (fun xs => xs ++ [createRequest r q o.prepared]) d s.createRequests }
  | .stopSandbox => { s with sandboxes := update (fun _ => o.stoppedNative) d s.sandboxes }
  | .startSandbox => { s with sandboxes := update (fun _ => o.startedNative) d s.sandboxes }
  | .deleteRegistrationProvider => providerEffect d (fun old => { old with registration := none }) s
  | .createRegistrationProvider => providerEffect d (fun old =>
      { old with registration := some o.prepared.registrationTokenRaw }) s
  | .deleteLoreProvider => providerEffect d (fun old =>
      { old with lorePresent := false, configured := false, refreshMaterial := none, credential := none }) s
  | .createLoreProvider => providerEffect d (fun old => { old with lorePresent := true }) s
  | .configureRefresh => providerEffect d (fun old =>
      { old with configured := true, refreshMaterial := some (refreshMaterial r o.prepared) }) s
  | .refresh => providerEffect d (fun old => { old with credential := (s.rows d).map client }) s
  | _ => s

def effect (d : Deployment) (r : Row) (q : Request) (o : Observations P)
    (a : Action) (s : State P) : State P :=
  match a with
  | .sql sql => sqlEffect d sql s
  | .rpc rpc => rpcEffect d r q o rpc s
  | .abort _ => s

def failureEffect (d : Deployment) (r : Row) (q : Request) (o : Observations P) (a : Action) (s : State P) : State P :=
  match a with
  | .sql statement => if o.sqlCommittedOnError then sqlEffect d statement s else s
  | .rpc .createSandbox => { s with
      sandboxes := update (fun _ => o.failedNative) d s.sandboxes,
      createRequests := if o.createSubmittedOnError then
        update (fun xs => xs ++ [createRequest r q o.prepared]) d s.createRequests
        else s.createRequests }
  | .rpc .deleteSandbox | .rpc .stopSandbox | .rpc .startSandbox =>
      { s with sandboxes := update (fun _ => o.failedNative) d s.sandboxes }
  | .rpc .deleteRegistrationProvider | .rpc .createRegistrationProvider | .rpc .deleteLoreProvider |
      .rpc .createLoreProvider | .rpc .configureRefresh | .rpc .refresh =>
      { s with providers := update (fun _ => o.failedProviders) d s.providers }
  | _ => s

inductive Event
  | success (action : Action)
  | refused (action : Action) (error : Error)
  | failureRecord (stage : Stage) (acknowledged committed : Bool)
  deriving DecidableEq, Repr

structure Outcome (P : Type) where
  state : State P
  error : Option Error
  events : List Event

def stopped (d : Deployment) (r : Row) (q : Request) (o : Observations P) (step : Step) (error : Error)
    (s : State P) : Outcome P :=
  let observed := failureEffect d r q o step.action s
  let final := match step.recorded with
    | some stage => if o.failureLogCommitted then
        sqlEffect d (.failed stage error.message) observed else observed
    | none => observed
  ⟨final, some error, [.refused step.action error] ++
    (step.recorded.toList.map (fun stage => .failureRecord stage o.failureLogAcknowledged o.failureLogCommitted))⟩

/-- UPDATE without a row affects zero rows. push uses RETURNING/fetch_optional
and produces the source's missing-instance error instead. -/
def observedError (d : Deployment) (o : Observations P) (step : Step) (s : State P) :
    Option Error :=
  match step.action with
  | .abort error => some error
  | .sql (.push _) => if (s.rows d).isNone then
        some ⟨"missing-instance", "no instance has deployment " ++ o.prepared.deploymentText d⟩
      else if o.fault = some step.action then some o.error else none
  | _ => if o.fault = some step.action then some o.error else none

/-- failed.sql logging can commit or fail, but never replaces the original error. -/
def run (d : Deployment) (r : Row) (q : Request) (o : Observations P) :
    List Step → State P → Outcome P
  | [], s => ⟨s, none, []⟩
  | step :: rest, s =>
      match observedError d o step s with
      | some error => stopped d r q o step error s
      | none =>
          let tail := run d r q o rest (effect d r q o step.action s)
          { tail with events := .success step.action :: tail.events }

def apply [DecidableEq P] (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) : Outcome P :=
  run d r q o (program r q o o.readNative) s

/-- Independent complete successful row specification, not a fold over statements. -/
def successRow (r : Row) (q : Request) (o : Observations P) (path : Choice) : Row :=
  let creating := path = .replace ∨ path = .create
  { r with
    ungrantedProviders := o.prepared.ungranted,
    pusher := some q.pusher,
    linkedRepository := some q.target.repository,
    trackedBranch := some q.target.trackedBranch,
    trackedName := some q.target.trackedName,
    linkedPusher := some q.target.pusher,
    linkedRevision := some q.target.revision,
    ownTree := some q.ownTree,
    revision := some q.revision,
    failedStage := none,
    failedMessage := none,
    servedAt := if path = .keep then r.servedAt else none,
    tokenDigest := if creating then o.prepared.registrationTokenDigest else r.tokenDigest,
    clientDigest := if creating then some o.prepared.clientSecretDigest else r.clientDigest,
    warning := if path = .create ∧ o.admission.configured then
      (if o.admission.count > o.admission.limit then some o.admission.message else none)
      else r.warning }

/-- client_read.sql digest predicate plus its exact projection. SHA-256 is an
explicit native primitive relation; the model does not equate a raw secret/digest. -/
def clientForSecret (hash : String → String) (raw : String) (r : Row) : Option Client :=
  if r.clientDigest = some (hash raw) then some (client r) else none

/-- Original source's omitted repository/tracked-ID assignments, retained solely
for a historical source-linked counterexample, not used by the fixed protocol. -/
def historicalLinked (t : Target) (r : Row) : Row :=
  { r with trackedName := some t.trackedName, linkedPusher := some t.pusher }

end FactoryRetarget
