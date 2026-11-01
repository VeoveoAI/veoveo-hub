import FactoryWatcherModel

/-! Public synthetic execution inputs for the manual existing-runner watcher
protocol. Only the two model modules are in this import graph; no correctness
leaf is imported. Expected complete values are constructed independently below.
The source UUID/hash/URL/protobuf/native-observation contracts still apply.
The issued script syncs branch@LATEST; argv is not checkout-byte evidence.

Native interpretation assumptions: one existing hub.instance row with its full
21-column representation, schema runner/key and digest-uniqueness constraints,
no concurrent external row mutation, parsed canonical UUID/32-lowerhex IDs,
64-lowerhex revisions/tree IDs, actual SHA-256 correspondence for the public
raw-token/digest pairs, and scoped native sandbox-name non-aliasing. The fixture
has one populated deployment and distinct registration/client digests; these
proofs do not implement SQL constraints, cryptography, protobuf or RPC semantics.
Parent links below are PINNED (follows=false): ordinary pinned resolve observes
the branch owning the supplied linked revision, not parentBranchId=333... .
The first observed .provisioning after create and a later read of .ready are
separate supplied native observations; their external phase transition is not
proved, nor is readiness inferred from create acknowledgement. -/
namespace FactoryRetarget.Watcher.Fixture

def oldTokenRaw : String := "MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDA"
def oldTokenDigest : String := "sha256:bc054391e37006f6cee04155c30f53e1a6681d08d81af073203486dff178c005"
def newTokenRaw : String := "MTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTE"
def newTokenDigest : String := "sha256:3a397971ec1a9c130aa8465cc7ea8917d9ea346a31d029d4e5941e164e77c323"
def oldClientRaw : String := "MjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjI"
def oldClientDigest : String := "sha256:4ae6c41c417433030ea7bfa9e43c841317dee20e522dfa136db3318ca29bbfe8"
def newClientRaw : String := "MzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzM"
def newClientDigest : String := "sha256:28f943cffb39fd37e62483942d6d7137dc50788bd9dded168f79d02875de71df"
def script : String := "set -e\nparent=$1 branch=$2 linked=$3 tracked=$4\nshift 4\nview() { printf '/*\\n'; for entry in \"$@\"; do printf '!/%s\\n' \"$entry\"; done; }\nif [ -d /sandbox/src/.lore ]; then\n  view \"$@\" > /sandbox/src/.lore/view\nelse\n  view \"$@\" > /sandbox/.view\n  /opt/veoveo/lore/usr/local/bin/lore clone \"$parent\" /sandbox/src --branch \"$branch\" --view /sandbox/.view\nfi\n[ -d /sandbox/link/.lore ] || /opt/veoveo/lore/usr/local/bin/lore clone \"$linked\" /sandbox/link --branch \"$tracked\"\ncd /sandbox/link\n/opt/veoveo/lore/usr/local/bin/lore sync --reset \"$tracked@LATEST\"\ncd /sandbox/src\n/opt/veoveo/lore/usr/local/bin/lore sync --reset \"$branch@LATEST\"\nexport VEOVEO_TRACKED_BRANCH=\"$tracked\"\nexec ./openshell/run"

def initial : Row where
  deployment := 1
  tokenDigest := oldTokenDigest
  createdAt := 0
  database := "proof_factory"
  branch := "proof/retarget"
  repository := "0123456789abcdef0123456789abcdef"
  servedAt := some 1
  pusher := some "https://identity.example#alice"
  revision := some (String.ofList (List.replicate 64 '1'))
  failedStage := none
  failedMessage := none
  warning := none
  clientDigest := some oldClientDigest
  ungrantedProviders := []
  link := some "team-a"
  linkedRepository := some "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  trackedBranch := some "11111111111111111111111111111111"
  trackedName := some "main"
  linkedPusher := some "https://identity.example#carol"
  linkedRevision := some (String.ofList (List.replicate 64 'c'))
  ownTree := some (String.ofList (List.replicate 64 'f'))


