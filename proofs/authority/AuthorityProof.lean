import HubAuthorityProof

open Aeneas Aeneas.Std
open hub_authority_proof

namespace AuthorityProof

/- Concrete finite-set representation and its canonical filter result. Neither
membership nor the current role evaluator is replaced by an authorized flag. -/
structure SetModel (S P : Type) where
  members : S → Set P
  filter : S → S → S
  filter_members : ∀ left right,
    members (filter left right) = members left ∩ members right

/- Operational laws for the native collection boundary, including totality. -/
def LawfulPermissions {S P : Type} (ops : authority.Permissions S P)
    (model : SetModel S P) : Prop := by
  classical
  exact (∀ set permission, ops.contains set permission = .ok (decide (permission ∈ model.members set))) ∧
  (∀ set, ops.is_empty set = .ok (decide (model.members set = ∅))) ∧
  (∀ set carried, ops.narrow set carried = .ok (model.filter set carried))

/- The current-role adapter's actual requested filter is a coarse explicit
premise of this slice. Proving role/override evaluation is separate work. -/
structure HeldModel (H S P : Type) (sets : SetModel S P) where
  current : H → Set P
  result : H → S → S
  result_members : ∀ held requested,
    sets.members (result held requested) = sets.members requested ∩ current held

def LawfulHeld {H S P : Type} {sets : SetModel S P}
    (ops : authority.Held H S) (model : HeldModel H S P sets) : Prop :=
  ∀ held requested, ops.held held requested = .ok (model.result held requested)

structure UsesModel (U S P : Type) (sets : SetModel S P) where
  declared : U → Set P
  firstMissing : U → S → Option P
  none_iff : ∀ uses requested, firstMissing uses requested = none ↔
    sets.members requested ⊆ declared uses
  some_invalid : ∀ uses requested permission, firstMissing uses requested = some permission →
    permission ∈ sets.members requested ∧ permission ∉ declared uses

def LawfulUses {U S P : Type} {sets : SetModel S P}
    (ops : authority.Uses U S P) (model : UsesModel U S P sets) : Prop :=
  ∀ uses requested, ops.first_undeclared uses requested = .ok (model.firstMissing uses requested)

def LawfulResource {R K : Type} (ops : authority.ResourceKey R K) (key : R → K) : Prop := by
  classical
  exact ∀ resource requested, ops.«matches» resource requested = .ok (decide (key resource = requested))

-- Classical decidability is part of Lean's ordinary foundation, not an axiom
-- about Rust sets, role policy, authentication or I/O.
noncomputable section

section Exchange
variable {U S P H R K F C A T D N : Type}
variable (sets : SetModel S P) (usesModel : UsesModel U S P sets)
variable (usesOps : authority.Uses U S P) (usesLaw : LawfulUses usesOps usesModel)
variable (heldModel : HeldModel H S P sets) (heldOps : authority.Held H S)
variable (heldLaw : LawfulHeld heldOps heldModel)
variable (key : R → K) (resourceOps : authority.ResourceKey R K)
variable (resourceLaw : LawfulResource resourceOps key)

include usesLaw in
theorem declare_exact (requested : S) (uses : U) :
    authority.declare usesOps requested uses = .ok
      (match usesModel.firstMissing uses requested with
       | none => .Ok { requested }
       | some permission => .Err permission) := by
  unfold LawfulUses at usesLaw
  unfold authority.declare
  rw [usesLaw]
  cases usesModel.firstMissing uses requested <;> simp only [bind_ok]

include usesLaw in
theorem declare_success_iff (requested : S) (uses : U) :
    authority.declare usesOps requested uses ⦃ result =>
      ∀ output, result = .Ok output ↔
        sets.members requested ⊆ usesModel.declared uses ∧ output.requested = requested ⦄ := by
  classical
  rw [declare_exact sets usesModel usesOps usesLaw]
  simp only [WP.spec_ok]
  cases missing : usesModel.firstMissing uses requested with
  | none =>
    have subset := (usesModel.none_iff uses requested).mp missing
    intro output
    cases output
    simp [subset, eq_comm]
  | some permission =>
    have notSubset : ¬ sets.members requested ⊆ usesModel.declared uses := by
      intro subset
      have invalid := usesModel.some_invalid uses requested permission missing
      exact invalid.2 (subset invalid.1)
    simp [notSubset]

/- Complete ordered target refusal, carrying the exact typed mismatched resource. -/
def expectedAddress (declared : authority.Declared S)
    (destination : authority.Destination R) (requested : Option K) :
    core.result.Result (authority.Addressed S R) (authority.TargetRefusal R) := by
  classical
  exact match destination with
  | .MissingProfile => .Err .MissingProfile
  | .InvalidResource => .Err .InvalidResource
  | .Resource resource => if requested = some (key resource) then
      .Ok { requested := declared.requested, resource }
    else .Err (.OtherResource resource)

