-- Shared native primitive representations; generated structs remain in their
-- distinct RequestCompositionHub namespace and require explicit typed bridges.
module
public import HubCurrentRoles.TypesExternal
@[expose] public section

structure RequestCompositionOpaque (tag : Nat) where
  value : _root_.String
  deriving DecidableEq

structure core.iter.adapters.filter_map.FilterMap (I F : Type _) where
  iter : I
  f : F

abbrev jsonwebtoken.errors.Error := RequestCompositionOpaque 0
abbrev reqwest_1.error.Error := RequestCompositionOpaque 1
abbrev serde_json.error.Error := RequestCompositionOpaque 2
abbrev veoveo_hub_sdk.internal_auth.HubInternalSigningKey := RequestCompositionOpaque 3
abbrev veoveo_types.error.IdentifierError := RequestCompositionOpaque 4
abbrev mcp.upstream_http.UpstreamTransport := RequestCompositionOpaque 5
