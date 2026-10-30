import HubCurrentRoles
import AuthorityComposition

open Aeneas Aeneas.Std veoveo_hub

-- Definition dependency/model-compilation checkpoint only. No new permission
-- theorem or Rust primitive refinement is established by these axiom audits.
#print axioms delegation.CurrentPermissions.Insts.Veoveo_hubDelegationAuthorityHeldBTreeSetPermissionNameGlobal.held
#print axioms registration.snapshot.RegistrationSnapshot.iter
#print axioms registration.snapshot.RegistrationSnapshot.iter.closure.Insts.CoreOpsFunctionFnMutTupleShared0ArcRegisteredServerSharedRegisteredServer.call_mut
#print axioms CurrentRolesNative.flatMapNext
#print axioms CurrentRolesNative.arcAsRef

#print axioms prost_types.protobuf.value.Kind
#print axioms serde_json.value.Value

#print axioms alloc.collections.btree.set.BTreeSetTGlobal.new
