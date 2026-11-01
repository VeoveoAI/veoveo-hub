import FactoryRetargetModel
import Lean.Util.CollectAxioms

/-! Standalone synthetic execution data. No result/output/recovery/choice proof
is imported. These public fixture bytes were used in the checked output witness.
Native hash/UUID/protobuf/default correspondence remains an explicit interpretation
condition; the model does not implement SHA-256 or token issuance. -/
namespace FactoryRetarget.FactoryControlFixture

/-- Public synthetic fixture constants: URL-safe no-pad encodings of 32 bytes,
with SHA-256 computed over the actual token string, as registration::digest does.
No secret/token value from an installation is used. -/
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

def requested : Request where
  target := ⟨"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "22222222222222222222222222222222",
    "release", "https://identity.example#dave", String.ofList (List.replicate 64 'd')⟩
  revision := String.ofList (List.replicate 64 '2')
  pusher := "https://identity.example#alice"
  ownTree := String.ofList (List.replicate 64 'f')
  ownEntries := ["openshell", "src"]

/-- render is the native UUID formatter; only the actually observed ID needs the
stated correspondence, not global truncation injectivity or freshness. -/
def prepared (render : Deployment → String) : Prepared Unit where
  image := "registry.example:5000/factory/0000-000000000001@sha256:" ++
    String.ofList (List.replicate 64 'b')
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

def previousRequest : Request :=
  { requested with
    target := ⟨"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "11111111111111111111111111111111",
      "main", "https://identity.example#carol", String.ofList (List.replicate 64 'c')⟩,
    revision := String.ofList (List.replicate 64 '1') }

def previousPrepared (render : Deployment → String) : Prepared Unit :=
  { prepared render with
    image := "registry.example:5000/factory/0000-000000000001@sha256:" ++
      String.ofList (List.replicate 64 'a'),
    registrationTokenRaw := oldTokenRaw,
    registrationTokenDigest := oldTokenDigest,
    clientSecretRaw := oldClientRaw,
    clientSecretDigest := oldClientDigest }

def previousSandbox (render : Deployment → String) : Sandbox Unit :=
  ⟨some (configuration initial previousRequest (previousPrepared render)), .ready, 0, ()⟩

def observations (render : Deployment → String) : Observations Unit where
  prepared := prepared render
  installationDatabase := true
  admission := ⟨false, 0, 1, false, ""⟩
  fault := none
  error := ⟨"unused", "unused on successful observation sequence"⟩
  sqlCommittedOnError := false
  failureLogCommitted := true
  failureLogAcknowledged := true
  readNative := some (previousSandbox render)
  failedNative := none
  failedProviders := ⟨none, false, false, none, none⟩
  createSubmittedOnError := false
  createdNative := some ⟨some (configuration initial requested (prepared render)), .provisioning, 0, ()⟩
  stoppedNative := some { previousSandbox render with phase := .stopped }
  startedNative := some { previousSandbox render with phase := .starting }

def state (render : Deployment → String) : State Unit where
  rows := fun key => if key = 1 then some initial else none
  sandboxes := fun key => if key = 1 then some (previousSandbox render) else none
  providers := fun key => if key = 1 then
    ⟨some oldTokenRaw, true, true, some (refreshMaterial initial (previousPrepared render)),
      some (client initial)⟩ else ⟨none, false, false, none, none⟩
  createRequests := fun _ => []


/-- Only this populated deployment is interpreted. No global UUID/truncated-name
injectivity is assumed from this formatter outside the singleton source fixture. -/
def uuid : String := "00000000-0000-0000-0000-000000000001"
def render (deployment : Deployment) : String := if deployment = 1 then uuid else "unobserved"

def expectedClient : Client :=
  ⟨1, some "https://identity.example#alice", "0123456789abcdef0123456789abcdef",
    some "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", some "https://identity.example#dave"⟩

def expectedRow : Row where
  deployment := 1
  tokenDigest := newTokenDigest
  createdAt := 0
  database := "proof_factory"
  branch := "proof/retarget"
  repository := "0123456789abcdef0123456789abcdef"
  servedAt := none
  pusher := some "https://identity.example#alice"
  revision := some (String.ofList (List.replicate 64 '2'))
  failedStage := none
  failedMessage := none
  warning := none
  clientDigest := some newClientDigest
  ungrantedProviders := []
  link := some "team-a"
  linkedRepository := some "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  trackedBranch := some "22222222222222222222222222222222"
  trackedName := some "release"
  linkedPusher := some "https://identity.example#dave"
  linkedRevision := some (String.ofList (List.replicate 64 'd'))
  ownTree := some (String.ofList (List.replicate 64 'f'))

