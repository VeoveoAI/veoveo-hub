import SnapshotAssemblyProof
import CollectionProof

open Aeneas Aeneas.Std RequestCompositionHub
open IteratorProof

namespace SelectedRegistrationProof
noncomputable section
local instance (p : Prop) : Decidable p := Classical.propDecidable p

abbrev Server := SnapshotAssemblyProof.Server
abbrev Snapshot := SnapshotAssemblyProof.Snapshot
abbrev Slug := veoveo_types.platform_names.ServerSlug
abbrev Deployment := veoveo_types.execution.DeploymentId

abbrev valuesIterator :=
  RequestCompositionHub.alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV
    Deployment (alloc.sync.Arc Server)
abbrev slugOps := RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3ArcRegisteredServerBool

abbrev entries (snapshot : Snapshot) : List Server := snapshot.servers.map Prod.snd

def filterModel (slug : Slug) :=
  filterEnumeration (listEnumeration Server) (fun state : Slug => state = slug)
    (fun server => decide (server.slug = slug))

theorem values_cursor_law : CursorLaw valuesIterator (listEnumeration Server) := by
  intro state _
  cases state <;> simp [valuesIterator,
    RequestCompositionHub.alloc.collections.btree.map.Values.Insts.CoreIterTraitsIteratorIteratorSharedAV.next,
    NextStep, listEnumeration, alloc.sync.Arc]

theorem slug_predicate_law (slug : Slug) :
    PredicateLaw slugOps (fun state => state = slug)
      (fun server => decide (server.slug = slug)) := by
  intro state same server
  subst state
  simp [slugOps,
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get.closure.Insts.CoreOpsFunctionFnMutTupleShared2Shared3ArcRegisteredServerBool.call_mut,
    RequestCompositionHub.alloc.sync.Arc.Insts.CoreOpsDerefDeref.deref,
    RequestCompositionHub.veoveo_types.platform_names.ServerSlug.Insts.CoreCmpPartialEqServerSlug.eq]

/-- Explicit finite-cursor recurrence proof for THIS operational model.
The earlier generic proof's enumeration laws are reused; equality of separately
elaborated partial_fixpoint definitions is neither assumed nor axiomatized. -/
theorem filter_next_law (slug : Slug)
    (state : core.iter.adapters.filter.Filter (List Server) Slug)
    (valid : (filterModel slug).valid state) :
    RequestCompositionHub.CurrentRolesNative.filterNext valuesIterator slugOps state
      ⦃ result => NextStep (filterModel slug) state result ⦄ := by
  unfold RequestCompositionHub.CurrentRolesNative.filterNext
  step with (values_cursor_law state.iter valid.1) as ⟨entry, next, taken⟩
  rcases taken with ⟨nextValid, entryEq, suffix⟩
  cases remaining : (listEnumeration Server).remaining state.iter with
  | nil =>
    simp only [remaining, List.head?_nil, List.tail_nil] at entryEq suffix
    cases entryEq
    simp [NextStep, filterModel, filterEnumeration, remaining, suffix, nextValid, valid.2]
  | cons head tail =>
    simp only [remaining, List.head?_cons, List.tail_cons] at entryEq suffix
    cases entryEq
    step with (slug_predicate_law slug state.predicate valid.2 head)
      as ⟨hit, test, hitEq, testValid⟩
    subst hit
    cases found : decide (head.slug = slug) with
    | true =>
      simp [NextStep, filterModel, filterEnumeration, remaining, suffix, found, nextValid, testValid]
    | false =>
      have recursive := filter_next_law slug
        ({ iter := next, predicate := test } : core.iter.adapters.filter.Filter (List Server) Slug)
        (show (filterModel slug).valid _ from ⟨nextValid, testValid⟩)
      apply WP.spec_mono recursive
      intro result specification
      simpa [NextStep, filterModel, filterEnumeration, remaining, suffix, found] using specification
termination_by ((listEnumeration Server).remaining state.iter).length
decreasing_by simp_all

def matching (snapshot : Snapshot) (slug : Slug) : List Server :=
  (entries snapshot).filter (fun server => decide (server.slug = slug))

def onlyMatch : List Server → Option Server
  | [server] => some server
  | _ => none

