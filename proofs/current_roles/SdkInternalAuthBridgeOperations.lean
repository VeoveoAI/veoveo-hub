module
public import SdkInternalAuthBridgeRecords
public import RequestCompositionSdk.Funs
@[expose] public section

open Aeneas Aeneas.Std

namespace SdkInternalAuthBridge
noncomputable section

abbrev HubRequest := veoveo_hub.veoveo_hub_sdk.internal_auth.ForwardedRequest
abbrev HubIssuer := veoveo_hub.veoveo_hub_sdk.internal_auth.HubInternalTokenIssuer

/-- Concrete connection to the checked prepared SDK implementation. This is not
an equivalence proof for SDK239's native body. The failure arm only totalizes the
formal adapter on out-of-native-range mathematical Hub values; it adds no Rust
token refusal or native admission rule. RequestPhysical makes it unreachable. -/
def issue (issuer : HubIssuer) (request : HubRequest) :
    Result (core.result.Result
      veoveo_hub.veoveo_hub_sdk.internal_auth.IssuedHubInternalToken
      veoveo_hub.veoveo_hub_sdk.internal_auth.InternalTokenError) :=
  match requestToSdk request with
  | none => .fail .integerOverflow
  | some sdk => do
      let result ← RequestCompositionSdk.internal_auth.HubInternalTokenIssuer.issue (issuerEquiv issuer) sdk
      match result with
      | .Ok token => .ok (.Ok (issuedToHub token))
      | .Err error => .ok (.Err (errorToHub error))

end
end SdkInternalAuthBridge
