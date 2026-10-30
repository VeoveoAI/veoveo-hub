import Mathlib.Data.List.Basic
import Mathlib.Tactic

/-!
A design model of the publication transaction, not an extraction of async Rust,
SQLx or PostgreSQL. Identifiers and row payloads below are abstract physical keys
and admitted serialized rows. Generating those rows is a separate correspondence
obligation. PostgreSQL commits the delta it receives; it does not check that Hub
finished a publication, compared its predecessor, or appended its audit batch.
-/
namespace ControlPublication

abbrev Id := Nat
abbrev Payload := Nat

def put {α : Type} (rows : Id → α) (key : Id) (value : α) : Id → α :=
  fun other => if other = key then value else rows other

def keepOrInsert (old : Option Payload) (row : Payload) : Option Payload :=
  match old with
  | none => some row
  | some value => some value

structure AuditRow where
  id : Id
  content : Payload
  deriving DecidableEq, Repr

structure ContextRows where
  id : Id
  tenant : Id
  tenantRow : Payload
  /-- Only mutable title/policy/owner/classification/labels/updated-at fields.
  Actual upsert preserves tenant/key/created-at on conflict; those are not encoded
  in this payload and their native correspondence is tested separately. -/
  row : Payload
  grants : List Payload
  memberships : List Payload
  deriving DecidableEq, Repr

/-- Publication-owned relational projection, not complete physical SQL rows.
Context payloads contain only the mutable upsert fields. Ordered lists represent ordinal
grant/membership rows; the object list is a supplied finite row enumeration, not
a guarantee about SQL result ordering. Unrelated and rejection rows are framed. -/
@[ext] structure Database where
  revisions : Id → Option Payload
  objects : Id → List Payload
  enterprise : Option Payload
  tenants : Id → Option Payload
  contexts : Id → Option Payload
  grants : Id → List Payload
  memberships : Id → List Payload
  active : Option Id
  audit : Id → Option Payload
  unrelated : Id → Option Payload
  rejectionAudit : Id → Option Payload

def emptyDatabase : Database where
  revisions := fun _ => none
  objects := fun _ => []
  enterprise := none
  tenants := fun _ => none
  contexts := fun _ => none
  grants := fun _ => []
  memberships := fun _ => []
  active := none
  audit := fun _ => none
  unrelated := fun _ => none
  rejectionAudit := fun _ => none

inductive Write where
  | revision (id : Id) (row : Payload)
  | objects (revision : Id) (rows : List Payload)
  | ensureEnterprise (row : Payload)
  | ensureTenant (id : Id) (row : Payload)
  | context (id : Id) (row : Payload)
  | clearGrants (id : Id)
  | clearMemberships (id : Id)
  | grant (id : Id) (row : Payload)
  | membership (id : Id) (row : Payload)
  | retainContexts (ids : List Id)
  | activate (revision : Id)
  | audit (rows : List AuditRow)
  deriving DecidableEq, Repr

/-- Idempotent exact-content append. The constraint below refuses an occupied ID
with different content. Existing identical records do not imply row growth. -/
def appendAudit (rows : Id → Option Payload) (batch : List AuditRow) : Id → Option Payload :=
  batch.foldl (fun result row => put result row.id (some row.content)) rows

def applyWrite (db : Database) : Write → Database
  | .revision id row => { db with revisions := put db.revisions id (some row) }
  | .objects revision rows => { db with objects := put db.objects revision rows }
  | .ensureEnterprise row => { db with enterprise := keepOrInsert db.enterprise row }
  | .ensureTenant id row =>
      { db with tenants := put db.tenants id (keepOrInsert (db.tenants id) row) }
  | .context id row => { db with contexts := put db.contexts id (some row) }
  | .clearGrants id => { db with grants := put db.grants id [] }
  | .clearMemberships id => { db with memberships := put db.memberships id [] }
  | .grant id row => { db with grants := put db.grants id (db.grants id ++ [row]) }
  | .membership id row =>
      { db with memberships := put db.memberships id (db.memberships id ++ [row]) }
  | .retainContexts ids =>
      { db with
        contexts := fun id => if id ∈ ids then db.contexts id else none
        grants := fun id => if id ∈ ids then db.grants id else []
        memberships := fun id => if id ∈ ids then db.memberships id else [] }
  | .activate revision => { db with active := some revision }
  | .audit rows => { db with audit := appendAudit db.audit rows }

def applyWrites (db : Database) (writes : List Write) : Database :=
  writes.foldl applyWrite db

/-- A server commit applies every staged statement to current committed state.
There is deliberately no completeness, CAS, PC or audit-presence guard. -/
def commit (db : Database) (staged : List Write) : Database := applyWrites db staged