def revision1 : String := String.ofList (List.replicate 64 '1')
def revision2 : String := String.ofList (List.replicate 64 '2')
def linkedC : String := String.ofList (List.replicate 64 'c')
def linkedD : String := String.ofList (List.replicate 64 'd')
def linkedE : String := String.ofList (List.replicate 64 'e')
def oldTree : String := String.ofList (List.replicate 64 'f')
def newTree : String := String.ofList (List.replicate 64 'a')
def alice := "https://identity.example#alice"
def frank := "https://identity.example#frank"
def carol := "https://identity.example#carol"
def dave := "https://identity.example#dave"
def erin := "https://identity.example#erin"
def parentRepository := "0123456789abcdef0123456789abcdef"
def repositoryA := "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
def repositoryB := "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
def branchA := "11111111111111111111111111111111"
def branchB := "22222222222222222222222222222222"
def uuid := "00000000-0000-0000-0000-000000000001"
def parentBranchId := "33333333333333333333333333333333"
def oldEntries := ["openshell", "old-program"]
def newEntries := ["openshell", "src", "current-program"]
def image (digit : Char) : String := "registry.example:5000/factory/0000-000000000001@sha256:" ++
  String.ofList (List.replicate 64 digit)
def render (deployment : Deployment) : String := if deployment = 1 then uuid else "unobserved"

def runnerKey : RunnerKey := ⟨parentRepository, "proof/retarget", "team-a"⟩
def captured : Projection := ⟨runnerKey, repositoryA, branchA, "main", some revision1,
  some alice, some linkedC, some carol⟩
def failureKey : FailureKey := ⟨uuid, runnerKey⟩
def unrelatedKey : FailureKey := ⟨"00000000-0000-0000-0000-000000000002", runnerKey⟩
def oldFailure : Failure := ⟨repositoryA, revision1, "previous linked preparation failed", 11⟩
def unrelatedFailure : Failure := ⟨repositoryB, revision2, "another watched deployment failed", 13⟩

/-- Independent complete issued request, not command/configuration/createRequest
from the execution model. Every modeled request/spec field is present. -/
def expectedArgv (repository tracked : String) (entries : List String) : List String :=
  ["/bin/sh", "-c", script, "runner", "grpcs://lore.example:41337/" ++ parentRepository,
    "proof/retarget", "grpcs://lore.example:41337/" ++ repository, tracked] ++ entries

def expectedRequest (repository tracked : String) (entries : List String) (builtImage : String) : CreateRequest Unit where
  name := "0000-000000000001"
  workspace := "proof-factory"
  spec := {
    environment := [("VEOVEO_DEPLOYMENT_ID", uuid), ("LORE_IDENTITY", uuid),
      ("VEOVEO_PUBLIC_BASE_URL", "http://0000-000000000001.internal:8080"),
      ("VEOVEO_PORT", "8080"), ("VEOVEO_HUB_ISSUER", "https://hub.example")]
    template := some { image := builtImage, resources := none, driver := none, remainder := () }
    policy := some ()
    providers := ["0000-000000000001-registration", "0000-000000000001-lore"]
    command := expectedArgv repository tracked entries
    restartAlways := true
    remainder := () }
  remainder := ()

def expectedNative (request : CreateRequest Unit) : Option (Sandbox Unit) :=
  some ⟨some request.spec, .provisioning, 0, ()⟩

def preparedFor (builtImage : String) : Prepared Unit where
  image := builtImage
  policy := ()
  resources := none
  driver := none
  providers := []
  ungranted := []
  loreBase := ⟨"grpcs", "lore.example:41337", "", none, none⟩
  workspace := "proof-factory"
  hubIssuer := "https://hub.example"
  port := 8080
  deploymentText := render
  runnerScript := script
  registrationTokenRaw := newTokenRaw
  registrationTokenDigest := newTokenDigest
  clientSecretRaw := newClientRaw
  clientSecretDigest := newClientDigest
  templateDefaults := ()
  specDefaults := ()
  requestDefaults := ()

def oldRequest : CreateRequest Unit := expectedRequest repositoryA branchA oldEntries (image 'a')
def advancedRequest : CreateRequest Unit := expectedRequest repositoryA branchA newEntries (image 'b')
def retargetRequest : CreateRequest Unit := expectedRequest repositoryB branchB newEntries (image 'b')
def currentRequest : CreateRequest Unit := expectedRequest repositoryA branchA newEntries (image 'c')
def branchChangedRequest : CreateRequest Unit := expectedRequest repositoryA branchB newEntries (image 'b')
def obsoleteProgramRequest : CreateRequest Unit := expectedRequest repositoryA branchA oldEntries (image 'd')