include resourceLaw in
theorem address_exact (declared : authority.Declared S)
    (destination : authority.Destination R) (requested : Option K) :
    authority.address resourceOps declared destination requested =
      .ok (expectedAddress key declared destination requested) := by
  classical
  unfold LawfulResource at resourceLaw
  cases destination with
  | MissingProfile => rfl
  | InvalidResource => rfl
  | Resource resource =>
    cases requested with
    | none => simp [authority.address, expectedAddress]
    | some requested =>
      by_cases same : key resource = requested
      · subst requested
        simp [authority.address, expectedAddress, resourceLaw]
      · have other : requested ≠ key resource := Ne.symm same
        simp [authority.address, expectedAddress, resourceLaw, same, other]

theorem provide_exact (addressed : authority.Addressed S R) (missing : Slice N) :
    authority.provide addressed missing = .ok
      (if missing.val = [] then .Ok { requested := addressed.requested, resource := addressed.resource }
       else .Err (.MissingProviders : authority.TargetRefusal R)) := by
  classical
  by_cases complete : missing.val = [] <;>
    simp [authority.provide, core.slice.Slice.is_empty, Slice.length, complete]

include heldLaw in
theorem grant_exact (provided : authority.Provided S R) (held : H)
    (context : authority.Context F C A T D) :
    authority.grant heldOps provided held context = .ok
      { permissions := heldModel.result held provided.requested, resource := provided.resource, context } := by
  unfold LawfulHeld at heldLaw
  simp [authority.grant, heldLaw]

/- A trace composed exclusively from extracted functions. The surrounding Rust
caller sequence and concrete catalog/token adapters remain reviewed/tested. -/
def exchangeTrace (requested : S) (uses : U) (destination : authority.Destination R)
    (requestedResource : Option K) (missing : Slice N) (held : H)
    (context : authority.Context F C A T D) :
    Result (Except (P ⊕ authority.TargetRefusal R) (authority.Grant S R F C A T D)) := do
  let declaredResult ← authority.declare usesOps requested uses
  match declaredResult with
  | .Err permission => Result.ok (.error (.inl permission))
  | .Ok declared =>
    let addressedResult ← authority.address resourceOps declared destination requestedResource
    match addressedResult with
    | .Err refusal => Result.ok (.error (.inr refusal))
    | .Ok addressed =>
      let providedResult ← authority.provide addressed missing
      match providedResult with
      | .Err refusal => Result.ok (.error (.inr refusal))
      | .Ok provided =>
        let output ← authority.grant heldOps provided held context
        Result.ok (.ok output)

def expectedExchange (requested : S) (uses : U) (destination : authority.Destination R)
    (requestedResource : Option K) (missing : Slice N) (held : H)
    (context : authority.Context F C A T D) :
    Except (P ⊕ authority.TargetRefusal R) (authority.Grant S R F C A T D) := by
  classical
  exact match usesModel.firstMissing uses requested with
  | some permission => .error (.inl permission)
  | none => match expectedAddress key { requested } destination requestedResource with
    | .Err refusal => .error (.inr refusal)
    | .Ok addressed => if missing.val = [] then
        .ok { permissions := heldModel.result held requested, resource := addressed.resource, context }
      else .error (.inr .MissingProviders)

include usesLaw heldLaw resourceLaw in
theorem exchange_exact (requested : S) (uses : U) (destination : authority.Destination R)
    (requestedResource : Option K) (missing : Slice N) (held : H)
    (context : authority.Context F C A T D) :
    exchangeTrace usesOps heldOps resourceOps requested uses destination requestedResource missing held context =
      .ok (expectedExchange sets usesModel heldModel key requested uses destination requestedResource missing held context) := by
  classical
  unfold exchangeTrace
  rw [declare_exact sets usesModel usesOps usesLaw]
  cases first : usesModel.firstMissing uses requested with
  | some permission => simp [expectedExchange, first]
  | none =>
    simp only [bind_ok]
    rw [address_exact key resourceOps resourceLaw]
    cases addressed : expectedAddress key { requested } destination requestedResource with
    | Err refusal => simp [expectedExchange, first, addressed]
    | Ok value =>
      have requestEq : value.requested = requested := by
        cases destination with
        | MissingProfile => cases addressed
        | InvalidResource => cases addressed
        | Resource resource =>
          by_cases same : requestedResource = some (key resource)
          · simp [expectedAddress, same] at addressed
            cases addressed
            rfl
          · simp [expectedAddress, same] at addressed
      simp only [bind_ok]
      rw [provide_exact]
      by_cases complete : missing.val = [] <;>
        simp [complete, grant_exact sets heldModel heldOps heldLaw,
          expectedExchange, first, addressed, requestEq]

