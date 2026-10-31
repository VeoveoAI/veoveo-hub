import SnapshotConstructorProof
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub IteratorProof

namespace RegisteredSurfacesProof
noncomputable section

abbrev Server := SnapshotConstructorProof.Server
abbrev Snapshot := SnapshotConstructorProof.Snapshot
abbrev Surface := veoveo_hub_sdk.registration.surfaces.SurfacesDeclaration
abbrev Route := veoveo_hub_sdk.registration.surfaces.PublicRouteDeclaration

/-- Complete native payload observations, retaining duplicate route occurrences
and their order. This states neither prefix parsing nor limit/rate admission. -/
structure RoutePayload where
  path : String
  maxBodyBytes : U64
  requestsPerMinute : U32

structure SurfacePayload where
  publicRoutes : List RoutePayload
  appOrigin : Bool

def routePayload (route : Route) : RoutePayload :=
  ⟨route.«prefix».value, route.max_body_bytes.value, route.requests_per_minute.value⟩

def surfacePayload (surface : Surface) : SurfacePayload :=
  ⟨surface.public_routes.val.map routePayload, surface.app_origin⟩

def surfacesPayload (server : Server) : Option SurfacePayload :=
  server.surfaces.map surfacePayload

/-- Explicit per-field semantics in addition to complete-row identity. -/
def SurfaceFields : Option Surface → Option SurfacePayload → Prop
  | none, none => True
  | some original, some observed =>
      original.app_origin = observed.appOrigin ∧
      List.Forall₂ (fun route payload => route.«prefix».value = payload.path ∧
        route.max_body_bytes.value = payload.maxBodyBytes ∧
        route.requests_per_minute.value = payload.requestsPerMinute)
        original.public_routes.val observed.publicRoutes
  | _, _ => False

theorem payload_fields (server : Server) :
    SurfaceFields server.surfaces (surfacesPayload server) := by
  unfold surfacesPayload
  cases surface : server.surfaces with
  | none => trivial
  | some value =>
    change value.app_origin = value.app_origin ∧
      List.Forall₂ _ value.public_routes.val (value.public_routes.val.map routePayload)
    refine ⟨rfl, ?_⟩
    induction value.public_routes.val with
    | nil => exact .nil
    | cons head tail recursive => exact .cons ⟨rfl, rfl, rfl⟩ recursive

private theorem member_split {α : Type} (row : α) (rows : List α) (member : row ∈ rows) :
    ∃ before after, rows = before ++ row :: after := by
  induction rows with
  | nil => cases member
  | cons head tail recursive =>
    rcases List.mem_cons.mp member with same | rest
    · subst head; exact ⟨[], tail, rfl⟩
    · rcases recursive rest with ⟨before, after, same⟩
      exact ⟨head :: before, after, by rw [same]; rfl⟩

private theorem lookup_member (rows : List SelectedCollectionProof.Row)
    (key : SelectedCollectionProof.Deployment) (row : Server)
    (found : SelectedRegistrationProof.lookupBy rows key = some row) : (key, row) ∈ rows := by
  induction rows with
  | nil => simp [SelectedRegistrationProof.lookupBy] at found
  | cons head tail recursive =>
    rcases head with ⟨headKey, headRow⟩
    by_cases same : headKey = key
    · have equal : headRow = row := by simpa [SelectedRegistrationProof.lookupBy, same] using found
      subst row
      subst headKey
      exact List.mem_cons_self
    · exact List.mem_cons_of_mem _ (recursive (by simpa [SelectedRegistrationProof.lookupBy, same] using found))

section Constructor
variable {S I : Type}
variable (ops : Aeneas.Std.core.iter.traits.collect.IntoIterator S Server I)

/-- A selected row is an unchanged actual constructor input occurrence. The
complete payload observation exposes all public route fields in their original
vector order and the app-origin bit. Same-key overwriting is permitted; no
whole-row union or successful namespace-helper behavior is invented. -/
theorem constructed_selected_surfaces (source : S) (initial : I)
    (model : Enumeration I Server) (cursor : CursorLaw ops.iteratorInst model)
    (opened : ops.into_iter source = .ok initial) (valid : model.valid initial)
    (snapshot : Snapshot)
    (constructed : registration.snapshot.RegistrationSnapshot.new ops source = .ok snapshot)
    (composition : SelectedMapCollectionProof.Composition) (selected : Snapshot)
    (selectedSuccess : registration.snapshot.RegistrationSnapshot.selected snapshot composition = .ok selected)
    (key : SelectedCollectionProof.Deployment) (row : Server)
    (member : (key, row) ∈ selected.servers) :
    ∃ before original after,
      model.remaining initial = before ++ original :: after ∧
      row = original ∧
      surfacesPayload row = surfacesPayload original ∧
      SurfaceFields original.surfaces (surfacesPayload row) := by
  have image := SelectedMapCollectionProof.selected_success_complete snapshot composition selected selectedSuccess
  rcases (image.2.2 key row).mp member with ⟨slug, _, found, _⟩
  have current := lookup_member snapshot.servers key row found
  have origin := SnapshotConstructorProof.new_success_rows ops source initial model cursor
    opened valid snapshot constructed (key, row) current
  rcases member_split row (model.remaining initial) origin with ⟨before, after, occurrence⟩
  exact ⟨before, row, after, occurrence, rfl, rfl, payload_fields row⟩
end Constructor
end
end RegisteredSurfacesProof

#print axioms RegisteredSurfacesProof.payload_fields
#print axioms RegisteredSurfacesProof.constructed_selected_surfaces
run_cmd do
  let standard := #[``propext, ``Classical.choice, ``Quot.sound]
  let pure ← Lean.collectAxioms ``RegisteredSurfacesProof.payload_fields
  unless (pure.filter (fun n => !standard.contains n)).isEmpty do
    throwError "unexpected surface-payload axiom: {pure}"
  let found ← Lean.collectAxioms ``RegisteredSurfacesProof.constructed_selected_surfaces
  unless (found.filter (fun n => !(standard ++
      #[``_root_.registration.snapshot.RegistrationSnapshot.build_namespaces]).contains n)).isEmpty do
    throwError "unexpected constructed/selected-surface axiom: {found}"