def observations (before after : CreateRequest Unit) : Observations Unit where
  prepared := preparedFor (after.spec.template.getD ⟨"", none, none, ()⟩).image
  installationDatabase := true
  admission := ⟨false, 0, 1, false, ""⟩
  fault := none
  error := ⟨"unused", "unused on successful observation sequence"⟩
  sqlCommittedOnError := false
  failureLogCommitted := true
  failureLogAcknowledged := true
  readNative := some ⟨some before.spec, .ready, 0, ()⟩
  failedNative := none
  failedProviders := ⟨none, false, false, none, none⟩
  createSubmittedOnError := false
  createdNative := expectedNative after
  stoppedNative := some ⟨some before.spec, .stopped, 0, ()⟩
  startedNative := some ⟨some before.spec, .starting, 0, ()⟩

def oldRefresh : RefreshMaterial := ⟨"proof-factory", "0000-000000000001-lore", "LORE_ACCESS_TOKEN",
  uuid, oldClientRaw, "oauth2-client-credentials", ["client_secret"]⟩
def newRefresh : RefreshMaterial := ⟨"proof-factory", "0000-000000000001-lore", "LORE_ACCESS_TOKEN",
  uuid, newClientRaw, "oauth2-client-credentials", ["client_secret"]⟩
def oldClient : Client := ⟨1, some alice, parentRepository, some repositoryA, some carol⟩
def oldProviders : Providers := ⟨some oldTokenRaw, true, true, some oldRefresh, some oldClient⟩
def expectedProviders (parentPusher repository linkedPusher : String) : Providers :=
  ⟨some newTokenRaw, true, true, some newRefresh,
    some ⟨1, some parentPusher, parentRepository, some repository, some linkedPusher⟩⟩

def initialState : WatchState Unit where
  factory := {
    rows := fun key => if key = 1 then some initial else none
    sandboxes := fun key => if key = 1 then some ⟨some oldRequest.spec, .ready, 0, ()⟩ else none
    providers := fun key => if key = 1 then oldProviders else ⟨none, false, false, none, none⟩
    createRequests := fun _ => [] }
  failures := fun key => if key = failureKey then some oldFailure else
    if key = unrelatedKey then some unrelatedFailure else none

def uncancelled : Cancellation := ⟨false, false, false⟩
def acknowledged : Publication := ⟨true, .ok (), 19⟩

/-- The parent request's own source inputs and resolve observations are data;
parentLink itself constructs the Request passed to Factory.apply. -/
def parentInput (repository : String) : ParentInput where
  key := runnerKey
  watchedDeployment := uuid
  requestedRepository := repository
  parentBranchId := parentBranchId
  pinnedRevision := if repository = repositoryB then linkedD else linkedC
  follows := false
  revision := revision2
  pusher := frank
  ownTree := newTree
  ownEntries := newEntries

def parentObservations (retarget : Bool) : ParentObservations Unit where
  query := .ok (some captured)
  resolve := fun _ _ => .ok {
    target := if retarget then ⟨repositoryB, branchB, "release", dave, linkedD⟩
      else ⟨repositoryA, branchA, "main", carol, linkedC⟩
    factory := observations oldRequest (if retarget then retargetRequest else advancedRequest) }
  recorded := fun _ _ => none
  cancellation := uncancelled
  publication := acknowledged

def firstParent (retarget : Bool) : WatchOutcome Unit :=
  parentLink 1 initial (parentInput (if retarget then repositoryB else repositoryA))
    (parentObservations retarget) initialState (by simp [initialState])

/-- The next backing row and current SQL projection are computed from the ACTUAL
first operation; no desired after-state, desired Request or captured parent data
is supplied as a current-read premise. getD is total fixture construction; positive
assertions also require the exact successful first row and projection below. -/
def afterRow (first : WatchOutcome Unit) : Row := (first.state.factory.rows 1).getD initial

def currentQuery (first : WatchOutcome Unit) : Except Error (Option Projection) :=
  .ok ((first.state.factory.rows 1).bind project)

def incoming : Notice := ⟨uuid, repositoryA, some ⟨linkedE, erin⟩⟩