def expectedArgv : List String :=
  ["/bin/sh", "-c", script, "runner",
    "grpcs://lore.example:41337/0123456789abcdef0123456789abcdef", "proof/retarget",
    "grpcs://lore.example:41337/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "22222222222222222222222222222222", "openshell", "src"]

def expectedRequest : CreateRequest Unit where
  name := "0000-000000000001"
  workspace := "proof-factory"
  spec := {
    environment := [("VEOVEO_DEPLOYMENT_ID", uuid), ("LORE_IDENTITY", uuid),
      ("VEOVEO_PUBLIC_BASE_URL", "http://0000-000000000001.internal:8080"),
      ("VEOVEO_PORT", "8080"), ("VEOVEO_HUB_ISSUER", "https://hub.example")]
    template := some {
      image := "registry.example:5000/factory/0000-000000000001@sha256:" ++
        String.ofList (List.replicate 64 'b')
      resources := none
      driver := none
      remainder := () }
    policy := some ()
    providers := ["0000-000000000001-registration", "0000-000000000001-lore"]
    command := expectedArgv
    restartAlways := true
    remainder := () }
  remainder := ()

def expectedRefresh : RefreshMaterial where
  workspace := "proof-factory"
  provider := "0000-000000000001-lore"
  credentialKey := "LORE_ACCESS_TOKEN"
  clientId := uuid
  clientSecretRaw := newClientRaw
  strategy := "oauth2-client-credentials"
  secretMaterialKeys := ["client_secret"]

def expectedProviders : Providers where
  registration := some newTokenRaw
  lorePresent := true
  configured := true
  refreshMaterial := some expectedRefresh
  credential := some expectedClient

def expectedNative : Option (Sandbox Unit) :=
  some ⟨some expectedRequest.spec, .provisioning, 0, ()⟩

def capacityMessage : String :=
  "https://identity.example#alice is the latest pusher of 1 instances, above the cap of 1"

def boundaryObservations : Observations Unit :=
  { observations render with readNative := none, admission := ⟨true, 1, 1, true, capacityMessage⟩ }

def rejectedRow : Row where
  deployment := 1
  tokenDigest := oldTokenDigest
  createdAt := 0
  database := "proof_factory"
  branch := "proof/retarget"
  repository := "0123456789abcdef0123456789abcdef"
  servedAt := none
  pusher := some "https://identity.example#alice"
  revision := some (String.ofList (List.replicate 64 '1'))
  failedStage := some .capacity
  failedMessage := some capacityMessage
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

/-- Complete wrong row expected from the one omitted-pair model fault. All other
21-field-row assignments remain fixed, so the witness checks more than one bit. -/
def omittedRow : Row :=
  { expectedRow with linkedRepository := some "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", trackedBranch := some "11111111111111111111111111111111" }

def omittedClient : Client :=
  ⟨1, some "https://identity.example#alice", "0123456789abcdef0123456789abcdef",
    some "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", some "https://identity.example#dave"⟩

def omittedProviders : Providers := { expectedProviders with credential := some omittedClient }

def prefixActions : List Action :=
  [.rpc .build, .rpc .policies, .rpc .template, .rpc .requestedProviders,
   .sql (.ungranted []), .sql (.push "https://identity.example#alice"),
   .rpc .readSandbox, .rpc .runnerApplied, .sql .unserved]

def creationActions : List Action :=
  [.sql (.target requested.target), .sql (.token newTokenDigest),
   .rpc .deleteRegistrationProvider, .rpc .createRegistrationProvider,
   .sql (.client newClientDigest), .rpc .deleteLoreProvider, .rpc .createLoreProvider,
   .rpc .configureRefresh, .rpc .refresh, .rpc .createSandbox,
   .sql (.linkedApplied (String.ofList (List.replicate 64 'd'))),
   .sql (.ownTree (String.ofList (List.replicate 64 'f'))),
   .sql (.applied (String.ofList (List.replicate 64 '2')))]

def retargetEvents : List Event :=
  (prefixActions ++ [Action.rpc .deleteSandbox] ++ creationActions).map Event.success

def boundaryEvents : List Event :=
  (prefixActions ++ [Action.rpc .countPushed, Action.sql (.warning none)] ++ creationActions).map Event.success

def rejectedEvents : List Event :=
  (prefixActions ++ [Action.rpc .countPushed, Action.sql (.failed .capacity capacityMessage)]).map Event.success ++
    [.refused (.abort ⟨"capacity", capacityMessage⟩) ⟨"capacity", capacityMessage⟩]

/-- Independent full observed result at this deployment; no successRow,
configuration, program or fold supplies the expected values. -/
def ExpectedRetarget (output : Outcome Unit) : Prop :=
  output.error = none ∧ output.state.rows 1 = some expectedRow ∧
  output.state.sandboxes 1 = expectedNative ∧ output.state.providers 1 = expectedProviders ∧
  output.state.createRequests 1 = [expectedRequest] ∧
  (output.state.rows 1).map client = some expectedClient ∧ output.events = retargetEvents

def ExpectedBoundary (output : Outcome Unit) : Prop :=
  output.error = none ∧ output.state.rows 1 = some expectedRow ∧
  output.state.sandboxes 1 = expectedNative ∧ output.state.providers 1 = expectedProviders ∧
  output.state.createRequests 1 = [expectedRequest] ∧
  (output.state.rows 1).map client = some expectedClient ∧ output.events = boundaryEvents

def WrongOmission (output : Outcome Unit) : Prop :=
  output.error = none ∧ output.state.rows 1 = some omittedRow ∧
  output.state.sandboxes 1 = expectedNative ∧ output.state.providers 1 = omittedProviders ∧
  output.state.createRequests 1 = [expectedRequest] ∧
  (output.state.rows 1).map client = some omittedClient ∧ output.events = retargetEvents ∧
  omittedRow ≠ expectedRow ∧ (expectedRequest.spec.command)[6]? =
    some "grpcs://lore.example:41337/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

def WrongRefusal (output : Outcome Unit) : Prop :=
  output.error = some ⟨"capacity", capacityMessage⟩ ∧
  output.state.rows 1 = some rejectedRow ∧ output.state.sandboxes 1 = none ∧
  output.state.providers 1 = (state render).providers 1 ∧
  output.state.createRequests 1 = [] ∧ output.events = rejectedEvents ∧
  rejectedRow ≠ expectedRow

end FactoryRetarget.FactoryControlFixture
