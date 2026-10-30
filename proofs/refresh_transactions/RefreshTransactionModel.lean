import RefreshPredicateProof
import Mathlib.Tactic

/-!
Handwritten design model of one tracked family incarnation and arbitrary finite
interleavings. It is not an extraction/refinement of async Rust, SQLx, PostgreSQL,
crypto, HTTP, or retention batching. Other family keys are framed; this segment
ends when its family is deleted and does not allow recreation under the same key.
Raw family/token payloads reuse the actual pure extraction's types. Statement
constraint/lock semantics and native row decoding remain correspondence premises.
-/
open Aeneas Aeneas.Std veoveo_hub
namespace RefreshTransaction
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Id := Nat
abbrev Key := uuid.Uuid
abbrev Timestamp := SessionFamilyProof.Timestamp
abbrev Family := store.refresh.RefreshFamily
abbrev Token := store.refresh.RefreshToken
abbrev Envelope := store.refresh.RefreshDeliveryEnvelope
abbrev Reason := store.refresh.RefreshRevocation
abbrev Binding := store.refresh.RefreshBinding
abbrev FamilyId := veoveo_hub_sdk.identity.HubRefreshFamilyId
abbrev Before := SessionFamilyProof.Before

def put {α β : Type} [DecidableEq α] (rows : α → β) (key : α) (value : β) : α → β :=
  fun other => if other = key then value else rows other

structure AuditRow where
  id : Id
  content : Nat
  deriving DecidableEq

structure Database where
  family : Option Family
  tokens : Key → Option Token
  audit : Id → Option Nat
  /-- Disjoint other-family rows and endpoint rejection/failure audits. -/
  unrelated : Id → Option Nat
  rejectionAudit : Id → Option Nat

def coalesce {α : Type} (old : Option α) (new : α) : Option α :=
  match old with
  | some value => some value
  | none => some new

def appendAudit (rows : Id → Option Nat) (batch : List AuditRow) :=
  batch.foldl (fun rows row => put rows row.id (some row.content)) rows

def ValidBatch : (Id → Option Nat) → List AuditRow → Prop
  | _, [] => True
  | rows, row :: rest =>
    (rows row.id = none ∨ rows row.id = some row.content) ∧
    ValidBatch (put rows row.id (some row.content)) rest

/-- Individual SQL effects. UPDATEs map existing rows only, and never insert a
missing predecessor. Audit is exact-content/idempotent at the Core boundary. -/
inductive Write where
  | insertFamily (family : Family)
  | insertToken (token : Token)
  | consume (key : Key) (replacement : Key) (now : Timestamp)
  | advance (generation : U64)
  | revoke (now : Timestamp) (reason : Reason)
  | markReplay (key : Key) (now : Timestamp)
  | audit (rows : List AuditRow)

def applyWrite (db : Database) : Write → Database
  | .insertFamily family => { db with family := some family }
  | .insertToken token => { db with tokens := put db.tokens token.id (some token) }
  | .consume key replacement now =>
    { db with
      tokens := put db.tokens key ((db.tokens key).map fun token =>
        { token with
          consumed_at := some now
          replacement := some replacement
          delivery := none }) }
  | .advance generation =>
    { db with family := db.family.map fun family => { family with current_generation := generation } }
  | .revoke now reason =>
    { db with family := db.family.map fun family =>
        { family with
          revoked_at := coalesce family.revoked_at now
          revocation := coalesce family.revocation reason } }
  | .markReplay key now =>
    { db with
      tokens := put db.tokens key ((db.tokens key).map fun token =>
        { token with replay_detected_at := coalesce token.replay_detected_at now }) }
  | .audit rows => { db with audit := appendAudit db.audit rows }

def applyWrites (db : Database) (writes : List Write) : Database := writes.foldl applyWrite db

/-- Primitive commit is deliberately unguarded. Missing comparison, missing audit,
or a premature commit are representable faulty programs, not impossible DB states. -/
def commit := applyWrites

def StoredGeneration (generation : U64) : Prop := generation.val ≤ 9223372036854775807