/-- Validate against both previously stored rows and earlier incoming records.
Two absent-ID drafts with different content cannot silently overwrite each other. -/
def ValidBatch (rows : Id → Option Payload) : List AuditRow → Prop
  | [] => True
  | row :: rest =>
      (rows row.id = none ∨ rows row.id = some row.content) ∧
      ValidBatch (put rows row.id (some row.content)) rest

/-- The relational constraints used by this projection. Other actual SQL checks,
FKs, admission and row encodings can refuse an operation; they cannot authorize
an otherwise forbidden program transition. Object-key uniqueness and detailed
identity constraints belong to the supplied admitted row plan. -/
def ValidWrite (db : Database) : Write → Prop
  | .revision id _ => db.revisions id = none
  | .objects revision _ => (db.revisions revision).isSome = true
  | .grant id _ | .membership id _ => (db.contexts id).isSome = true
  | .activate id => (db.revisions id).isSome = true
  | .audit rows => ValidBatch db.audit rows
  | _ => True

def contextWrites (context : ContextRows) : List Write :=
  [.ensureTenant context.tenant context.tenantRow,
   .context context.id context.row, .clearGrants context.id, .clearMemberships context.id] ++
  context.grants.map (.grant context.id) ++
  context.memberships.map (.membership context.id)

structure Plan where
  revision : Id
  document : Payload
  expected : Option Id
  objects : List Payload
  enterprise : Payload
  contexts : List ContextRows
  batch : List AuditRow
  deriving DecidableEq, Repr

def Plan.contextWrites (plan : Plan) : List Write :=
  (if plan.contexts.isEmpty then [] else [.ensureEnterprise plan.enterprise]) ++
  plan.contexts.flatMap ControlPublication.contextWrites ++
  [.retainContexts (plan.contexts.map ContextRows.id)]

def Plan.writes (plan : Plan) : List Write :=
  [.revision plan.revision plan.document, .objects plan.revision plan.objects] ++
  plan.contextWrites ++ [.activate plan.revision, .audit plan.batch]

/-- Independent after-image of one context's rows. This bulk assignment is used
only as a specification, not as the program or PostgreSQL commit primitive. -/
def replaceContext (db : Database) (context : ContextRows) : Database :=
  { db with
    tenants := put db.tenants context.tenant (keepOrInsert (db.tenants context.tenant) context.tenantRow)
    contexts := put db.contexts context.id (some context.row)
    grants := put db.grants context.id context.grants
    memberships := put db.memberships context.id context.memberships }

def expectedContexts (db : Database) (plan : Plan) : Database :=
  let db := if plan.contexts.isEmpty then db
    else { db with enterprise := keepOrInsert db.enterprise plan.enterprise }
  let db := plan.contexts.foldl replaceContext db
  { db with
    contexts := fun id => if id ∈ plan.contexts.map ContextRows.id then db.contexts id else none
    grants := fun id => if id ∈ plan.contexts.map ContextRows.id then db.grants id else []
    memberships := fun id => if id ∈ plan.contexts.map ContextRows.id then db.memberships id else [] }

def expectedPublication (db : Database) (plan : Plan) : Database :=
  let rows := expectedContexts
    { db with
      revisions := put db.revisions plan.revision (some plan.document)
      objects := put db.objects plan.revision plan.objects } plan
  { rows with
    active := some plan.revision
    audit := appendAudit db.audit plan.batch }

inductive Phase where
  | idle | begun | locked | writing | ready | committing | committed | abortPending | aborted
  deriving DecidableEq, Repr

inductive Response where
  | pending | success | error
  deriving DecidableEq, Repr

structure Transaction where
  plan : Plan
  phase : Phase := .idle
  staged : List Write := []
  remaining : List Write := []
  observed : Option Id := none
  response : Response := .pending
  deriving DecidableEq, Repr

structure CommitEvent where
  publisher : Id
  predecessor : Option Id
  revision : Id
  deriving DecidableEq, Repr

structure State where
  db : Database
  transactions : Id → Transaction
  lock : Option Id
  commits : List CommitEvent

def initial (db : Database) (plans : Id → Plan) : State where
  db
  transactions := fun id => { plan := plans id }
  lock := none
  commits := []

def updateTransaction (state : State) (id : Id) (transaction : Transaction) : State :=
  { state with transactions := put state.transactions id transaction }

def release (owner : Option Id) (id : Id) : Option Id :=
  if owner = some id then none else owner

def ActivePhase (phase : Phase) : Prop :=
  phase = .locked ∨ phase = .writing ∨ phase = .ready ∨ phase = .committing