/-- The actual getter accepts exactly one matching raw map value. Zero values
and ambiguity are both refused; namespace helper contents are irrelevant. -/
theorem get_exact (snapshot : Snapshot) (slug : Slug) :
    RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get snapshot slug =
      .ok (onlyMatch (matching snapshot slug)) := by
  apply CollectionProof.exact_of_total
  unfold RequestCompositionHub.registration.snapshot.RegistrationSnapshot.get
  simp only [RequestCompositionHub.alloc.collections.btree.map.BTreeMap.values,
    RequestCompositionHub.core.iter.traits.iterator.Iterator.filter.default,
    RequestCompositionHub.core.iter.adapters.filter.Filter.Insts.CoreIterTraitsIteratorIterator.next,
    bind_ok, bind_tc_ok]
  step with (filter_next_law slug { iter := entries snapshot, predicate := slug }
    ⟨trivial, rfl⟩) as ⟨first, next, firstSpec⟩
  rcases firstSpec with ⟨nextValid, firstHead, nextRemaining⟩
  change first = (matching snapshot slug).head? at firstHead
  change (filterModel slug).remaining next = (matching snapshot slug).tail at nextRemaining
  cases filteredRows : matching snapshot slug with
  | nil =>
    have firstNone : first = none := by
      simpa [filteredRows] using firstHead
    rw [firstNone]
    simp [RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitTry.branch,
      RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitFromResidualOptionNever.from_residual,
      filteredRows, onlyMatch]
  | cons server rest =>
    have firstSome : first = some server := by
      simpa [filteredRows] using firstHead
    rw [firstSome]
    simp only [RequestCompositionHub.core.option.Option.Insts.CoreOpsTry_traitTry.branch, bind_ok, bind_tc_ok]
    step with (filter_next_law slug next nextValid) as ⟨second, last, secondSpec⟩
    have remaining : (filterModel slug).remaining next = rest := by
      simpa [filteredRows] using nextRemaining
    have secondHead : second = rest.head? := by
      simpa only [remaining] using secondSpec.2.1
    cases rest with
    | nil =>
      simp only [List.head?_nil] at secondHead
      subst second
      simp [RequestCompositionHub.alloc.sync.Arc.Insts.CoreConvertAsRef.as_ref,
        RequestCompositionHub.core.bool.Bool.then_some, core.option.Option.is_none,
        filteredRows, onlyMatch]
    | cons other tail =>
      simp only [List.head?_cons] at secondHead
      subst second
      simp [RequestCompositionHub.alloc.sync.Arc.Insts.CoreConvertAsRef.as_ref,
        RequestCompositionHub.core.bool.Bool.then_some, core.option.Option.is_none,
        filteredRows, onlyMatch]

/-- Independent map-value lookup; no authorized/desired route is supplied. -/
def lookupBy : List (Deployment × Server) → Deployment → Option Server
  | [], _ => none
  | (key, server) :: rest, query =>
    if key = query then some server else lookupBy rest query

abbrev mapLookup : List (Deployment × Server) → Deployment → Result (Option Server) := RequestCompositionHub.alloc.collections.btree.map.BTreeMap.get
  RequestCompositionHub.alloc.alloc.Global.Insts.CoreAllocAllocatorClone
  (RequestCompositionHub.core.borrow.Borrow.Blanket Deployment)
  RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd
  RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd

theorem lookup_exact (servers : List (Deployment × Server)) (query : Deployment) :
    mapLookup servers query = .ok (lookupBy servers query) := by
  induction servers with
  | nil => rfl
  | cons entry rest ih =>
    rcases entry with ⟨key, server⟩
    cases key
    cases query
    have tail := ih
    simp only [mapLookup, RequestCompositionHub.alloc.collections.btree.map.BTreeMap.get] at tail
    simp [mapLookup, RequestCompositionHub.alloc.collections.btree.map.BTreeMap.get,
      RequestCompositionHub.CurrentRolesSDK.mapGetBy, RequestCompositionHub.core.borrow.Borrow.Blanket,
      RequestCompositionHub.core.borrow.Borrow.Blanket.borrow,
      RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd,
      RequestCompositionHub.veoveo_types.execution.DeploymentId.Insts.CoreCmpOrd.cmp,
      lookupBy, _root_.compare_eq_iff_eq, tail]
    all_goals split <;> rfl


end
end SelectedRegistrationProof

#print axioms SelectedRegistrationProof.values_cursor_law
#print axioms SelectedRegistrationProof.slug_predicate_law
#print axioms SelectedRegistrationProof.filter_next_law
#print axioms SelectedRegistrationProof.get_exact
#print axioms SelectedRegistrationProof.lookup_exact

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``SelectedRegistrationProof.values_cursor_law,
      ``SelectedRegistrationProof.slug_predicate_law, ``SelectedRegistrationProof.filter_next_law,
      ``SelectedRegistrationProof.get_exact, ``SelectedRegistrationProof.lookup_exact] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed getter/lookup axiom in {declaration}: {found}"