def ValidWrite (db : Database) : Write → Prop
  | .insertFamily family => db.family = none ∧ family.current_generation = 0#u64 ∧
      family.revoked_at = none ∧ family.revocation = none ∧ Before family.issued_at family.expires_at
  | .insertToken token =>
      db.tokens token.id = none ∧ StoredGeneration token.generation ∧
      Before token.issued_at token.expires_at ∧
      (∃ family, db.family = some family ∧ token.family = family.id) ∧
      (∀ key old, db.tokens key = some old → old.digest ≠ token.digest) ∧
      (∀ key old, db.tokens key = some old →
        old.family = token.family → old.generation ≠ token.generation)
  | .consume key replacement _ =>
      db.tokens key = none ∨ ∃ successor, db.tokens replacement = some successor
  | .advance generation => StoredGeneration generation
  | .revoke _ _ => True
  | .markReplay _ _ => True
  | .audit rows => ValidBatch db.audit rows

/-- Explicit physical-domain constraints assumed at the model entry, not an
atomic wellformed-store oracle. SQL migration/decoder correspondence is reviewed
and native-tested separately. Other-family rows are outside this projection. -/
def WellFormed (db : Database) : Prop :=
  (∀ family, db.family = some family → StoredGeneration family.current_generation ∧
    Before family.issued_at family.expires_at ∧
    (family.revoked_at = none ↔ family.revocation = none)) ∧
  (∀ key token, db.tokens key = some token →
    key = token.id ∧ StoredGeneration token.generation ∧
    Before token.issued_at token.expires_at ∧
    (∃ family, db.family = some family ∧ token.family = family.id) ∧
    (token.replacement.isSome = true → token.consumed_at.isSome = true) ∧
    (∀ next, token.replacement = some next → ∃ successor, db.tokens next = some successor)) ∧
  (∀ left right a b, db.tokens left = some a → db.tokens right = some b →
    left ≠ right → a.digest ≠ b.digest ∧ a.generation ≠ b.generation)

/-- Fields filled by the native NewRefreshToken-to-stored-token constructor.
These are representation facts, not additional Rust validation guards. -/
def CandidateProjection (family : Family) (candidate : Token) : Prop :=
  candidate.family = family.id ∧ candidate.consumed_at = none ∧
  candidate.replacement = none ∧ candidate.replay_detected_at = none

/-- The four actual validation observations, evaluated before either consumed or
fresh branches. Native stored i64 range and uniqueness remain insertion checks. -/
def ReplacementShape (family : Family) (token candidate : Token) : Prop :=
  some candidate.generation = U64.checked_add token.generation 1#u64 ∧
  candidate.expires_at = family.expires_at ∧ Before candidate.issued_at candidate.expires_at ∧
  candidate.delivery.isSome = true

/-- Reuse the checked exact condition of the actual extracted predicate. The
transaction program still performs its own pointer lookup and caches the row. -/
abbrev CanDeliver := RefreshPredicateProof.Deliverable

def Successor (db : Database) (token : Token) : Option Token := token.replacement.bind db.tokens

inductive Outcome where
  | rotated (successor : Token)
  | duplicate (successor : Token)
  | replay
  | revoked (family : Family)
  | invalid

inductive Operation where
  | rotate
  | revoke (binding : Binding)

structure Request where
  operation : Operation := .rotate
  digest : veoveo_types.digest.Sha256Digest
  candidate : Token
  /-- Sampled before lock waiting; never silently replaced by commit time. -/
  now : Timestamp
  success : AuditRow
  duplicate : AuditRow

/-- Actual store branch order after validation. Binding/current catalog and cipher
checks are caller boundaries; this does not add them to the store. -/
def choose (db : Database) (request : Request) (family : Family) (token : Token) :
    Outcome × List Write :=
  if token.consumed_at.isSome || token.replay_detected_at.isSome then
    match Successor db token with
    | some successor =>
      if CanDeliver family token successor request.now then
        (.duplicate successor, [.audit [request.duplicate]])
      else (.replay, [.revoke request.now .TokenReplay, .markReplay token.id request.now])
    | none => (.replay, [.revoke request.now .TokenReplay, .markReplay token.id request.now])
  else if ¬ Before request.now token.expires_at ∨ ¬ Before request.now family.expires_at ∨
      family.revoked_at.isSome = true ∨ token.generation ≠ family.current_generation then
    (.invalid, [])
  else (.rotated request.candidate,
    [.insertToken request.candidate, .consume token.id request.candidate.id request.now,
      .advance request.candidate.generation, .audit [request.success]])

