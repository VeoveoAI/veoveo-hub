import FactoryRetargetProof
import Lean.Util.CollectAxioms

namespace FactoryRetarget
variable {P : Type} [DecidableEq P]

/-- A successful whole modeled run cannot have passed an abort anywhere in its
program, regardless of earlier RPC/SQL uncertainty or logging observations. -/
theorem successful_run_no_abort (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (steps : List Step) (s : State P)
    (normal : (run d r q o steps s).error = none) :
    ∀ step ∈ steps, ∀ error, step.action ≠ .abort error := by
  induction steps generalizing s with
  | nil => simp
  | cons first rest recursive =>
      cases observed : observedError d o first s with
      | some error => simp [run, observed] at normal
      | none =>
          have tailNormal : (run d r q o rest (effect d r q o first.action s)).error = none := by
            simpa [run, observed] using normal
          intro step member error abort
          rcases List.mem_cons.mp member with same | later
          · subst step
            simp [observedError, abort] at observed
          · exact recursive _ tailNormal step later error abort

/-- Capacity acceptance is now derived, not supplied as a redundant premise. -/
theorem success_implies_capacity (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (normal : (apply d r q o s).error = none) :
    CapacityAllowed o o.readNative := by
  cases native : o.readNative with
  | some old => simp [CapacityAllowed, native]
  | none =>
      cases configured : o.admission.configured with
      | false => simp [CapacityAllowed, native, configured]
      | true =>
          by_cases count : o.admission.count ≤ o.admission.limit
          · simp [CapacityAllowed, native, count]
          · cases enforced : o.admission.enforce with
            | false => simp [CapacityAllowed, native, enforced]
            | true =>
                exfalso
                let error : Error := ⟨"capacity", o.admission.message⟩
                have overflow : o.admission.count > o.admission.limit := by omega
                have member : direct (.abort error) ∈ program r q o o.readNative := by
                  simp [program, choice, admission, native, configured, enforced,
                    overflow, error, direct, recorded]
                exact successful_run_no_abort d r q o _ s normal _ member error rfl

theorem successful_apply_complete (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (normal : (apply d r q o s).error = none) :
    (apply d r q o s).state.rows d =
      some (successRow r q o (choice r q o.prepared o.readNative)) :=
  successful_apply_exact d r q o s existing key runner
    (success_implies_capacity d r q o s normal) normal

/-- Source restart condition; this is not readiness inferred from an RPC ack. -/
def alreadyStarted (native : Option (Sandbox P)) : Bool :=
  match native with
  | none => false
  | some old => decide (old.phase ≠ .provisioning ∧
      (old.phase ≠ .starting ∨ old.restartCount > 0))

/-- Independent path-sensitive observation specification. Keep may retain a
previous image/environment/restart policy; no wanted-full-config equality. -/
def nativeOutput (r : Row) (q : Request) (o : Observations P) : Option (Sandbox P) :=
  match choice r q o.prepared o.readNative with
  | .keep => o.readNative
  | .restart => if alreadyStarted o.readNative then o.startedNative else o.readNative
  | .replace | .create => o.createdNative

def providerOutput (r : Row) (q : Request) (o : Observations P) (before : Providers) : Providers :=
  let current := some (client (successRow r q o (choice r q o.prepared o.readNative)))
  match choice r q o.prepared o.readNative with
  | .keep => before
  | .restart => { before with credential := current }
  | .replace | .create =>
      { registration := some o.prepared.registrationTokenRaw,
        lorePresent := true, configured := true,
        refreshMaterial := some (refreshMaterial r o.prepared), credential := current }

def requestOutput (r : Row) (q : Request) (o : Observations P)
    (before : List (CreateRequest P)) : List (CreateRequest P) :=
  match choice r q o.prepared o.readNative with
  | .keep | .restart => before
  | .replace | .create => before ++ [createRequest r q o.prepared]

set_option maxHeartbeats 800000 in
/-- Pure full-state algebra, separate from the independent output specifications.
Native returned observations stay arbitrary, and keep never invokes refresh. -/
theorem complete_output_algebra (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (runner : Runner r) (capacity : CapacityAllowed o o.readNative) :
    let final := foldEffects d r q o (program r q o o.readNative) s
    final.sandboxes d = nativeOutput r q o ∧
    final.providers d = providerOutput r q o (s.providers d) ∧
    final.createRequests d = requestOutput r q o (s.createRequests d) := by
  have linked : r.link.isSome = true := runner.1
  -- Discharge the small lifecycle decision before expanding any state records.
  have retainedUnchanged :
      choice r q o.prepared o.readNative = .keep ∨
        choice r q o.prepared o.readNative = .restart → changed r q = false := by
    intro retained
    cases native : o.readNative with
    | none => simp [choice, native] at retained
    | some old =>
        cases spec : old.spec with
        | none => simp [choice, native, spec] at retained
        | some config =>
            cases retarget : changed r q with
            | false => rfl
            | true => simp [choice, native, spec, retarget] at retained
  cases selected : choice r q o.prepared o.readNative with
  | keep =>
      have unchanged := retainedUnchanged (Or.inl selected)
      simp only [program, selected, unchanged]
      split_ifs <;>
        simp_all [foldEffects, effect, rpcEffect, sqlEffect, providerEffect, update,
          updateRow, nativeOutput, providerOutput, requestOutput, successRow, client]
  | restart =>
      have unchanged := retainedUnchanged (Or.inr selected)
      simp only [program, selected, unchanged]
      split_ifs <;>
        simp_all [foldEffects, effect, rpcEffect, sqlEffect, providerEffect, update,
          updateRow, nativeOutput, providerOutput, requestOutput, successRow, client,
          alreadyStarted]
      all_goals cases native : o.readNative <;> simp_all
      all_goals intro phase count; simp_all
  | replace =>
      simp only [program, selected, creation]
      split_ifs <;>
        simp_all [foldEffects, effect, rpcEffect, sqlEffect, providerEffect, update,
          updateRow, nativeOutput, providerOutput, requestOutput, successRow, client]
  | create =>
      have absent : o.readNative = none := by
        cases native : o.readNative with
        | none => rfl
        | some old =>
            cases spec : old.spec with
            | none => simp [choice, native, spec] at selected
            | some config =>
                simp only [choice, native, spec] at selected
                split_ifs at selected <;> contradiction
      simp only [program, selected, admission, creation]
      split_ifs <;>
        simp_all [foldEffects, effect, rpcEffect, sqlEffect, providerEffect, update,
          updateRow, nativeOutput, providerOutput, requestOutput, successRow, client,
          CapacityAllowed]
      all_goals omega

/-- Complete row and path-sensitive outputs after success. Actual SQL/client,
UUID/name/hash/protobuf and native RPC interpretation remain external, scoped
contracts; this is a theorem of the manual protocol, not extracted Rust. -/
theorem successful_outputs_complete (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (normal : (apply d r q o s).error = none) :
    let final := (apply d r q o s).state
    final.rows d = some (successRow r q o (choice r q o.prepared o.readNative)) ∧
    final.sandboxes d = nativeOutput r q o ∧
    final.providers d = providerOutput r q o (s.providers d) ∧
    final.createRequests d = requestOutput r q o (s.createRequests d) ∧
    Frame d s final := by
  have capacity := success_implies_capacity d r q o s normal
  have allEffects := run_success_fold d r q o (program r q o o.readNative) s normal
  have outputs := complete_output_algebra d r q o s existing runner capacity
  refine ⟨successful_apply_complete d r q o s existing key runner normal, ?_, ?_, ?_,
    apply_frame d r q o s⟩
  · simpa only [apply, allEffects] using outputs.1
  · simpa only [apply, allEffects] using outputs.2.1
  · simpa only [apply, allEffects] using outputs.2.2

/-- A retained native configuration need only agree on the fields source actually
compares. The new image/environment/restart policy are not claimed equal. -/
theorem keep_preserves_outputs (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (normal : (apply d r q o s).error = none)
    (keep : choice r q o.prepared o.readNative = .keep) :
    let final := (apply d r q o s).state
    final.sandboxes d = o.readNative ∧ final.providers d = s.providers d ∧
    final.createRequests d = s.createRequests d := by
  have outputs := successful_outputs_complete d r q o s existing key runner normal
  simpa [nativeOutput, providerOutput, requestOutput, keep] using
    And.intro outputs.2.1 (And.intro outputs.2.2.1 outputs.2.2.2.1)

theorem restart_outputs (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (normal : (apply d r q o s).error = none)
    (restart : choice r q o.prepared o.readNative = .restart) :
    let final := (apply d r q o s).state
    final.sandboxes d = (if alreadyStarted o.readNative then o.startedNative else o.readNative) ∧
    final.providers d = { s.providers d with
      credential := some (client (successRow r q o .restart)) } ∧
    final.createRequests d = s.createRequests d := by
  have outputs := successful_outputs_complete d r q o s existing key runner normal
  simpa [nativeOutput, providerOutput, requestOutput, restart] using
    And.intro outputs.2.1 (And.intro outputs.2.2.1 outputs.2.2.2.1)

/-- Exact issued create request/material and observed outcome; no conclusion that
an arbitrary post-ack native spec/phase equals the request or is ready. -/
theorem creation_outputs (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (existing : s.rows d = some r)
    (key : r.deployment = d) (runner : Runner r)
    (normal : (apply d r q o s).error = none)
    (creating : choice r q o.prepared o.readNative = .replace ∨
      choice r q o.prepared o.readNative = .create) :
    let final := (apply d r q o s).state
    final.sandboxes d = o.createdNative ∧
    final.providers d =
      { registration := some o.prepared.registrationTokenRaw,
        lorePresent := true, configured := true,
        refreshMaterial := some (refreshMaterial r o.prepared),
        credential := some (client (successRow r q o (choice r q o.prepared o.readNative))) } ∧
    final.createRequests d = s.createRequests d ++ [createRequest r q o.prepared] := by
  have outputs := successful_outputs_complete d r q o s existing key runner normal
  rcases creating with replace | create
  · simpa [nativeOutput, providerOutput, requestOutput, replace] using
      And.intro outputs.2.1 (And.intro outputs.2.2.1 outputs.2.2.2.1)
  · simpa [nativeOutput, providerOutput, requestOutput, create] using
      And.intro outputs.2.1 (And.intro outputs.2.2.1 outputs.2.2.2.1)

theorem no_fault_observation (d : Deployment) (o : Observations P) (step : Step)
    (s : State P) (before : Row) (existing : s.rows d = some before)
    (noFault : o.fault = none) (notAbort : ∀ error, step.action ≠ .abort error) :
    observedError d o step s = none := by
  cases action : step.action with
  | abort error => exact False.elim (notAbort error action)
  | sql statement =>
      cases statement <;> simp [observedError, action, noFault, existing]
  | rpc operation => simp [observedError, action, noFault]

/-- Constructive acceptance uses external no-error observations and absence of
source aborts, not an assumption about our operation's successful return. -/
theorem no_fault_run (d : Deployment) (original : Row) (q : Request)
    (o : Observations P) (steps : List Step) (s : State P) (before : Row)
    (existing : s.rows d = some before) (noFault : o.fault = none)
    (noAbort : ∀ step ∈ steps, ∀ error, step.action ≠ .abort error) :
    (run d original q o steps s).error = none := by
  induction steps generalizing s before with
  | nil => rfl
  | cons first rest recursive =>
      have observed := no_fault_observation d o first s before existing noFault
        (noAbort first (by simp))
      simp only [run, observed]
      exact recursive _ _ (effect_row d original before q o first.action s existing)
        (fun step member => noAbort step (by simp [member]))

theorem changed_present_accepts (d : Deployment) (r : Row) (q : Request)
    (o : Observations P) (s : State P) (old : Sandbox P)
    (existing : s.rows d = some r) (noFault : o.fault = none)
    (present : o.readNative = some old) (retarget : changed r q = true) :
    (apply d r q o s).error = none := by
  have noAbort : ∀ step ∈ program r q o o.readNative,
      ∀ error, step.action ≠ .abort error := by
    cases installed : o.installationDatabase <;> cases spec : old.spec <;>
      simp [program, choice, creation, present, retarget, installed, spec, direct, recorded]
  exact no_fault_run d r q o _ s r existing noFault noAbort


namespace PopulatedWitness
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

/-- Singleton represented store: both UNIQUE constraints hold without assuming
universal collision freedom or unrelated secret freshness. -/
theorem stored_row_unique (render : Deployment → String) (left right : Deployment)
    (a b : Row) (ha : (state render).rows left = some a)
    (hb : (state render).rows right = some b) : left = right := by
  by_cases l : left = 1
  · by_cases r : right = 1
    · exact l.trans r.symm
    · simp [state, r] at hb
  · simp [state, l] at ha

theorem fixture_secret_shapes :
    oldTokenRaw.length = 43 ∧ newTokenRaw.length = 43 ∧
    oldClientRaw.length = 43 ∧ newClientRaw.length = 43 ∧
    oldTokenDigest ≠ newTokenDigest ∧ oldClientDigest ≠ newClientDigest := by decide

/-- Populated constructive replacement, with the native SHA/UUID interpretation
stated independently of the model result. Unit values stand for the fixed valid
policy and per-role protobuf defaults, not verified native protobuf execution. -/
theorem changed_target_success (render : Deployment → String) (hash : String → String)
    (uuid : render 1 = "00000000-0000-0000-0000-000000000001")
    (oldRegistrationHash : hash oldTokenRaw = oldTokenDigest)
    (registrationHash : hash newTokenRaw = newTokenDigest)
    (oldCustodyHash : hash oldClientRaw = oldClientDigest)
    (custodyHash : hash newClientRaw = newClientDigest) :
    let output := apply 1 initial requested (observations render) (state render)
    output.error = none ∧
    output.state.rows 1 = some (successRow initial requested (observations render) .replace) ∧
    output.state.createRequests 1 = [createRequest initial requested (prepared render)] ∧
    (output.state.providers 1).refreshMaterial.map RefreshMaterial.clientId =
      some "00000000-0000-0000-0000-000000000001" ∧
    clientForSecret hash newClientRaw (successRow initial requested (observations render) .replace) =
      some (client (successRow initial requested (observations render) .replace)) := by
  have binding : (state render).rows 1 = some initial := by simp [state]
  have runner : Runner initial := by simp [Runner, initial]
  have key : initial.deployment = 1 := rfl
  have retarget : changed initial requested = true := by decide
  have normal := changed_present_accepts 1 initial requested (observations render)
    (state render) (previousSandbox render) binding rfl rfl retarget
  have path : choice initial requested (prepared render) (some (previousSandbox render)) = .replace := by
    simp [choice, retarget, previousSandbox]
  have outputs := successful_outputs_complete 1 initial requested (observations render)
    (state render) binding key runner normal
  refine ⟨normal, ?_, ?_, ?_, ?_⟩
  · simpa [observations, path] using outputs.1
  · simpa [requestOutput, observations, path, state] using outputs.2.2.2.1
  · rw [outputs.2.2.1]
    have selected : choice initial requested (observations render).prepared
        (observations render).readNative = .replace := path
    simp only [providerOutput, selected]
    simp [refreshMaterial, observations, initial, prepared, uuid]
  · simp [clientForSecret, successRow, observations, prepared, custodyHash]

end PopulatedWitness

#print axioms success_implies_capacity
#print axioms successful_apply_complete
#print axioms successful_outputs_complete
#print axioms keep_preserves_outputs
#print axioms restart_outputs
#print axioms creation_outputs
#print axioms changed_present_accepts
#print axioms PopulatedWitness.changed_target_success

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``successful_run_no_abort, ``success_implies_capacity,
      ``successful_apply_complete, ``complete_output_algebra, ``successful_outputs_complete,
      ``keep_preserves_outputs, ``restart_outputs, ``creation_outputs,
      ``no_fault_observation, ``no_fault_run, ``changed_present_accepts,
      ``PopulatedWitness.stored_row_unique, ``PopulatedWitness.fixture_secret_shapes,
      ``PopulatedWitness.changed_target_success] do
    let found ← Lean.collectAxioms declaration
    let unexpected := found.filter (fun name => !allowed.contains name)
    unless unexpected.isEmpty do
      throwError "unexpected native-output protocol axiom in {declaration}: {unexpected}"

end FactoryRetarget
