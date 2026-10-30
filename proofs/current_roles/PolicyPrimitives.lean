import CollectionProof

open Aeneas Aeneas.Std veoveo_hub IteratorProof CollectionProof NativeSetProof

namespace PolicyPrimitives
noncomputable section
local instance (predicate : Prop) : Decidable predicate := Classical.propDecidable predicate

theorem set_cursor_law (T : Type) : CursorLaw
    (alloc.collections.btree.set.Iter.Insts.CoreIterTraitsIteratorIteratorSharedAT T)
    (listEnumeration T) := by
  intro state _
  exact list_next_correct state

theorem set_collect_exact {I T : Type}
    (iterator : Aeneas.Std.core.iter.traits.iterator.Iterator I T)
    (model : Enumeration I T) (cursorLaw : CursorLaw iterator model)
    (nativeOrder : core.cmp.Ord T) (order : Comparison T)
    (orderLaw : LawfulComparison nativeOrder.cmp order) (state : I) (valid : model.valid state) :
    Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default iterator
      (alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator nativeOrder) state =
      .ok (model.remaining state).toFinset := by
  unfold Aeneas.Std.core.iter.traits.iterator.Iterator.collect.default
  simp only [alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator,
    alloc.collections.btree.set.BTreeSetTGlobal.Insts.CoreIterTraitsCollectFromIterator.from_iter,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket,
    Aeneas.Std.core.iter.traits.collect.IntoIterator.Blanket.into_iter, bind_ok]
  rw [collect_set_exact iterator model cursorLaw nativeOrder.cmp order orderLaw state ∅ valid]
  simp

theorem set_extend_exact {T A : Type} (nativeOrder : core.cmp.Ord T)
    (copy : core.marker.Copy T) (allocator : core.alloc.AllocatorClone A)
    (order : Comparison T) (orderLaw : LawfulComparison nativeOrder.cmp order)
    (initial added : alloc.collections.btree.set.BTreeSet T A) :
    alloc.collections.btree.set.BTreeSet.Insts.CoreIterTraitsCollectExtendSharedAT.extend
      nativeOrder copy allocator (SharedABTreeSet.Insts.CoreIterTraitsCollectIntoIteratorSharedATIter T allocator)
      initial added = .ok (initial ∪ added) := by
  unfold alloc.collections.btree.set.BTreeSet.Insts.CoreIterTraitsCollectExtendSharedAT.extend
  simp only [SharedABTreeSet.Insts.CoreIterTraitsCollectIntoIteratorSharedATIter,
    SharedABTreeSet.Insts.CoreIterTraitsCollectIntoIteratorSharedATIter.into_iter, bind_ok]
  rw [collect_set_exact _ (listEnumeration T) (set_cursor_law T) nativeOrder.cmp order orderLaw
    added.toList initial trivial]
  simp [listEnumeration]

#print axioms set_cursor_law
#print axioms set_collect_exact
#print axioms set_extend_exact
end
end PolicyPrimitives
