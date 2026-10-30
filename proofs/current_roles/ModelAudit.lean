import HubCurrentRoles
open Aeneas Aeneas.Std veoveo_hub
-- Stage 1: definition dependency audit only; no policy theorem is claimed.
#print axioms policy.grants.held_roles
#print axioms policy.grants.effective_risks
#print axioms policy.grants.granted_permissions
#print axioms control_plane.authorization_config.InstallationRole.allows
#print axioms CurrentRolesNative.filterNext
#print axioms CurrentRolesNative.any
#print axioms CurrentRolesNative.collectSet
