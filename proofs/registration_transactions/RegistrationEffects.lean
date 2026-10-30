import RegistrationSpecification
import Lean.Util.CollectAxioms

namespace RegistrationTransaction
variable {C L M R P F : Type}
variable [DecidableEq C] [DecidableEq L] [DecidableEq M]

theorem applyWrites_append (db : Database C L M R P F) (left right : List (Write C L M R P)) :
    applyWrites db (left ++ right) = applyWrites (applyWrites db left) right :=
  List.foldl_append

theorem admit_exact (db : Database C L M R P F) (request : Request C L M R P) :
    applyWrites db (admitWrites request) = expectedReplica db request := rfl

theorem rowMap_cons (head : Registration C L M R) (tail : List (Registration C L M R)) :
    rowMap (head :: tail) = put (rowMap tail) head.client (some head) := rfl

theorem rowMap_some_iff (rows : List (Registration C L M R))
    (unique : (rows.map (·.client)).Nodup) (key : C) (row : Registration C L M R) :
    rowMap rows key = some row ↔ row ∈ rows ∧ row.client = key := by
  induction rows with
  | nil => simp [rowMap]
  | cons head tail ih =>
    simp only [List.map_cons, List.nodup_cons] at unique
    rw [rowMap_cons]
    by_cases same : key = head.client
    · subst key
      simp only [put, eq_self_iff_true, ite_true, Option.some.injEq]
      constructor
      · intro equal
        subst row
        exact ⟨by simp, rfl⟩
      · rintro ⟨member, equal⟩
        rcases List.mem_cons.mp member with headEqual | inTail
        · exact headEqual.symm
        · exact False.elim (unique.1 (List.mem_map.mpr ⟨row, inTail, equal⟩))
    · simp only [put, if_neg same]
      rw [ih unique.2]
      constructor
      · rintro ⟨member, equal⟩
        exact ⟨List.mem_cons_of_mem _ member, equal⟩
      · rintro ⟨member, equal⟩
        rcases List.mem_cons.mp member with headEqual | inTail
        · subst row
          exact False.elim (same equal.symm)
        · exact ⟨inTail, equal⟩

theorem rowMap_none_of_not_mem (rows : List (Registration C L M R)) (key : C)
    (absent : key ∉ rows.map (·.client)) : rowMap rows key = none := by
  induction rows with
  | nil => rfl
  | cons head tail ih =>
    simp only [List.map_cons, List.mem_cons, not_or] at absent
    simp [rowMap_cons, put, absent.1, ih absent.2]

theorem registeredLogin_listing (db : Database C L M R P F)
    (rows : List (Registration C L M R)) (listing : Listing db rows) (login : L) :
    registeredLogin db login ↔ login ∈ rows.map (·.login) := by
  constructor
  · rintro ⟨client, row, stored, equal⟩
    rw [listing.2] at stored
    have member := ((rowMap_some_iff rows listing.1 client row).mp stored).1
    exact List.mem_map.mpr ⟨row, member, equal⟩
  · intro member
    obtain ⟨row, inRows, equal⟩ := List.mem_map.mp member
    refine ⟨row.client, row, ?_, equal⟩
    rw [listing.2]
    exact (rowMap_some_iff rows listing.1 row.client row).mpr ⟨inRows, rfl⟩

theorem delete_listing_clears (db : Database C L M R P F)
    (rows : List (Registration C L M R)) (listing : Listing db rows) :
    (applyWrite db (.deleteRows (rows.map (·.client)))).registrations = fun _ => none := by
  funext key
  by_cases member : key ∈ rows.map (·.client)
  · simp only [applyWrite, if_pos member]
  · simp only [applyWrite, if_neg member, listing.2, rowMap_none_of_not_mem rows key member]

theorem drop_logins_exact (db : Database C L M R P F) (logins : List L) :
    applyWrites db (logins.map Write.dropLogin) =
      { db with logins := fun login => if login ∈ logins then none else db.logins login } := by
  induction logins generalizing db with
  | nil => rfl
  | cons head tail ih =>
    change applyWrites (applyWrite db (.dropLogin head)) (tail.map Write.dropLogin) = _
    rw [ih]
    apply Database.ext <;> try rfl
    funext login
    by_cases same : login = head <;> by_cases member : login ∈ tail <;>
      simp [applyWrite, put, same, member]

/-- The sequential replacement contract is independent of the staged program.
This proves their equality when DELETE captured the complete current projection;
later history proofs must derive that snapshot stability from real locking. -/
theorem replacement_exact (db : Database C L M R P F) (request : Request C L M R P)
    (rows : List (Registration C L M R)) (listing : Listing db rows) :
    applyWrites db (replaceWrites request rows) = expectedReplacement db request := by
  let base := applyWrites db
    [.deployment request.candidate.metadata, .deleteRows (rows.map (·.client))]
  have clears : base.registrations = fun _ => none :=
    delete_listing_clears (applyWrite db (.deployment request.candidate.metadata)) rows listing
  unfold replaceWrites
  simp only [applyWrites_append, admit_exact]
  have maps : rows.map (fun row => Write.dropLogin row.login) =
      (rows.map (·.login)).map (Write.dropLogin : L → Write C L M R P) := by
    rw [List.map_map]
    rfl
  rw [maps]
  change expectedReplica (applyWrites base ((rows.map (·.login)).map Write.dropLogin)) request = _
  rw [drop_logins_exact]
  apply Database.ext
  · rfl
  · funext client
    change put base.registrations request.candidate.client (some request.candidate) client = _
    rw [clears]
    rfl
  · funext login
    by_cases same : login = request.candidate.login
    · simp only [expectedReplica, expectedReplacement, put, same, ite_true]
    · by_cases member : login ∈ rows.map (·.login)
      · have present := (registeredLogin_listing db rows listing login).mpr member
        simp only [expectedReplica, expectedReplacement, put, same, member, present, ite_false, ite_true]
      · have absent : ¬registeredLogin db login :=
          fun present => member ((registeredLogin_listing db rows listing login).mp present)
        simp only [expectedReplica, expectedReplacement, put, same, member, absent, ite_false]
        rfl
  · rfl

#print axioms admit_exact
#print axioms replacement_exact
run_cmd do
  for declaration in [``admit_exact, ``replacement_exact] do
    let axioms ← Lean.collectAxioms declaration
    for name in axioms do
      unless name == ``propext || name == ``Classical.choice || name == ``Quot.sound do
        throwError "unreviewed registration effects axiom: {name}"

end RegistrationTransaction