/-- Both metadata coordinates select the synthetic source observation. Wrong
post-relevance use of old captured metadata changes tree, entries AND image/native
observation, while the real backing row/current SQL read stay current. -/
def prepare (args : Arguments) : Except Error (Source Unit) :=
  if args.parentRevision = revision1 ∧ args.parentPusher = alice then
    .ok ⟨oldTree, oldEntries, observations advancedRequest obsoleteProgramRequest⟩
  else if args.parentRevision = revision2 ∧ args.parentPusher = frank then
    .ok ⟨newTree, newEntries, observations advancedRequest currentRequest⟩
  else .error ⟨"fixture-unexpected-source", "no observation for this parent revision and pusher"⟩

def laterObservations (first : WatchOutcome Unit) : ResumeObservations Unit where
  query := currentQuery first
  prepare := prepare
  trackedIdParses := fun _ => true
  branch := fun _ => .ok ⟨"main", false⟩
  cancellation := uncancelled
  publication := acknowledged

def later (first : WatchOutcome Unit) : WatchOutcome Unit :=
  resume 1 (afterRow first) captured incoming (laterObservations first) first.state

/-- Full independent expected 21-column row, not updateRow/successRow/fold. -/
def expectedRow (repository tracked name parentRevision parentPusher linkedRevision linkedPusher tree : String) : Row where
  deployment := 1
  tokenDigest := newTokenDigest
  createdAt := 0
  database := "proof_factory"
  branch := "proof/retarget"
  repository := parentRepository
  servedAt := none
  pusher := some parentPusher
  revision := some parentRevision
  failedStage := none
  failedMessage := none
  warning := none
  clientDigest := some newClientDigest
  ungrantedProviders := []
  link := some "team-a"
  linkedRepository := some repository
  trackedBranch := some tracked
  trackedName := some name
  linkedPusher := some linkedPusher
  linkedRevision := some linkedRevision
  ownTree := some tree

def advancedRow : Row := expectedRow repositoryA branchA "main" revision2 frank linkedC carol newTree
def retargetedRow : Row := expectedRow repositoryB branchB "release" revision2 frank linkedD dave newTree
def resumedRow : Row := expectedRow repositoryA branchA "main" revision2 frank linkedE erin newTree
def branchChangedRow : Row := expectedRow repositoryA branchB "release" revision2 frank linkedD dave newTree

def advancedClient : Client := ⟨1, some frank, parentRepository, some repositoryA, some carol⟩
def retargetedClient : Client := ⟨1, some frank, parentRepository, some repositoryB, some dave⟩
def resumedClient : Client := ⟨1, some frank, parentRepository, some repositoryA, some erin⟩

/-- Each local output assertion retains whole values, not just key equality.
Unrelated-key universal frames are already supplied by generic history proofs. -/
def FullValues (output : WatchOutcome Unit) (row : Row) (native : Option (Sandbox Unit))
    (providers : Providers) (requests : List (CreateRequest Unit)) (visibleClient : Client) : Prop :=
  output.exit = .prepared ∧ output.state.factory.rows 1 = some row ∧
  output.state.factory.sandboxes 1 = native ∧ output.state.factory.providers 1 = providers ∧
  output.state.factory.createRequests 1 = requests ∧
  (output.state.factory.rows 1).map client = some visibleClient ∧
  output.state.failures failureKey = none ∧ output.state.failures unrelatedKey = some unrelatedFailure

/-- Actual successful first retarget plus queued captured A. The complete B state
is preserved; its full client and issued request are exposed independently. -/
def RetargetHistory : Prop :=
  let first := firstParent true
  let second := later first
  FullValues first retargetedRow (expectedNative retargetRequest)
    (expectedProviders frank repositoryB dave) [retargetRequest] retargetedClient ∧
  second.exit = .skipped ∧ second.factoryOutcome.isNone = true ∧
  second.state.factory.rows 1 = some retargetedRow ∧
  second.state.factory.sandboxes 1 = expectedNative retargetRequest ∧
  second.state.factory.providers 1 = expectedProviders frank repositoryB dave ∧
  second.state.factory.createRequests 1 = [retargetRequest] ∧
  (second.state.factory.rows 1).map client = some retargetedClient ∧
  second.state.failures failureKey = none ∧ second.state.failures unrelatedKey = some unrelatedFailure