/- Acceptance is not vacuous: all declared uses, exact resource and provider
coverage are sufficient, and the successful output preserves every context field. -/
include usesLaw heldLaw resourceLaw in
theorem exchange_success_iff (requested : S) (uses : U)
    (destination : authority.Destination R) (requestedResource : Option K)
    (missing : Slice N) (held : H) (context : authority.Context F C A T D) :
    exchangeTrace usesOps heldOps resourceOps requested uses destination requestedResource missing held context
      ⦃ result => ∀ output, result = .ok output ↔
        sets.members requested ⊆ usesModel.declared uses ∧ missing.val = [] ∧
        ∃ resource, destination = .Resource resource ∧ requestedResource = some (key resource) ∧
          output = { permissions := heldModel.result held requested, resource, context } ⦄ := by
  classical
  rw [exchange_exact sets usesModel usesOps usesLaw heldModel heldOps heldLaw key resourceOps resourceLaw]
  simp only [WP.spec_ok]
  intro output
  unfold expectedExchange
  cases first : usesModel.firstMissing uses requested with
  | some permission =>
    have notSubset : ¬ sets.members requested ⊆ usesModel.declared uses := by
      intro subset
      have invalid := usesModel.some_invalid uses requested permission first
      exact invalid.2 (subset invalid.1)
    simp [notSubset]
  | none =>
    have subset := (usesModel.none_iff uses requested).mp first
    cases destination <;> simp [expectedAddress, subset]
    rename_i resource
    by_cases same : requestedResource = some (key resource) <;>
      by_cases complete : missing.val = [] <;> simp [same, complete, and_comm, eq_comm]
end Exchange

section Downstream
variable {S P R M : Type}
variable (sets : SetModel S P) (ops : authority.Permissions S P)
variable (law : LawfulPermissions ops sets)

/- Exact route semantics. A permission route checks the carried permission;
current-policy consistency is a separate premise for final grant membership. -/
def RouteAllowed (carried narrowed : S) (route : authority.Route P) : Prop :=
  match route with
  | .Authenticated => True
  | .Permission permission => permission ∈ sets.members carried
  | .Server => sets.members narrowed ≠ ∅
  | .Undeclared => False

def expectedDecision (decision : authority.Decision S R M) (managed : Bool)
    (delegation : Option (S × authority.Route P)) (policyDeny missingPermission : R) :
    authority.Decision S R M := by
  classical
  exact if managed = false then { decision with allowed := false, reason := policyDeny }
    else if decision.allowed = false then decision
    else match delegation with
      | none => decision
      | some (carried, route) =>
        let narrowed := sets.filter decision.granted carried
        if RouteAllowed sets carried narrowed route then { decision with granted := narrowed }
        else { decision with allowed := false, reason := missingPermission, granted := narrowed }

include law in
theorem narrow_exact (decision : authority.Decision S R M) (managed : Bool)
    (delegation : Option (S × authority.Route P)) (policyDeny missingPermission : R) :
    authority.narrow ops decision managed delegation policyDeny missingPermission =
      .ok (expectedDecision sets decision managed delegation policyDeny missingPermission) := by
  classical
  cases managed <;> cases allowed : decision.allowed <;>
    cases delegation with
    | none => simp [authority.narrow, expectedDecision, allowed]
    | some pair =>
      rcases pair with ⟨carried, route⟩
      cases route <;>
        simp [authority.narrow, expectedDecision, allowed, law.1, law.2.1, law.2.2, RouteAllowed]
      all_goals split <;> simp_all

/- Exact acceptance/refusal with preserved entire metadata, not a safety-only
claim that could also be met by refusing every request. -/
include law in
theorem narrow_success_iff (decision : authority.Decision S R M) (managed : Bool)
    (delegation : Option (S × authority.Route P)) (policyDeny missingPermission : R) :
    authority.narrow ops decision managed delegation policyDeny missingPermission ⦃ result =>
      result.allowed = true ↔ managed = true ∧ decision.allowed = true ∧
        (∀ carried route, delegation = some (carried, route) →
          RouteAllowed sets carried (sets.filter decision.granted carried) route) ⦄ := by
  classical
  rw [narrow_exact sets ops law]
  simp only [WP.spec_ok]
  cases managed <;> cases allowed : decision.allowed <;>
    cases delegation with
    | none => simp [expectedDecision, allowed]
    | some pair =>
      rcases pair with ⟨carried, route⟩
      by_cases access : RouteAllowed sets carried (sets.filter decision.granted carried) route <;>
        simp [expectedDecision, allowed, access]

/- With the actual policy's permission consistency premise, accepted permission
routes retain that permission in the final narrowed authority. -/
include law in
theorem accepted_permission_survives (decision : authority.Decision S R M)
    (carried : S) (permission : P) (policyDeny missingPermission : R)
    (consistent : permission ∈ sets.members decision.granted) :
    authority.narrow ops decision true (some (carried, .Permission permission))
      policyDeny missingPermission ⦃ result => result.allowed = true →
        permission ∈ sets.members result.granted ∧ result.metadata = decision.metadata ⦄ := by
  classical
  rw [narrow_exact sets ops law]
  simp only [WP.spec_ok]
  cases allowed : decision.allowed with
  | false => simp [expectedDecision, allowed]
  | true =>
    by_cases carriedPermission : permission ∈ sets.members carried <;>
      simp [expectedDecision, allowed, RouteAllowed, carriedPermission, sets.filter_members, consistent]
end Downstream

end

#print axioms declare_exact
#print axioms declare_success_iff
#print axioms address_exact
#print axioms provide_exact
#print axioms grant_exact
#print axioms exchange_exact
#print axioms exchange_success_iff
#print axioms narrow_exact
#print axioms narrow_success_iff
#print axioms accepted_permission_survives

end AuthorityProof
