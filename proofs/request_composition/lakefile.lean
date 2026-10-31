import Lake
open Lake DSL
require hubCurrentRolesProof from "../current_roles"
package hubRequestCompositionProof
@[default_target]
lean_lib RequestCompositionHub
@[default_target]
lean_lib SnapshotAssemblyProof

-- Requires the explicit private patched-tool workflow in tooling/README.md.
lean_lib SelectedRegistrationProof

lean_lib SelectedCollectionProof

lean_lib SelectedMapInsertionProof

lean_lib SelectedMapCollectionProof

lean_lib SelectedRouteContextProof
lean_lib SnapshotConstructorProof