def CheckedPhase (phase : Phase) : Prop :=
  phase = .writing ∨ phase = .ready ∨ phase = .committing

def PrecommitPhase (phase : Phase) : Prop :=
  phase = .begun ∨ phase = .locked ∨ phase = .writing ∨ phase = .ready

/-- Arbitrary schedules of protocol-following publishers. Failure is possible at
every precommit phase; dropping the SQLx handle only requests abort. Commit and
reply delivery are separate, so error is compatible with server commit. -/
inductive Step : State → State → Prop where
  | begin (state : State) (id : Id)
      (phase : (state.transactions id).phase = .idle) :
      Step state (updateTransaction state id { state.transactions id with phase := .begun })
  | lock (state : State) (id : Id)
      (phase : (state.transactions id).phase = .begun) (free : state.lock = none) :
      Step state { updateTransaction state id
        { state.transactions id with phase := .locked, observed := state.db.active } with lock := some id }
  | compare (state : State) (id : Id)
      (phase : (state.transactions id).phase = .locked)
      (same : (state.transactions id).observed = (state.transactions id).plan.expected) :
      Step state (updateTransaction state id
        { state.transactions id with phase := .writing, remaining := (state.transactions id).plan.writes })
  | stale (state : State) (id : Id)
      (phase : (state.transactions id).phase = .locked)
      (changed : (state.transactions id).observed ≠ (state.transactions id).plan.expected) :
      Step state (updateTransaction state id
        { state.transactions id with phase := .abortPending, response := .error })
  | write (state : State) (id : Id) (write : Write) (tail : List Write)
      (phase : (state.transactions id).phase = .writing)
      (next : (state.transactions id).remaining = write :: tail)
      (valid : ValidWrite (applyWrites state.db (state.transactions id).staged) write) :
      Step state (updateTransaction state id
        { state.transactions id with staged := (state.transactions id).staged ++ [write], remaining := tail })
  | ready (state : State) (id : Id)
      (phase : (state.transactions id).phase = .writing)
      (done : (state.transactions id).remaining = []) :
      Step state (updateTransaction state id { state.transactions id with phase := .ready })
  | requestCommit (state : State) (id : Id)
      (phase : (state.transactions id).phase = .ready) :
      Step state (updateTransaction state id { state.transactions id with phase := .committing })
  | serverCommit (state : State) (id : Id)
      (phase : (state.transactions id).phase = .committing) :
      Step state { updateTransaction state id { state.transactions id with phase := .committed } with
        db := commit state.db (state.transactions id).staged
        lock := release state.lock id
        commits := state.commits ++ [⟨id, (state.transactions id).plan.expected,
          (state.transactions id).plan.revision⟩] }
  | fail (state : State) (id : Id)
      (precommit : PrecommitPhase (state.transactions id).phase) :
      Step state (updateTransaction state id
        { state.transactions id with phase := .abortPending, response := .error })
  | commitUncertain (state : State) (id : Id)
      (phase : (state.transactions id).phase = .committing)
      (waiting : (state.transactions id).response = .pending) :
      Step state (updateTransaction state id { state.transactions id with response := .error })
  | serverAbort (state : State) (id : Id)
      (phase : (state.transactions id).phase = .abortPending) :
      Step state { updateTransaction state id
        { state.transactions id with phase := .aborted, staged := [], remaining := [] } with
        lock := release state.lock id }
  | serverAbortUncertain (state : State) (id : Id)
      (phase : (state.transactions id).phase = .committing)
      (uncertain : (state.transactions id).response = .error) :
      Step state { updateTransaction state id
        { state.transactions id with phase := .aborted, staged := [], remaining := [] } with
        lock := release state.lock id }
  | acknowledge (state : State) (id : Id)
      (phase : (state.transactions id).phase = .committed)
      (waiting : (state.transactions id).response = .pending) :
      Step state (updateTransaction state id { state.transactions id with response := .success })
  | loseAcknowledgement (state : State) (id : Id)
      (phase : (state.transactions id).phase = .committed)
      (waiting : (state.transactions id).response = .pending) :
      Step state (updateTransaction state id { state.transactions id with response := .error })
  | unrelated (state : State) (key : Id) (row : Payload) :
      Step state { state with db := { state.db with unrelated := put state.db.unrelated key (some row) } }
  | rejectionAudit (state : State) (key : Id) (row : Payload) :
      Step state { state with db := { state.db with rejectionAudit := put state.db.rejectionAudit key (some row) } }

inductive Execution : State → State → Prop where
  | refl (state : State) : Execution state state
  | tail {first middle last : State} : Execution first middle → Step middle last → Execution first last

end ControlPublication