/-- Explicit client revocation checks binding and expiry, then retains the first
reason/time. An already-revoked or invalid result drops its read transaction. -/
def chooseRevocation (request : Request) (binding : Binding) (family : Family) (token : Token) :
    Outcome × List Write :=
  if ¬ RefreshPredicateProof.Bound family binding ∨ ¬ Before request.now token.expires_at ∨
      ¬ Before request.now family.expires_at then (.invalid, [])
  else if family.revoked_at.isSome then (.revoked family, [])
  else (.revoked { family with revoked_at := some request.now, revocation := some .ClientRevocation },
    [.revoke request.now .ClientRevocation])


inductive Phase where
  | idle | waiting | lockRequested | locked | read | writing | ready | committing | committed | abortPending | aborted
  deriving DecidableEq
/-- Empty programs are early returns; actual Rust drops rather than commits. -/
def decisionPhase (writes : List Write) : Phase :=
  if writes.isEmpty then .abortPending else .writing

/-- Store-result acknowledgement only; state decrypt/digest checks and HTTP
response delivery occur outside this model. -/
inductive Response where
  | pending | success | error
  deriving DecidableEq

structure Transaction where
  request : Request
  phase : Phase := .idle
  lookupFamily : Option FamilyId := none
  familyRead : Option Family := none
  tokenRead : Option Token := none
  outcome : Outcome := .invalid
  program : List Write := []
  staged : List Write := []
  remaining : List Write := []
  response : Response := .pending

structure RotationEvent where
  caller : Id
  predecessor : Key
  successor : Key
  generation : U64

structure State where
  db : Database
  transactions : Id → Transaction
  familyLock : Option Id
  tokenLocks : Key → Option Id
  auditLocks : Id → Option Id
  rotations : List RotationEvent

/-- Maintenance SQL also takes token row locks. Plain token reads take no such
lock, so a later prune can make a subsequent consume/mark update affect zero rows. -/
def TokenFootprint (db : Database) : Write → List Key
  | .insertToken token => [token.id]
  | .consume key _ _ | .markReplay key _ => if (db.tokens key).isSome then [key] else []
  | _ => []
def AuditFootprint : Write → List Id
  | .audit rows => rows.map AuditRow.id
  | _ => []
def Available {α : Type} (locks : α → Option Id) (owner : Id) (keys : List α) : Prop :=
  ∀ key ∈ keys, locks key = none ∨ locks key = some owner

def acquire {α : Type} [DecidableEq α] (locks : α → Option Id) (owner : Id) (keys : List α) :=
  fun key => if key ∈ keys then some owner else locks key
def release {α : Type} (locks : α → Option Id) (owner : Id) :=
  fun key => if locks key = some owner then none else locks key

def update (state : State) (id : Id) (tx : Transaction) : State :=
  { state with transactions := put state.transactions id tx }

def initial (db : Database) (requests : Id → Request) : State where
  db
  transactions := fun id => { request := requests id }
  familyLock := none
  tokenLocks := fun _ => none
  auditLocks := fun _ => none
  rotations := []

/-- Native replacement FK uses ON DELETE SET NULL. -/
def deleteToken (tokens : Key → Option Token) (key : Key) : Key → Option Token :=
  fun other =>
    if other = key then none
    else (tokens other).map fun row =>
      if row.replacement = some key then { row with replacement := none } else row