/-- Current parent2/Frank first operation, then a genuinely old parent1/Alice
candidate. Positive acceptance includes the actual second Factory channel and
complete independent row/client/native/provider/request/argv observations. -/
def ProgramAdvanceHistory : Prop :=
  let first := firstParent false
  let second := later first
  FullValues first advancedRow (expectedNative advancedRequest)
    (expectedProviders frank repositoryA carol) [advancedRequest] advancedClient ∧
  FullValues second resumedRow (expectedNative currentRequest)
    (expectedProviders frank repositoryA erin) [advancedRequest, currentRequest] resumedClient ∧
  second.factoryOutcome.isSome = true ∧
  (second.factoryOutcome.bind (fun output => output.error)) = none ∧
  (second.state.factory.createRequests 1).map (fun request => request.spec.command) =
    [expectedArgv repositoryA branchA newEntries, expectedArgv repositoryA branchA newEntries] ∧
  Trace.logicalPrepare ⟨runnerKey, revision2, frank, uuid, repositoryA, linkedE, erin, branchA⟩ ∈ second.trace


/-- A concrete selected native delete error. It may have removed the sandbox;
this fixture observes absence, retains the complete row/provider effects that
precede it, and records the original create-stage error. -/
def deleteError : Error := ⟨"fixture-delete", "native delete acknowledgement was lost"⟩
def failedLaterObservations (first : WatchOutcome Unit) : ResumeObservations Unit :=
  { laterObservations first with prepare := fun args => (prepare args).map (fun source =>
      { source with factory := { source.factory with fault := some (.rpc .deleteSandbox), error := deleteError } }) }

def failedLater : WatchOutcome Unit :=
  let first := firstParent false
  resume 1 (afterRow first) captured incoming (failedLaterObservations first) first.state

def failedRow : Row :=
  { advancedRow with linkedPusher := some erin, failedStage := some .create, failedMessage := some deleteError.message }

def failedEvents : List Event :=
  [Action.rpc .build, .rpc .policies, .rpc .template, .rpc .requestedProviders,
    .sql (.ungranted []), .sql (.push frank),
    .sql (.target ⟨repositoryA, branchA, "main", erin, linkedE⟩),
    .rpc .readSandbox, .rpc .runnerApplied, .sql .unserved].map Event.success ++
  [.refused (.rpc .deleteSandbox) deleteError, .failureRecord .create true true]

def FailedFactoryHistory : Prop :=
  let first := firstParent false
  FullValues first advancedRow (expectedNative advancedRequest)
    (expectedProviders frank repositoryA carol) [advancedRequest] advancedClient ∧
  FullValues failedLater failedRow none (expectedProviders frank repositoryA carol)
    [advancedRequest] resumedClient ∧
  failedLater.factoryOutcome.map (fun output => output.error) = some (some deleteError) ∧
  failedLater.factoryOutcome.map (fun output => output.events) = some failedEvents ∧
  Trace.log .factory deleteError ∈ failedLater.trace ∧
  Trace.statusAttempt failureKey none ∈ failedLater.trace

/-- A native preparation error first publishes a complete failure, even though
its status reply is lost. The next actual parent call acknowledges clearing it.
The distinct watched-deployment failure must survive both operations. -/
def preparationError : Error := ⟨"fixture-prepare", "parent source read refused"⟩
def replyLost : Error := ⟨"fixture-status", "publication reply was lost"⟩
def refusedObservations : ResumeObservations Unit where
  query := .ok (some captured)
  prepare := fun _ => .error preparationError
  trackedIdParses := fun _ => true
  branch := fun _ => .ok ⟨"main", false⟩
  cancellation := uncancelled
  publication := ⟨true, .error replyLost, 41⟩

def refused : WatchOutcome Unit := resume 1 initial captured incoming refusedObservations initialState

def parentAfterRefusal : WatchOutcome Unit :=
  parentLink 1 (afterRow refused) (parentInput repositoryA) (parentObservations false) refused.state
    (by decide)

def RefusalThenParentHistory : Prop :=
  refused.exit = .preparationRefused preparationError ∧ refused.factoryOutcome.isNone = true ∧
  refused.state.factory.rows 1 = some initial ∧
  refused.state.factory.sandboxes 1 = some ⟨some oldRequest.spec, .ready, 0, ()⟩ ∧
  refused.state.factory.providers 1 = oldProviders ∧ refused.state.factory.createRequests 1 = [] ∧
  refused.state.failures failureKey = some ⟨repositoryA, revision1, preparationError.message, 41⟩ ∧
  refused.state.failures unrelatedKey = some unrelatedFailure ∧
  Trace.statusReply (.error replyLost) ∈ refused.trace ∧ Trace.log .status replyLost ∈ refused.trace ∧
  FullValues parentAfterRefusal advancedRow (expectedNative advancedRequest)
    (expectedProviders frank repositoryA carol) [advancedRequest] advancedClient


