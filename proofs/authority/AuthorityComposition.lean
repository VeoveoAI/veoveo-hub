import AuthorityProof

open Aeneas Aeneas.Std
open hub_authority_proof AuthorityProof

namespace AuthorityComposition

noncomputable section

variable {U S P H R K F C A T D N V M : Type}
variable (sets : SetModel S P) (usesModel : UsesModel U S P sets)
variable (usesOps : authority.Uses U S P) (usesLaw : LawfulUses usesOps usesModel)
variable (heldModel : HeldModel H S P sets) (heldOps : authority.Held H S)
variable (heldLaw : LawfulHeld heldOps heldModel)
variable (key : R → K) (resourceOps : authority.ResourceKey R K)
variable (resourceLaw : LawfulResource resourceOps key)
variable (ops : authority.Permissions S P) (law : LawfulPermissions ops sets)

include usesLaw heldLaw resourceLaw in
/-- Every returned permission is requested, declared and currently held; this
uses the explicitly stated current-role filter law, not a proof of role policy. -/
theorem issued_membership_bound (requested : S) (uses : U)
    (destination : authority.Destination R) (requestedResource : Option K)
    (missing : Slice N) (held : H) (context : authority.Context F C A T D) :
    exchangeTrace usesOps heldOps resourceOps requested uses destination requestedResource missing held context
      ⦃ result => ∀ output, result = .ok output → output.context = context ∧
        ∀ permission, permission ∈ sets.members output.permissions ↔
          permission ∈ sets.members requested ∧ permission ∈ usesModel.declared uses ∧
          permission ∈ heldModel.current held ⦄ := by
  classical
  apply WP.spec_mono (exchange_success_iff sets usesModel usesOps usesLaw
    heldModel heldOps heldLaw key resourceOps resourceLaw requested uses destination
    requestedResource missing held context)
  intro result success output accepted
  rcases (success output).mp accepted with ⟨subset, _, resource, _, _, outputEq⟩
  subst output
  constructor
  · rfl
  · intro permission
    rw [heldModel.result_members]
    simp only [Set.mem_inter_iff]
    constructor
    · rintro ⟨requestedPermission, currentPermission⟩
      exact ⟨requestedPermission, subset requestedPermission, currentPermission⟩
    · rintro ⟨requestedPermission, _, currentPermission⟩
      exact ⟨requestedPermission, currentPermission⟩

include law in
/-- Accepted delegated output retains metadata and exactly the intersection of
current route policy authority and the carried grant. Denied payloads are not
misrepresented as filtered output. -/
theorem downstream_membership_bound (decision : authority.Decision S V M)
    (managed : Bool) (carried : S) (route : authority.Route P)
    (policyDeny missingPermission : V) :
    authority.narrow ops decision managed (some (carried, route)) policyDeny missingPermission
      ⦃ output => output.allowed = true → output.metadata = decision.metadata ∧
        sets.members output.granted = sets.members decision.granted ∩ sets.members carried ⦄ := by
  classical
  rw [narrow_exact sets ops law]
  simp only [WP.spec_ok]
  cases managed <;> cases allowed : decision.allowed <;>
    by_cases access : RouteAllowed sets carried (sets.filter decision.granted carried) route <;>
      simp [expectedDecision, allowed, access, sets.filter_members]

include usesLaw heldLaw resourceLaw law in
/-- A composed extracted-function trace, with all original grant context and
policy metadata preserved. The actual asynchronous caller/token adapters remain
outside the formal boundary; no loopback or new runtime pipeline is modeled. -/
theorem exchange_then_downstream_bound (requested : S) (uses : U)
    (destination : authority.Destination R) (requestedResource : Option K)
    (missing : Slice N) (held : H) (context : authority.Context F C A T D)
    (decision : authority.Decision S V M) (managed : Bool) (route : authority.Route P)
    (policyDeny missingPermission : V) :
    exchangeTrace usesOps heldOps resourceOps requested uses destination requestedResource missing held context
      ⦃ result => ∀ issued, result = .ok issued →
        authority.narrow ops decision managed (some (issued.permissions, route)) policyDeny missingPermission
          ⦃ output => output.allowed = true → issued.context = context ∧
            output.metadata = decision.metadata ∧
            ∀ permission, permission ∈ sets.members output.granted ↔
              permission ∈ sets.members decision.granted ∧
              permission ∈ sets.members requested ∧ permission ∈ usesModel.declared uses ∧
              permission ∈ heldModel.current held ⦄ ⦄ := by
  classical
  apply WP.spec_mono (issued_membership_bound sets usesModel usesOps usesLaw
    heldModel heldOps heldLaw key resourceOps resourceLaw requested uses destination
    requestedResource missing held context)
  intro result issueBound issued accepted
  have grantBound := issueBound issued accepted
  apply WP.spec_mono (downstream_membership_bound sets ops law decision managed
    issued.permissions route policyDeny missingPermission)
  intro output outputBound allowed
  rcases outputBound allowed with ⟨metadata, permissionSets⟩
  refine ⟨grantBound.1, metadata, ?_⟩
  intro permission
  rw [permissionSets]
  simp only [Set.mem_inter_iff, grantBound.2 permission]

#print axioms issued_membership_bound
#print axioms downstream_membership_bound
#print axioms exchange_then_downstream_bound

end
end AuthorityComposition