/-- Constrained protocol, separate from the unguarded commit primitive. Each
writer stages deltas and holds acquired locks through the server decision. -/
inductive Step : State → State → Prop where
  | begin (s : State) (id : Id) (phase : (s.transactions id).phase = .idle) :
      Step s (update s id { s.transactions id with phase := .waiting })
  | queryFamily (s : State) (id : Id) (token : Token)
      (phase : (s.transactions id).phase = .waiting)
      (present : s.db.tokens token.id = some token)
      (digest : token.digest = (s.transactions id).request.digest) :
      Step s (update s id { s.transactions id with phase := .lockRequested, lookupFamily := some token.family })
  | missingDigest (s : State) (id : Id)
      (phase : (s.transactions id).phase = .waiting)
      (missing : ∀ key token, s.db.tokens key = some token → token.digest ≠ (s.transactions id).request.digest) :
      Step s (update s id { s.transactions id with phase := .abortPending })
  | lock (s : State) (id : Id) (family : Family)
      (phase : (s.transactions id).phase = .lockRequested) (free : s.familyLock = none)
      (target : (s.transactions id).lookupFamily = some family.id)
      (present : s.db.family = some family) :
      Step s { update s id { s.transactions id with phase := .locked, familyRead := some family }
        with familyLock := some id }
  | missingFamily (s : State) (id : Id)
      (phase : (s.transactions id).phase = .lockRequested) (missing : s.db.family = none) :
      Step s (update s id { s.transactions id with phase := .abortPending })
  | readToken (s : State) (id : Id) (token : Token)
      (phase : (s.transactions id).phase = .locked)
      (present : s.db.tokens token.id = some token)
      (digest : token.digest = (s.transactions id).request.digest) :
      Step s (update s id { s.transactions id with phase := .read, tokenRead := some token })
  | missingAfterWait (s : State) (id : Id)
      (phase : (s.transactions id).phase = .locked)
      (missing : ∀ key token, s.db.tokens key = some token → token.digest ≠ (s.transactions id).request.digest) :
      Step s (update s id { s.transactions id with phase := .abortPending })
  | decide (s : State) (id : Id) (family : Family) (token : Token)
      (phase : (s.transactions id).phase = .read)
      (operation : (s.transactions id).request.operation = .rotate)
      (familyRead : (s.transactions id).familyRead = some family)
      (tokenRead : (s.transactions id).tokenRead = some token)
      (projection : CandidateProjection family (s.transactions id).request.candidate)
      (shape : ReplacementShape family token (s.transactions id).request.candidate) :
      Step s (update s id { s.transactions id with
        phase := decisionPhase (choose s.db (s.transactions id).request family token).2,
        outcome := (choose s.db (s.transactions id).request family token).1,
        program := (choose s.db (s.transactions id).request family token).2,
        remaining := (choose s.db (s.transactions id).request family token).2 })
  | invalidReplacement (s : State) (id : Id) (family : Family) (token : Token)
      (phase : (s.transactions id).phase = .read)
      (operation : (s.transactions id).request.operation = .rotate)
      (familyRead : (s.transactions id).familyRead = some family)
      (tokenRead : (s.transactions id).tokenRead = some token)
      (bad : ¬ ReplacementShape family token (s.transactions id).request.candidate) :
      Step s (update s id { s.transactions id with phase := .abortPending, response := .error })
  | decideRevocation (s : State) (id : Id) (family : Family) (token : Token) (binding : Binding)
      (phase : (s.transactions id).phase = .read)
      (operation : (s.transactions id).request.operation = .revoke binding)
      (familyRead : (s.transactions id).familyRead = some family)
      (tokenRead : (s.transactions id).tokenRead = some token) :
      Step s (update s id { s.transactions id with
        phase := decisionPhase (chooseRevocation (s.transactions id).request binding family token).2
        outcome := (chooseRevocation (s.transactions id).request binding family token).1
        program := (chooseRevocation (s.transactions id).request binding family token).2
        remaining := (chooseRevocation (s.transactions id).request binding family token).2 })
  | write (s : State) (id : Id) (head : Write) (tail : List Write)
      (phase : (s.transactions id).phase = .writing)
      (next : (s.transactions id).remaining = head :: tail)
      (valid : ValidWrite (applyWrites s.db (s.transactions id).staged) head)
      (tokens : Available s.tokenLocks id (TokenFootprint (applyWrites s.db (s.transactions id).staged) head))
      (audit : Available s.auditLocks id (AuditFootprint head)) :
      Step s { update s id { s.transactions id with
        staged := (s.transactions id).staged ++ [head], remaining := tail } with
        tokenLocks := acquire s.tokenLocks id (TokenFootprint (applyWrites s.db (s.transactions id).staged) head)
        auditLocks := acquire s.auditLocks id (AuditFootprint head) }
  | ready (s : State) (id : Id) (phase : (s.transactions id).phase = .writing)
      (done : (s.transactions id).remaining = []) :
      Step s (update s id { s.transactions id with phase := .ready })
  | requestCommit (s : State) (id : Id) (phase : (s.transactions id).phase = .ready) :
      Step s (update s id { s.transactions id with phase := .committing })
  | serverCommit (s : State) (id : Id) (phase : (s.transactions id).phase = .committing) :
      Step s { update s id { s.transactions id with phase := .committed } with
        db := commit s.db (s.transactions id).staged
        familyLock := if s.familyLock = some id then none else s.familyLock
        tokenLocks := release s.tokenLocks id
        auditLocks := release s.auditLocks id
        rotations := match (s.transactions id).outcome, (s.transactions id).tokenRead with
          | .rotated token, some predecessor => s.rotations ++ [⟨id, predecessor.id, token.id, token.generation⟩]
          | _, _ => s.rotations }
  | fail (s : State) (id : Id)
      (phase : (s.transactions id).phase ∈ [.waiting, .lockRequested, .locked, .read, .writing, .ready]) :
      Step s (update s id { s.transactions id with phase := .abortPending, response := .error })
  | uncertain (s : State) (id : Id) (phase : (s.transactions id).phase = .committing) :
      Step s (update s id { s.transactions id with response := .error })
  | abort (s : State) (id : Id)
      (phase : (s.transactions id).phase = .abortPending ∨
        (s.transactions id).phase = .committing ∧ (s.transactions id).response = .error) :
      Step s { update s id { s.transactions id with phase := .aborted, staged := [], remaining := [] }
        with familyLock := if s.familyLock = some id then none else s.familyLock
             tokenLocks := release s.tokenLocks id, auditLocks := release s.auditLocks id }
  | returnAfterDrop (s : State) (id : Id)
      (phase : (s.transactions id).phase = .aborted) (pending : (s.transactions id).response = .pending) :
      Step s (update s id { s.transactions id with response := .success })
  | respond (s : State) (id : Id) (phase : (s.transactions id).phase = .committed)
      (pending : (s.transactions id).response = .pending) (response : Response)
      (done : response = .success ∨ response = .error) :
      Step s (update s id { s.transactions id with response })
  | clearEnvelope (s : State) (key : Key) (token : Token) (envelope : Envelope) (now : Timestamp)
      (present : s.db.tokens key = some token) (has : token.delivery = some envelope)
      (expired : ¬ Before now envelope.expires_at) (free : s.tokenLocks key = none) :
      Step s { s with db := { s.db with tokens := put s.db.tokens key (some { token with delivery := none }) } }
  | pruneToken (s : State) (key : Key) (token : Token) (now : Timestamp)
      (present : s.db.tokens key = some token) (expired : ¬ Before now token.expires_at)
      (free : s.tokenLocks key = none)
      (referencesFree : ∀ other row, s.db.tokens other = some row → row.replacement = some key → s.tokenLocks other = none) :
      Step s { s with db := { s.db with tokens := deleteToken s.db.tokens key } }
  | pruneFamily (s : State) (family : Family) (now : Timestamp)
      (present : s.db.family = some family) (expired : ¬ Before now family.expires_at)
      (free : s.familyLock = none) (tokensFree : ∀ key, s.tokenLocks key = none) :
      Step s { s with db := { s.db with family := none, tokens := fun _ => none } }
  | unrelated (s : State) (key value : Id) :
      Step s { s with db := { s.db with unrelated := put s.db.unrelated key (some value) } }
  | rejectionAudit (s : State) (key value : Id) :
      Step s { s with db := { s.db with rejectionAudit := put s.db.rejectionAudit key (some value) } }

inductive Execution : State → State → Prop where
  | refl (s : State) : Execution s s
  | tail {first middle last : State} : Execution first middle → Step middle last → Execution first last

end
end RefreshTransaction
