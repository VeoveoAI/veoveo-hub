import RegistrationAuditedLifetimeModel

/-! Handwritten instance-registration service continuation, not Rust extraction.
Observation functions supply native results, not RPC implementations or proofs of
ClientInformation construction. `knownSnapshot = none` means NO preservation
claim, never deleted state. Interpreting `some` requires faithful read/no-write
observations and no external durable writer during this suffix. Cache/catalog
effects of reload are not represented by the durable snapshot. -/
namespace RegistrationPostcommit
open RegistrationAuditedLifetime (Activity)

inductive ServiceError (Internal Rejection : Type)
  | internal (error : Internal)
  | rejected (error : Rejection)
  | unauthorized (tokenSent : Bool)
  deriving DecidableEq

structure Ended (Client Server Login : Type) where
  client : Client
  server : Server
  login : Login
  deriving DecidableEq

/-- Opaque `issued` retains the raw issuance/configuration inputs not represented
by the store model: token/password/time/statement/database URL/trust settings.
The continuation preserves arguments, not their native construction/admission. -/
structure Context (Client Login Metadata Issued : Type) where
  client : Client
  login : Login
  metadata : Metadata
  issued : Issued
  deriving DecidableEq

structure Projection (Metadata Server Deployment : Type) where
  server : Metadata → Server
  deployment : Metadata → Deployment

inductive MarkerObservation (Internal : Type)
  /-- Observed statement refusal without a write, e.g. BEFORE UPDATE exception.
  This is not a policy or an assertion about every served error. -/
  | confirmedNoWriteRefusal (error : Internal)
  /-- Both success and uncertain error may have changed the full instance. -/
  | generalOutcome (result : Except Internal Unit)

inductive OldCall (Client Server Login : Type)
  | ended (row : Ended Client Server Login) (activity : Activity)
  | reload (position : Nat) (client : Client)
  deriving DecidableEq

inductive Call (Client Server Login Metadata Issued Deployment Grant : Type)
  | old (call : OldCall Client Server Login)
  | served (deployment : Deployment)
  | reloadNew (client : Client)
  /-- Source telemetry at service.rs422, before fallible grant/information work;
  this event is NOT proof of a successful caller response. -/
  | registeredLog (server : Server) (client : Client) (login : Login)
  | database (context : Context Client Login Metadata Issued)
  | information (context : Context Client Login Metadata Issued) (grant : Grant)
  | returnSuccess
  deriving DecidableEq

structure Outcome (Snapshot Call Error Output : Type) where
  knownSnapshot : Option Snapshot
  calls : List Call
  result : Except Error Output

structure OldOutcome (Client Server Login Internal : Type) where
  calls : List (OldCall Client Server Login)
  result : Except Internal Unit

structure Observations (Client Deployment Context Internal Rejection Grant Output : Type) where
  oldReload : Nat → Client → Except Internal Unit
  marker : Deployment → MarkerObservation Internal
  newReload : Client → Except Internal Unit
  database : Context → Except Internal Grant
  /-- information() already returns the complete typed RegistrationError.
  In particular its Rejected result is not rewrapped as Internal. -/
  information : Context → Grant → Except (ServiceError Internal Rejection) Output

variable {K C S L M U D G E R O : Type}
abbrev Calls := List (Call C S L M U D G)
abbrev Result := Outcome K (Call C S L M U D G) (ServiceError E R) O

def oldCalls (position : Nat) : List (Ended C S L) → List (OldCall C S L)
  | [] => []
  | row :: rows => [.ended row .replace, .reload position row.client] ++ oldCalls (position + 1) rows

def reloadOld (observe : Nat → C → Except E Unit) (position : Nat) :
    List (Ended C S L) → OldOutcome C S L E
  | [] => ⟨[], .ok ()⟩
  | row :: rows =>
      let attempted := [OldCall.ended row .replace, OldCall.reload position row.client]
      match observe position row.client with
      | .error error => ⟨attempted, .error error⟩
      | .ok () =>
          let rest := reloadOld observe (position + 1) rows
          ⟨attempted ++ rest.calls, rest.result⟩

def AllOldOk (observe : Nat → C → Except E Unit) (position : Nat) : List (Ended C S L) → Prop
  | [] => True
  | row :: rows => observe position row.client = .ok () ∧ AllOldOk observe (position + 1) rows

/-- Only called after a general served success. Snapshot knowledge is already
unknown; no statement about a complete live instance is made on this branch. -/
def afterMarker (projection : Projection M S D) (context : Context C L M U)
    (observe : Observations C D (Context C L M U) E R G O) (before : Calls (C := C) (S := S) (L := L) (M := M) (U := U) (D := D) (G := G)) :
    Result (K := K) (C := C) (S := S) (L := L) (M := M) (U := U) (D := D) (G := G) (E := E) (R := R) (O := O) :=
  let withNew := before ++ [.reloadNew context.client]
  match observe.newReload context.client with
  | .error error => ⟨none, withNew, .error (.internal error)⟩
  | .ok () =>
      let withDatabase := withNew ++
        [.registeredLog (projection.server context.metadata) context.client context.login, .database context]
      match observe.database context with
      | .error error => ⟨none, withDatabase, .error (.internal error)⟩
      | .ok grant =>
          let withInformation := withDatabase ++ [.information context grant]
          match observe.information context grant with
          | .error error => ⟨none, withInformation, .error error⟩
          | .ok output => ⟨none, withInformation ++ [.returnSuccess], .ok output⟩

/-- Starts only after actual instance-store success/returned ended rows. URL
environment selection precedes this cut. The function contains no commit,
rollback, SQL writes, cache implementation or native response constructor. -/
def finishRegistration (snapshot : K) (ended : List (Ended C S L))
    (projection : Projection M S D) (context : Context C L M U)
    (observe : Observations C D (Context C L M U) E R G O) :
    Result (K := K) (C := C) (S := S) (L := L) (M := M) (U := U) (D := D) (G := G) (E := E) (R := R) (O := O) :=
  let old := reloadOld observe.oldReload 0 ended
  let completed := old.calls.map Call.old
  match old.result with
  | .error error => ⟨some snapshot, completed, .error (.internal error)⟩
  | .ok () =>
      let withMarker := completed ++ [.served (projection.deployment context.metadata)]
      match observe.marker (projection.deployment context.metadata) with
      | .confirmedNoWriteRefusal error => ⟨some snapshot, withMarker, .error (.internal error)⟩
      | .generalOutcome (.error error) => ⟨none, withMarker, .error (.internal error)⟩
      | .generalOutcome (.ok ()) => afterMarker projection context observe withMarker

end RegistrationPostcommit