/-- Same repository, different tracked branch: the second half of the actual
resume relevance OR. Its pinned revision is the observation owned by branchB. -/
def branchChangedInput : ParentInput := { parentInput repositoryA with pinnedRevision := linkedD }
def branchChangedObservations : ParentObservations Unit :=
  { parentObservations false with resolve := fun _ _ => .ok {
      target := ⟨repositoryA, branchB, "release", dave, linkedD⟩
      factory := observations oldRequest branchChangedRequest } }

def branchChangedParent : WatchOutcome Unit :=
  parentLink 1 initial branchChangedInput branchChangedObservations initialState (by simp [initialState])

def BranchChangedHistory : Prop :=
  let first := branchChangedParent
  let second := later first
  FullValues first branchChangedRow (expectedNative branchChangedRequest)
    (expectedProviders frank repositoryA dave) [branchChangedRequest]
    ⟨1, some frank, parentRepository, some repositoryA, some dave⟩ ∧
  second.exit = .skipped ∧ second.factoryOutcome.isNone = true ∧
  second.state.factory.rows 1 = some branchChangedRow ∧
  second.state.factory.sandboxes 1 = expectedNative branchChangedRequest ∧
  second.state.factory.providers 1 = expectedProviders frank repositoryA dave ∧
  second.state.factory.createRequests 1 = [branchChangedRequest] ∧
  (second.state.factory.rows 1).map client = some ⟨1, some frank, parentRepository, some repositoryA, some dave⟩ ∧
  second.state.failures failureKey = none ∧ second.state.failures unrelatedKey = some unrelatedFailure

/-- Actual recorded fallback observation: native recorded reads using the
requested B link inputs, yet returns existing A's target and linked metadata.
The model composes that observation; it does not verify native resolve/recorded.
Factory can succeed on A while the full status still records the B failure. -/
def resolveError : Error := ⟨"fixture-resolve", "requested B revision could not be resolved"⟩
def fallbackInput : ParentInput := parentInput repositoryB

def fallbackObservations : ParentObservations Unit where
  query := .ok (some captured)
  resolve := fun _ _ => .error resolveError
  recorded := fun _ _ => some {
    target := ⟨repositoryA, branchA, "main", carol, linkedC⟩
    factory := observations oldRequest advancedRequest }
  cancellation := uncancelled
  publication := ⟨true, .error replyLost, 43⟩

def fallbackParent : WatchOutcome Unit :=
  parentLink 1 initial fallbackInput fallbackObservations initialState (by simp [initialState])

def RecordedFallbackHistory : Prop :=
  fallbackParent.exit = .prepared ∧ fallbackParent.factoryOutcome.isSome = true ∧
  fallbackParent.factoryOutcome.bind (fun output => output.error) = none ∧
  fallbackParent.state.factory.rows 1 = some advancedRow ∧
  fallbackParent.state.factory.sandboxes 1 = expectedNative advancedRequest ∧
  fallbackParent.state.factory.providers 1 = expectedProviders frank repositoryA carol ∧
  fallbackParent.state.factory.createRequests 1 = [advancedRequest] ∧
  (fallbackParent.state.factory.rows 1).map client = some advancedClient ∧
  fallbackParent.state.failures failureKey = some ⟨repositoryB, revision2, resolveError.message, 43⟩ ∧
  fallbackParent.state.failures unrelatedKey = some unrelatedFailure ∧
  Trace.logicalResolve fallbackInput (some captured) ∈ fallbackParent.trace ∧
  Trace.logicalRecorded fallbackInput captured ∈ fallbackParent.trace ∧
  Trace.resolveReply (.error resolveError) ∈ fallbackParent.trace ∧
  Trace.log .resolve resolveError ∈ fallbackParent.trace ∧
  Trace.statusReply (.error replyLost) ∈ fallbackParent.trace ∧ Trace.log .status replyLost ∈ fallbackParent.trace

end FactoryRetarget.Watcher.Fixture
